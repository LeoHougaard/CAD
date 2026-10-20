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
              "redo", "save", "mesh", "topology", "close"]
EDIT_OPERATIONS = ["add_object", "set", "expression", "geometry", "constraint",
                   "datum", "move_geometry", "construction", "delete_geometry", "delete_constraint"]
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


def topology(shape):
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
        else:
            endpoints = [encode(edge.valueAt(edge.FirstParameter)), encode(edge.valueAt(edge.LastParameter))]
            geometry_type = edge.Curve.TypeId
        edges.append({"subelement": key, "mapped_name": shape.getElementMappedName(key) or None,
                      "geometry_type": geometry_type, "center": encode(edge.CenterOfMass),
                      "bounds": bounds(edge), "length": edge.Length, "endpoints": endpoints,
                      "closed": edge.isClosed(), "degenerated": edge.Degenerated,
                      "orientation": edge.Orientation,
                      "faces": ["Face" + str(i) for i, face in enumerate(native_faces, 1)
                                if any(face.isSame(ancestor) for ancestor in adjacent)]})
    return {"coordinate_frame": "native_shape", "bounds": bounds(shape), "faces": faces, "edges": edges}


def world_from_shape(obj):
    if hasattr(obj, "getGlobalPlacement") and hasattr(obj, "Placement"):
        return encode(obj.getGlobalPlacement().multiply(obj.Placement.inverse()))
    return encode(App.Placement())


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
        result.append(entry)
    return result


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
            "undo_count": doc.UndoCount, "redo_count": doc.RedoCount,
            "available": doc.Name not in _poisoned}


def prepare_state(doc):
    state = snapshot(doc, _revisions.get(doc.Name, 0))
    # Serialization is part of accepting a mutation, before it becomes durable
    # in the native undo history or visible under a new revision.
    json.dumps(state, allow_nan=False)
    return state


def publish_state(doc, state):
    bump(doc)
    state.update(revision=_revisions[doc.Name], undo_count=doc.UndoCount, redo_count=doc.RedoCount)
    return state


def validate(doc):
    for obj in doc.Objects:
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
    elif op in ("datum", "move_geometry", "construction", "delete_geometry", "delete_constraint"):
        if not obj.isDerivedFrom("Sketcher::SketchObject"):
            fail("invalid_request", "This edit requires a native SketchObject")
        constraint = op in ("datum", "delete_constraint")
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
        raise
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
                "object_types": sorted(OBJECT_TYPES), "geometry_kinds": ["line", "circle", "arc"],
                "edit_operations": EDIT_OPERATIONS}
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
            validate(doc)
            state = prepare_state(doc)
        except Exception:
            App.closeDocument(doc.Name)
            raise
        return publish_state(doc, state)
    if op not in OPERATIONS:
        fail("unsupported_operation", "Unsupported operation: " + str(op))
    doc = document(request, writable=op in ("transaction", "undo", "redo", "save", "close"))
    if op == "inspect":
        return snapshot(doc)
    if op == "transaction":
        return transaction(doc, request)
    if op in ("undo", "redo"):
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
        return {"document": key, "closed": True}
    if op in ("mesh", "topology"):
        if doc.Name in _poisoned:
            fail("document_unavailable", "Rollback failed; close and reopen this document")
        validate(doc)
        obj = object_in(doc, request["object"])
        if not hasattr(obj, "Shape") or obj.Shape.isNull():
            fail("no_shape", "Object has no native display shape")
        if op == "topology":
            return {"document": doc.Name, "revision": _revisions[doc.Name], "object": obj.Name,
                    "world_from_shape": world_from_shape(obj),
                    **topology(obj.Shape)}
        deflection = number(request.get("deflection", 0.1))
        if deflection <= 0:
            fail("invalid_request", "Deflection must be positive")
        positions, triangles, faces = [], [], []
        for index, face in enumerate(obj.Shape.Faces, 1):
            points, facets = face.tessellate(deflection)
            offset, first = len(positions), len(triangles)
            positions.extend(encode(p) for p in points)
            triangles.extend([[offset + i for i in tri] for tri in facets])
            faces.append({"subelement": "Face" + str(index), "first_triangle": first,
                          "triangle_count": len(facets)})
        return {"document": doc.Name, "revision": _revisions[doc.Name], "object": obj.Name,
                "world_from_shape": world_from_shape(obj),
                "positions": positions, "triangles": triangles, "faces": faces}


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
