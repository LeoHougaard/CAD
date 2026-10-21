"""Persistent native FreeCAD worker. Launch with FreeCADCmd, not system Python."""

import json
import math
import os
from pathlib import Path
import re
import sys
import traceback

# FreeCAD and its extensions may write directly to fd 1. Keep only our dedicated
# descriptor for responses; redirect both Python and native diagnostics to stderr.
_protocol = os.fdopen(os.dup(sys.stdout.fileno()), "w", encoding="utf-8", buffering=1)
os.dup2(sys.stderr.fileno(), sys.stdout.fileno())

import FreeCAD as App
import Part
import Sketcher
import PartDesign  # Register the native feature classes in headless mode.


VERSION = 1
OPERATIONS = ["capabilities", "new", "open", "inspect", "transaction", "undo",
              "redo", "save", "mesh", "topology", "close", "preview",
              "preview_commit", "preview_cancel", "sketch_begin", "sketch_finish", "sketch_cancel"]
EDIT_OPERATIONS = ["add_object", "set", "expression", "geometry", "constraint",
                   "datum", "move_geometry", "construction", "delete_geometry", "delete_constraint",
                   "constraint_state", "trim_geometry"]
OBJECT_TYPES = {
    "App::Part", "PartDesign::Body", "Sketcher::SketchObject",
    "PartDesign::Pad", "PartDesign::Pocket", "PartDesign::Fillet",
    "PartDesign::Chamfer", "PartDesign::Revolution", "PartDesign::Groove",
    "PartDesign::AdditiveLoft", "PartDesign::SubtractiveLoft",
    "PartDesign::AdditivePipe", "PartDesign::SubtractivePipe",
    "PartDesign::Plane", "PartDesign::Line", "PartDesign::Point",
    "Part::Box", "Part::Cylinder", "Part::Sphere", "Part::Cone",
    "Part::Torus", "Part::Cut", "Part::Fuse", "Part::Common", "Part::MultiFuse",
    "Part::Fillet", "Part::Chamfer", "Part::Extrusion", "Part::Revolution",
}
_revisions = {}
_poisoned = set()
_previews = {}
_sketch_sessions = {}
_clock = 0


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
    return App.Vector(*(number(v) for v in value))


def sketch_index(value, count):
    if type(value) is not int or not 0 <= value < count:
        fail("invalid_request", "Native sketch index is outside the current list")
    return value


def bump(doc):
    global _clock
    _clock += 1
    _revisions[doc.Name] = _clock


def document(request, writable=False):
    key = request.get("document")
    if key not in _revisions:
        fail("not_found", "Document is not open in this worker")
    doc = App.getDocument(key)
    if writable:
        if key in _poisoned and request.get("op") != "close":
            fail("document_unavailable", "Rollback failed; close and reopen this document")
        expected = request.get("expected_revision")
        if expected is None:
            fail("missing_revision", "Document mutations require expected_revision")
        if type(expected) is not int or expected != _revisions[key]:
            fail("stale_revision", "Document revision changed")
        if key in _previews and not request.get("op", "").startswith("preview") and not (
                key in _poisoned and request.get("op") == "close") and not (
                key in _sketch_sessions and request.get("op") == "sketch_cancel"):
            fail("preview_active", "Finish or cancel the active preview before another write")
        session = _sketch_sessions.get(key)
        if session and not (key in _poisoned and request.get("op") == "close"):
            op = request.get("op")
            if op not in {"transaction", "undo", "redo", "preview", "preview_commit", "preview_cancel",
                          "sketch_finish", "sketch_cancel"}:
                fail("sketch_active", "Finish or cancel sketch editing before another document write")
            token = request.get("token") if op in {"sketch_finish", "sketch_cancel"} else request.get("sketch_token")
            if token != session["token"]:
                fail("sketch_owner", "Sketch editing belongs to another session")
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


def decode_property(doc, type_id, value):
    if type_id in ("App::PropertyLink", "App::PropertyLinkChild", "App::PropertyLinkGlobal"):
        return None if value is None else object_in(doc, value["link"])
    if type_id in ("App::PropertyLinkSub", "App::PropertyLinkSubChild", "App::PropertyLinkSubGlobal"):
        if value is None:
            return None
        return (object_in(doc, value["link"]), value.get("subelements", []))
    if type_id in ("App::PropertyLinkList", "App::PropertyLinkListChild", "App::PropertyLinkListGlobal"):
        return [object_in(doc, item["link"]) for item in value]
    if type_id in ("App::PropertyLinkSubList", "App::PropertyLinkSubListChild", "App::PropertyLinkSubListGlobal"):
        values = value if isinstance(value, list) else [value]
        return [(object_in(doc, item["link"]), item.get("subelements", [])) for item in values]
    if type_id in ("App::PropertyVector", "App::PropertyVectorDistance", "App::PropertyPosition"):
        return vector(value)
    if type_id == "App::PropertyPlacement":
        rotation = value.get("rotation", [0, 0, 0, 1])
        if not isinstance(rotation, list) or len(rotation) != 4:
            fail("invalid_request", "Placement rotation requires quaternion [x,y,z,w]")
        return App.Placement(vector(value["position"]), App.Rotation(*(number(v) for v in rotation)))
    scalars = {"String", "Bool", "Integer", "IntegerConstraint", "Float", "FloatConstraint",
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
    if type_id.startswith("App::PropertyLinkSub"):
        def sublink(item):
            return None if item[0] is None else {"link": item[0].Name, "subelements": list(item[1])}
        if value is None:
            return None
        if "List" in type_id:
            return [sublink(item) for item in value]
        return sublink(value)
    return encode(value)


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
    return {"coordinate_frame": "native_shape", "bounds": bounds(shape), "faces": faces, "edges": edges}


def world_from_shape(obj):
    if hasattr(obj, "getGlobalPlacement") and hasattr(obj, "Placement"):
        return encode(obj.getGlobalPlacement().multiply(obj.Placement.inverse()))
    return encode(App.Placement())


def oriented_normal(face, u, v):
    normal = face.normalAt(u, v)
    length = normal.Length
    if not math.isfinite(length) or length < 1e-12:
        fail("display_error", "Native face returned an undefined display normal")
    return [normal.x / length, normal.y / length, normal.z / length]


def sketch_geometry(obj):
    result = []
    for index, geo in enumerate(obj.Geometry):
        entry = {"index": index, "geometry_type": geo.TypeId, "kind": "unsupported",
                 "construction": obj.getConstruction(index)}
        if isinstance(geo, Part.LineSegment):
            entry.update(kind="line", start=encode(geo.StartPoint), end=encode(geo.EndPoint))
        elif isinstance(geo, (Part.Circle, Part.ArcOfCircle)):
            entry.update(center=encode(geo.Center), radius=geo.Radius, axis=encode(geo.Axis))
            if isinstance(geo, Part.ArcOfCircle):
                entry.update(kind="arc", start=encode(geo.StartPoint), end=encode(geo.EndPoint),
                             mid=encode(geo.value((geo.FirstParameter + geo.LastParameter) / 2)))
            else:
                entry["kind"] = "circle"
        elif isinstance(geo, Part.Point):
            entry.update(kind="point", position=[geo.X, geo.Y, geo.Z])
        result.append(entry)
    return result


def preview_metadata(doc):
    active = _previews.get(doc.Name)
    return {"token": active["token"], "label": active["label"]} if active else None


def sketch_metadata(doc):
    session = _sketch_sessions.get(doc.Name)
    return {"token": session["token"], "object": session["object"]} if session else None


def history_counts(doc):
    session = _sketch_sessions.get(doc.Name)
    if session:
        return {"undo_count": session["cursor"], "redo_count": len(session["groups"]) - session["cursor"]}
    return {"undo_count": doc.UndoCount, "redo_count": doc.RedoCount}


def snapshot(doc, revision=None):
    objects = []
    for obj in doc.Objects:
        entry = {"name": obj.Name, "type": obj.TypeId, "label": obj.Label,
                 "valid": obj.isValid(), "status": obj.getStatusString(),
                 "state": list(obj.State), "properties": {}, "property_types": {}}
        if hasattr(obj, "getGlobalPlacement"):
            entry["global_placement"] = encode(obj.getGlobalPlacement())
        for prop in obj.PropertiesList:
            try:
                type_id = obj.getTypeIdOfProperty(prop)
                entry["properties"][prop] = encode_property(type_id, getattr(obj, prop))
                entry["property_types"][prop] = type_id
            except (TypeError, AttributeError):
                pass
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
                               "degrees_of_freedom": obj.DoF,
                               "conflicting_constraints": list(obj.ConflictingConstraints),
                               "redundant_constraints": list(obj.RedundantConstraints),
                               "geometry": sketch_geometry(obj),
                               "constraints": [{"index": i, "type": c.Type, "first": c.First,
                                                "first_pos": c.FirstPos, "second": c.Second,
                                                "second_pos": c.SecondPos, "third": c.Third,
                                                "third_pos": c.ThirdPos, "value": c.Value,
                                                "name": c.Name, "driving": c.Driving,
                                                "virtual": c.InVirtualSpace, "active": c.IsActive}
                                               for i, c in enumerate(obj.Constraints)]}
        objects.append(entry)
    return {"document": doc.Name, "revision": _revisions[doc.Name] if revision is None else revision, "objects": objects,
            **history_counts(doc), "sketch_edit": sketch_metadata(doc),
            "available": doc.Name not in _poisoned, "preview": preview_metadata(doc)}


def prepare_state(doc):
    state = snapshot(doc, _revisions.get(doc.Name, 0))
    # Serialization is part of accepting a mutation, before it becomes durable
    # in the native undo history or visible under a new revision.
    json.dumps(state, allow_nan=False)
    return state


def publish_state(doc, state):
    bump(doc)
    state.update(revision=_revisions[doc.Name], **history_counts(doc))
    return state


def validate(doc, whole=False):
    session = _sketch_sessions.get(doc.Name)
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
        if obj.TypeId in OBJECT_TYPES and obj.TypeId not in {
            "App::Part", "PartDesign::Body", "Sketcher::SketchObject",
            "PartDesign::Plane", "PartDesign::Line", "PartDesign::Point",
        } and hasattr(obj, "Shape") and obj.Shape.isNull():
            fail("invalid_model", obj.Name + ": native result is null")


def apply_edit(doc, edit):
    op = edit.get("op", edit.get("kind"))
    if op == "add_object":
        type_id, key = edit["type"], name(edit["name"])
        if type_id not in OBJECT_TYPES:
            fail("unsupported_object_type", "Unsupported native object type: " + str(type_id))
        if doc.getObject(key):
            fail("name_conflict", "Object name already exists: " + key)
        if edit.get("body"):
            body = object_in(doc, edit["body"])
            if not body.isDerivedFrom("PartDesign::Body"):
                fail("invalid_request", "Owner must be a PartDesign Body")
            obj = body.newObject(type_id, key)
        else:
            obj = doc.addObject(type_id, key)
        if obj.Name != key:
            fail("invalid_request", "FreeCAD changed the requested object name")
        return {"object": obj.Name}
    obj = object_in(doc, edit["object"])
    if op == "set":
        prop = edit["property"]
        type_id = property_type(obj, prop)
        setattr(obj, prop, decode_property(doc, type_id, edit["value"]))
    elif op == "expression":
        prop = edit["property"]
        # Native expression paths can address components, e.g. Placement.Base.x.
        property_type(obj, prop.split(".")[0].split("[")[0])
        expression = edit["expression"]
        if expression is not None and not isinstance(expression, str):
            fail("invalid_request", "Expression must be a string or null")
        obj.setExpression(prop, expression)
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
            else:
                fail("unsupported_geometry", "Unsupported sketch geometry: " + str(kind))
            index = obj.addGeometry(geo, bool(edit.get("construction", False)))
        else:
            args = edit.get("args", [])
            if not isinstance(args, list) or not all(isinstance(v, (str, int, float)) and not isinstance(v, bool) for v in args):
                fail("invalid_request", "Constraint args must be native string/number arguments")
            index = obj.addConstraint(Sketcher.Constraint(edit["kind"], *args))
        if index < 0:
            fail("invalid_model", "Native sketch edit failed")
        return {"object": obj.Name, "index": index}
    elif op in ("datum", "move_geometry", "construction", "delete_geometry", "delete_constraint",
                "constraint_state", "trim_geometry"):
        if not obj.isDerivedFrom("Sketcher::SketchObject"):
            fail("invalid_request", "This edit requires a native SketchObject")
        constraint = op in ("datum", "delete_constraint", "constraint_state")
        index = sketch_index(edit["index"], obj.ConstraintCount if constraint else obj.GeometryCount)
        if op == "datum":
            value = edit["value"]
            if not isinstance(value, str):
                number(value)
            quantity = App.Units.Quantity(value)
            number(quantity.Value)
            if isinstance(value, str) and quantity.Unit != obj.getDatum(index).Unit:
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
    if doc.Name in _sketch_sessions:
        return sketch_transaction(doc, request)
    doc.openTransaction(request.get("label", "Edit"))
    try:
        results = [apply_edit(doc, edit) for edit in edits]
        if doc.recompute() is False:
            fail("invalid_model", "Native recompute failed")
        validate(doc)
        state = prepare_state(doc)
        json.dumps(results, allow_nan=False)
        doc.commitTransaction()
    except Exception:
        try:
            doc.abortTransaction()
            doc.recompute()
            validate(doc)
        except Exception:
            _poisoned.add(doc.Name)
            traceback.print_exc(file=sys.stderr)
        # Abort restores model edits, but native element aliases and redo
        # history may have changed. Never reuse identities from before it.
        bump(doc)
        raise
    publish_state(doc, state)
    return {"document": doc.Name, "revision": _revisions[doc.Name],
            "edits": results, "state": state}


def sketch_edits(session, edits):
    if not isinstance(edits, list) or not edits:
        fail("invalid_request", "Sketch editing requires a nonempty edits list")
    for edit in edits:
        if not isinstance(edit, dict) or edit.get("object") != session["object"] or edit.get("op") not in {
            "set", "expression", "geometry", "constraint", "datum", "move_geometry", "construction",
            "delete_geometry", "delete_constraint", "constraint_state", "trim_geometry",
        }:
            fail("sketch_active", "Only edits to the active sketch are allowed during sketch editing")


def solve_sketch(doc, session):
    sketch = object_in(doc, session["object"])
    if sketch.solve() != 0 or sketch.recompute() is False:
        fail("invalid_model", "Native sketch solver rejected the edit")
    validate(doc)


def replay_sketch(doc, session, candidate=None):
    if session["opened"]:
        doc.abortTransaction()
        session["opened"] = False
    groups = session["groups"][:session["cursor"]]
    if groups or candidate:
        doc.openTransaction(session["label"])
        session["opened"] = True
        for group in groups:
            for edit in group["edits"]:
                apply_edit(doc, edit)
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
        _poisoned.add(doc.Name)
        traceback.print_exc(file=sys.stderr)
    bump(doc)


def sketch_transaction(doc, request):
    session = _sketch_sessions[doc.Name]
    edits = request["edits"]
    sketch_edits(session, edits)
    old_groups, old_cursor = session["groups"], session["cursor"]
    try:
        if not session["opened"]:
            doc.openTransaction(session["label"])
            session["opened"] = True
        results = [apply_edit(doc, edit) for edit in edits]
        solve_sketch(doc, session)
        session["groups"] = old_groups[:old_cursor] + [{"label": request.get("label", "Edit sketch"), "edits": edits}]
        session["cursor"] = len(session["groups"])
        state = prepare_state(doc)
        json.dumps(results, allow_nan=False)
    except Exception:
        session["groups"], session["cursor"] = old_groups, old_cursor
        restore_sketch(doc, session)
        raise
    publish_state(doc, state)
    return {"document": doc.Name, "revision": _revisions[doc.Name], "edits": results, "state": state}


def sketch_history(doc, op):
    session = _sketch_sessions[doc.Name]
    cursor = session["cursor"]
    if (op == "undo" and cursor == 0) or (op == "redo" and cursor == len(session["groups"])):
        fail("nothing_to_" + op, "No sketch action to " + op)
    session["cursor"] += -1 if op == "undo" else 1
    try:
        replay_sketch(doc, session)
        state = prepare_state(doc)
    except Exception:
        session["cursor"] = cursor
        restore_sketch(doc, session)
        raise
    return publish_state(doc, state)


def sketch_session(doc, request):
    token = request.get("token")
    if not isinstance(token, str) or not token or len(token) > 256:
        fail("invalid_request", "Sketch token requires a nonempty string of at most 256 characters")
    if request["op"] == "sketch_begin":
        if doc.Name in _sketch_sessions:
            fail("sketch_active", "Document already has an active sketch editing session")
        obj = object_in(doc, request["object"])
        if not obj.isDerivedFrom("Sketcher::SketchObject"):
            fail("invalid_request", "Sketch editing requires a native SketchObject")
        validate(doc, whole=True)
        _sketch_sessions[doc.Name] = {"token": token, "object": obj.Name, "label": "Edit " + obj.Label,
                                     "groups": [], "cursor": 0, "opened": False,
                                     "context_revision": _revisions[doc.Name]}
        return publish_state(doc, prepare_state(doc))
    session = _sketch_sessions.get(doc.Name)
    if session is None:
        fail("no_sketch_edit", "Document has no active sketch editing session")
    if request["op"] == "sketch_cancel":
        try:
            if session["opened"]:
                doc.abortTransaction()
                session["opened"] = False
            doc.recompute()
            validate(doc, whole=True)
        except Exception:
            _poisoned.add(doc.Name)
            bump(doc)
            raise
    else:
        try:
            if doc.recompute() is False:
                fail("invalid_model", "Native finish recompute failed")
            validate(doc, whole=True)
            state = prepare_state(doc)
            json.dumps(state, allow_nan=False)
            if session["opened"]:
                doc.commitTransaction()
        except Exception:
            restore_sketch(doc, session)
            raise
    del _sketch_sessions[doc.Name]
    _previews.pop(doc.Name, None)
    return publish_state(doc, prepare_state(doc))


def sketch_preview(doc, request, token, active):
    session = _sketch_sessions[doc.Name]
    op = request["op"]
    if op != "preview":
        if active is None:
            fail("no_preview", "Document has no active preview")
        if op == "preview_commit":
            session["groups"] = session["groups"][:session["cursor"]] + [active]
            session["cursor"] = len(session["groups"])
        else:
            try:
                replay_sketch(doc, session)
            except Exception:
                _poisoned.add(doc.Name)
                bump(doc)
                raise
        del _previews[doc.Name]
        return publish_state(doc, prepare_state(doc))
    edits = request.get("edits")
    sketch_edits(session, edits)
    label = request.get("label", active["label"] if active else "Edit sketch")
    if not isinstance(label, str):
        fail("invalid_request", "Preview label must be a string")
    if not session["opened"] and doc.RedoCount:
        fail("preview_redo_conflict", "Native redo history is present; commit an edit to branch history")
    candidate = {"token": token, "label": label, "edits": edits}
    try:
        results = replay_sketch(doc, session, candidate)
        _previews[doc.Name] = candidate
        state = prepare_state(doc)
        json.dumps(results, allow_nan=False)
    except Exception:
        if active is None:
            _previews.pop(doc.Name, None)
        else:
            _previews[doc.Name] = active
        restore_sketch(doc, session, active)
        raise
    publish_state(doc, state)
    return {"document": doc.Name, "revision": _revisions[doc.Name], "edits": results, "state": state}


def preview(doc, request):
    token = request.get("token")
    if not isinstance(token, str) or not token or len(token) > 256:
        fail("invalid_request", "Preview token requires a nonempty string of at most 256 characters")
    active = _previews.get(doc.Name)
    if active and active["token"] != token:
        fail("preview_owner", "Preview belongs to another gesture")
    if doc.Name in _sketch_sessions:
        return sketch_preview(doc, request, token, active)
    op = request["op"]
    if op != "preview":
        if active is None:
            fail("no_preview", "Document has no active preview")
        # Build and serialize the response before committing native history.
        if op == "preview_commit":
            state = prepare_state(doc)
            state["preview"] = None
            try:
                doc.commitTransaction()
            except Exception:
                _poisoned.add(doc.Name)
                bump(doc)
                raise
        else:
            try:
                doc.abortTransaction()
                doc.recompute()
                validate(doc)
                state = prepare_state(doc)
                state["preview"] = None
            except Exception:
                _poisoned.add(doc.Name)
                _previews.pop(doc.Name, None)
                bump(doc)
                raise
        del _previews[doc.Name]
        return publish_state(doc, state)
    edits = request.get("edits")
    if not isinstance(edits, list) or not edits:
        fail("invalid_request", "Preview requires a nonempty edits list")
    label = request.get("label", active["label"] if active else "Edit")
    if not isinstance(label, str):
        fail("invalid_request", "Preview label must be a string")
    # FreeCAD clears redo as soon as an actual transaction starts, including
    # transactions later aborted. Refuse before mutation rather than erase
    # history merely by opening a dialog or dragging and pressing Escape.
    if active is None and doc.RedoCount:
        fail("preview_redo_conflict", "Native redo history is present; use a committed edit to branch history")
    if active:
        try:
            doc.abortTransaction()
            doc.recompute()
            validate(doc)
        except Exception:
            _poisoned.add(doc.Name)
            bump(doc)
            raise
    doc.openTransaction(label)
    try:
        results = [apply_edit(doc, edit) for edit in edits]
        if doc.recompute() is False:
            fail("invalid_model", "Native recompute failed")
        validate(doc)
        state = prepare_state(doc)
        state["preview"] = {"token": token, "label": label}
        json.dumps(results, allow_nan=False)
    except Exception:
        try:
            doc.abortTransaction()
            doc.recompute()
            if active:
                doc.openTransaction(active["label"])
                for edit in active["edits"]:
                    apply_edit(doc, edit)
                doc.recompute()
            validate(doc)
        except Exception:
            _poisoned.add(doc.Name)
            traceback.print_exc(file=sys.stderr)
        # Abort/replay can change native element aliases even for an initial
        # candidate that failed. Callers must re-inspect before another write.
        bump(doc)
        raise
    _previews[doc.Name] = {"token": token, "label": label, "edits": edits}
    publish_state(doc, state)
    return {"document": doc.Name, "revision": _revisions[doc.Name],
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
                "object_types": sorted(OBJECT_TYPES), "geometry_kinds": ["line", "circle", "arc", "point"],
                "edit_operations": EDIT_OPERATIONS}
    # Native transactions are coordinated by FreeCAD's application, including
    # across documents. Another document's write can commit the open gesture.
    # Keep a worker-wide write lease until its owner commits or cancels.
    if _sketch_sessions and op in {"new", "open", "transaction", "undo", "redo", "save", "close",
                                  "preview", "preview_commit", "preview_cancel",
                                  "sketch_begin", "sketch_finish", "sketch_cancel"}:
        if op in {"new", "open"} or request.get("document") not in _sketch_sessions:
            fail("sketch_active", "Finish or cancel sketch editing before another document write")
    if _previews and op in {"new", "open", "transaction", "undo", "redo", "save", "close",
                            "preview", "preview_commit", "preview_cancel", "sketch_begin", "sketch_finish", "sketch_cancel"}:
        if op in {"new", "open"} or request.get("document") not in _previews:
            fail("preview_active", "Finish or cancel the active preview before another document write")
    if op in ("new", "open"):
        if op == "new":
            key = name(request["name"])
            if key in App.listDocuments():
                fail("name_conflict", "Document name already exists")
            doc = App.newDocument(key)
        else:
            path = native_path(request["path"])
            if not Path(path).is_file():
                fail("not_found", "Native document does not exist")
            if any(os.path.normcase(d.FileName) == os.path.normcase(path) for d in App.listDocuments().values()):
                fail("already_open", "Native document is already open")
            doc = App.openDocument(path)
        try:
            doc.UndoMode = 1
            doc.recompute()
            # Untouched restored sketches can retain the Python DoF default 0
            # until their solver runs, even when they are underconstrained.
            # Initialize native freedom/conflict state before publishing it.
            for obj in doc.Objects:
                if obj.isDerivedFrom("Sketcher::SketchObject") and obj.solve() != 0:
                    fail("invalid_model", obj.Name + ": native sketch solver rejected restored constraints")
            doc.recompute()
            validate(doc)
            state = prepare_state(doc)
        except Exception:
            App.closeDocument(doc.Name)
            raise
        return publish_state(doc, state)
    if op not in OPERATIONS:
        fail("unsupported_operation", "Unsupported operation: " + str(op))
    doc = document(request, writable=op in ("transaction", "undo", "redo", "save", "close",
                                           "preview", "preview_commit", "preview_cancel",
                                           "sketch_begin", "sketch_finish", "sketch_cancel"))
    if op == "inspect":
        return snapshot(doc)
    if op == "transaction":
        return transaction(doc, request)
    if op in ("sketch_begin", "sketch_finish", "sketch_cancel"):
        return sketch_session(doc, request)
    if op in ("preview", "preview_commit", "preview_cancel"):
        return preview(doc, request)
    if op in ("undo", "redo"):
        if doc.Name in _sketch_sessions:
            return sketch_history(doc, op)
        if getattr(doc, "UndoCount" if op == "undo" else "RedoCount") == 0:
            fail("nothing_to_" + op, "No native transaction to " + op)
        getattr(doc, op)()
        try:
            doc.recompute()
            validate(doc)
            state = prepare_state(doc)
        except Exception:
            try:
                getattr(doc, "redo" if op == "undo" else "undo")()
                doc.recompute()
                validate(doc)
            except Exception:
                _poisoned.add(doc.Name)
                traceback.print_exc(file=sys.stderr)
            raise
        return publish_state(doc, state)
    if op == "save":
        validate(doc)
        path = native_path(request["path"])
        doc.saveAs(path)
        bump(doc)
        return {"document": doc.Name, "revision": _revisions[doc.Name], "path": path}
    if op == "close":
        key = doc.Name
        App.closeDocument(key)
        del _revisions[key]
        _poisoned.discard(key)
        _previews.pop(key, None)
        _sketch_sessions.pop(key, None)
        return {"document": key, "closed": True}
    if op in ("mesh", "topology"):
        if doc.Name in _poisoned:
            fail("document_unavailable", "Rollback failed; close and reopen this document")
        validate(doc)
        obj = object_in(doc, request["object"])
        if not hasattr(obj, "Shape") or obj.Shape.isNull():
            fail("no_shape", "Object has no native display shape")
        deflection = number(request.get("deflection", 0.1))
        if deflection <= 0:
            fail("invalid_request", "Deflection must be positive")
        if op == "topology":
            return {"document": doc.Name, "revision": _revisions[doc.Name], "object": obj.Name,
                    "preview": preview_metadata(doc),
                    "sketch_edit": sketch_metadata(doc),
                    "context_revision": _sketch_sessions[doc.Name]["context_revision"] if doc.Name in _sketch_sessions and obj.Name != _sketch_sessions[doc.Name]["object"] else _revisions[doc.Name],
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
        return {"document": doc.Name, "revision": _revisions[doc.Name], "object": obj.Name,
                "preview": preview_metadata(doc),
                "sketch_edit": sketch_metadata(doc),
                "context_revision": _sketch_sessions[doc.Name]["context_revision"] if doc.Name in _sketch_sessions and obj.Name != _sketch_sessions[doc.Name]["object"] else _revisions[doc.Name],
                "world_from_shape": world_from_shape(obj),
                "positions": positions, "normals": normals, "triangles": triangles, "faces": faces}


def main():
    for line in sys.stdin:
        request = None
        try:
            request = json.loads(line, parse_constant=lambda value: fail("invalid_request", "Nonfinite JSON number"))
            if not isinstance(request, dict):
                fail("invalid_request", "Request must be a JSON object")
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
    for key in list(_revisions):
        App.closeDocument(key)


if __name__ == "__main__":
    main()
