"""Native sketch curves and projected references. No independent geometry model."""

import math

import FreeCAD as App
import Part


class GeometryError(ValueError):
    pass


# Numeric constructor forms supported by pinned FreeCAD 1.1.4 ConstraintPyImp.
# Its invalid-constructor path can terminate the native process on Windows.
# Guard the binding, while leaving geometric validity to the native solver.
CONSTRAINT_SIGNATURES = {
    "Horizontal": ("i", "iiii"), "Vertical": ("i", "iiii"), "Block": ("i",),
    "Parallel": ("ii",), "Equal": ("ii",), "Coincident": ("iiii",),
    "Tangent": ("ii", "iii", "iiii"), "Perpendicular": ("ii", "iii", "iiii"),
    "TangentViaPoint": ("iiii",), "PerpendicularViaPoint": ("iiii",), "PointOnObject": ("iii",),
    "Distance": ("in", "iin", "iiin", "iiiin"),
    "DistanceX": ("in", "iin", "iiiin"), "DistanceY": ("in", "iin", "iiiin"),
    "Angle": ("in", "iin", "iiiin"), "AngleViaPoint": ("iiiin",),
    "Radius": ("in",), "Diameter": ("in",), "Weight": ("in",),
    "Symmetric": ("iiiii", "iiiiii"), "SnellsLaw": ("iiiiin",),
}
for _alignment, _signature in {
    "EllipseMajorDiameter": "ii", "EllipseMinorDiameter": "ii", "HyperbolaMajor": "ii",
    "HyperbolaMinor": "ii", "ParabolaFocalAxis": "ii", "EllipseFocus1": "iii",
    "EllipseFocus2": "iii", "HyperbolaFocus": "iii", "ParabolaFocus": "iii",
    "BSplineControlPoint": "iiii", "BSplineKnotPoint": "iiii",
}.items():
    CONSTRAINT_SIGNATURES["InternalAlignment:" + _alignment] = (_signature,)


def validate_constraint(kind, args):
    if not isinstance(kind, str) or kind not in CONSTRAINT_SIGNATURES:
        raise GeometryError("Unsupported native constraint kind: " + str(kind))
    if not isinstance(args, list):
        raise GeometryError("Constraint args must be a list")
    for signature in CONSTRAINT_SIGNATURES[kind]:
        if len(signature) != len(args):
            continue
        if all((type(value) is int and -(2**31) <= value < 2**31) if token == "i" else (
                type(value) in (int, float) and math.isfinite(value)) for token, value in zip(signature, args)):
            return
    raise GeometryError("Invalid native constructor arguments for " + kind)


def finite(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise GeometryError("Expected a finite number")
    return value


def vec(value):
    if not isinstance(value, (list, tuple)) or len(value) != 3:
        raise GeometryError("Expected a three-number point")
    return App.Vector(*(finite(component) for component in value))


def xyz(value):
    return [value.x, value.y, value.z]


def make_geometry(edit):
    kind = edit["kind"]
    if kind in {"ellipse", "ellipse_arc", "hyperbola_arc"}:
        cls = Part.Hyperbola if kind == "hyperbola_arc" else Part.Ellipse
        curve = cls(vec(edit["major"]), vec(edit["minor"]), vec(edit["center"]))
        if kind == "ellipse":
            return curve
        cls = Part.ArcOfHyperbola if kind == "hyperbola_arc" else Part.ArcOfEllipse
        return cls(curve, finite(edit["start_parameter"]), finite(edit["end_parameter"]))
    if kind == "parabola_arc":
        curve = Part.Parabola()
        curve.Center = vec(edit["center"])
        curve.Focal = finite(edit["focal"])
        curve.AngleXU = finite(edit.get("angle", 0))
        return Part.ArcOfParabola(curve, finite(edit["start_parameter"]), finite(edit["end_parameter"]))
    if kind == "bspline":
        poles = [vec(point) for point in edit["poles"]]
        kwargs = {}
        for key in ("degree", "periodic", "weights", "knots"):
            if key in edit:
                kwargs[key] = edit[key]
        if "multiplicities" in edit:
            kwargs["mults"] = edit["multiplicities"]
        if "degree" in kwargs and type(kwargs["degree"]) is not int:
            raise GeometryError("B-spline degree must be an integer")
        for key in ("weights", "knots"):
            if key in kwargs:
                kwargs[key] = [finite(value) for value in kwargs[key]]
        curve = Part.BSplineCurve()
        # FreeCAD generates degree and knots when omitted, including periodic
        # and rational curves. Preserve its native parameterization.
        curve.buildFromPolesMultsKnots(poles, **kwargs)
        return curve
    raise GeometryError("Unsupported native curve kind: " + str(kind))


def geometry_entry(geo, index, construction, deflection=0.03, sketch=None):
    entry = {"index": index, "geometry_type": geo.TypeId, "kind": "unsupported",
             "construction": bool(construction), "points": [], "handles": [], "editable": True}

    def handle(point, position):
        if sketch is not None:
            position = sketch.getPoint(index, point)
        entry["handles"].append({"index": index, "point": point, "position": xyz(position),
                                 "draggable": index >= 0})

    if isinstance(geo, Part.Point):
        point = App.Vector(geo.X, geo.Y, geo.Z)
        entry.update(kind="point", position=xyz(point), points=[xyz(point)])
        handle(1, point)
        return entry
    if isinstance(geo, Part.LineSegment):
        entry.update(kind="line", start=xyz(geo.StartPoint), end=xyz(geo.EndPoint))
        handle(1, geo.StartPoint)
        handle(2, geo.EndPoint)
    elif isinstance(geo, (Part.Circle, Part.ArcOfCircle)):
        entry.update(kind="arc" if isinstance(geo, Part.ArcOfCircle) else "circle",
                     center=xyz(geo.Center), radius=geo.Radius, axis=xyz(geo.Axis))
        if isinstance(geo, Part.ArcOfCircle):
            entry.update(start=xyz(geo.StartPoint), end=xyz(geo.EndPoint),
                         mid=xyz(geo.value((geo.FirstParameter + geo.LastParameter) / 2)))
            handle(1, geo.StartPoint)
            handle(2, geo.EndPoint)
        handle(3, geo.Center)
    elif isinstance(geo, (Part.Ellipse, Part.ArcOfEllipse, Part.ArcOfHyperbola)):
        kind = "ellipse" if isinstance(geo, Part.Ellipse) else (
            "ellipse_arc" if isinstance(geo, Part.ArcOfEllipse) else "hyperbola_arc")
        entry.update(kind=kind, center=xyz(geo.Center), major_radius=geo.MajorRadius,
                     minor_radius=geo.MinorRadius, axis=xyz(geo.Axis), angle=geo.AngleXU)
        if kind != "ellipse":
            handle(1, geo.StartPoint)
            handle(2, geo.EndPoint)
        handle(3, geo.Center)
    elif isinstance(geo, Part.ArcOfParabola):
        entry.update(kind="parabola_arc", center=xyz(geo.Center), focal=geo.Focal,
                     axis=xyz(geo.Axis), angle=geo.AngleXU)
        handle(1, geo.StartPoint)
        handle(2, geo.EndPoint)
        handle(3, geo.Center)
    elif isinstance(geo, Part.BSplineCurve):
        entry.update(kind="bspline", poles=[xyz(point) for point in geo.getPoles()],
                     degree=geo.Degree, knots=geo.getKnots(), multiplicities=geo.getMultiplicities(),
                     weights=geo.getWeights(), periodic=geo.isPeriodic(), rational=geo.isRational())
        if not geo.isPeriodic():
            handle(1, geo.StartPoint)
            handle(2, geo.EndPoint)
    else:
        entry["editable"] = False
    if hasattr(geo, "FirstParameter"):
        entry.update(first_parameter=geo.FirstParameter, last_parameter=geo.LastParameter)
    try:
        if not all(math.isfinite(p) and abs(p) < 1e50 for p in (geo.FirstParameter, geo.LastParameter)):
            raise GeometryError("Unbounded sketch curve has no finite display interval")
        edge = geo.toShape()
        points = edge.discretize(Deflection=finite(deflection))
        if len(points) > 2048:
            points = edge.discretize(Number=2048)
        entry["points"] = [xyz(point) for point in points]
    except Exception as error:
        # Display limitations must not discard a valid native curve or prevent
        # saving its authoritative document.
        entry["display_error"] = str(error)
    return entry


def _links(obj):
    links = []
    for source, subelements in obj.ExternalGeometry:
        for subelement in subelements:
            link = {"link": source.Name, "subelements": [subelement]}
            keys = {source.Name + "." + subelement}
            if hasattr(source, "Shape"):
                mapped = source.Shape.getElementMappedName(subelement)
                if mapped:
                    keys.add(source.Name + "." + mapped)
                    keys.add(source.Name + ".;" + mapped.lstrip(";"))
            links.append((link, keys))
    return links


def externals(obj):
    links = _links(obj)
    types = obj.ExternalTypes
    result = []
    for index, geo in enumerate(obj.ExternalGeo[2:]):
        extension = geo.getExtensionOfType("Sketcher::ExternalGeometryExtension")
        matches = [(i, link) for i, (link, keys) in enumerate(links) if extension.Ref in keys]
        source_index, source = matches[0] if len(matches) == 1 else (None, None)
        geometry_index = -3 - index
        geometry = geometry_entry(geo, geometry_index, True, sketch=obj)
        geometry["editable"] = False
        mode = types[source_index] if source_index is not None and source_index < len(types) else None
        result.append({"index": index, "geometry_index": geometry_index, "source": source,
                       "source_index": source_index, "persistent_reference": extension.Ref,
                       "defining": extension.testFlag("Defining"),
                       "intersection": mode in (1, 2) if mode is not None else None,
                       "projection": mode in (0, 2) if mode is not None else None,
                       "missing": extension.testFlag("Missing"),
                       "frozen": extension.testFlag("Frozen"), "geometry": geometry})
    return result


def external_edit(obj, edit):
    op = edit["op"]
    if op == "external_geometry":
        for field in ("defining", "intersection"):
            if type(edit.get(field, False)) is not bool:
                raise GeometryError("External " + field + " state must be a boolean")
        source = obj.Document.getObject(edit["source"])
        if source is None or source == obj or obj in source.OutListRecursive:
            raise GeometryError("External reference requires another existing object without a dependency cycle")
        subelement = edit["subelement"]
        if not isinstance(subelement, str) or not subelement:
            raise GeometryError("External reference requires a native subelement name")
        obj.addExternal(source.Name, subelement, edit.get("defining", False), edit.get("intersection", False))
        return
    index = edit["index"]
    if type(index) is not int or not 0 <= index < len(obj.ExternalGeo) - 2:
        raise GeometryError("External geometry index is outside the current native list")
    if op == "delete_external":
        obj.delExternal(index)
    elif op == "external_state":
        if "defining" in edit:
            if type(edit["defining"]) is not bool:
                raise GeometryError("External defining state must be a boolean")
            obj.setConstruction(-3 - index, not edit["defining"])
        if "intersection" in edit:
            if type(edit["intersection"]) is not bool:
                raise GeometryError("External intersection state must be a boolean")
            reference = externals(obj)[index]["source_index"]
            types = list(obj.ExternalTypes)
            if reference is None or reference >= len(types):
                raise GeometryError("Native external source could not be resolved")
            # ExternalTypes addresses flattened native source links, whereas
            # index addresses cached geometry. One link can generate many curves.
            types[reference] = 1 if edit["intersection"] else 0
            obj.ExternalTypes = types
    else:
        raise GeometryError("Unsupported external geometry edit: " + str(op))
