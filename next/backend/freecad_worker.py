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
EDIT_OPERATIONS = ["add_object", "set", "expression", "geometry", "constraint",
                   "datum", "move_geometry", "construction", "delete_geometry", "delete_constraint", "refraction", "via_point",
                   "constraint_state", "trim_geometry", "remove_object", "binder_support",
                   "expose_internal", "external_geometry", "external_state", "delete_external", "import_exchange", "detach_sketch"]
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

def document(request, writable=False):
    key = request.get("document")
    hash(key)  # Preserve native request errors for unhashable document identifiers.
    if revision_request("lookup", key) is None:
        fail("not_found", "Document is not open in this worker")
    doc = App.getDocument(key)
    if writable:
        lease("pre_revision", key, request=request)
        revision_request("check", key, expected_revision=request.get("expected_revision"))
        lease("local", key, request=request)
        if request.get("op") not in ("close", "preview_cancel", "sketch_cancel"):
            validate_assembly_proxies(doc)
        # FreeCAD 1.1 transactions are app-wide. A change to an inactive
        # document also enrolls the active document in its native history.
        # Select the owner only after the existing write guards have admitted
        # the request. Reads, export, and close retain their activation.
        if request.get("op") not in ("close", "export_exchange"):
            App.setActiveDocument(doc.Name)
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
    for type_id in sorted(OBJECT_TYPES):
        doc = App.newDocument("FeatureSchema")
        try:
            obj = doc.addObject(type_id, "Feature")
            _feature_schemas[type_id] = {"type": type_id, **property_snapshot(obj)}
            if type_id == "PartDesign::Hole":
                HoleTask.initialize(obj)
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
    return [SketchGeometry.geometry_entry(geo, index, obj.getConstruction(index), sketch=obj)
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
    validate_assembly_proxies(doc)
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
    return {"document": doc.Name, "revision": revision_request("current", doc.Name) if revision is None else revision, "objects": objects,
            "units": DocumentUnits.settings(doc),
            **history_counts(doc), "sketch_edit": sketch_metadata(doc),
            "available": lease("metadata", doc.Name)["available"], "preview": preview_metadata(doc)}


def prepare_state(doc):
    state = snapshot(doc, revision_request("peek", doc.Name))
    # Serialization is part of accepting a mutation, before it becomes durable
    # in the native undo history or visible under a new revision.
    json.dumps(state, allow_nan=False)
    return state


def publish_state(doc, state):
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
    if op == "binder_support":
        if obj.TypeId not in {"PartDesign::ShapeBinder", "PartDesign::SubShapeBinder"}:
            fail("invalid_request", "Binder support requires a native ShapeBinder or SubShapeBinder")
        # FreeCAD reserves Support from generic property editors but its native
        # binder creation/editing command assigns this property explicitly.
        obj.Support = decode_property(doc, property_type(obj, "Support"), edit["value"])
    elif op == "remove_object":
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


def transaction(doc, request):
    edits = request["edits"]
    if not isinstance(edits, list) or not edits:
        fail("invalid_request", "Transaction requires a nonempty edits list")
    preflight_constraints(edits)
    if sketch_metadata(doc):
        return sketch_transaction(doc, request)
    doc.openTransaction(request.get("label", "Edit"))
    try:
        results = [apply_edit(doc, edit) for edit in edits]
        if recompute_document(doc) is False:
            fail("invalid_model", "Native recompute failed")
        validate(doc)
        state = prepare_state(doc)
        json.dumps(results, allow_nan=False)
        doc.commitTransaction()
    except Exception:
        try:
            abort_transaction(doc)
            recompute_document(doc)
            validate(doc)
        except Exception:
            lease("poison", doc.Name)
            traceback.print_exc(file=sys.stderr)
        # Abort restores model edits, but native element aliases and redo
        # history may have changed. Never reuse identities from before it.
        bump(doc)
        raise
    publish_state(doc, state)
    return {"document": doc.Name, "revision": revision_request("current", doc.Name),
            "edits": results, "state": state}


def preflight_constraints(edits):
    for edit in edits:
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
    preflight_constraints(edits)
    label = preview_ready(doc, request)
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
        results = [apply_edit(doc, edit) for edit in edits]
        if recompute_document(doc) is False:
            fail("invalid_model", "Native recompute failed")
        validate(doc)
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
                    apply_edit(doc, edit)
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


def dispatch(request):
    if request.get("version") != VERSION:
        fail("unsupported_version", "Expected protocol version 1")
    if type(request.get("id")) is not int:
        fail("invalid_request", "Request id must be an integer")
    op = request.get("op")
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
                             "component_sources": ["local_document"], "external_file_insertion": False}}
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
    lease("global", request=request)
    if op in ("new", "open"):
        before = set(App.listDocuments())
        active = App.ActiveDocument
        try:
            if op == "new":
                key = name(request["name"])
                if key in before:
                    fail("name_conflict", "Document name already exists")
                doc = App.newDocument(key)
            else:
                path = native_path(request["path"])
                if not Path(path).is_file():
                    fail("not_found", "Native document does not exist")
                if any(os.path.normcase(d.FileName) == os.path.normcase(path) for d in App.listDocuments().values()):
                    fail("already_open", "Native document is already open")
                doc = App.openDocument(path)
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
        except Exception:
            for key in set(App.listDocuments()) - before:
                App.closeDocument(key)
                _native_suppression.detach(key)
            if active is not None and active.Name in App.listDocuments():
                App.setActiveDocument(active.Name)
            raise
        return publish_state(doc, state)
    if op not in OPERATIONS:
        fail("unsupported_operation", "Unsupported operation: " + str(op))
    doc = document(request, writable=op in ("transaction", "undo", "redo", "save", "close",
                                           "preview", "preview_commit", "preview_cancel",
                                           "sketch_begin", "sketch_finish", "sketch_cancel", "export_exchange", "document_settings"))
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
        return transaction(doc, request)
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
        validate(doc)
        path = native_path(request["path"])
        doc.saveAs(path)
        bump(doc)
        return {"document": doc.Name, "revision": revision_request("current", doc.Name), "path": path}
    if op == "close":
        key = doc.Name
        App.closeDocument(key)
        _native_suppression.detach(key)
        revision_request("close", key)
        lease("close", key)
        _native_previews.pop(key, None)
        _native_sketch_sessions.pop(key, None)
        journal(key, "end")
        return {"document": key, "closed": True}
    if op in ("mesh", "topology"):
        lease("read_guard", doc.Name)
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
                        "ok": False, "error": {"code": exc.code if isinstance(exc, Failure) else "native_error",
                                                "message": str(exc)}}
        _protocol.write(json.dumps(response, allow_nan=False, separators=(",", ":")) + "\n")
        _protocol.flush()
    for key in list(App.listDocuments()):
        App.closeDocument(key)


if __name__ == "__main__":
    main()
