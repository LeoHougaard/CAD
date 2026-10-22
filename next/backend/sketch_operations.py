"""Native Sketcher task operations. Caller owns transactions, solve and revision.

All geometry and constraints remain native. Calls follow pinned FreeCAD 1.1.4
SketchObjectPyImp.cpp; arc extension additionally registers its native property
change because the native method otherwise changes an arc handle in place.
"""

import math
import xml.etree.ElementTree as ET

import FreeCAD as App
import Part
import Sketcher


OPERATIONS = ("fillet_geometry", "split_geometry", "extend_geometry", "carbon_copy",
              "spline_degree", "spline_knot", "spline_pole", "spline_weight")


class OperationError(ValueError):
    def __init__(self, message, code="invalid_request"):
        super().__init__(message)
        self.code = code


def integer(value, minimum, maximum, label):
    if type(value) is not int or not minimum <= value <= maximum:
        raise OperationError(label + " is outside the native range")
    return value


def finite(value):
    if type(value) not in (int, float) or not math.isfinite(value):
        raise OperationError("Expected a finite number")
    return value


def quantity(value, unit):
    if not isinstance(value, str):
        return finite(value)
    result = App.Units.Quantity(value)
    if result.Unit != App.Units.Quantity("1 " + unit).Unit:
        raise OperationError("Expected a native " + unit + " quantity", "dimension_mismatch")
    if not unit:
        return finite(result.Value)
    converted = result.getValueAs(unit)
    return finite(converted.Value if hasattr(converted, "Value") else float(converted))


def vector(value):
    if not isinstance(value, list) or len(value) != 3:
        raise OperationError("Expected three sketch-local coordinates")
    return App.Vector(*(finite(component) for component in value))


def flag(edit, key, default):
    result = edit.get(key, default)
    if type(result) is not bool:
        raise OperationError(key + " must be boolean")
    return result


def curve_index(sketch, edit, spline=False):
    index = integer(edit.get("index"), 0, sketch.GeometryCount - 1, "Geometry index")
    if spline and not isinstance(sketch.Geometry[index], Part.BSplineCurve):
        raise OperationError("Select a native B-spline")
    return index


def spline_controls(sketch):
    """Read actual exposed native circle IDs; never create controls for a read."""
    result = []
    for constraint in sketch.Constraints:
        # Native persisted constraint content contains the internal pole index;
        # the pinned Python binding has no separate accessor for that field.
        if constraint.Type != "InternalAlignment":
            continue
        data = ET.fromstring(constraint.Content).attrib
        if data.get("InternalAlignmentType") == "9":
            result.append({"spline_index": constraint.Second, "pole_index": int(data["InternalAlignmentIndex"]),
                           "control_index": constraint.First})
    return result


def pole_control(sketch, index, pole):
    integer(pole, 0, sketch.Geometry[index].NbPoles - 1, "B-spline pole index")
    sketch.exposeInternalGeometry(index)
    if sketch.solve() != 0:
        raise OperationError("Native exposed spline controls failed to solve", "invalid_geometry")
    for control in spline_controls(sketch):
        if control["spline_index"] == index and control["pole_index"] == pole:
            return control["control_index"]
    raise OperationError("Native B-spline control circle was not exposed", "invalid_geometry")


def apply(sketch, edit):
    """Apply one native task; never open, commit, recompute or synthesize history."""
    if not sketch.isDerivedFrom("Sketcher::SketchObject"):
        raise OperationError("This operation requires a native SketchObject")
    try:
        return _apply(sketch, edit)
    except OperationError:
        raise
    except Exception as error:
        raise OperationError(str(error), "invalid_geometry") from error


def _apply(sketch, edit):
    op = edit.get("op")
    if op not in OPERATIONS:
        raise OperationError("Unsupported native sketch operation: " + str(op))
    if op == "carbon_copy":
        source = sketch.Document.getObject(edit.get("source", ""))
        if source is None or source == sketch or not source.isDerivedFrom("Sketcher::SketchObject"):
            raise OperationError("Carbon Copy requires another native sketch in this document")
        # Native carbonCopy enforces DAG, Body/Part scope and axis alignment,
        # and installs native expressions linking copied driving dimensions.
        sketch.carbonCopy(source.Name, flag(edit, "construction", True))
        return {"object": sketch.Name}
    if op == "fillet_geometry":
        radius = quantity(edit["radius"], "mm")
        if radius <= 0:
            raise OperationError("Fillet radius must be positive")
        options = (flag(edit, "trim", True), flag(edit, "preserve_corner", False), flag(edit, "chamfer", False))
        if "indices" in edit:
            indices, positions = edit["indices"], edit.get("positions")
            if not isinstance(indices, list) or len(indices) != 2 or not isinstance(positions, list) or len(positions) != 2:
                raise OperationError("Select two curves and two sketch-local pick positions")
            first, second = [integer(value, 0, sketch.GeometryCount - 1, "Geometry index") for value in indices]
            if first == second:
                raise OperationError("Select two distinct curves")
            sketch.fillet(first, second, vector(positions[0]), vector(positions[1]), radius, *options)
        else:
            index = curve_index(sketch, edit)
            point = integer(edit.get("point"), 1, 2, "Endpoint")
            sketch.fillet(index, point, radius, *options)
        return {"object": sketch.Name}
    index = curve_index(sketch, edit, spline=op.startswith("spline_"))
    if op == "split_geometry":
        sketch.split(index, vector(edit["position"]))
    elif op == "extend_geometry":
        geo = sketch.Geometry[index]
        if not isinstance(geo, (Part.LineSegment, Part.ArcOfCircle)):
            raise OperationError("Native extend supports line segments and circular arcs")
        point = integer(edit.get("point"), 1, 2, "Endpoint")
        increment = quantity(edit["increment"], "rad" if isinstance(geo, Part.ArcOfCircle) else "mm")
        if isinstance(geo, Part.ArcOfCircle):
            # Pinned native extend changes an arc handle in place. Register a
            # native property assignment first so native abort/Undo can restore
            # it. Geometry values retain their native persistent IDs/extensions.
            sketch.Geometry = list(sketch.Geometry)
        sketch.extend(index, increment, point)
    elif op == "spline_degree":
        change = integer(edit.get("change"), -24, 24, "Degree change")
        if change == 0:
            raise OperationError("Degree change must be nonzero")
        target = sketch.Geometry[index].Degree + change
        integer(target, 1, 25, "Native B-spline degree")
        if change > 0:
            sketch.increaseBSplineDegree(index, change)
            sketch.exposeInternalGeometry(index)
        elif not sketch.decreaseBSplineDegree(index, -change):
            raise OperationError("Native B-spline degree approximation failed", "invalid_geometry")
        # Decrease replaces geometry and deletes its constraints natively. The
        # next snapshot supplies the resulting native indices; do not guess one.
    elif op == "spline_knot":
        if "parameter" in edit:
            parameter = finite(edit["parameter"])
            multiplicity = integer(edit.get("multiplicity", 1), 1, sketch.Geometry[index].Degree, "Knot multiplicity")
            sketch.insertBSplineKnot(index, parameter, multiplicity)
        else:
            knot = integer(edit.get("knot"), 1, sketch.Geometry[index].NbKnots, "Native one-based knot index")
            change = integer(edit.get("change"), -25, 25, "Multiplicity change")
            if change == 0:
                raise OperationError("Multiplicity change must be nonzero")
            sketch.modifyBSplineKnotMultiplicity(index, knot, change)
        sketch.exposeInternalGeometry(index)
    elif op == "spline_pole":
        control = pole_control(sketch, index, edit.get("pole"))
        sketch.moveGeometry(control, 3, vector(edit["position"]), False)
        return {"object": sketch.Name, "index": control, "point": 3}
    elif op == "spline_weight":
        value = quantity(edit["value"], "")
        if value <= 0:
            raise OperationError("B-spline weight must be positive")
        control = pole_control(sketch, index, edit.get("pole"))
        releases = edit.get("release_constraints", [])
        if not isinstance(releases, list) or len(set(releases)) != len(releases):
            raise OperationError("Release constraints must be distinct native indices")
        for item in releases:
            integer(item, 0, sketch.ConstraintCount - 1, "Constraint index")
            existing = sketch.Constraints[item]
            if existing.Type != "Equal" or control not in (existing.First, existing.Second):
                raise OperationError("Only selected Equal constraints on this control circle can be released")
        for item in sorted(releases, reverse=True):
            sketch.delConstraint(item)
        constraints = [(i, c) for i, c in enumerate(sketch.Constraints) if c.Type == "Weight" and c.First == control]
        if constraints:
            sketch.setDatum(constraints[0][0], App.Units.Quantity(value))
        else:
            sketch.addConstraint(Sketcher.Constraint("Weight", control, value))
        return {"object": sketch.Name, "index": control}
    else:
        raise OperationError("Unsupported native sketch operation: " + str(op))
    return {"object": sketch.Name, "index": index}
