"""Persistent native FreeCAD worker. Launch with FreeCADCmd, not system Python."""

import json
import math
import os
from pathlib import Path
import re
import sys
import traceback

# The pipe protocol is UTF-8 even when portable Windows Python uses cp1252.
sys.stdin.reconfigure(encoding="utf-8")

# FreeCAD and its extensions may write directly to fd 1. Keep only our dedicated
# descriptor for responses; redirect both Python and native diagnostics to stderr.
_protocol = os.fdopen(os.dup(sys.stdout.fileno()), "w", encoding="utf-8", buffering=1)
os.dup2(sys.stderr.fileno(), sys.stdout.fileno())

import FreeCAD as App
import Part
import Sketcher
import PartDesign  # Register the native feature classes in headless mode.
import sketch_geometry as SketchGeometry
import sketch_operations as SketchOperations
import feature_history as FeatureHistory
import profile_references as ProfileReferences
import expression_links as ExpressionLinks
import assembly as AssemblyAuthoring
import assembly_sources as AssemblySources
import native_saves as NativeSaves
import exchange as Exchange
import document_units as DocumentUnits
import measurements as Measurements
import hole_task as HoleTask
import suppression as Suppression
import sketch_journal as SketchJournal
import sketch_checkpoints as SketchCheckpoints
import session_callbacks as SessionCallbacks

SketchJournal.configure(_protocol)
_native_suppression = Suppression.register()


VERSION = 1
OPERATIONS = ["capabilities", "new", "open", "inspect", "transaction", "undo",
              "redo", "save", "mesh", "topology", "close", "preview",
              "preview_commit", "preview_cancel", "sketch_begin", "sketch_finish", "sketch_cancel", "feature_schema", "export_exchange"]
OPERATIONS += ["measure", "document_settings", "quantities"]
OPERATIONS += ["assembly_sources"]
EDIT_OPERATIONS = ["add_object", "set", "expression", "geometry", "constraint",
                   "datum", "move_geometry", "construction", "delete_geometry", "delete_constraint", "refraction", "via_point",
                   "constraint_state", "trim_geometry", "remove_object", "binder_support",
                   "expose_internal", "external_geometry", "auto_external_coincident", "external_state", "delete_external", "import_exchange", "detach_sketch"]
EDIT_OPERATIONS += sorted(AssemblyAuthoring.OPERATIONS)
EDIT_OPERATIONS += list(SketchOperations.OPERATIONS)
EDIT_OPERATIONS += list(FeatureHistory.OPERATIONS)
EDIT_OPERATIONS += list(ProfileReferences.OPERATIONS)
OBJECT_TYPES = {
    "App::Part", "Part::Feature", "PartDesign::Body", "Sketcher::SketchObject",
    "PartDesign::Pad", "PartDesign::Pocket", "PartDesign::Fillet",
    "PartDesign::Chamfer", "PartDesign::Revolution", "PartDesign::Groove",
    "PartDesign::Boolean",
    "PartDesign::AdditiveLoft", "PartDesign::SubtractiveLoft",
    "PartDesign::AdditivePipe", "PartDesign::SubtractivePipe",
    "PartDesign::Plane", "PartDesign::Line", "PartDesign::Point",
    "PartDesign::CoordinateSystem", "PartDesign::Hole", "PartDesign::Thickness", "PartDesign::Draft",
    "PartDesign::LinearPattern", "PartDesign::PolarPattern", "PartDesign::Mirrored",
    "PartDesign::MultiTransform", "PartDesign::Scaled", "PartDesign::ShapeBinder", "PartDesign::SubShapeBinder",
    "Part::Box", "Part::Cylinder", "Part::Sphere", "Part::Cone",
    "Part::Torus", "Part::Cut", "Part::Fuse", "Part::Common", "Part::MultiFuse",
    "Part::Fillet", "Part::Chamfer", "Part::Extrusion", "Part::Revolution",
}
OBJECT_TYPES.update("PartDesign::" + operation + shape
                    for operation in ("Additive", "Subtractive")
                    for shape in ("Box", "Cylinder", "Sphere", "Cone", "Ellipsoid", "Torus", "Prism", "Wedge", "Helix"))
_feature_schemas = {}
# Opaque native replay payloads only. Rust owns all public tokens and leases.
_native_previews = {}
_native_sketch_sessions = {}
# Failed new/Open owners contain no accepted model state. Rust still owns their
# revisions and poisoned leases; only inspection of recovery metadata and Close
# are allowed until the native document is actually gone.
_failed_loads = {}
# A nested source loader and the root rollback can encounter the same failed
# Close. One public action must never retry that native Close implicitly.
_load_close_errors = {}
# Native handles captured before an attempted Close whose outcome could not be
# queried. Keep them private; recovery snapshots contain no native model data.
_uncertain_closes = {}


class Failure(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def fail(code, message):
    raise Failure(code, message)


def name(value):
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", value):
        fail("invalid_request", "Names must be ASCII identifiers")
    return value


def number(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        fail("invalid_request", "Expected a finite number")
    return value


def vector(value):
    if not isinstance(value, list) or len(value) != 3:
        fail("invalid_request", "Expected a three-number vector")
    return App.Vector(*(DocumentUnits.value(v, "mm") for v in value))


def sketch_index(value, count):
    if type(value) is not int or not 0 <= value < count:
        fail("invalid_request", "Native sketch index is outside the current list")
    return value


def revision_request(action, key, **parameters):
    try:
        return SessionCallbacks.call(SessionCallbacks.REVISIONS, action, key, **parameters)
    except SessionCallbacks.CallbackError as error:
        fail(error.code, str(error))


def bump(doc):
    return revision_request("bump", doc.Name)

def native_references(obj, property_name=None):
    values = [getattr(obj, property_name)] if property_name is not None else list(obj.OutList)
    if property_name is None:
        for prop in obj.PropertiesList:
            if obj.getTypeIdOfProperty(prop).startswith(("App::PropertyLink", "App::PropertyXLink")):
                values.append(getattr(obj, prop))
    while values:
        value = values.pop()
        if isinstance(value, App.DocumentObject):
            yield value
        elif isinstance(value, (tuple, list)):
            values.extend(value)


def native_dependencies(doc):
    """Follow native reference edges, including hidden link properties and cycles."""
    documents = {}
    pending = [doc]
    while pending:
        owner = pending.pop()
        if owner.Name in documents:
            continue
        documents[owner.Name] = owner
        for obj in owner.Objects:
            pending.extend(value.Document for value in native_references(obj))
    return documents


def native_object_dependencies(objects):
    pending = list(objects)
    visited = set()
    while pending:
        obj = pending.pop()
        identity = (obj.Document.Name, obj.ID)
        if identity not in visited:
            visited.add(identity)
            yield obj
            pending.extend(native_references(obj))


def terminal_removal_safe(doc, request):
    if not reference_removals(doc, request.get("edits")):
        return False
    # Opening another app-wide transaction would commit/enroll unknown work.
    if App.getActiveTransaction() or any(d.HasPendingTransaction for d in App.listDocuments().values()):
        fail("recovery_busy", "Native transaction is still pending; close dependents without editing references")
    changed = {(doc.Name, object_in(doc, edit["object"]).ID) for edit in request["edits"]}
    # A document-level cycle can still have an isolated removable link. Prove
    # isolation through actual object edges, including hidden native properties.
    for other in App.listDocuments().values():
        if other.Name == doc.Name:
            continue
        for obj in native_object_dependencies(other.Objects):
            identity = (obj.Document.Name, obj.ID)
            if identity in changed:
                fail("document_referenced", "Reference removal would mutate native dependent " + other.Name)
    for edit in request["edits"]:
        obj = object_in(doc, edit["object"])
        if edit["op"] != "remove_object" or obj.TypeId != "App::Link" or obj.ElementCount:
            fail("document_referenced", "Terminal recovery can remove only an isolated native Link object")
        if not any(target.Document.Name != doc.Name and not lease("metadata", target.Document.Name)["available"]
                   for target in native_object_dependencies([obj])):
            fail("document_unavailable", "Terminal recovery can remove only references to unavailable owners")
    for other in native_dependencies(doc).values():
        if other.Name != doc.Name and any("Touched" in obj.State for obj in other.Objects):
            fail("recovery_busy", "Native dependency needs recompute; reference removal would change another owner")
    return True


def admit_global(request):
    try:
        lease("global", request=request)
        return False
    except Failure as error:
        if error.code not in ("sketch_active", "preview_active") or request.get("op") not in ("close", "transaction"):
            raise
        key = request.get("document")
        if revision_request("lookup", key) is None:
            raise error
        doc = App.getDocument(key)
        dependencies = native_dependencies(doc)
        # Keep healthy/unrelated global conflicts and their error ordering.
        # The typed callback below independently verifies authoritative poison.
        if not any(owner != key and not lease("metadata", owner)["available"]
                   for owner in dependencies):
            raise error
        lease("pre_revision", key, request=request)
        revision_request("check", key, expected_revision=request.get("expected_revision"))
        lease("local", key, request=request)
        recovery = "close_dependent"
        if request["op"] == "transaction":
            if not reference_removals(doc, request.get("edits")):
                raise error
            recovery = "remove_references"
        lease("terminal_recovery", key, request=request, recovery=recovery,
              dependencies=sorted(dependencies))
        if request["op"] == "transaction":
            terminal_removal_safe(doc, request)
        return True


def dependency_recovery(doc):
    if not _failed_loads:
        return None
    blocked = sorted(set(native_dependencies(doc)) & _failed_loads.keys())
    if blocked:
        return {"documents": blocked, "action": "remove_references_or_close_dependent",
                "recovery": [_failed_loads[key] for key in blocked]}
    return None


def check_dependencies(doc):
    recovery = dependency_recovery(doc)
    if recovery is not None:
        fail("document_unavailable", "Native references reach quarantined owners: " + json.dumps(recovery, sort_keys=True))


def reference_removals(doc, edits):
    if not isinstance(edits, list) or not edits:
        return False
    for edit in edits:
        if not isinstance(edit, dict):
            return False
        if edit.get("op") not in ("remove_object", "expression", "set"):
            return False
        obj = object_in(doc, edit.get("object"))
        if "native_id" in edit and (type(edit["native_id"]) is not int or edit["native_id"] != obj.ID):
            fail("stale_reference", "Native recovery object identity changed")
        if edit.get("op") == "remove_object" or edit.get("op") == "expression" and edit.get("expression") is None:
            continue
        if (edit.get("op") == "set" and edit.get("value") in (None, [])
                and property_type(obj, edit.get("property")).startswith(("App::PropertyLink", "App::PropertyXLink"))):
            continue
        return False
    return True


def accepted_dependents(doc):
    dependents = []
    for other in App.listDocuments().values():
        if other.Name == doc.Name or other.Name in _failed_loads:
            continue
        revision = revision_request("lookup", other.Name)
        if revision is not None and doc.Name in native_dependencies(other):
            dependents.append((other, revision))
    return dependents


def check_close_dependents(doc):
    dependents = [{"document": other.Name, "revision": revision} for other, revision in accepted_dependents(doc)]
    if dependents:
        fail("document_referenced", "Native Close would clear accepted references: " + json.dumps({
            "document": doc.Name, "dependents": dependents,
            "action": "remove_references_or_close_dependents_first"}, sort_keys=True))


def document(request, writable=False):
    key = request.get("document")
    hash(key)  # Preserve native request errors for unhashable document identifiers.
    if revision_request("lookup", key) is None:
        fail("not_found", "Document is not open in this worker")
    if key in _uncertain_closes:
        if writable:
            lease("pre_revision", key, request=request)
            revision_request("check", key, expected_revision=request.get("expected_revision"))
            lease("local", key, request=request)
        fail("document_unavailable", "Native Close outcome is unresolved: " + key)
    doc = App.getDocument(key)
    if writable:
        lease("pre_revision", key, request=request)
        revision_request("check", key, expected_revision=request.get("expected_revision"))
        lease("local", key, request=request)
        if _failed_loads and request.get("op") != "close" and not (request.get("op") == "transaction" and reference_removals(doc, request.get("edits"))):
            check_dependencies(doc)
        AssemblySources.recovery_guard(doc, request)
        if request.get("op") not in ("close", "preview_cancel", "sketch_cancel"):
            validate_assembly_proxies(doc)
        # FreeCAD 1.1 transactions are app-wide. A change to an inactive
        # document also enrolls the active document in its native history.
        # Select the owner only after the existing write guards have admitted
        # the request. Reads, export, and close retain their activation.
        if request.get("op") not in ("close", "export_exchange"):
            App.setActiveDocument(doc.Name)
    else:
        if doc.Name not in _failed_loads and request.get("op") != "inspect":
            check_dependencies(doc)
        AssemblySources.recovery_guard(doc, request)
    return doc


def object_in(doc, value):
    obj = doc.getObject(name(value))
    if obj is None:
        fail("not_found", "Object does not exist: " + value)
    return obj


def property_type(obj, prop):
    if not isinstance(prop, str) or prop not in obj.PropertiesList:
        fail("unsupported_property", "Unknown native property: " + str(prop))
    return obj.getTypeIdOfProperty(prop)


def decode_property(doc, type_id, value, current=None):
    if isinstance(value, dict) and "quantity_input" in value:
        if not hasattr(current, "Unit"):
            fail("dimension_mismatch", "This native property is not a quantity")
        try:
            return DocumentUnits.property_input(value, current.Unit)
        except (ValueError, TypeError, KeyError) as error:
            fail("dimension_mismatch", str(error))
    if type_id == "App::PropertyVector" and isinstance(value, list):
        if len(value) != 3:
            fail("invalid_request", "Expected a three-number vector")
        return App.Vector(*(number(v) for v in value))
    if type_id in ("App::PropertyLink", "App::PropertyLinkChild", "App::PropertyLinkGlobal", "App::PropertyXLink", "App::PropertyLinkHidden"):
        return None if value is None else object_in(doc, value["link"])
    if type_id in ("App::PropertyLinkSub", "App::PropertyLinkSubChild", "App::PropertyLinkSubGlobal", "App::PropertyXLinkSub"):
        if value is None:
            return None
        return (object_in(doc, value["link"]), value.get("subelements", []))
    if type_id in ("App::PropertyLinkList", "App::PropertyLinkListChild", "App::PropertyLinkListGlobal", "App::PropertyLinkListHidden"):
        return [object_in(doc, item["link"]) for item in (value or [])]
    if type_id in ("App::PropertyLinkSubList", "App::PropertyLinkSubListChild", "App::PropertyLinkSubListGlobal", "App::PropertyXLinkSubList"):
        values = [] if value is None else value if isinstance(value, list) else [value]
        # Native LinkSubList drops tuples with an empty subelement list. The
        # empty subname denotes the whole object and must be retained explicitly.
        return [(object_in(doc, item["link"]), item.get("subelements") or [""]) for item in values]
    if type_id in ("App::PropertyVector", "App::PropertyVectorDistance", "App::PropertyPosition"):
        return vector(value)
    if type_id == "App::PropertyVectorList":
        return [vector(item) for item in value]
    if type_id == "App::PropertyMap" and isinstance(value, dict) and all(
            isinstance(k, str) and isinstance(v, str) for k, v in value.items()):
        return value
    if type_id == "App::PropertyColor" and isinstance(value, list) and len(value) in (3, 4):
        return tuple(number(component) for component in value)
    if type_id == "Part::PropertyFilletEdges" and isinstance(value, list):
        if any(not isinstance(item, list) or len(item) != 3 or type(item[0]) is not int for item in value):
            fail("invalid_request", "Fillet edges require [edge index, radius1, radius2] triples")
        return [(item[0], DocumentUnits.value(item[1], "mm"), DocumentUnits.value(item[2], "mm")) for item in value]
    if type_id == "App::PropertyPlacement":
        rotation = value.get("rotation", [0, 0, 0, 1])
        if not isinstance(rotation, list) or len(rotation) != 4:
            fail("invalid_request", "Placement rotation requires quaternion [x,y,z,w]")
        if "axis_angle" in value:
            aa = value["axis_angle"]
            axis = App.Vector(*(number(v) for v in aa["axis"]))
            if axis.Length == 0:
                fail("invalid_request", "Choose a nonzero rotation axis")
            rotation = App.Rotation(axis, DocumentUnits.value(aa["angle"], "deg"))
        else:
            rotation = App.Rotation(*(number(v) for v in rotation))
        return App.Placement(vector(value["position"]), rotation)
    scalars = {"String", "UUID", "Bool", "Integer", "IntegerConstraint", "Float", "FloatConstraint",
               "Enumeration", "Length", "Distance", "Angle", "Area", "Volume", "Quantity",
               "QuantityConstraint", "Percent", "Precision", "Speed", "Acceleration"}
    lists = {"StringList", "IntegerList", "FloatList", "BoolList"}
    suffix = type_id.removeprefix("App::Property")
    if suffix in scalars and (value is None or isinstance(value, (str, int, float, bool))):
        return value
    if suffix in lists and isinstance(value, list) and all(isinstance(v, (str, int, float, bool)) for v in value):
        return value
    fail("unsupported_property", "Unsupported native property type: " + type_id)


def encode(value):
    if value is None or isinstance(value, (str, bool, int)):
        return value
    if isinstance(value, float):
        return value if math.isfinite(value) else None
    if isinstance(value, (tuple, list)):
        return [encode(v) for v in value]
    if isinstance(value, dict) and all(isinstance(k, str) for k in value):
        return {k: encode(v) for k, v in value.items()}
    if isinstance(value, App.Vector):
        return [value.x, value.y, value.z]
    if isinstance(value, App.Placement):
        return {"position": encode(value.Base), "rotation": list(value.Rotation.Q)}
    if isinstance(value, App.Rotation):
        return list(value.Q)
    if hasattr(value, "Name") and hasattr(value, "Document"):
        return {"link": value.Name}
    if hasattr(value, "Value") and hasattr(value, "Unit"):
        return value.Value
    raise TypeError("Native property is not exposed")


def encode_property(type_id, value):
    if type_id.startswith(("App::PropertyLinkSub", "App::PropertyXLinkSub")):
        def sublink(item):
            return None if item[0] is None else {"link": item[0].Name, "subelements": list(item[1])}
        if value is None:
            return None
        if "List" in type_id:
            return [sublink(item) for item in value]
        return sublink(value)
    return encode(value)


def property_metadata(obj, prop):
    status = list(obj.getPropertyStatus(prop))
    mode = list(obj.getEditorMode(prop))
    type_id = obj.getTypeIdOfProperty(prop)
    # Shape and the sketch lists are native computed data, despite lacking the
    # ReadOnly editor flag. Their supported changes have dedicated operations.
    computed = prop.startswith("_") or type_id in {"Part::PropertyPartShape", "Part::PropertyShapeHistory", "Sketcher::PropertyGeometryList", "Part::PropertyGeometryList",
                           "Sketcher::PropertyConstraintList", "App::PropertyExpressionEngine"}
    result = {"group": obj.getGroupOfProperty(prop), "documentation": obj.getDocumentationOfProperty(prop),
              "status": status, "read_only": computed or "ReadOnly" in mode or "ReadOnly" in status,
              "hidden": "Hidden" in mode or "Hidden" in status, "editor_mode": mode}
    if type_id == "App::PropertyEnumeration":
        result["enumerations"] = list(obj.getEnumerationsOfProperty(prop))
        if obj.TypeId == "PartDesign::Hole" and prop == "ThreadType":
            result["hole_task_version"] = 1
            result["hole_task_enumerations"] = HoleTask.enumerations()
            result["hole_task_cut_angles"] = HoleTask.cut_angles()
    if prop == "Support" and obj.TypeId in {"PartDesign::ShapeBinder", "PartDesign::SubShapeBinder"}:
        result["task_editable"] = True
    # Native TaskExtrudeParameters writes these even when a previous successful
    # recompute left its property editor read-only. Incomplete mode/target edits
    # must be assembled before recomputation, as in the native task dialog.
    if obj.TypeId in {"PartDesign::Pad", "PartDesign::Pocket"} and prop in {
            "Type2", "Length", "Length2", "TaperAngle", "TaperAngle2",
            "UpToFace", "UpToFace2", "UpToShape", "UpToShape2", "Offset", "Offset2",
            "Direction", "ReferenceAxis", "AlongSketchNormal"}:
        result["task_editable"] = True
    return result


def property_snapshot(obj):
    result = {"properties": {}, "property_types": {}, "property_metadata": {},
              "expressions": dict(obj.ExpressionEngine) if hasattr(obj, "ExpressionEngine") else {}}
    for prop in obj.PropertiesList:
        type_id = obj.getTypeIdOfProperty(prop)
        result["property_types"][prop] = type_id
        result["property_metadata"][prop] = property_metadata(obj, prop)
        try:
            quantity = DocumentUnits.metadata(type_id, getattr(obj, prop), DocumentUnits.index(obj.Document))
            if quantity is not None:
                result["property_metadata"][prop]["quantity"] = quantity
            if type_id == "Part::PropertyFilletEdges":
                result["property_metadata"][prop]["quantity_default"] = DocumentUnits.describe(DocumentUnits.native(1, "mm"), DocumentUnits.index(obj.Document))
        except (TypeError, AttributeError):
            pass
        try:
            result["properties"][prop] = encode_property(type_id, getattr(obj, prop))
        except (TypeError, AttributeError):
            pass
    return result


def writable_property(obj, prop, expression=False):
    type_id = property_type(obj, prop)
    HoleTask.prepare_input(obj, prop)
    if expression and obj.isDerivedFrom("Sketcher::SketchObject") and prop == "Constraints":
        return type_id
    metadata = property_metadata(obj, prop)
    task_write = obj.TypeId in {"PartDesign::Pad", "PartDesign::Pocket"} and metadata.get("task_editable", False)
    if metadata["read_only"] and not task_write:
        fail("read_only_property", "Native property is read-only: " + prop)
    return type_id


def initialize_feature_schemas():
    # Native object creation can interfere with application-wide transactions.
    # Build the JSON cache before accepting requests, then retain no native doc.
    doc = App.newDocument("FeatureSchema")
    try:
        for type_id in sorted(OBJECT_TYPES):
            obj = doc.addObject(type_id, "Feature")
            _feature_schemas[type_id] = {"type": type_id, **property_snapshot(obj)}
            if type_id == "PartDesign::Hole":
                HoleTask.initialize(obj)
            # Include native children such as Body origins. The next template
            # must start empty, with the same object names and native defaults.
            for item in reversed(doc.Objects):
                doc.removeObject(item.Name)
    finally:
        App.closeDocument(doc.Name)


def bounds(shape):
    box = shape.BoundBox
    if not box.isValid():
        return None
    return {"min": [box.XMin, box.YMin, box.ZMin], "max": [box.XMax, box.YMax, box.ZMax]}


def topology(shape, deflection=0.1):
    faces, edges = [], []
    native_faces = shape.Faces
    for index, face in enumerate(native_faces, 1):
        key = "Face" + str(index)
        surface = face.Surface
        faces.append({"subelement": key, "mapped_name": shape.getElementMappedName(key) or None,
                      "geometry_type": surface.TypeId, "center": encode(face.CenterOfMass),
                      "bounds": bounds(face), "area": face.Area, "orientation": face.Orientation,
                      "normal": encode(face.normalAt(0, 0)) if isinstance(surface, Part.Plane) else None})
    for index, edge in enumerate(shape.Edges, 1):
        key = "Edge" + str(index)
        adjacent = shape.ancestorsOfType(edge, Part.Face)
        # OCCT legitimately has degenerate edges at poles. They have no curve.
        if edge.Degenerated:
            endpoints = [encode(vertex.Point) for vertex in edge.Vertexes]
            if len(endpoints) == 1:
                endpoints *= 2
            geometry_type = None
            points = endpoints
        else:
            endpoints = [encode(edge.valueAt(edge.FirstParameter)), encode(edge.valueAt(edge.LastParameter))]
            geometry_type = edge.Curve.TypeId
            points = endpoints if isinstance(edge.Curve, Part.Line) else [
                encode(point) for point in edge.discretize(Deflection=deflection)]
        edges.append({"subelement": key, "mapped_name": shape.getElementMappedName(key) or None,
                      "geometry_type": geometry_type, "center": encode(edge.CenterOfMass),
                      "bounds": bounds(edge), "length": edge.Length, "endpoints": endpoints,
                      "points": points,
                      "closed": edge.isClosed(), "degenerated": edge.Degenerated,
                      "orientation": edge.Orientation,
                      "faces": ["Face" + str(i) for i, face in enumerate(native_faces, 1)
                                if any(face.isSame(ancestor) for ancestor in adjacent)]})
    vertices = [{"subelement": "Vertex" + str(index), "point": encode(vertex.Point),
                 "mapped_name": shape.getElementMappedName("Vertex" + str(index)) or None}
                for index, vertex in enumerate(shape.Vertexes, 1)]
    return {"coordinate_frame": "native_shape", "bounds": bounds(shape), "faces": faces, "edges": edges,
            "vertices": vertices}


def world_from_shape(obj):
    if obj.TypeId == "App::Link":
        return encode(AssemblyAuthoring.link_world_from_shape(obj))
    if hasattr(obj, "getGlobalPlacement") and hasattr(obj, "Placement"):
        return encode(obj.getGlobalPlacement().multiply(obj.Placement.inverse()))
    return encode(App.Placement())


def effective_visibility(obj):
    containers = [obj]
    seen = {obj.Name}
    parent = obj.getParentGeoFeatureGroup()
    while parent is not None and parent.Name not in seen:
        containers.append(parent)
        seen.add(parent.Name)
        parent = parent.getParentGeoFeatureGroup()
    # Origin features belong geometrically to the Body, while their native
    # immediate owner Origin also controls their display visibility.
    containers.extend(parent for parent in obj.InList if parent.TypeId == "App::Origin")
    return all(bool(getattr(parent, "Visibility", True)) for parent in containers)


def reference_display(obj):
    kinds = {"App::Origin": "origin", "App::Line": "line", "App::Plane": "plane", "App::Point": "point",
             "PartDesign::Plane": "plane", "PartDesign::Line": "line", "PartDesign::Point": "point",
             "PartDesign::CoordinateSystem": "coordinate_system"}
    kind = kinds.get(obj.TypeId)
    if kind is None:
        return None
    return {"kind": kind, "placement": encode(obj.getGlobalPlacement()),
            "visible": effective_visibility(obj),
            "size": [float(getattr(obj, "Length", 20)), float(getattr(obj, "Width", 20))],
            "subelement": {"plane": "Face1", "line": "Edge1", "point": "Vertex1"}.get(kind, ""),
            "line_axis": "x" if obj.TypeId == "App::Line" else "z"}


def oriented_normal(face, u, v):
    normal = face.normalAt(u, v)
    length = normal.Length
    if not math.isfinite(length) or length < 1e-12:
        fail("display_error", "Native face returned an undefined display normal")
    return [normal.x / length, normal.y / length, normal.z / length]


def sketch_geometry(obj):
    return [{**SketchGeometry.geometry_entry(geo, index, obj.getConstruction(index), sketch=obj),
             "persistent_id": obj.getGeometryId(index)}
            for index, geo in enumerate(obj.Geometry)]


def lease(action, document=None, **parameters):
    try:
        return SessionCallbacks.call(SessionCallbacks.LEASES, action, document, **parameters)
    except SessionCallbacks.CallbackError as error:
        fail(error.code, str(error))


def preview_ready(doc, request, opened=False):
    facts = dict(request=request, opened=opened)
    plan = lease("preview_ready", doc.Name, **facts)
    if plan["needs_redo"]:
        lease("preview_ready", doc.Name, **facts, redo=doc.RedoCount)
    return plan["label"]


def preview_metadata(doc):
    return lease("metadata", doc.Name)["preview"]


def sketch_metadata(doc):
    return lease("metadata", doc.Name)["sketch_edit"]


def history_counts(doc):
    session = sketch_metadata(doc)
    if session:
        return journal(doc, "counts")
    return {"undo_count": doc.UndoCount, "redo_count": doc.RedoCount}


def owner_body(obj):
    if obj.isDerivedFrom("PartDesign::Body"):
        return obj
    owner = getattr(obj, "_Body", None)
    if owner is not None and owner.isDerivedFrom("PartDesign::Body"):
        return owner
    parent = obj.getParentGeoFeatureGroup() if hasattr(obj, "getParentGeoFeatureGroup") else None
    return parent if parent is not None and parent.isDerivedFrom("PartDesign::Body") else None


def body_states(doc):
    bodies = [obj for obj in doc.Objects if obj.isDerivedFrom("PartDesign::Body")]
    consumed = {body.Name: set() for body in bodies}
    for body in bodies:
        if body.Tip is None:
            continue
        for feature in [body.Tip, *body.Tip.OutListRecursive]:
            if feature.isDerivedFrom("PartDesign::Boolean") and not feature.Suppressed:
                for tool in feature.Group:
                    if tool.Name in consumed:
                        consumed[tool.Name].add(feature.Name)
    result = {}
    for body in bodies:
        tip = body.Tip
        displayed = (tip is not None and not consumed[body.Name] and effective_visibility(tip)
                     and hasattr(tip, "Shape") and not tip.Shape.isNull() and len(tip.Shape.Solids) > 0)
        result[body.Name] = {"tip": tip.Name if tip else None,
                             "consumed_by": sorted(consumed[body.Name]),
                             "display_object": tip.Name if displayed else None}
    return result


def snapshot(doc, revision=None):
    if doc.Name in _failed_loads:
        return {"document": doc.Name, "revision": revision_request("current", doc.Name),
                "objects": [], "available": lease("metadata", doc.Name)["available"],
                "preview": None, "sketch_edit": None, "recovery": _failed_loads[doc.Name],
                "close_dependents": close_dependents_metadata(doc)}
    recovery = dependency_recovery(doc)
    if recovery is not None:
        # Only public inspection of an already registered owner can expose
        # recovery metadata. Candidate snapshots must fail before acceptance.
        if revision is not None:
            check_dependencies(doc)
        return {"document": doc.Name, "revision": revision_request("current", doc.Name),
                "objects": [], "available": False, "preview": None, "sketch_edit": None,
                "recovery": recovery, "close_dependents": close_dependents_metadata(doc)}
    validate_assembly_proxies(doc)
    recovery_only = AssemblySources.recovery_metadata(doc) is not None
    native_available = lease("metadata", doc.Name)["available"]
    objects = []
    bodies = body_states(doc)
    for obj in doc.Objects:
        entry = {"name": obj.Name, "type": obj.TypeId, "label": obj.Label,
                 "effective_visibility": effective_visibility(obj),
                 "valid": obj.isValid(), "status": obj.getStatusString(),
                 "state": list(obj.State), **property_snapshot(obj), **AssemblyAuthoring.metadata(obj),
                 **Suppression.metadata(obj)}
        owner = owner_body(obj)
        entry["body"] = owner.Name if owner else None
        entry["history_flags"] = FeatureHistory.flags(obj, owner)
        if obj.Name in bodies:
            entry["body_state"] = bodies[obj.Name]
        if hasattr(obj, "getGlobalPlacement"):
            entry["global_placement"] = encode(obj.getGlobalPlacement())
        elif obj.TypeId == "App::Link":
            entry["global_placement"] = encode(AssemblyAuthoring.link_global_placement(obj))
        reference = reference_display(obj)
        if reference is not None:
            entry["reference_display"] = reference
        if hasattr(obj, "Shape"):
            shape = obj.Shape
            empty = shape.isNull()
            entry["shape"] = {"null": empty, "valid": shape.isValid() if not empty else False,
                              "volume": 0 if empty else shape.Volume, "area": 0 if empty else shape.Area,
                              "solids": len(shape.Solids), "faces": len(shape.Faces),
                              "edges": len(shape.Edges), "vertices": len(shape.Vertexes),
                              "bounds": None if empty else bounds(shape)}
        if obj.isDerivedFrom("Sketcher::SketchObject"):
            entry["sketch"] = {"geometry_count": obj.GeometryCount, "constraint_count": obj.ConstraintCount,
                               "task_capabilities": SketchOperations.capabilities(),
                               "degrees_of_freedom": obj.DoF,
                               "conflicting_constraints": list(obj.ConflictingConstraints),
                               "redundant_constraints": list(obj.RedundantConstraints),
                               "geometry": sketch_geometry(obj),
                               "axes": SketchGeometry.axes(obj),
                               "spline_controls": SketchOperations.spline_controls(obj),
                               "externals": SketchGeometry.externals(obj),
                               "constraints": [{"index": i, "type": c.Type, "first": c.First,
                                                "first_pos": c.FirstPos, "second": c.Second,
                                                "second_pos": c.SecondPos, "third": c.Third,
                                                "third_pos": c.ThirdPos, "value": c.Value,
                                                **DocumentUnits.constraint(obj, c),
                                                "name": c.Name, "driving": c.Driving,
                                                "virtual": c.InVirtualSpace, "active": c.IsActive,
                                                **SketchGeometry.angle_witness(obj, c)}
                                               for i, c in enumerate(obj.Constraints)]}
        objects.append(entry)
    state = {"document": doc.Name, "path": doc.FileName, "save_recovery": NativeSaves.metadata(doc), "recovery_only": recovery_only, "revision": revision_request("current", doc.Name) if revision is None else revision, "objects": objects,
            "units": DocumentUnits.settings(doc),
            **history_counts(doc), "sketch_edit": sketch_metadata(doc),
            "available": native_available and not recovery_only,
            "recovery_available": native_available and recovery_only, "preview": preview_metadata(doc)}
    if not state["available"]:
        state["close_dependents"] = close_dependents_metadata(doc)
    return state



def close_dependents_metadata(doc):
    result = []
    for other, revision in accepted_dependents(doc):
        entry = {"document": other.Name, "revision": revision, "path": other.FileName}
        if not lease("metadata", doc.Name)["available"] and lease("metadata", other.Name)["available"]:
            edits = [{"op": "remove_object", "object": obj.Name, "native_id": obj.ID}
                     for obj in other.Objects if obj.TypeId == "App::Link" and not obj.ElementCount
                     and any(target.Document.Name == doc.Name for target in native_object_dependencies([obj]))]
            if edits:
                try:
                    if terminal_removal_safe(other, {"edits": edits}):
                        entry["reference_removals"] = edits
                except Failure:
                    pass  # Offer no reference action without a native safety proof.
        result.append(entry)
    return result


def prepare_state(doc):
    state = snapshot(doc, revision_request("peek", doc.Name))
    # Serialization is part of accepting a mutation, before it becomes durable
    # in the native undo history or visible under a new revision.
    json.dumps(state, allow_nan=False)
    return state


def publish_state(doc, state):
    check_dependencies(doc)
    current_revision = bump(doc)
    state.update(revision=current_revision, **history_counts(doc))
    return state


def validate_assembly_proxies(doc):
    try:
        AssemblyAuthoring.validate_proxies(doc)
    except AssemblyAuthoring.AssemblyError as error:
        fail(error.code, str(error))


def check_suppression():
    try:
        _native_suppression.check()
    except RuntimeError as error:
        fail("suppression_state_unavailable", str(error))


def abort_transaction(doc):
    with _native_suppression.history():
        doc.abortTransaction()


def native_history(doc, operation):
    with _native_suppression.history():
        getattr(doc, operation)()


def recompute_document(doc):
    check_dependencies(doc)
    validate_assembly_proxies(doc)
    result = doc.recompute()
    check_suppression()
    if result is not False and not _native_sketch_sessions.get(doc.Name):
        try:
            result = AssemblyAuthoring.solve_document(doc)
        except AssemblyAuthoring.AssemblyError as error:
            fail(error.code, str(error))
    return result


def validate(doc, whole=False):
    check_dependencies(doc)
    validate_assembly_proxies(doc)
    if whole:
        try:
            AssemblyAuthoring.solve_document(doc)
        except AssemblyAuthoring.AssemblyError as error:
            fail(error.code, str(error))
    session = _native_sketch_sessions.get(doc.Name)
    objects = [object_in(doc, session["object"])] if session and not whole else doc.Objects
    for obj in objects:
        if not obj.isValid():
            fail("invalid_model", obj.Name + ": " + obj.getStatusString())
        if obj.isDerivedFrom("Sketcher::SketchObject"):
            for diagnostic in ("ConflictingConstraints", "RedundantConstraints", "MalformedConstraints"):
                if getattr(obj, diagnostic):
                    fail("invalid_model", obj.Name + ": " + diagnostic)
        if hasattr(obj, "Shape") and not obj.Shape.isNull() and not obj.Shape.isValid():
            fail("invalid_model", obj.Name + ": native shape is invalid")
        if obj.isDerivedFrom("PartDesign::Boolean") and not obj.Suppressed and len(obj.Shape.Solids) == 0:
            fail("invalid_model", obj.Name + ": native Boolean result contains no solid")
        transformation_step = any(parent.isDerivedFrom("PartDesign::MultiTransform") and
                                  obj in parent.Transformations for parent in obj.InList)
        suppressed = obj.isDerivedFrom("PartDesign::Feature") and obj.Suppressed
        if not suppressed and not transformation_step and obj.TypeId in OBJECT_TYPES and obj.TypeId not in {
            "App::Part", "PartDesign::Body", "Sketcher::SketchObject",
            "PartDesign::Plane", "PartDesign::Line", "PartDesign::Point", "PartDesign::CoordinateSystem",
            "PartDesign::ShapeBinder", "PartDesign::SubShapeBinder",
        } and hasattr(obj, "Shape") and obj.Shape.isNull():
            fail("invalid_model", obj.Name + ": native result is null")


def set_boolean_tools(feature, tools):
    target = owner_body(feature)
    if target is None or not tools or len({tool.Name for tool in tools}) != len(tools):
        fail("invalid_request", "Boolean requires distinct tool Bodies and a target Body")
    for tool in tools:
        if not tool.isDerivedFrom("PartDesign::Body") or tool == target:
            fail("invalid_request", "Boolean tools must be other native Bodies")
        if target in tool.OutListRecursive:
            fail("invalid_request", "Boolean tool would create a native dependency cycle")
        parent = tool.getParentGeoFeatureGroup()
        if parent is not None and parent.isDerivedFrom("PartDesign::Boolean") and parent != feature:
            fail("invalid_request", "Tool Body is already consumed by another Boolean")
        if tool.Tip is None or len(tool.Tip.Shape.Solids) == 0:
            fail("invalid_request", "Boolean tool Body requires a solid native tip")
    # Preserve the pinned native Group semantics, including its placement
    # behavior. World-preserving reparenting would be a separate operation.
    feature.Group = tools


def auto_external_coincident(doc, obj, edit):
    """Attach a new line endpoint to a verified current native sketch vertex.

    This runs inside the existing owned sketch transaction and journal action.
    It never derives an external solver index from source order or position.
    """
    session = _native_sketch_sessions.get(doc.Name)
    if session is None or session["object"] != obj.Name:
        fail("sketch_owner", "Automatic projection requires the active sketch session")
    source = object_in(doc, edit.get("source"))
    if (source == obj or source.Document != doc or source.TypeId != "Sketcher::SketchObject"
            or not effective_visibility(source) or obj in source.OutListRecursive):
        fail("invalid_geometry", "Source must be a visible independent sketch in this document")
    subelement, mapped = edit.get("subelement"), edit.get("mapped_name")
    if (not isinstance(subelement, str) or re.fullmatch(r"Vertex[1-9][0-9]*", subelement) is None
            or not isinstance(mapped, str) or not mapped
            or source.Shape.isNull() or source.Shape.getElementMappedName(subelement) != mapped):
        fail("stale_reference", "Native source vertex identity changed or is unavailable")
    index, point = edit.get("index"), edit.get("point")
    if (type(index) is not int or not 0 <= index < obj.GeometryCount
            or type(point) is not int or point not in (1, 2)
            or not isinstance(obj.Geometry[index], Part.LineSegment)):
        fail("invalid_geometry", "Automatic projection requires a native line endpoint")
    source_placement, target_placement = source.getGlobalPlacement(), obj.getGlobalPlacement()
    source_normal = source_placement.Rotation.multVec(App.Vector(0, 0, 1))
    target_normal = target_placement.Rotation.multVec(App.Vector(0, 0, 1))
    if (source_normal.cross(target_normal).Length > 1e-9 or
            abs((source_placement.Base - target_placement.Base).dot(target_normal)) > 1e-7):
        fail("invalid_geometry", "Automatic projection requires coplanar sketches")
    before = SketchGeometry.externals(obj)
    if any(not e.get("persistent_reference") or e.get("source") is None
           or e.get("missing") is not False or e.get("frozen") is not False
           or type(e.get("defining")) is not bool or type(e.get("projection")) is not bool
           or type(e.get("intersection")) is not bool for e in before):
        fail("invalid_geometry", "Existing native external metadata is unresolved")
    source_link = {"link": source.Name, "subelements": [subelement]}
    candidates = [e for e in before if e["source"] == source_link]
    if len(candidates) > 1:
        fail("invalid_geometry", "Native source vertex has ambiguous external ownership")
    if not candidates:
        try:
            SketchGeometry.external_edit(obj, {"op": "external_geometry", "source": source.Name,
                                               "subelement": subelement})
        except (SketchGeometry.GeometryError, ValueError, TypeError, KeyError) as error:
            fail("invalid_geometry", str(error))
        if obj.solve() != 0:
            fail("invalid_model", "Native sketch solver rejected the projected vertex")
        candidates = [e for e in SketchGeometry.externals(obj) if e["source"] == source_link]
    if len(candidates) != 1:
        fail("invalid_geometry", "Native projected vertex ownership could not be resolved")
    entry = candidates[0]
    allowed_refs = {source.Name + "." + mapped, source.Name + ".;" + mapped.lstrip(";")}
    handles = entry["geometry"].get("handles", [])
    if (entry["persistent_reference"] not in allowed_refs or entry["missing"] is not False
            or entry["frozen"] is not False or entry["projection"] is not True
            or entry["intersection"] is not False or entry["defining"] is not False
            or entry["geometry"]["kind"] != "point" or len(handles) != 1
            or handles[0].get("index") != entry["geometry_index"]
            or handles[0].get("point") != 1 or handles[0].get("draggable") is not False):
        fail("invalid_geometry", "Native projected vertex metadata is incomplete or ambiguous")
    signed = entry["geometry_index"]
    if type(signed) is not int or signed > -3:
        fail("invalid_geometry", "Native projected vertex has an invalid solver identity")
    try:
        SketchGeometry.validate_constraint("Coincident", [index, point, signed, 1])
    except SketchGeometry.GeometryError as error:
        fail("invalid_geometry", str(error))
    constraint = obj.addConstraint(Sketcher.Constraint("Coincident", index, point, signed, 1))
    if constraint < 0:
        fail("invalid_model", "Native Coincident constraint failed")
    return {"object": obj.Name, "index": constraint, "external": signed,
            "persistent_reference": entry["persistent_reference"]}


def apply_edit(doc, edit):
    result = apply_native_edit(doc, edit)
    check_suppression()
    return result


def apply_native_edit(doc, edit):
    op = edit.get("op", edit.get("kind"))
    if op in AssemblyAuthoring.OPERATIONS:
        try:
            return AssemblyAuthoring.apply(doc, edit)
        except AssemblyAuthoring.AssemblyError as error:
            fail(error.code, str(error))
    if op in ProfileReferences.OPERATIONS:
        try:
            return ProfileReferences.apply(doc, edit)
        except ProfileReferences.ReferenceError as error:
            fail(error.code, str(error))
    if op in FeatureHistory.OPERATIONS:
        lease("history_guard", doc.Name)
        try:
            return FeatureHistory.apply(doc, edit)
        except FeatureHistory.HistoryError as error:
            fail(error.code, str(error))
    if op in SketchOperations.OPERATIONS:
        try:
            return SketchOperations.apply(object_in(doc, edit["object"]), edit)
        except SketchOperations.OperationError as error:
            fail(error.code, str(error))
    if op == "import_exchange":
        try:
            return Exchange.import_into(doc, edit)
        except Exchange.ExchangeError as error:
            fail(error.code, str(error))
    if op == "add_object":
        type_id, key = edit["type"], name(edit["name"])
        if "origin_plane" in edit and (type_id != "Sketcher::SketchObject" or not edit.get("body")
                or edit["origin_plane"] not in ("XY_Plane", "XZ_Plane", "YZ_Plane")):
            fail("invalid_request", "Origin-plane attachment requires a Body-owned sketch and native plane role")
        if type_id not in OBJECT_TYPES:
            fail("unsupported_object_type", "Unsupported native object type: " + str(type_id))
        if doc.getObject(key):
            fail("name_conflict", "Object name already exists: " + key)
        if edit.get("body"):
            body = object_in(doc, edit["body"])
            if not body.isDerivedFrom("PartDesign::Body"):
                fail("invalid_request", "Owner must be a PartDesign Body")
            if type_id == "PartDesign::Boolean" and (
                    body.Tip is None or not hasattr(body.Tip, "Shape") or len(body.Tip.Shape.Solids) == 0):
                fail("invalid_request", "Boolean target Body requires a solid native tip")
            obj = body.newObject(type_id, key)
        else:
            if type_id == "PartDesign::Boolean":
                fail("invalid_request", "A PartDesign Boolean requires a target Body owner")
            obj = doc.addObject(type_id, key)
        if obj.Name != key:
            fail("invalid_request", "FreeCAD changed the requested object name")
        if "origin_plane" in edit:
            planes = [feature for feature in body.Origin.OriginFeatures if feature is not None
                      and feature.isDerivedFrom("App::Plane") and feature.Role == edit["origin_plane"]]
            if len(planes) != 1:
                fail("invalid_request", "Body origin must contain exactly one plane with the requested native role")
            obj.AttachmentSupport = [(planes[0], [""])]
            obj.MapMode = "FlatFace"
        return {"object": obj.Name}
    obj = object_in(doc, edit["object"])
    if op == "auto_external_coincident":
        if not obj.isDerivedFrom("Sketcher::SketchObject"):
            fail("invalid_request", "Automatic projection requires a native SketchObject")
        return auto_external_coincident(doc, obj, edit)
    if op == "binder_support":
        if obj.TypeId not in {"PartDesign::ShapeBinder", "PartDesign::SubShapeBinder"}:
            fail("invalid_request", "Binder support requires a native ShapeBinder or SubShapeBinder")
        # FreeCAD reserves Support from generic property editors but its native
        # binder creation/editing command assigns this property explicitly.
        obj.Support = decode_property(doc, property_type(obj, "Support"), edit["value"])
    elif op == "remove_object":
        if "native_id" in edit and (type(edit["native_id"]) is not int or edit["native_id"] != obj.ID):
            fail("stale_reference", "Native recovery object identity changed")
        owner = owner_body(obj)
        dependents = [item.Name for item in obj.InList if item != owner and not (
            item.isDerivedFrom("App::Part") and obj in item.Group)]
        if dependents:
            fail("dependent_objects", "Native dependents prevent deletion: " + ", ".join(dependents))
        if obj.isDerivedFrom("PartDesign::Body") and obj.Group:
            fail("dependent_objects", "Delete the Body's native features before deleting the Body")
        restore_tip = owner is not None and owner != obj and owner.Tip == obj
        previous = getattr(obj, "BaseFeature", None) if restore_tip else None
        if previous is not None and owner_body(previous) != owner:
            fail("invalid_request", "Native base feature is outside the deleted tip's Body")
        doc.removeObject(obj.Name)
        if restore_tip:
            owner.Tip = previous
            if previous is not None:
                previous.Visibility = True
    elif op == "set":
        prop = edit["property"]
        if HoleTask.unchanged_forced_input(obj, prop, edit["value"]):
            return {"object": obj.Name}
        type_id = writable_property(obj, prop)
        value = decode_property(doc, type_id, edit["value"], getattr(obj, prop))
        if obj.isDerivedFrom("PartDesign::Boolean") and prop == "Group":
            set_boolean_tools(obj, value)
        elif obj.isDerivedFrom("PartDesign::Boolean") and prop == "BaseFeature":
            if value is None or value == obj or owner_body(value) != owner_body(obj) or obj in value.OutListRecursive:
                fail("invalid_request", "Boolean base must be an earlier feature in its target Body")
            setattr(obj, prop, value)
        else:
            setattr(obj, prop, value)
    elif op == "detach_sketch":
        if not obj.isDerivedFrom("Sketcher::SketchObject"):
            fail("invalid_request", "Detaching a sketch requires a native SketchObject")
        doc.recompute()
        obj.positionBySupport()
        placement = obj.Placement.copy()
        obj.MapMode = "Deactivated"
        obj.AttachmentSupport = []
        obj.Placement = placement
    elif op == "expression":
        prop = edit["property"]
        # Native canonical component paths can start with a self-relative dot.
        # Check the underlying property's permissions, preserving the path for
        # FreeCAD's own parser and persisted expression identifiers.
        writable_property(obj, prop.removeprefix(".").split(".")[0].split("[")[0], expression=True)
        expression = edit["expression"]
        if expression is not None and not isinstance(expression, str):
            fail("invalid_request", "Expression must be a string or null")
        obj.setExpression(prop, expression)
        HoleTask.resolve_controller(obj, prop)
    elif op in {"external_geometry", "external_state", "delete_external"}:
        if not obj.isDerivedFrom("Sketcher::SketchObject"):
            fail("invalid_request", "External geometry requires a native SketchObject")
        try:
            SketchGeometry.external_edit(obj, edit)
        except (SketchGeometry.GeometryError, ValueError, TypeError, KeyError) as error:
            fail("invalid_geometry", str(error))
    elif op == "via_point":
        if not obj.isDerivedFrom("Sketcher::SketchObject"):
            fail("invalid_request", "Via-point constraints require a native SketchObject")
        try:
            return SketchGeometry.via_point(obj, edit)
        except (SketchGeometry.GeometryError, ValueError, TypeError, KeyError) as error:
            fail("invalid_via_point", str(error))
    elif op == "refraction":
        if not obj.isDerivedFrom("Sketcher::SketchObject"):
            fail("invalid_request", "Refraction requires a native SketchObject")
        try:
            return SketchGeometry.refraction(obj, edit)
        except (SketchGeometry.GeometryError, ValueError, TypeError, KeyError) as error:
            fail("invalid_refraction", str(error))
    elif op in ("geometry", "constraint"):
        if not obj.isDerivedFrom("Sketcher::SketchObject"):
            fail("invalid_request", "Geometry and constraints require a native SketchObject")
        if op == "geometry":
            kind = edit["kind"]
            if kind == "line":
                geo = Part.LineSegment(vector(edit["start"]), vector(edit["end"]))
            elif kind == "circle":
                geo = Part.Circle(vector(edit["center"]), App.Vector(0, 0, 1), number(edit["radius"]))
            elif kind == "arc":
                geo = Part.Arc(vector(edit["start"]), vector(edit["mid"]), vector(edit["end"]))
            elif kind == "point":
                geo = Part.Point(vector(edit["position"]))
            elif kind in {"ellipse", "ellipse_arc", "bspline", "hyperbola_arc", "parabola_arc"}:
                try:
                    geo = SketchGeometry.make_geometry(edit)
                except Exception as error:
                    fail("invalid_geometry", str(error))
            else:
                fail("unsupported_geometry", "Unsupported sketch geometry: " + str(kind))
            index = obj.addGeometry(geo, bool(edit.get("construction", False)))
        else:
            args = edit.get("args", [])
            if not isinstance(args, list) or not all(isinstance(v, (str, int, float)) and not isinstance(v, bool) for v in args):
                fail("invalid_request", "Constraint args must be native string/number arguments")
            try:
                SketchGeometry.validate_constraint(edit["kind"], args)
            except SketchGeometry.GeometryError as error:
                fail("invalid_request", str(error))
            index = obj.addConstraint(Sketcher.Constraint(edit["kind"], *args))
        if index < 0:
            fail("invalid_model", "Native sketch edit failed")
        return {"object": obj.Name, "index": index}
    elif op in ("datum", "move_geometry", "construction", "delete_geometry", "delete_constraint",
                "constraint_state", "trim_geometry", "expose_internal"):
        if not obj.isDerivedFrom("Sketcher::SketchObject"):
            fail("invalid_request", "This edit requires a native SketchObject")
        constraint = op in ("datum", "delete_constraint", "constraint_state")
        index = sketch_index(edit["index"], obj.ConstraintCount if constraint else obj.GeometryCount)
        if op == "datum":
            value = edit["value"]
            if "input_default_unit" in edit:
                kind = obj.Constraints[index].Type
                unit = "rad" if kind == "Angle" else "" if kind in {"Weight", "SnellsLaw"} else "mm"
                try:
                    parsed = DocumentUnits.parse(value, unit, edit["input_default_unit"])
                    value = DocumentUnits.describe(DocumentUnits.native(parsed, unit), DocumentUnits.index(doc))["input"]
                except (ValueError, TypeError) as error:
                    fail("dimension_mismatch", str(error))
            if not isinstance(value, str):
                number(value)
            quantity = App.Units.Quantity(value)
            number(quantity.Value)
            # FreeCAD 1.1.4 getDatum excludes SnellsLaw although setDatum works.
            # Via-point angles preserve native setDatum's quantity policy too.
            native_angle = obj.Constraints[index].Type == "Angle" and obj.Constraints[index].Third != -2000
            if (isinstance(value, str) and obj.Constraints[index].Type != "SnellsLaw"
                    and not native_angle
                    and quantity.Unit != obj.getDatum(index).Unit):
                fail("dimension_mismatch", "Datum unit does not match the native constraint dimension")
            native_constraint = obj.Constraints[index]
            if (quantity.Value < 0 and native_constraint.Type == "Distance"
                    and native_constraint.FirstPos != 0 and native_constraint.SecondPos == 0
                    and native_constraint.Second != -2000):
                fail("unsupported_signed_distance", "Negative distances between these elements are not supported yet. Enter a positive distance.")
            obj.setDatum(index, quantity)
        elif op == "move_geometry":
            point, relative = edit["point"], edit.get("relative", False)
            if type(point) is not int or not 0 <= point <= 3 or type(relative) is not bool:
                fail("invalid_request", "Point must be 0..3 and relative must be boolean")
            obj.moveGeometry(index, point, vector(edit["position"]), relative)
        elif op == "construction":
            construction = edit.get("construction", not obj.getConstruction(index))
            if type(construction) is not bool:
                fail("invalid_request", "Construction state must be boolean")
            obj.setConstruction(index, construction)
        elif op == "constraint_state":
            fields = {"driving": obj.setDriving, "active": obj.setActive,
                      "virtual": obj.setVirtualSpace, "name": obj.renameConstraint}
            if not any(field in edit for field in fields):
                fail("invalid_request", "Constraint state requires a driving, active, virtual or name field")
            for field in fields:
                if field in edit and type(edit[field]) is not (str if field == "name" else bool):
                    fail("invalid_request", "Constraint state " + field + " has an invalid type")
            for field, setter in fields.items():
                if field in edit:
                    setter(index, edit[field])
        elif op == "trim_geometry":
            obj.trim(index, vector(edit["position"]))
        elif op == "expose_internal":
            obj.exposeInternalGeometry(index)
        elif op == "delete_geometry":
            obj.delGeometry(index)
        else:
            obj.delConstraint(index)
        return {"object": obj.Name, "index": index}
    else:
        fail("unsupported_edit", "Unsupported transaction edit: " + str(op))
    return {"object": obj.Name}


def transaction(doc, request, terminal_recovery=False):
    edits = request["edits"]
    if not isinstance(edits, list) or not edits:
        fail("invalid_request", "Transaction requires a nonempty edits list")
    preflight_constraints(edits)
    recovering_references = bool(_failed_loads and set(native_dependencies(doc)) & _failed_loads.keys())
    affected = {other.Name: other for other, _ in accepted_dependents(doc)} if recovering_references and not terminal_recovery else {}
    for other in affected.values():
        if not lease("metadata", other.Name)["available"]:
            fail("document_unavailable", "Close unavailable dependent " + other.Name + " before repairing native references")
    if sketch_metadata(doc):
        return sketch_transaction(doc, request)
    recovery_only = AssemblySources.recovery_metadata(doc) is not None
    repair_only = recovery_only and all(
        edit.get('op') == 'assembly_relink' and edit.get('solve_joints') is True
        and (assembly := AssemblyAuthoring.owner(object_in(doc, edit['object']))) is not None
        and assembly.Name == edit.get('assembly')
        and AssemblySources.same_revision(AssemblySources.stored(object_in(doc, edit['object'])), edit.get('external_source'))
        for edit in edits)
    prepare_assembly_sources(doc, edits)
    if repair_only:
        # Repairing the captured original source is recovery, not an authoring
        # transaction. Preserve pending native Redo and every accepted history
        # entry. An explicit different source revision still authors normally.
        validate(doc)
        AssemblySources.check(doc)
        state = prepare_state(doc)
        return {'document': doc.Name, 'revision': revision_request('current', doc.Name),
                'edits': [{'object': edit['object'], 'recovered': True} for edit in edits], 'state': state}
    doc.openTransaction(request.get("label", "Edit"))
    try:
        results = [apply_edit(doc, edit) for edit in edits]
        if recompute_document(doc) is False:
            fail("invalid_model", "Native recompute failed")
        validate(doc)
        AssemblySources.check(doc)
        # A reference-removal recovery can change native followers as well as
        # this owner. Never make a newly clean follower readable at an old
        # revision with a stale BRep. Native ordering/recompute stay native.
        solved = {doc.Name}
        for other in affected.values():
            if dependency_recovery(other) is not None or not lease("metadata", other.Name)["available"]:
                continue
            for owner in other.getDependentDocuments():
                if owner.Name not in solved and doc.Name in native_dependencies(owner):
                    if dependency_recovery(owner) is None and lease("metadata", owner.Name)["available"]:
                        if recompute_document(owner) is False:
                            fail("invalid_model", "Native dependent recompute failed: " + owner.Name)
                        validate(owner)
                        solved.add(owner.Name)
        state = prepare_state(doc)
        json.dumps(results, allow_nan=False)
        AssemblySources.check(doc)
        doc.commitTransaction()
        for other in affected.values():
            bump(other)
    except Exception:
        try:
            abort_transaction(doc)
            if not recovering_references:
                recompute_document(doc)
                validate(doc)
        except Exception:
            lease("poison", doc.Name)
            for other in affected.values():
                lease("poison", other.Name)
            traceback.print_exc(file=sys.stderr)
        # Abort restores model edits, but native element aliases and redo
        # history may have changed. Never reuse identities from before it.
        for other in affected.values():
            bump(other)
        bump(doc)
        raise
    publish_state(doc, state)
    return {"document": doc.Name, "revision": revision_request("current", doc.Name),
            "edits": results, "state": state}


def preflight_constraints(edits):
    for edit in edits:
        # New bindings can resolve external native owners. Defer them during
        # failed-load recovery before opening native history; clearing remains
        # an admitted reference-removal operation.
        if _failed_loads and isinstance(edit, dict) and edit.get("op") == "expression" and edit.get("expression") is not None:
            fail("document_unavailable", "Close quarantined owners before binding native expressions")
        try:
            SketchOperations.preflight(edit)
        except SketchOperations.OperationError as error:
            fail(error.code, str(error))
        if isinstance(edit, dict) and edit.get("op") in {"constraint", "refraction", "via_point"}:
            try:
                if edit.get("op") == "via_point" and edit.get("kind") not in {"TangentViaPoint", "PerpendicularViaPoint", "AngleViaPoint"}:
                    raise SketchGeometry.GeometryError("Unsupported native via-point constraint")
                SketchGeometry.validate_constraint("SnellsLaw" if edit.get("op") == "refraction" else edit.get("kind"), edit.get("args", []))
            except SketchGeometry.GeometryError as error:
                fail("invalid_request", str(error))


def sketch_edits(doc, edits):
    lease("sketch_edits_shape", doc.Name, edits=edits)
    preflight_constraints(edits)
    lease("sketch_edits", doc.Name, edits=edits)


def solve_sketch(doc, session):
    sketch = object_in(doc, session["object"])
    if sketch.solve() != 0 or sketch.recompute() is False:
        fail("invalid_model", "Native sketch solver rejected the edit")
    validate(doc)


def journal(doc, action, **parameters):
    try:
        return SketchJournal.call(action, doc if isinstance(doc, str) else doc.Name, **parameters)
    except SketchJournal.JournalError as error:
        fail(error.code, str(error))


def replay_sketch(doc, session, candidate=None):
    if session["opened"]:
        abort_transaction(doc)
        session["opened"] = False
    checkpoint = journal(doc, "checkpoint")
    if checkpoint or candidate:
        doc.openTransaction(session["label"])
        session["opened"] = True
        sketch = object_in(doc, session["object"])
        if candidate and candidate.get("native_checkpoint"):
            with _native_suppression.history():
                SketchCheckpoints.restore(sketch, candidate["native_checkpoint"])
            check_suppression()
            results = []
        else:
            if checkpoint:
                with _native_suppression.history():
                    SketchCheckpoints.restore(sketch, checkpoint)
                check_suppression()
                solve_sketch(doc, session)
            results = [apply_edit(doc, edit) for edit in candidate["edits"]] if candidate else []
    else:
        results = []
    solve_sketch(doc, session)
    return results


def restore_sketch(doc, session, candidate=None):
    try:
        replay_sketch(doc, session, candidate)
    except Exception:
        lease("poison", doc.Name)
        traceback.print_exc(file=sys.stderr)
    bump(doc)


def sketch_transaction(doc, request):
    session = _native_sketch_sessions[doc.Name]
    edits = request["edits"]
    sketch_edits(doc, edits)
    automatic = [edit for edit in edits if edit.get("op") == "auto_external_coincident"]
    if automatic:
        target = object_in(doc, session["object"])
        points = [e.get("point") for e in automatic]
        if (len(automatic) > 2 or any(type(p) is not int or p not in (1, 2) for p in points)
                or len(set(points)) != len(points)
                or any(e.get("index") != target.GeometryCount for e in automatic)
                or edits[0].get("op") != "geometry" or edits[0].get("kind") != "line"
                or sum(e.get("op") == "geometry" for e in edits) != 1
                or any(e.get("op") not in {"geometry", "constraint", "auto_external_coincident"} for e in edits)):
            fail("invalid_request", "Automatic projection belongs to one newly drawn line")
    staged = False
    try:
        if not session["opened"]:
            doc.openTransaction(session["label"])
            session["opened"] = True
        results = [apply_edit(doc, edit) for edit in edits]
        solve_sketch(doc, session)
        checkpoint = SketchCheckpoints.capture(object_in(doc, session["object"]))
        journal(doc, "stage_accept", group={"label": request.get("label", "Edit sketch"), "edits": edits,
                                            "native_checkpoint": checkpoint})
        staged = True
        state = prepare_state(doc)
        json.dumps(results, allow_nan=False)
    except Exception:
        if staged:
            journal(doc, "resolve", accepted=False)
        restore_sketch(doc, session)
        raise
    journal(doc, "resolve", accepted=True)
    publish_state(doc, state)
    return {"document": doc.Name, "revision": revision_request("current", doc.Name), "edits": results, "state": state}


def sketch_history(doc, op):
    session = _native_sketch_sessions[doc.Name]
    journal(doc, "stage_" + op)
    try:
        replay_sketch(doc, session)
        state = prepare_state(doc)
    except Exception:
        journal(doc, "resolve", accepted=False)
        restore_sketch(doc, session)
        raise
    journal(doc, "resolve", accepted=True)
    return publish_state(doc, state)


def sketch_session(doc, request):
    lease("sketch_validate", doc.Name, request=request)
    if request["op"] == "sketch_begin":
        try:
            SketchCheckpoints.require_runtime()
        except SketchCheckpoints.checkpoint_native.CheckpointUnavailable as error:
            fail("native_checkpoint_unavailable", str(error))
        obj = object_in(doc, request["object"])
        if not obj.isDerivedFrom("Sketcher::SketchObject"):
            fail("invalid_request", "Sketch editing requires a native SketchObject")
        validate(doc, whole=True)
        journal(doc, "begin")
        lease("sketch_begin", doc.Name, request=request, object=obj.Name)
        _native_sketch_sessions[doc.Name] = {"object": obj.Name, "label": "Edit " + obj.Label,
                                     "opened": False,
                                     "context_revision": revision_request("current", doc.Name)}
        try:
            state = prepare_state(doc)
        except Exception:
            journal(doc, "end")
            del _native_sketch_sessions[doc.Name]
            lease("sketch_end", doc.Name)
            raise
        return publish_state(doc, state)
    session = _native_sketch_sessions[doc.Name]
    if request["op"] == "sketch_cancel":
        try:
            if session["opened"]:
                abort_transaction(doc)
                session["opened"] = False
            recompute_document(doc)
            validate(doc, whole=True)
            state = prepare_state(doc)
            state.update(sketch_edit=None, preview=None)
            json.dumps(state, allow_nan=False)
        except Exception:
            # Abort/replay can change native identities. Restore the owned session
            # and gesture, then publish a new revision even if restoration poisons it.
            restore_sketch(doc, session, _native_previews.get(doc.Name))
            raise
    else:
        try:
            if recompute_document(doc) is False:
                fail("invalid_model", "Native finish recompute failed")
            validate(doc, whole=True)
            state = prepare_state(doc)
            state.update(sketch_edit=None, preview=None)
            json.dumps(state, allow_nan=False)
            if session["opened"]:
                doc.commitTransaction()
        except Exception:
            restore_sketch(doc, session)
            raise
    journal(doc, "end")
    del _native_sketch_sessions[doc.Name]
    _native_previews.pop(doc.Name, None)
    lease("sketch_end", doc.Name)
    # Reuse the serialized geometry snapshot. Only infallible metadata and native
    # history counts change after acceptance; publish_state bumps before reading counts.
    return publish_state(doc, state)


def sketch_preview(doc, request, active):
    session = _native_sketch_sessions[doc.Name]
    op = request["op"]
    if op != "preview":
        if op == "preview_commit":
            journal(doc, "stage_accept", group=active)
            try:
                state = prepare_state(doc)
                state["preview"] = None
                json.dumps(state, allow_nan=False)
            except Exception:
                journal(doc, "resolve", accepted=False)
                raise
            journal(doc, "resolve", accepted=True)
        else:
            try:
                replay_sketch(doc, session)
                state = prepare_state(doc)
                state["preview"] = None
                json.dumps(state, allow_nan=False)
            except Exception:
                restore_sketch(doc, session, active)
                raise
        del _native_previews[doc.Name]
        lease("preview_end", doc.Name)
        return publish_state(doc, state)
    edits = request.get("edits")
    sketch_edits(doc, edits)
    label = preview_ready(doc, request, opened=session["opened"])
    candidate = {"label": label, "edits": edits}
    staged = False
    try:
        results = replay_sketch(doc, session, candidate)
        candidate["native_checkpoint"] = SketchCheckpoints.capture(object_in(doc, session["object"]))
        _native_previews[doc.Name] = candidate
        lease("preview_stage", doc.Name, request=request, label=label)
        staged = True
        state = prepare_state(doc)
        json.dumps(results, allow_nan=False)
    except Exception:
        if staged:
            lease("preview_resolve", doc.Name, accepted=False)
        if active is None:
            _native_previews.pop(doc.Name, None)
        else:
            _native_previews[doc.Name] = active
        restore_sketch(doc, session, active)
        raise
    lease("preview_resolve", doc.Name, accepted=True)
    publish_state(doc, state)
    return {"document": doc.Name, "revision": revision_request("current", doc.Name), "edits": results, "state": state}


def apply_preview_edit(doc, edit):
    if edit.get("op") != "create_revolve_axis":
        return apply_edit(doc, edit)
    master = object_in(doc, edit["master"])
    profile = object_in(doc, edit["profile"])
    body = object_in(doc, edit["body"])
    key = name(edit["name"])
    geometry_id = edit.get("geometry_id")
    if (master.TypeId != "Sketcher::SketchObject" or profile.TypeId != "Sketcher::SketchObject"
            or master == profile or not body.isDerivedFrom("PartDesign::Body")
            or owner_body(profile) != body or owner_body(master) != body or type(geometry_id) is not int
            or not effective_visibility(master)):
        fail("stale_reference", "Choose a visible construction line on another sketch in this document")
    if body in master.OutListRecursive or profile in master.OutListRecursive:
        fail("invalid_request", "Construction axis would create a native dependency cycle")
    matches = [index for index in range(master.GeometryCount) if master.getGeometryId(index) == geometry_id]
    if (len(matches) != 1 or not master.getConstruction(matches[0])
            or not isinstance(master.Geometry[matches[0]], Part.LineSegment)):
        fail("stale_reference", "The selected construction line no longer exists")
    if doc.getObject(key):
        fail("name_conflict", "Axis dependency name already exists: " + key)
    axis = body.newObject("PartDesign::Line", key)
    if axis.Name != key:
        fail("invalid_request", "FreeCAD changed the requested axis dependency name")
    axis.Label = "Revolve axis dependency"
    axis.AttachmentSupport = [(master, ["edge" + str(matches[0] + 1)])]
    axis.MapMode = "TwoPointLine"
    axis.Visibility = False
    return {"object": axis.Name, "master": master.Name, "master_id": master.ID,
            "geometry_id": geometry_id, "index": matches[0]}


def validate_revolve_axis_preview(edits):
    axes = [edit for edit in edits if isinstance(edit, dict) and edit.get("op") == "create_revolve_axis"]
    if not axes:
        return
    axis = axes[0]
    features = [edit for edit in edits if isinstance(edit, dict) and edit.get("op") == "add_object"
                and edit.get("type") == "PartDesign::Revolution"]
    if len(axes) != 1 or edits[0] != axis or len(features) != 1 or features[0].get("body") != axis.get("body"):
        fail("invalid_request", "A construction-axis dependency requires one new Revolution preview")
    feature = features[0].get("name")
    def has_link(prop, target, subs=None):
        return any(edit.get("op") == "set" and edit.get("object") == feature
                   and edit.get("property") == prop and isinstance(edit.get("value"), dict)
                   and edit["value"].get("link") == target
                   and (subs is None or edit["value"].get("subelements") == subs)
                   for edit in edits if isinstance(edit, dict))
    if not has_link("Profile", axis.get("profile")) or not has_link("ReferenceAxis", axis.get("name"), [""]):
        fail("invalid_request", "The Revolution must use the selected profile and its new axis dependency")


def preview(doc, request):
    lease("preview_validate", doc.Name, request=request)
    active = _native_previews.get(doc.Name)
    if sketch_metadata(doc):
        return sketch_preview(doc, request, active)
    op = request["op"]
    if op != "preview":
        # Build and serialize the response before committing native history.
        if op == "preview_commit":
            state = prepare_state(doc)
            state["preview"] = None
            try:
                doc.commitTransaction()
            except Exception:
                lease("poison", doc.Name)
                bump(doc)
                raise
        else:
            try:
                abort_transaction(doc)
                recompute_document(doc)
                validate(doc)
                state = prepare_state(doc)
                state["preview"] = None
            except Exception:
                lease("poison", doc.Name)
                _native_previews.pop(doc.Name, None)
                lease("preview_end", doc.Name)
                bump(doc)
                raise
        del _native_previews[doc.Name]
        lease("preview_end", doc.Name)
        return publish_state(doc, state)
    edits = request.get("edits")
    if not isinstance(edits, list) or not edits:
        fail("invalid_request", "Preview requires a nonempty edits list")
    validate_revolve_axis_preview(edits)
    preflight_constraints(edits)
    label = preview_ready(doc, request)
    prepare_assembly_sources(doc, edits, active is not None)
    if active:
        try:
            abort_transaction(doc)
            recompute_document(doc)
            validate(doc)
        except Exception:
            lease("poison", doc.Name)
            bump(doc)
            raise
    doc.openTransaction(label)
    staged = False
    try:
        results = [apply_preview_edit(doc, edit) for edit in edits]
        if recompute_document(doc) is False:
            fail("invalid_model", "Native recompute failed")
        validate(doc)
        AssemblySources.check(doc)
        lease("preview_stage", doc.Name, request=request, label=label)
        staged = True
        state = prepare_state(doc)
        json.dumps(results, allow_nan=False)
    except Exception:
        if staged:
            lease("preview_resolve", doc.Name, accepted=False)
        try:
            abort_transaction(doc)
            recompute_document(doc)
            if active:
                doc.openTransaction(active["label"])
                for edit in active["edits"]:
                    apply_preview_edit(doc, edit)
                recompute_document(doc)
            validate(doc)
        except Exception:
            lease("poison", doc.Name)
            traceback.print_exc(file=sys.stderr)
        # Abort/replay can change native element aliases even for an initial
        # candidate that failed. Callers must re-inspect before another write.
        bump(doc)
        raise
    lease("preview_resolve", doc.Name, accepted=True)
    _native_previews[doc.Name] = {"label": label, "edits": edits}
    publish_state(doc, state)
    return {"document": doc.Name, "revision": revision_request("current", doc.Name),
            "edits": results, "state": state}


def native_path(value):
    if not isinstance(value, str) or not value or Path(value).suffix.lower() != ".fcstd":
        fail("invalid_request", "A native .FCStd file path is required")
    return str(Path(value).expanduser().resolve())


def close_document(doc):
    key = doc.Name
    close_loaded_document(key)
    return finish_closed_document(key)


def finish_closed_document(key):
    revision_request("close", key)
    lease("close", key)
    _uncertain_closes.pop(key, None)
    recovery = _failed_loads.pop(key, None)
    if recovery is not None:
        recovery["documents"].remove(key)
    _native_previews.pop(key, None)
    _native_sketch_sessions.pop(key, None)
    journal(key, "end")
    NativeSaves.close(key)
    AssemblySources.close_root(key)
    AssemblySources._dependencies.pop(key, None)
    result = {"document": key, "closed": True}
    before = set(App.listDocuments()) - set(AssemblySources._dependencies)
    active = App.ActiveDocument.Name if App.ActiveDocument else ""
    readonly_before = set(AssemblySources._readonly_paths)
    try:
        AssemblySources.release(registered_root)
    except Exception as error:
        # The requested root really closed. Retain any failed readonly teardown
        # as named quarantine, while reporting that partial cleanup truthfully.
        try:
            cleanup_failed_load("source_cleanup", None, before, active, error, readonly_before)
        except Exception as cleanup_error:
            result["cleanup_error"] = str(cleanup_error)
    diagnostics = [value for value in _load_close_errors.values() if value.get("native_closed")]
    if diagnostics:
        message = "Close cleanup: " + json.dumps(diagnostics, allow_nan=False, sort_keys=True)
        result["cleanup_error"] = result.get("cleanup_error", "") + message
    if _uncertain_closes:
        # The root really closed. Offer only named unavailable metadata, never
        # another owner's model, for the existing explicit Close confirmation.
        result["cleanup_recovery"] = uncertain_close_state(sorted(_uncertain_closes)[0])
    return result


def uncertain_close_state(key):
    return {"document": key, "revision": revision_request("current", key),
            "objects": [], "available": False, "preview": None, "sketch_edit": None,
            "path": _uncertain_closes[key][2], "recovery": _failed_loads[key]}


def close_loaded_document(key):
    if key in _load_close_errors:
        previous = _load_close_errors[key]
        raise Failure(previous["code"], previous["message"])
    pending = _uncertain_closes.get(key)
    doc = App.listDocuments().get(key) if pending is not None else App.getDocument(key)
    already_absent = doc is None
    if already_absent:
        doc, uid, path = pending
    else:
        uid, path = str(doc.Uid), doc.FileName or None
    diagnostics = list(_failed_loads.get(key, {}).get("cleanup_errors", []))

    def presence():
        try:
            current = App.listDocuments().get(key)
        except Exception as error:
            # Native Close may already have deleted or replaced the captured
            # owner. Neither its name nor a copied writable Uid proves identity.
            previous = _failed_loads.pop(key, None)
            if previous is not None:
                previous["documents"].remove(key)
            _uncertain_closes[key] = (doc, uid, path)
            lease("poison", key)
            revision_request("bump", key)
            _failed_loads[key] = {"operation": "close", "path": path, "action": "close",
                "documents": [key], "error_code": "document_unavailable",
                "error": "Native Close presence query failed: " + str(error),
                "cleanup_errors": diagnostics + [cleanup_diagnostic(key, error)]}
            fail("document_unavailable", "Native Close outcome is unresolved: " + key + ": " + str(error)
                 + "\nClose cleanup: " + json.dumps(_failed_loads[key]["cleanup_errors"],
                                                      allow_nan=False, sort_keys=True))
        if current is not None:
            try:
                same = current == doc and str(doc.Uid) == uid
            except Exception:
                same = False  # A deleted native handle must not authorize name reuse.
            if not same:
                # A callback replaced the native owner. Do not detach or publish
                # the new owner under the old revision, observer, or source claim.
                error = Failure("document_unavailable", "Native owner changed during Close: " + key)
                previous = _failed_loads.pop(key, None)
                if previous is not None:
                    previous["documents"].remove(key)
                lease("poison", key)
                revision_request("bump", key)
                _uncertain_closes.pop(key, None)
                _failed_loads[key] = {"operation": "close", "path": current.FileName or None,
                    "error": str(error), "error_code": error.code, "action": "close", "documents": [key],
                    "cleanup_errors": diagnostics, "replaced_document_uid": uid,
                    "document_uid": str(current.Uid)}
                source = AssemblySources._dependencies.get(key)
                if source is not None and source["document_uid"] == uid:
                    AssemblySources._dependencies.pop(key)
                raise error
        return current is not None

    try:
        if not already_absent:
            check_close_dependents(doc)
            try:
                App.closeDocument(key)
            except Exception as error:
                diagnostics.append(cleanup_diagnostic(key, error))
                if presence():
                    raise
        if presence():
            raise RuntimeError("Native Close left the document open")
        try:
            _native_suppression.detach(key)
        except Exception as error:
            diagnostics.append(cleanup_diagnostic(key, error))
            presence()
            # The captured native handle is gone. Finish only its local observer
            # lifetime bookkeeping; this does not claim the thrown hook succeeded.
            _native_suppression.documents.discard(key)
            _native_suppression.pending = {k: v for k, v in _native_suppression.pending.items() if k[0] != key}
        presence()
    except Exception as error:
        _load_close_errors[key] = cleanup_diagnostic(key, error)
        raise
    if diagnostics:
        _load_close_errors[key] = {"document": key, "code": "native_error",
            "message": "Native Close completed with cleanup diagnostics", "native_closed": True,
            "diagnostics": diagnostics}


def restore_active_document(active):
    App.setActiveDocument(active if active in App.listDocuments() else "")
    # FreeCAD 1.1.4 leaves this Python attribute stale when the native
    # active owner is cleared with an empty name.
    App.ActiveDocument = App.activeDocument()


def cleanup_diagnostic(key, error):
    return {"document": key, "code": getattr(error, "code", "native_error"),
            "message": str(error)}


def load_cleanup_failure(recovery):
    return Failure(recovery["error_code"], recovery["error"]
                   + "\nLoad cleanup: " + json.dumps(recovery, allow_nan=False, sort_keys=True))


def cleanup_failed_load(operation, path, before, active, error, readonly_before):
    cleanup_errors = []
    try:
        for key in sorted(set(App.listDocuments()) - before):
            try:
                close_loaded_document(key)
                AssemblySources._dependencies.pop(key, None)
            except Exception as cleanup_error:
                cleanup_errors.append(cleanup_diagnostic(key, cleanup_error))
    finally:
        AssemblySources._readonly_paths.clear()
        AssemblySources._readonly_paths.update(readonly_before)
        try:
            restore_active_document(active)
        except Exception as restore_error:
            cleanup_errors.append(cleanup_diagnostic("active document restore", restore_error))
    # Nested source cleanup may already have closed an owner despite a thrown
    # hook. Preserve that diagnostic with the original load error, not as a survivor.
    cleanup_errors.extend(value for value in _load_close_errors.values() if value.get("native_closed"))
    # An unresolved Close can have deleted its owner. Native presence alone
    # cannot enumerate the recovery targets created by this public action.
    uncertain = (set(_uncertain_closes) & set(_load_close_errors)) - before
    survivors = sorted((set(App.listDocuments()) - before) | uncertain)
    for key in survivors:
        previous = _failed_loads.get(key)
        if previous is not None:
            # Retain the first Close/detach error as well as the later failed
            # presence query before replacing keyed metadata with this batch.
            cleanup_errors.extend(previous["cleanup_errors"])
    if path is None and operation == "source_cleanup":
        files = {_uncertain_closes[key][2] if key in _uncertain_closes
                 else App.getDocument(key).FileName for key in survivors}
        if len(files) == 1:
            path = files.pop()
    recovery = {"operation": operation, "error": str(error), "error_code": getattr(error, "code", "native_error"),
                "cleanup_errors": cleanup_errors, "action": "close", "path": path, "documents": survivors}
    for key in survivors:
        lease("poison", key)
        revision_request("bump", key)
        _failed_loads[key] = recovery
    if cleanup_errors or survivors:
        raise load_cleanup_failure(recovery) from error


def retry_failed_open(path, operation="open"):
    recovery = next((batch for batch in _failed_loads.values()
                     if batch["operation"] == operation
                     and batch["path"] is not None
                     and os.path.normcase(batch["path"]) == os.path.normcase(path)), None)
    if recovery is None:
        return
    active = App.ActiveDocument.Name if App.ActiveDocument else ""
    previous_errors = len(recovery["cleanup_errors"])
    try:
        for key in tuple(recovery["documents"]):
            # Only the exact failed-load batch is eligible. Explicit Close and
            # later name reuse remove this identity before any new owner exists.
            if _failed_loads.get(key) is not recovery:
                continue
            try:
                doc = document({"op": "close", "document": key,
                                "expected_revision": revision_request("current", key)}, writable=True)
                close_document(doc)
            except Exception as error:
                recovery["cleanup_errors"].append(cleanup_diagnostic(key, error))
    finally:
        try:
            restore_active_document(active)
        except Exception as error:
            recovery["cleanup_errors"].append(cleanup_diagnostic("active document restore", error))
    recovery["documents"] = [key for key in recovery["documents"] if _failed_loads.get(key) is recovery]
    if recovery["documents"] or len(recovery["cleanup_errors"]) != previous_errors:
        raise load_cleanup_failure(recovery)


def registered_root(key):
    return revision_request('lookup', key) is not None


def validate_external_source(doc):
    # Exactly the authoritative restored-input and proxy admission used by root
    # Open. Sources stay readonly on disk; stale saved BReps are regenerated in
    # their own native document before any root transaction can begin.
    _native_suppression.attach(doc)
    HoleTask.restore_inputs(doc, doc.FileName)
    recompute_document(doc)
    for obj in doc.Objects:
        if obj.isDerivedFrom('Sketcher::SketchObject') and obj.solve() != 0:
            fail('invalid_model', obj.Name + ': native source sketch solver rejected restored constraints')
    recompute_document(doc)
    validate(doc)


def prepare_assembly_sources(doc, edits, active=False):
    recover_external_edges(doc)
    if any(e.get('op') == 'assembly_relink' and e.get('solve_joints') is not True for e in edits):
        fail('assembly_update_mode_unavailable', 'Pinned native Assembly recompute solves automatically. Independent link update without solving is unavailable without changing global preferences.')
    for edit in edits:
        if edit.get('op') == 'assembly_insert' and edit.get('external_source') is not None and not doc.FileName:
            fail('assembly_save_required', 'Save as the root assembly FCStd before external insertion')
        if edit.get('op') == 'assembly_relink':
            old = AssemblySources.stored(object_in(doc, edit['object']))
            selected = edit.get('external_source', {})
            if old is None or any(old.get(k) != selected.get(k) for k in ('document_uid', 'object', 'object_id', 'type')):
                fail('source_identity', 'Relink requires the same native document UUID and object ID')
    external = [e for e in edits if e.get('external_source') is not None]
    if active and external:
        # Never open/close native documents inside an owned preview. Only already
        # loaded, immutable sources may participate in replacement/replay.
        for edit in external:
            AssemblySources.resolve(edit['external_source'])
    else:
        before = set(App.listDocuments())
        readonly_before = set(AssemblySources._readonly_paths)
        active_owner = App.ActiveDocument.Name if App.ActiveDocument else ""
        paths = {str(Path(e['external_source']['path']).expanduser().resolve()) for e in external
                 if isinstance(e.get('external_source'), dict) and isinstance(e['external_source'].get('path'), str)}
        path = next(iter(paths)) if len(paths) == 1 else None
        if path is not None:
            retry_failed_open(path, "assembly_sources")
            retry_failed_open(path, "source_cleanup")
            before = set(App.listDocuments())
        try:
            AssemblySources.prepare(edits, registered_root, validate_external_source)
        except Exception as error:
            cleanup_failed_load("assembly_sources", path, before, active_owner, error, readonly_before)
            raise
    # Relink is the deliberate recovery action for a changed or missing source.
    # All other mutation routes reject stale sources before native history starts.
    if not any(e.get('op') == 'assembly_relink' for e in edits):
        AssemblySources.check(doc)


def recover_external_edges(doc):
    if AssemblySources.recovery_metadata(doc) is not None:
        try:
            AssemblySources.recover_edges(doc)
        finally:
            # Native restoration may change aliases even on rejection. Existing
            # admission already checked revision, lease and registered ownership.
            bump(doc)


def dispatch(request):
    if request.get("version") != VERSION:
        fail("unsupported_version", "Expected protocol version 1")
    if type(request.get("id")) is not int:
        fail("invalid_request", "Request id must be an integer")
    op = request.get("op")
    _load_close_errors.clear()
    if op == "capabilities":
        return {"protocol_version": VERSION, "freecad_version": list(App.Version()),
                "occt_version": Part.OCC_VERSION, "process_id": os.getpid(), "operations": OPERATIONS,
                "object_types": sorted(OBJECT_TYPES), "geometry_kinds": ["line", "circle", "arc", "point",
                    "ellipse", "ellipse_arc", "bspline", "hyperbola_arc", "parabola_arc"],
                "constraint_signatures": SketchGeometry.CONSTRAINT_SIGNATURES,
                "sketch_tasks": SketchOperations.capabilities(),
                "sketch_checkpoints": SketchCheckpoints.capabilities(),
                "edit_operations": EDIT_OPERATIONS,
                "suppression": {"persistent_state_version": 1, "native_map_seed": True,
                                "legacy_requires_original_state": True},
                "assembly": {"container_type": "Assembly::AssemblyObject", "instance_type": "App::Link",
                             "joint_types": list(AssemblyAuthoring.JOINT_TYPES),
                             "component_sources": ["local_document", "external_fcstd"], "external_file_insertion": True,
                             "source_identity_version": 1, "in_place_source_reload": False}}
    if op == "feature_schema":
        type_id = request.get("type")
        if type_id not in _feature_schemas:
            fail("unsupported_object_type", "Unsupported native object type: " + str(type_id))
        if request.get("document") is not None:
            doc = document(request)
            lease("read_guard", doc.Name)
            revision_request("check", doc.Name, expected_revision=request.get("expected_revision"))
            return DocumentUnits.schema(_feature_schemas[type_id], doc, request.get("values", {}))
        return _feature_schemas[type_id]
    terminal_recovery = admit_global(request)
    previous_active = App.ActiveDocument.Name if App.ActiveDocument else ""
    if _uncertain_closes and op in ("new", "open", "assembly_sources"):
        fail("document_unavailable", "Resolve uncertain native Close before loading documents: "
             + ", ".join(sorted(_uncertain_closes)))
    key = request.get("document")
    if isinstance(key, str) and key in _uncertain_closes:
        if op == "inspect":
            return uncertain_close_state(key)
        if op == "close":
            lease("pre_revision", key, request=request)
            revision_request("check", key, expected_revision=request.get("expected_revision"))
            lease("local", key, request=request)
            try:
                close_loaded_document(key)
                return finish_closed_document(key)
            finally:
                if terminal_recovery:
                    restore_active_document(previous_active)
    if op in ("new", "open"):
        if op == "open":
            path = native_path(request["path"])
            # One explicit same-path Open retries this batch's Close exactly
            # once. Never retry native work in a loop or reopen a partial batch.
            retry_failed_open(path)
            if any(d.FileName and AssemblySources.same_file(d.FileName, path)
                   for key, d in App.listDocuments().items() if key in _failed_loads):
                fail("document_unavailable", "This native file still has a quarantined owner; close its failed-load batch before Open")
        before = set(App.listDocuments())
        readonly_before = set(AssemblySources._readonly_paths)
        active = App.ActiveDocument.Name if App.ActiveDocument else ""
        try:
            if op == "new":
                key = name(request["name"])
                if key in before:
                    fail("name_conflict", "Document name already exists")
                doc = App.newDocument(key)
            else:
                path = native_path(request["path"])
                if AssemblySources.protected_path(path):
                    fail('source_ownership', 'This FCStd is owned as a readonly assembly dependency in this worker. It cannot become an editable root. Open an independent worker to edit source documents explicitly.')
                if not Path(path).is_file():
                    fail("not_found", "Native document does not exist")
                if any(os.path.normcase(d.FileName) == os.path.normcase(path) for d in App.listDocuments().values()):
                    fail("already_open", "Native document is already open")
                AssemblySources.prepare_open(path, registered_root, validate_external_source)
                doc = App.openDocument(path)
                AssemblySources.claim(before, lambda key: key == doc.Name or registered_root(key), validate_external_source)
                AssemblySources.check(doc)
            _native_suppression.attach(doc)
            doc.UndoMode = 1
            if op == "open":
                HoleTask.restore_inputs(doc, path)
            recompute_document(doc)
            # Untouched restored sketches can retain the Python DoF default 0
            # until their solver runs, even when they are underconstrained.
            # Initialize native freedom/conflict state before publishing it.
            for obj in doc.Objects:
                if obj.isDerivedFrom("Sketcher::SketchObject") and obj.solve() != 0:
                    fail("invalid_model", obj.Name + ": native sketch solver rejected restored constraints")
            recompute_document(doc)
            validate(doc)
            state = prepare_state(doc)
        except Exception as error:
            cleanup_failed_load(op, path if op == "open" else None, before, active, error, readonly_before)
            raise
        return publish_state(doc, state)
    if op not in OPERATIONS:
        fail("unsupported_operation", "Unsupported operation: " + str(op))
    doc = document(request, writable=op in ("transaction", "undo", "redo", "save", "close",
                                           "preview", "preview_commit", "preview_cancel",
                                           "sketch_begin", "sketch_finish", "sketch_cancel", "export_exchange", "document_settings"))
    if op == 'assembly_sources':
        # Loading is native global activity even though source ownership is read
        # only. Use exactly the Rust lease/revision/poison guards, without
        # selecting another active document or opening a native transaction.
        lease('pre_revision', doc.Name, request=request)
        revision_request('check', doc.Name, expected_revision=request.get('expected_revision'))
        lease('local', doc.Name, request=request)
        if not doc.FileName:
            fail('assembly_save_required', 'Save as the root assembly FCStd before choosing an external component')
        path = native_path(request['path'])
        retry_failed_open(path, "assembly_sources")
        retry_failed_open(path, "source_cleanup")
        before = set(App.listDocuments())
        active = App.ActiveDocument.Name if App.ActiveDocument else ""
        readonly_before = set(AssemblySources._readonly_paths)
        try:
            result = AssemblySources.browse(path, registered_root, validate_external_source)
        except Exception as error:
            cleanup_failed_load("assembly_sources", path, before, active, error, readonly_before)
            raise
        return {'document': doc.Name, 'revision': revision_request('current', doc.Name), **result}
    if op == "quantities":
        lease("read_guard", doc.Name)
        revision_request("check", doc.Name, expected_revision=request.get("expected_revision"))
        try:
            items = DocumentUnits.query(doc, request.get("items"))
        except (ValueError, TypeError, KeyError) as error:
            fail("invalid_quantity", str(error))
        return {"document": doc.Name, "revision": revision_request("current", doc.Name), "items": items}
    if op == "document_settings":
        selected = request.get("unit_system")
        choices = list(doc.getEnumerationsOfProperty("UnitSystem"))
        if type(selected) is not int or not 0 <= selected < len(choices):
            fail("invalid_request", "Choose a native document unit system")
        previous = doc.UnitSystem
        if previous == choices[selected]:
            return snapshot(doc)
        history = (doc.UndoCount, doc.RedoCount)
        try:
            # UnitSystem is NOT transaction undoable. No transaction is opened,
            # aborted, committed or synthesized for this document setting.
            doc.UnitSystem = selected
            state = prepare_state(doc)
            if doc.UnitSystem != choices[selected] or history != (doc.UndoCount, doc.RedoCount):
                fail("settings_state", "Native setting or history changed unexpectedly")
            state["revision"] = bump(doc)
            return state
        except Exception:
            try:
                doc.UnitSystem = previous
                prepare_state(doc)
                if doc.UnitSystem != previous or history != (doc.UndoCount, doc.RedoCount):
                    raise RuntimeError("Native settings restoration did not match its baseline")
            except Exception:
                lease("poison", doc.Name)
                traceback.print_exc(file=sys.stderr)
            raise
    if op == "measure":
        lease("read_guard", doc.Name)
        revision_request("check", doc.Name, expected_revision=request.get("expected_revision"))
        AssemblySources.check(doc)
        active = sketch_metadata(doc)
        try:
            result = Measurements.measure(doc, request, active["object"] if active else None)
        except Measurements.MeasurementError as error:
            fail(error.code, str(error))
        return {"document": doc.Name, "revision": revision_request("current", doc.Name),
                "preview": preview_metadata(doc), "sketch_edit": active, **result}
    if op == "inspect":
        return snapshot(doc)
    if op == "transaction":
        try:
            return transaction(doc, request, terminal_recovery)
        finally:
            if terminal_recovery:
                restore_active_document(previous_active)
    if op == "export_exchange":
        validate(doc)
        try:
            result = Exchange.export_from(doc, request)
        except Exchange.ExchangeError as error:
            fail(error.code, str(error))
        result["exported_objects"] = result.pop("objects")
        return {"document": doc.Name, "revision": revision_request("current", doc.Name), **result}
    if op in ("sketch_begin", "sketch_finish", "sketch_cancel"):
        return sketch_session(doc, request)
    if op in ("preview", "preview_commit", "preview_cancel"):
        return preview(doc, request)
    if op in ("undo", "redo"):
        if sketch_metadata(doc):
            return sketch_history(doc, op)
        if getattr(doc, "UndoCount" if op == "undo" else "RedoCount") == 0:
            fail("nothing_to_" + op, "No native transaction to " + op)
        names_before = {obj.Name for obj in doc.Objects}
        native_history(doc, op)
        try:
            ExpressionLinks.repair_new(doc, names_before)
            recompute_document(doc)
            validate(doc)
            AssemblySources.check(doc)
            state = prepare_state(doc)
        except Exception:
            try:
                names_before = {obj.Name for obj in doc.Objects}
                native_history(doc, "redo" if op == "undo" else "undo")
                ExpressionLinks.repair_new(doc, names_before)
                recompute_document(doc)
                validate(doc)
            except Exception:
                lease("poison", doc.Name)
                traceback.print_exc(file=sys.stderr)
            raise
        return publish_state(doc, state)
    if op == "save":
        recovering_edges = AssemblySources.recovery_metadata(doc) is not None
        recover_external_edges(doc)
        validate(doc)
        AssemblySources.check(doc)
        path = native_path(request["path"])
        if AssemblySources.protected_path(path):
            fail('source_ownership', 'Readonly source paths cannot be overwritten, including after browsing or closing their native documents')
        if any(d.FileName and AssemblySources.same_file(d.FileName, path)
               for d in App.listDocuments().values() if d != doc):
            fail('source_ownership', 'Save cannot overwrite another root or readonly source file')
        if any(obj.TypeId == 'App::Link' and AssemblySources.stored(obj) is not None for obj in doc.Objects):
            try:
                NativeSaves.save(doc, path)
            except NativeSaves.SaveError as error:
                bump(doc)
                fail(error.code, str(error))
        else:
            doc.saveAs(path)
            NativeSaves.close(doc.Name)
        bump(doc)
        result = {"document": doc.Name, "revision": revision_request("current", doc.Name), "path": path, "save_recovery": None, "recovery_only": False}
        if recovering_edges:
            # The Editor may hold an unresolved snapshot from a rejected Save.
            # Publish the repaired native graph with the successful retry.
            result["state"] = snapshot(doc)
        return result
    if op == "close":
        try:
            return close_document(doc)
        finally:
            if terminal_recovery:
                restore_active_document(previous_active)
    if op in ("mesh", "topology"):
        lease("read_guard", doc.Name)
        AssemblySources.check(doc)
        validate(doc)
        obj = object_in(doc, request["object"])
        if not hasattr(obj, "Shape") or obj.Shape.isNull():
            fail("no_shape", "Object has no native display shape")
        deflection = number(request.get("deflection", 0.1))
        if deflection <= 0:
            fail("invalid_request", "Deflection must be positive")
        if op == "topology":
            return {"document": doc.Name, "revision": revision_request("current", doc.Name), "object": obj.Name,
                    "preview": preview_metadata(doc),
                    "sketch_edit": sketch_metadata(doc),
                    "context_revision": _native_sketch_sessions[doc.Name]["context_revision"] if sketch_metadata(doc) and obj.Name != sketch_metadata(doc)["object"] else revision_request("current", doc.Name),
                    "world_from_shape": world_from_shape(obj),
                    **topology(obj.Shape, deflection)}
        positions, normals, triangles, faces = [], [], [], []
        for index, face in enumerate(obj.Shape.Faces, 1):
            points, facets = face.tessellate(deflection)
            offset, first = len(positions), len(triangles)
            positions.extend(encode(p) for p in points)
            surface = face.Surface
            if isinstance(surface, Part.Plane):
                normals.extend([oriented_normal(face, 0, 0)] * len(points))
            else:
                normals.extend(oriented_normal(face, *surface.parameter(point)) for point in points)
            triangles.extend([[offset + i for i in tri] for tri in facets])
            faces.append({"subelement": "Face" + str(index), "first_triangle": first,
                          "triangle_count": len(facets)})
        return {"document": doc.Name, "revision": revision_request("current", doc.Name), "object": obj.Name,
                "preview": preview_metadata(doc),
                "sketch_edit": sketch_metadata(doc),
                "context_revision": _native_sketch_sessions[doc.Name]["context_revision"] if sketch_metadata(doc) and obj.Name != sketch_metadata(doc)["object"] else revision_request("current", doc.Name),
                "world_from_shape": world_from_shape(obj),
                "positions": positions, "normals": normals, "triangles": triangles, "faces": faces}


def main():
    initialize_feature_schemas()
    for line in sys.stdin:
        request = None
        try:
            request = json.loads(line, parse_constant=lambda value: fail("invalid_request", "Nonfinite JSON number"))
            if not isinstance(request, dict):
                fail("invalid_request", "Request must be a JSON object")
            SketchJournal.request(request.get("id"))
            result = dispatch(request)
            response = {"version": VERSION, "id": request["id"], "ok": True, "result": result}
        except Exception as exc:
            if not isinstance(exc, Failure):
                traceback.print_exc(file=sys.stderr)
            response = {"version": VERSION, "id": request.get("id") if isinstance(request, dict) else None,
                        "ok": False, "error": {"code": exc.code if isinstance(exc, (Failure, AssemblySources.SourceError)) else "native_error",
                                                "message": str(exc)}}
        _protocol.write(json.dumps(response, allow_nan=False, separators=(",", ":")) + "\n")
        _protocol.flush()
    for key in list(App.listDocuments()):
        App.closeDocument(key)


# Source scratch/autoload lifetimes share the same guarded native Close and
# saved-name observer ordering as writable roots and failed-Open recovery.
AssemblySources.close_native = close_loaded_document
AssemblySources.restore_active = restore_active_document
AssemblySources.guard_existing = check_dependencies


if __name__ == "__main__":
    main()
