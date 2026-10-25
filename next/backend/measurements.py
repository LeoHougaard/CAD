"""Read-only native BRep measurements. No recompute, transaction or preferences."""
import math
import re

import FreeCAD as App
import Part
import sketch_geometry as SketchGeometry


class MeasurementError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def reject(code, message):
    raise MeasurementError(code, message)


def point(value):
    result = [float(value.x), float(value.y), float(value.z)]
    if not all(math.isfinite(x) for x in result):
        reject("measurement_failed", "Native geometry returned a non-finite point")
    return result


def schema(doc):
    # UnitSystem is a native enumeration using the same order as Units schemas.
    # Pass the document's schema directly; never alter process/global preferences.
    return doc.getEnumerationsOfProperty("UnitSystem").index(doc.UnitSystem)


def quantity(value, unit, units):
    value = float(value)
    if not math.isfinite(value):
        reject("measurement_failed", "Native geometry returned a non-finite quantity")
    q = App.Units.Quantity(value, App.Units.Unit(unit))
    display, _, _ = App.Units.schemaTranslate(q, units)
    return {"value": value, "unit": unit, "display": display}


def metric(key, label, value, unit, units):
    return {"key": key, "label": label, **quantity(value, unit, units)}


def measured_point(value, units):
    values = point(value)
    return {"value": values, "display": ", ".join(quantity(x, "mm", units)["display"] for x in values)}


def resolve(doc, ref, active_sketch):
    if not isinstance(ref, dict):
        reject("invalid_request", "Expected a native measurement reference")
    name = ref.get("object")
    if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
        reject("invalid_request", "Choose a native object name")
    obj = doc.getObject(name)
    if obj is None:
        reject("not_found", "Measurement object no longer exists: " + name)
    path = ref.get("subelement", "")
    if not isinstance(path, str) or len(path) > 4096 or "\x00" in path:
        reject("invalid_request", "Expected a native subobject path")
    pick = ref.get("sketch_geometry")
    if active_sketch and name != active_sketch:
        reject("stale_context", "Finish the active sketch to measure regenerated model geometry")
    if pick is not None:
        if path or not obj.isDerivedFrom("Sketcher::SketchObject") or not isinstance(pick, dict):
            reject("invalid_request", "Choose one native sketch geometry reference")
        index, pos = pick.get("index"), pick.get("point")
        if type(index) is not int or type(pos) is not int or not 0 <= pos <= 3:
            reject("invalid_request", "Invalid native sketch geometry reference")
        try:
            # Native Sketcher owns indices, construction/external geometry and
            # exposed point semantics. No geometry is read from render samples.
            if index >= 0:
                if index >= obj.GeometryCount:
                    raise ValueError("Sketch geometry no longer exists")
                geometry = obj.Geometry[index]
            elif index <= -1 and -index <= len(obj.ExternalGeo):
                geometry = obj.ExternalGeo[-index - 1]
            else:
                raise ValueError("Sketch geometry no longer exists")
            if index in (-1, -2) and not pos:
                raise ValueError("Infinite sketch axes have no finite length")
            if pos and (index, pos) != (-1, 1):
                handles = SketchGeometry.geometry_entry(geometry, index, False, sketch=obj)["handles"]
                if index in (-1, -2) or not any(handle["point"] == pos for handle in handles):
                    raise ValueError("Choose a native exposed sketch point")
            shape = Part.Vertex(obj.getPoint(index, pos)) if pos else geometry.toShape()
            shape = shape.transformed(obj.getGlobalPlacement().toMatrix())
        except Exception as error:
            reject("unsupported_reference", "Native sketch reference is unavailable: " + str(error))
    else:
        parent = obj.getParentGeoFeatureGroup()
        initial = parent.getGlobalPlacement().toMatrix() if parent else App.Matrix()
        try:
            shape = obj.getSubObject(path, 0, initial)
            if shape is None and not path:
                # App::Part has no own BRep subobject. Native getShape supplies
                # its native compound and resolves Links, without mesh conversion.
                shape = Part.getShape(obj, "", initial, True)
        except Exception as error:
            reject("not_found", "Native measurement reference is unavailable: " + str(error))
    if not isinstance(shape, Part.Shape) or shape.isNull():
        reject("not_found", "Native measurement reference has no shape: " + name + "." + path)
    if not shape.isValid():
        reject("invalid_model", "Selected native shape is invalid")
    return shape


def direction(shape):
    if shape.ShapeType == "Edge" and not shape.Degenerated and isinstance(shape.Curve, Part.Line):
        return "line", shape.Curve.Direction
    if shape.ShapeType == "Face" and isinstance(shape.Surface, Part.Plane):
        return "plane", shape.normalAt(0, 0)
    return None


def inspect_shape(shape, ref, units):
    kind = shape.ShapeType
    result = {"reference": ref, "shape_type": kind, "metrics": [],
              "center": measured_point(shape.Point if kind == "Vertex" else shape.CenterOfGravity, units),
              "properties": []}
    metrics = result["metrics"]
    if kind != "Vertex":
        metrics.append(metric("length", "Length" if kind in ("Edge", "Wire") else "Boundary length", shape.Length, "mm", units))
    if shape.Faces:
        metrics.append(metric("area", "Area", shape.Area, "mm^2", units))
    if shape.Solids:
        metrics.append(metric("volume", "Volume", shape.Volume, "mm^3", units))
    properties = result["properties"]
    properties.append({"label": "Topology", "value": f"{len(shape.Vertexes)} vertices, {len(shape.Edges)} edges, {len(shape.Faces)} faces, {len(shape.Solids)} solids"})
    geometry = None
    if kind == "Edge":
        properties.append({"label": "Closed", "value": str(shape.isClosed()).lower()})
        properties.append({"label": "Degenerate", "value": str(shape.Degenerated).lower()})
        if not shape.Degenerated:
            geometry = shape.Curve
    elif kind == "Face":
        geometry = shape.Surface
    if geometry is not None:
        result["geometry_type"] = geometry.TypeId
        if isinstance(geometry, (Part.Circle, Part.Cylinder, Part.Sphere)):
            metrics.append(metric("radius", "Radius", geometry.Radius, "mm", units))
            metrics.append(metric("diameter", "Diameter", 2 * geometry.Radius, "mm", units))
        if hasattr(geometry, "Center"):
            result["geometry_center"] = measured_point(geometry.Center, units)
        if hasattr(geometry, "Axis"):
            result["axis"] = point(geometry.Axis)
    native_direction = direction(shape)
    if native_direction:
        result["direction"] = point(native_direction[1])
    return result


def measure(doc, request, active_sketch=None):
    refs = request.get("selections")
    if not isinstance(refs, list) or not 1 <= len(refs) <= 2:
        reject("invalid_request", "Select one or two native vertices, edges, faces or objects")
    if len(refs) == 2 and refs[0] == refs[1]:
        reject("invalid_request", "Select two distinct native references")
    units = schema(doc)
    shapes = [resolve(doc, ref, active_sketch) for ref in refs]
    result = {"unit_system": doc.UnitSystem,
              "selections": [inspect_shape(s, r, units) for s, r in zip(shapes, refs)]}
    if len(shapes) == 2:
        distance, pairs, _ = shapes[0].distToShape(shapes[1])
        if not pairs:
            reject("measurement_failed", "Native distance did not provide witness points")
        result["distance"] = {"metric": metric("distance", "Closest distance", distance, "mm", units),
                              "witnesses": [[measured_point(a, units), measured_point(b, units)] for a, b in pairs]}
        directions = [direction(s) for s in shapes]
        if all(directions):
            (kind_a, a), (kind_b, b) = directions
            angle = a.getAngle(b)
            if kind_a != kind_b:
                angle = abs(math.pi / 2 - angle)
                label = "Line to plane angle"
            else:
                label = "Line direction angle" if kind_a == "line" else "Face normal angle"
            result["angle"] = metric("angle", label, math.degrees(angle), "deg", units)
        else:
            result["angle_unavailable"] = "Angle requires straight edges or planar faces; curved surfaces and vertices have no unique direction"
    return result
