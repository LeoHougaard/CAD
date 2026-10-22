"""Native PartDesign profile references. Caller owns transactions and revision.

The independent Sketch policy follows pinned TaskFeaturePick property copying,
with deliberate corrections: never prune the source, bind expressions to the
copy immediately, and remove copied external geometry and its persistent cache.
"""

import math
import re

import FreeCAD as App
import expression_links as ExpressionLinks


OPERATIONS = ("reference_copy",)
_LINK_TYPES = tuple(App.Base.TypeId.fromName(name) for name in (
    "App::PropertyLink", "App::PropertyLinkList", "App::PropertyLinkSub", "App::PropertyLinkSubList"))
_SKETCH_PROPERTIES = ("Geometry", "Constraints", "ExternalTypes", "ExternalGeo", "ArcFitTolerance",
                      "InternalShape", "MakeInternals", "_ExternalGeoVersion")


class ReferenceError(ValueError):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def identifier(value):
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", value):
        raise ReferenceError("invalid_request", "Reference copies require native object names")
    return value


def object_in(doc, name):
    obj = doc.getObject(identifier(name))
    if obj is None:
        raise ReferenceError("not_found", "Native reference object does not exist: " + name)
    return obj


def constraint_signature(constraint):
    return {key: getattr(constraint, key) for key in (
        "Type", "First", "FirstPos", "Second", "SecondPos", "Third", "ThirdPos", "Value", "Name",
        "Driving", "InVirtualSpace", "IsActive")}


def internal_constraints(source):
    # Native GeoEnum::RefExt=-3 and GeoEnum::GeoUndef=-2000. Coordinate axes and
    # origin references are retained, matching native delConstraintsToExternal.
    return [constraint_signature(c) for c in source.Constraints if c.First > -3
            and (c.Second > -3 or c.Second == -2000) and (c.Third > -3 or c.Third == -2000)]


def same_constraints(copy, expected):
    actual = [constraint_signature(c) for c in copy.Constraints]
    if len(actual) != len(expected):
        return False
    for left, right in zip(actual, expected):
        for key in left:
            if key == "Value":
                if not math.isclose(left[key], right[key], rel_tol=1e-9, abs_tol=1e-7):
                    return False
            elif left[key] != right[key]:
                return False
    return True


def same_geometry(source, copy):
    frozen = [g for g in source.ExternalGeo[2:] if g.getExtensionOfType("Sketcher::ExternalGeometryExtension").testFlag("Defining")]
    if source.GeometryCount + len(frozen) != copy.GeometryCount or source.Placement != copy.Placement:
        return False
    for index, (left, right) in enumerate(zip([*source.Geometry, *frozen], copy.Geometry)):
        if left.TypeId != right.TypeId or (source.getConstruction(index) if index < source.GeometryCount else False) != copy.getConstruction(index):
            return False
        a, b = left.toShape(), right.toShape()
        values_a = [a.Length, *[getattr(a.BoundBox, key) for key in ("XMin", "YMin", "ZMin", "XMax", "YMax", "ZMax")]]
        values_b = [b.Length, *[getattr(b.BoundBox, key) for key in ("XMin", "YMin", "ZMin", "XMax", "YMax", "ZMax")]]
        if any(not math.isclose(x, y, rel_tol=1e-9, abs_tol=1e-7) for x, y in zip(values_a, values_b)):
            return False
    # Defining external geometry contributes to the native profile shape. It
    # cannot silently disappear merely because reference links were removed.
    return (len(source.Shape.Edges) == len(copy.Shape.Edges)
            and len(source.Shape.Wires) == len(copy.Shape.Wires)
            and math.isclose(source.Shape.Length, copy.Shape.Length, rel_tol=1e-9, abs_tol=1e-7))


def independent(doc, source, body, name):
    if not source.isDerivedFrom("Sketcher::SketchObject"):
        raise ReferenceError("unsupported_reference", "Independent profile copying requires a native Sketch")
    frozen = [g for g in source.ExternalGeo[2:] if g.getExtensionOfType("Sketcher::ExternalGeometryExtension").testFlag("Defining")]
    expected = internal_constraints(source)
    entries = ExpressionLinks.entries(source)
    copy = doc.addObject(source.TypeId, name)
    if copy.Name != name:
        raise ReferenceError("name_conflict", "Native reference name changed during creation")
    body.addObject(copy)
    eligible = []
    for prop in source.PropertiesList:
        native_type = App.Base.TypeId.fromName(source.getTypeIdOfProperty(prop))
        if source.getGroupOfProperty(prop) == "Attachment" or any(native_type.isDerivedFrom(t) for t in _LINK_TYPES):
            continue
        if prop not in copy.PropertiesList or source.getTypeIdOfProperty(prop) != copy.getTypeIdOfProperty(prop):
            raise ReferenceError("unsupported_reference", "Native copy property is unavailable: " + prop)
        eligible.append(prop)
    # Geometry and constraints must exist before expressions. Native restore
    # finalization below, rather than a client graph, rebuilds persistent IDs.
    for prop in [*_SKETCH_PROPERTIES, *(p for p in eligible if p not in _SKETCH_PROPERTIES)]:
        if prop not in eligible or prop == "ExpressionEngine":
            continue
        if prop == "Label":
            copy.Label = copy.Name
        else:
            copy.restorePropertyContent(prop, source.dumpPropertyContent(prop))
    # Individual property restoration does not finalize Sketch persistent maps.
    # Without this native object roundtrip delExternal can erase all constraints.
    copy.restoreContent(copy.dumpContent())
    # Earlier materializations can leave external identity on internal curves.
    # Native clones retain their Sketch geometry IDs while removing that identity.
    if any(geometry.hasExtensionOfType("Sketcher::ExternalGeometryExtension") for geometry in copy.Geometry):
        internal = [geometry.clone() for geometry in copy.Geometry]
        for geometry in internal:
            geometry.deleteExtensionOfType("Sketcher::ExternalGeometryExtension")
        copy.Geometry = internal
    # Bind after native restore finalization. Restoring an already-bound engine
    # can leave its external dependency edges stale until the next file reopen.
    for entry in entries:
        copy.setExpression(entry["path"], entry["expression"], entry.get("comment", ""))
    copy.recompute()
    if copy.solve() != 0:
        raise ReferenceError("invalid_model", "Native copied sketch constraints could not be solved")
    # Preserve defining profile curves as independent native internal geometry.
    for geometry in frozen:
        internal = geometry.copy()
        internal.deleteExtensionOfType("Sketcher::ExternalGeometryExtension")
        index = copy.addGeometry(internal, False)
        # Native addGeometry(False) retains an existing construction flag.
        copy.setConstruction(index, False)
        if copy.Geometry[index].hasExtensionOfType("Sketcher::ExternalGeometryExtension"):
            raise ReferenceError("invalid_model", "Native materialized curve retained an external identity")
    copy.recompute()
    while len(copy.ExternalGeo) > 2:
        before = len(copy.ExternalGeo)
        copy.delExternal(-3)
        if len(copy.ExternalGeo) >= before:
            raise ReferenceError("invalid_model", "Native external geometry cleanup made no progress")
    copy.recompute()
    if copy.solve() != 0 or not same_constraints(copy, expected):
        raise ReferenceError("invalid_model", "Native independent copy could not preserve internal constraints")
    if copy.ExternalGeometry or len(copy.ExternalGeo) != 2:
        raise ReferenceError("invalid_model", "Native independent copy retained external geometry")
    if copy.AttachmentSupport or copy.MapMode != "Deactivated":
        raise ReferenceError("invalid_model", "Native independent copy remained attached")
    if not same_geometry(source, copy):
        raise ReferenceError("unsupported_reference", "Native independent copy changed the solved profile geometry")
    return copy


def apply(doc, edit):
    if not isinstance(edit, dict) or set(edit) - {"op", "source", "name", "body", "mode"}:
        raise ReferenceError("invalid_request", "Unexpected native reference-copy fields")
    if edit.get("op") != "reference_copy":
        raise ReferenceError("invalid_request", "Expected reference_copy")
    source = object_in(doc, edit.get("source"))
    body = object_in(doc, edit.get("body"))
    name = identifier(edit.get("name"))
    mode = edit.get("mode")
    if mode not in ("independent", "dependent"):
        raise ReferenceError("invalid_request", "Reference mode must be independent or dependent")
    if not body.isDerivedFrom("PartDesign::Body"):
        raise ReferenceError("invalid_request", "Reference target must be a native PartDesign Body")
    if not source.isDerivedFrom("Part::Feature") or source.isDerivedFrom("PartDesign::Body"):
        raise ReferenceError("unsupported_reference", "Select a native profile feature")
    if doc.getObject(name) is not None:
        raise ReferenceError("name_conflict", "Native reference name already exists: " + name)
    try:
        if mode == "independent":
            copy = independent(doc, source, body, name)
        else:
            copy = body.newObject("PartDesign::ShapeBinder", name)
            if copy.Name != name:
                raise ReferenceError("name_conflict", "Native reference name changed during creation")
            # Whole-object native LinkSubList entry must contain the empty subname.
            copy.Support = [(source, [""])]
        return {"object": copy.Name}
    except ReferenceError:
        raise
    except Exception as exception:
        raise ReferenceError("invalid_model", "Native profile copy failed: " + str(exception)) from exception
