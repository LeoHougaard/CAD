"""Test-only observer for public datum admission; production behavior is unchanged."""
import io
import json
import os
from pathlib import Path
import sys
import zipfile

worker_path = Path(os.environ["CAD_DATUM_WORKER"]).resolve()
sys.path.insert(0, str(worker_path.parent))
import freecad_worker as worker
assert Path(worker.__file__).resolve() == worker_path
trace = Path(os.environ["CAD_DATUM_TRACE"])
original_snapshot = worker.snapshot
original_dispatch = worker.dispatch
original_preflight = worker.preflight_rejected_datum
original_apply = worker.apply_native_edit
current = None


def log(event):
    with trace.open("a", encoding="utf8") as output:
        output.write(json.dumps(event) + "\n")


def snapshot(doc, revision=None):
    state = original_snapshot(doc, revision)
    for entry in state["objects"]:
        obj = doc.getObject(entry["name"])
        entry["observed_native_id"] = obj.ID
        if "sketch" in entry:
            native = {"ids": [obj.getGeometryId(i) for i in range(obj.GeometryCount)]}
            for prop in ("Geometry", "Constraints", "ExpressionEngine"):
                with zipfile.ZipFile(io.BytesIO(bytes(obj.dumpPropertyContent(prop)))) as archive:
                    native[prop] = archive.read("Persistence.xml").decode("utf8")
            entry["sketch"]["native_xml"] = native
    return state


def preflight(doc, session, edits):
    log({"event": "preflight_enter", "request": current, "edits": edits})
    return original_preflight(doc, session, edits)


def apply(doc, edit):
    if edit.get("op") == "datum":
        log({"event": "apply_datum_entry", "request": current, "edit": edit})
    return original_apply(doc, edit)


def dispatch(request):
    global current
    current = request.get("id")
    log({"event": "dispatch", "request": current, "op": request.get("op")})
    return original_dispatch(request)


worker.snapshot = snapshot
worker.preflight_rejected_datum = preflight
worker.apply_native_edit = apply
worker.dispatch = dispatch
worker.main()
