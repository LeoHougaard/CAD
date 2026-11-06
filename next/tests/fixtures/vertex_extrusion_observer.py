"""Native lifecycle test fixture. Never used by the production launcher."""
import json
import os
import sys
import io
import zipfile
import xml.etree.ElementTree as ET
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "backend"))
import freecad_worker as worker
import Part

dispatch = worker.dispatch
native_snapshot = worker.snapshot


def snapshot_with_native_xml(doc, revision=None):
    state = native_snapshot(doc, revision)
    # Test-only native inspection. Production snapshots are not an XML oracle.
    for entry in state["objects"]:
        native = doc.getObject(entry["name"])
        entry["test_native_id"] = native.ID
        if any(prop in native.PropertiesList for prop in worker.VertexExtrusion.OWNER_TAGS):
            tags = {}
            for prop in worker.VertexExtrusion.OWNER_TAGS:
                if prop not in native.PropertiesList:
                    continue
                with zipfile.ZipFile(io.BytesIO(native.dumpPropertyContent(prop))) as archive:
                    tags[prop] = {"value": getattr(native, prop), "status": list(native.getPropertyStatus(prop)),
                                  "xml": archive.read("Persistence.xml").decode("utf-8")}
            entry["test_owner_tags"] = {"document_uid":str(doc.Uid), "tags":tags}
            if Path(doc.FileName).name == "vertex-new.FCStd":
                with zipfile.ZipFile(doc.FileName) as saved:
                    root = ET.fromstring(saved.read("Document.xml"))
                objects = [o for o in root.iter("Object") if o.get("name") == native.Name
                           and o.find("Properties") is not None]
                if len(objects) == 1:
                    entry["test_saved_tag_xml"] = {prop:ET.tostring(p, encoding="unicode")
                        for p in objects[0].find("Properties") if (prop := p.get("name")) in worker.VertexExtrusion.OWNER_TAGS}
        if native.TypeId == "PartDesign::Plane":
            with zipfile.ZipFile(io.BytesIO(native.dumpPropertyContent("AttachmentSupport"))) as archive:
                entry["test_support_xml"] = archive.read("Persistence.xml").decode("utf-8")
        if native.TypeId == "PartDesign::SubShapeBinder":
            with zipfile.ZipFile(io.BytesIO(native.dumpPropertyContent("Support"))) as archive:
                entry["test_support_xml"] = archive.read("Persistence.xml").decode("utf-8")
        if native.TypeId == "PartDesign::Pad" and worker.VertexExtrusion.owned_helpers(native, worker.owner_body):
            metadata = entry.get("vertex_extrusion") or {}
            source = doc.getObject(metadata.get("source", ""))
            original_body = worker.owner_body(source) if source else None
            lower = original_body.Tip if original_body else None
            if lower and not native.Shape.isNull() and not lower.Shape.isNull():
                common = native.Shape.common(lower.Shape).Volume
                cut = native.Shape.cut(lower.Shape).Volume
                entry["test_vertex_csg"] = {"common": common, "common_repr": repr(common),
                    "cut": cut, "cut_repr": repr(cut), "pad": native.Shape.Volume,
                    "pad_repr": repr(native.Shape.Volume), "lower": lower.Shape.Volume,
                    "lower_repr": repr(lower.Shape.Volume), "original_body": original_body.Name,
                    "original_body_id": original_body.ID, "original_tip": lower.Name, "original_tip_id": lower.ID}
            target = doc.getObject(metadata.get("target", ""))
            plane = doc.getObject(metadata.get("plane", ""))
            vertex = metadata.get("vertex")
            if target and plane and isinstance(vertex, str) and not native.Shape.isNull():
                point = target.Shape.getElement(vertex).Point
                target_parent = target.getGlobalPlacement().multiply(target.Placement.inverse())
                world = target_parent.multVec(point)
                plane_placement = plane.getGlobalPlacement()
                normal = plane_placement.Rotation.multVec(worker.App.Vector(0, 0, 1))
                source_placement = source.getGlobalPlacement()
                source_normal = source_placement.Rotation.multVec(worker.App.Vector(0, 0, 1))
                parent = native.getGlobalPlacement().multiply(native.Placement.inverse())
                vertices = [parent.multVec(v.Point) for v in native.Shape.Vertexes]
                entry["test_vertex_geometry"] = {"target_world": [world.x, world.y, world.z],
                    "plane_origin": [plane_placement.Base.x, plane_placement.Base.y, plane_placement.Base.z],
                    "plane_normal": [normal.x, normal.y, normal.z],
                    "target_plane_distance": (world-plane_placement.Base).dot(normal),
                    "source_plane_origin": [source_placement.Base.x, source_placement.Base.y, source_placement.Base.z],
                    "source_plane_normal": [source_normal.x, source_normal.y, source_normal.z],
                    "target_source_distance": (world-source_placement.Base).dot(source_normal),
                    "vertices": [[p.x,p.y,p.z] for p in vertices],
                    "vertex_plane_distances": [(p-plane_placement.Base).dot(normal) for p in vertices],
                    "vertex_source_distances": [(p-source_placement.Base).dot(source_normal) for p in vertices]}
            for prop in ("Profile", "UpToFace"):
                with zipfile.ZipFile(io.BytesIO(native.dumpPropertyContent(prop))) as archive:
                    entry["test_" + prop + "_xml"] = archive.read("Persistence.xml").decode("utf-8")
        if native.TypeId == "PartDesign::Revolution" and not native.Shape.isNull():
            entry["test_axis"] = {"base": [native.Base.x, native.Base.y, native.Base.z],
                                  "direction": [native.Axis.x, native.Axis.y, native.Axis.z]}
            points = [(0,0),(-40,0),(-40,62),(-27.5,74.5),(-27.5,100),(0,100),(0,0)]
            wire = Part.makePolygon([worker.App.Vector(x,y,0) for x,y in points])
            face = Part.Face(wire)
            wrong = face.revolve(worker.App.Vector(10,0,0), worker.App.Vector(0,1,0), 360)
            entry["test_csg"] = {
                "wrong_delta": native.Shape.cut(wrong).Volume + wrong.cut(native.Shape).Volume,
            }
        if "sketch" in entry:
            sketch = doc.getObject(entry["name"])
            entry["sketch"]["native_xml"] = {
                "geometry": [g.Content for g in sketch.Geometry],
                "constraints": [c.Content for c in sketch.Constraints],
                "expressions": list(sketch.ExpressionEngine), "dof": sketch.DoF,
                "persistent_ids": [sketch.getGeometryId(i) for i in range(sketch.GeometryCount)],
                "element_map": {kind + str(i): sketch.Shape.getElementMappedName(kind + str(i))
                    for kind, count in (("Edge", len(sketch.Shape.Edges)), ("Vertex", len(sketch.Shape.Vertexes)))
                    for i in range(1, count + 1)}}
            for prop in ("Geometry", "Constraints", "ExpressionEngine"):
                with zipfile.ZipFile(io.BytesIO(sketch.dumpPropertyContent(prop))) as archive:
                    entry["sketch"]["native_xml"][prop] = archive.read("Persistence.xml").decode("utf-8")
    return state


worker.snapshot = snapshot_with_native_xml
validate = worker.validate
close_document = worker.App.closeDocument
remaining = 0
fail_cleanup_undo = False
fail_close = False
trace = Path(os.environ["CAD_LIFECYCLE_OUTPUT"]) / ("lifecycle-worker-%s.jsonl" % os.getpid())


def validate_fault(*args, **kwargs):
    global remaining
    result = validate(*args, **kwargs)
    if remaining:
        remaining -= 1
        raise RuntimeError("lifecycle fixture: validation failure after native validation")
    return result


def close_fault(name):
    global fail_close
    if fail_close:
        fail_close = False
        raise RuntimeError("lifecycle fixture: native Close failed once")
    return close_document(name)


def dispatch_fault(request):
    global remaining, fail_cleanup_undo, fail_close
    with trace.open("a", encoding="utf-8") as output:
        output.write(json.dumps(request) + "\n")
    fault = request.pop("lifecycle_fault", None)
    remaining = {"validate_once": 1, "validate_twice": 2}.get(fault, 0)
    if request.pop("lifecycle_fail_cleanup_undo", False):
        fail_cleanup_undo = True
    if request.pop("lifecycle_fail_close_once", False):
        fail_close = True
    if request.get("op") == "undo" and fail_cleanup_undo:
        fail_cleanup_undo = False
        remaining = 1
    try:
        return dispatch(request)
    finally:
        remaining = 0


worker.validate = validate_fault
worker.App.closeDocument = close_fault
worker.dispatch = dispatch_fault
worker.main()
