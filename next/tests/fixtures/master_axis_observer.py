"""Native lifecycle test fixture. Never used by the production launcher."""
import json
import os
import sys
import io
import zipfile
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
