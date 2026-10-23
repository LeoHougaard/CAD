"""Native lifecycle test fixture. Never used by the production launcher."""
import json
import os
import sys
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "backend"))
import freecad_worker as worker

dispatch = worker.dispatch
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
