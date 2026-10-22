"""Native sketch curves and projected references. No independent geometry model."""

import math
import io
import zipfile
import xml.etree.ElementTree as ET

import FreeCAD as App
import Part
import Sketcher


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


def _extension_metadata(geo):
    extension = geo.getExtensionOfType("Sketcher::ExternalGeometryExtension")
    return {"persistent_reference": extension.Ref,
            "defining": extension.testFlag("Defining"),
            "missing": extension.testFlag("Missing"), "frozen": extension.testFlag("Frozen")}


def restore_point_extensions(obj, property_name, geometries):
    """Recover native extensions discarded by 1.1.4 GeomPoint::getPyObject.

    PropertyGeometryList::getPyObject and Save traverse the same native list.
    Use that exact property order, never source-link order or coordinates.
    Restore only detached Python points; the document remains untouched.
    """
    if not any(isinstance(geo, Part.Point) for geo in geometries):
        return geometries
    with zipfile.ZipFile(io.BytesIO(obj.dumpPropertyContent(property_name))) as archive:
        native = ET.fromstring(archive.read("Persistence.xml")).find("GeometryList")
    records = list(native)
    if int(native.attrib["count"]) != len(geometries) or len(records) != len(geometries) or any(
            node.attrib.get("type") != geo.TypeId for node, geo in zip(records, geometries)):
        raise GeometryError("Native " + property_name + " property serialization does not match its geometry list")
    result = list(geometries)
    for slot, geo in enumerate(geometries):
        if not isinstance(geo, Part.Point):
            continue
        content = ET.Element("Content")
        content.extend(records[slot])
        payload = io.BytesIO()
        with zipfile.ZipFile(payload, "w") as archive:
            archive.writestr("Persistence.xml", ET.tostring(content, encoding="utf-8"))
        detached = Part.Point()
        detached.restoreContent(bytearray(payload.getvalue()))
        if not detached.hasExtensionOfType("Sketcher::SketchGeometryExtension"):
            raise GeometryError("Native point serialization has no Sketch geometry metadata")
        result[slot] = detached
    return result


def externals(obj):
    links = _links(obj)
    types = obj.ExternalTypes
    geometries = obj.ExternalGeo
    point_metadata, metadata_error = {}, None
    if any(isinstance(geo, Part.Point) and not geo.hasExtensionOfType("Sketcher::ExternalGeometryExtension")
           for geo in geometries[2:]):
        try:
            recovered = restore_point_extensions(obj, "ExternalGeo", geometries)
            point_metadata = {slot: _extension_metadata(geo) for slot, geo in enumerate(recovered)
                              if isinstance(geo, Part.Point)}
        except Exception as error:
            # Metadata/display limitations must not reject valid native geometry.
            # Keep unknown fields explicit and block source-dependent edits below.
            metadata_error = "Native projected-point metadata unavailable: " + str(error)
    result = []
    for index, geo in enumerate(geometries[2:]):
        metadata = (_extension_metadata(geo) if geo.hasExtensionOfType("Sketcher::ExternalGeometryExtension")
                    else point_metadata.get(index + 2))
        if metadata is None:
            metadata = {"persistent_reference": None, "defining": None, "missing": None, "frozen": None}
        matches = [(i, link) for i, (link, keys) in enumerate(links) if metadata["persistent_reference"] in keys]
        source_index, source = matches[0] if len(matches) == 1 else (None, None)
        geometry_index = -3 - index
        geometry = geometry_entry(geo, geometry_index, True, sketch=obj)
        geometry["editable"] = False
        mode = types[source_index] if source_index is not None and source_index < len(types) else None
        entry = {"index": index, "geometry_index": geometry_index, "source": source,
                 "source_index": source_index, **metadata,
                 "intersection": mode in (1, 2) if mode is not None else None,
                 "projection": mode in (0, 2) if mode is not None else None, "geometry": geometry}
        if metadata["persistent_reference"] is None:
            entry["metadata_error"] = metadata_error or "Native external geometry has no readable extension"
        result.append(entry)
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
        if externals(obj)[index]["persistent_reference"] is None:
            raise GeometryError("Native external metadata is unresolved; cannot safely delete its source group")
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


def refraction(obj, edit):
    args = edit.get("args", [])
    validate_constraint("SnellsLaw", args)
    first, first_pos, second, second_pos, boundary, _ = args
    value = edit.get("value", args[-1])
    if not isinstance(value, str):
        finite(value)
    quantity = App.Units.Quantity(value)
    finite(quantity.Value)
    if len({first, second, boundary}) != 3 or not all(p in (1, 2) for p in (first_pos, second_pos)):
        raise GeometryError("Refraction needs distinct rays, line endpoints and a different boundary")
    if all(index < 0 for index in (first, second, boundary)):
        raise GeometryError("Refraction cannot use external geometry only")
    def geometry(index):
        if 0 <= index < obj.GeometryCount:
            return obj.Geometry[index]
        if index <= -3 and -index <= len(obj.ExternalGeo):
            return obj.ExternalGeo[-index - 1]
        raise GeometryError("Refraction geometry is outside the current native list")
    for index in (first, second):
        if not isinstance(geometry(index), Part.LineSegment):
            raise GeometryError("Refraction rays must be straight line endpoints")
    curve = geometry(boundary)
    if not isinstance(curve, (Part.LineSegment, Part.Circle, Part.ArcOfCircle, Part.Ellipse,
                              Part.ArcOfEllipse, Part.ArcOfHyperbola, Part.ArcOfParabola, Part.BSplineCurve)):
        raise GeometryError("Select a native boundary curve supported by Sketcher")
    if boundary >= 0 and obj.GeometryFacadeList[boundary].InternalType == "BSplineControlPoint":
        raise GeometryError("A B-spline weight circle cannot be the refraction boundary")
    prepared = []
    def add(kind, *values):
        index = obj.addConstraint(Sketcher.Constraint(kind, *values))
        if index < 0 or obj.solve() != 0:
            raise GeometryError("Native Sketcher rejected " + kind + " for refraction on " + curve.TypeId
                                + "; conflicting " + str(list(obj.ConflictingConstraints))
                                + ", redundant " + str(list(obj.RedundantConstraints)))
        return index
    # Pinned GUI Utils::IsPointAlreadyOnCurve and CommandConstraints.cpp.
    # Each native addition solves before the next native geometric test.
    point = obj.getPoint(first, first_pos)
    if not obj.isPointOnCurve(second, point.x, point.y):
        prepared.append(add("Coincident", first, first_pos, second, second_pos))
    point = obj.getPoint(first, first_pos)
    if not obj.isPointOnCurve(boundary, point.x, point.y):
        prepared.append(add("PointOnObject", first, first_pos, boundary))
    index = add("SnellsLaw", first, first_pos, second, second_pos, boundary, quantity.Value)
    return {"object": obj.Name, "index": index, "preparation": prepared}


def via_point(obj, edit):
    kind, args = edit.get("kind"), edit.get("args", [])
    if kind not in {"TangentViaPoint", "PerpendicularViaPoint"}:
        raise GeometryError("Via-point creation supports Tangent and Perpendicular only")
    validate_constraint(kind, args)
    first, second, point_index, point_pos = args
    if first == second:
        raise GeometryError("Select two distinct whole curves and one exposed native point")
    def geometry(index):
        if 0 <= index < obj.GeometryCount:
            return obj.Geometry[index]
        if index <= -3 and -index <= len(obj.ExternalGeo):
            return obj.ExternalGeo[-index - 1]
        raise GeometryError("Via-point geometry is outside the current native list")
    curves = {index: geometry(index) for index in (first, second)}
    for curve in curves.values():
        if not isinstance(curve, (Part.LineSegment, Part.Circle, Part.ArcOfCircle, Part.Ellipse,
                                  Part.ArcOfEllipse, Part.ArcOfHyperbola, Part.ArcOfParabola, Part.BSplineCurve)):
            raise GeometryError("Select whole native curves for a via-point constraint")
        if Sketcher.GeometryFacade(curve).InternalType == "BSplineControlPoint":
            raise GeometryError("A B-spline weight circle cannot be a via-point curve")
    def fixed(index):
        return index < 0 or any(c.Type == "Block" and c.First == index and c.FirstPos == 0 for c in obj.Constraints)
    if fixed(first) and fixed(second):
        raise GeometryError("Cannot constrain two fixed curves; external and blocked curves are fixed")
    if (point_index, point_pos) != (-1, 1):
        point_geometry = geometry(point_index)
        handles = geometry_entry(point_geometry, point_index, False, sketch=obj)["handles"]
        if not any(handle["point"] == point_pos for handle in handles):
            raise GeometryError("Select a native point handle exposed by this sketch")
    prepared = []
    # Pinned GUI rechecks curve 1 after curve 2 because native solving may move
    # the selected point. B-spline via-point constraints imply point-on-object.
    for index in (first, second, first):
        if isinstance(curves[index], Part.BSplineCurve):
            continue
        point = obj.getPoint(point_index, point_pos)
        if not obj.isPointOnCurve(index, point.x, point.y):
            added = obj.addConstraint(Sketcher.Constraint("PointOnObject", point_index, point_pos, index))
            if added < 0:
                raise GeometryError("Native Sketcher rejected PointOnObject preparation for " + kind)
            # Native addConstraint solves already. Keep solving between tests,
            # but assess diagnostics after the native spline substitution.
            obj.solve()
            prepared.append(added)
    index = obj.addConstraint(Sketcher.Constraint(kind, *args))
    if index < 0:
        raise GeometryError("Native Sketcher rejected " + kind)
    # Match removeRedundantPointOnObject exactly: it matches the point geometry
    # ID, not FirstPos, and only the selected B-spline curves. Deleting through
    # the native API preserves expression paths and all unrelated constraints.
    removed = [i for i, c in enumerate(obj.Constraints) if c.Type == "PointOnObject"
               and c.First == point_index and c.Second in curves
               and isinstance(curves[c.Second], Part.BSplineCurve)]
    for remove in reversed(removed):
        obj.delConstraint(remove)
    if obj.solve() != 0:
        raise GeometryError("Native Sketcher rejected " + kind + "; conflicting "
                            + str(list(obj.ConflictingConstraints)) + ", redundant "
                            + str(list(obj.RedundantConstraints)))
    return {"object": obj.Name, "index": index - len(removed),
            "preparation": [i - sum(old < i for old in removed) for i in prepared],
            "removed_point_on_object": removed}
