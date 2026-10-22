"""Inject snapshot failures through the real host and verify ownership/revisions.

The generated wrapper is test-only. Production exposes no failure-injection op.
Native transactions, solving and serialization still run before the injected fault.
"""
import argparse
import json
from pathlib import Path

from native_workflows import add, constraint, objects, set_value, transaction
from sketch_editor_workflows import SketchSession


FAULT_WORKER = '''import sys
sys.path.insert(0, BACKEND)
import freecad_worker as worker
prepare = worker.prepare_state
dispatch = worker.dispatch
remaining = 0
calls = 0
last_calls = 0

def prepare_fault(doc):
    global remaining, calls
    calls += 1
    state = prepare(doc)
    if remaining:
        remaining -= 1
        if remaining == 0:
            state["nonfinite_probe"] = float("nan")
            worker.json.dumps(state, allow_nan=False)
    return state

def dispatch_fault(request):
    global remaining, calls, last_calls
    if request.get("op") == "test_prepare_calls":
        return {"prepare_calls": last_calls}
    remaining = request.get("fail_snapshot_number", 0)
    calls = 0
    try:
        return dispatch(request)
    finally:
        last_calls = calls

worker.prepare_state = prepare_fault
worker.dispatch = dispatch_fault
worker.main()
'''

CASES = [
    ("sketch_preview_commit_prepare_failure", "preview_commit", 1),
    ("finish_no_second_prepare", "sketch_finish", 2),
    ("sketch_cancel_prepare_failure", "sketch_cancel", 1),
    ("sketch_preview_cancel_prepare_failure", "preview_cancel", 1),
    ("sketch_begin_prepare_failure", "sketch_begin", 1),
    ("finish_first_prepare_failure", "sketch_finish", 1),
    ("native_preview_commit_prepare_failure", "preview_commit", 1),
    ("native_preview_cancel_prepare_failure", "preview_cancel", 1),
]


def capture(state):
    item = objects(state).get("Sketch")
    length = (item["sketch"]["constraints"][0]["value"] if item else
              objects(state)["Box"]["properties"]["Length"])
    return {key: state[key] for key in (
        "revision", "undo_count", "redo_count", "preview", "sketch_edit", "available"
    )} | {"length": length}


def workflow(host, runtime, case, op, number):
    session = SketchSession(host, runtime)
    try:
        doc = session.call("new", name="LifecycleProbe")["document"]
        if case.startswith("native_"):
            transaction(session, doc, "Baseline", [add("Part::Box", "Box"), set_value("Box", "Length", 10)])
            session.call("preview", document=doc, token="gesture", edits=[set_value("Box", "Length", 40)])
        else:
            transaction(session, doc, "Baseline", [add("Sketcher::SketchObject", "Sketch"),
                dict(op="geometry", object="Sketch", kind="line", start=[0, 0, 0], end=[10, 0, 0]),
                constraint("Sketch", "Distance", 0, 10), constraint("Sketch", "Horizontal", 0),
                constraint("Sketch", "Coincident", 0, 1, -1, 1)])
            if op != "sketch_begin":
                session.call("sketch_begin", document=doc, object="Sketch", token="review")
                for length in [20, 30]:
                    transaction(session, doc, "Accept", [dict(op="datum", object="Sketch", index=0, value=length)])
                session.call("undo", document=doc)
                if op in {"preview_commit", "preview_cancel"}:
                    session.call("preview", document=doc, token="gesture", edits=[
                        dict(op="datum", object="Sketch", index=0, value=40)])
        before = session.call("inspect", document=doc)
        token = "gesture" if op.startswith("preview") else "review"
        arguments = dict(document=doc, token=token, fail_snapshot_number=number)
        if op == "sketch_begin":
            arguments["object"] = "Sketch"
        succeeds = case == "finish_no_second_prepare"
        outcome = session.call(op, expect_error=None if succeeds else "native_error", **arguments)
        prepare_calls = session.call("test_prepare_calls")["prepare_calls"]
        after = session.call("inspect", document=doc)
        old, new = capture(before), capture(after)
        entry = dict(before=before, outcome=outcome, after=after, prepare_calls=prepare_calls)
        if case == "sketch_preview_commit_prepare_failure":
            assert new == old, entry
            accepted = session.call("preview_commit", document=doc, token=token, expected_revision=old["revision"])
            assert accepted["revision"] == old["revision"] + 1
            assert accepted["undo_count"] == 2 and accepted["redo_count"] == 0 and accepted["preview"] is None
            entry["retry"] = accepted
        elif case == "finish_no_second_prepare":
            assert prepare_calls == 1
            assert new["revision"] == old["revision"] + 1 and new["sketch_edit"] is None
            assert new["undo_count"] == 2 and new["redo_count"] == 0 and new["length"] == 20
            session.call("undo", document=doc)
            restored = session.call("inspect", document=doc)
            assert capture(restored)["length"] == 10
            entry["native_document_undo"] = restored
        elif case in {"sketch_cancel_prepare_failure", "sketch_preview_cancel_prepare_failure", "finish_first_prepare_failure"}:
            assert new == old | {"revision": old["revision"] + 1}, entry
            session.call(op, document=doc, token=token, expected_revision=old["revision"], expect_error="stale_revision")
            assert capture(session.call("inspect", document=doc)) == new
            retried = session.call(op, document=doc, token=token)
            assert retried["revision"] == new["revision"] + 1
            if op == "preview_cancel":
                assert capture(retried)["length"] == 20 and retried["redo_count"] == 1
            entry["retry"] = retried
        elif case == "sketch_begin_prepare_failure":
            assert new == old and new["sketch_edit"] is None
            begun = session.call("sketch_begin", document=doc, object="Sketch", token=token, expected_revision=old["revision"])
            assert begun["revision"] == old["revision"] + 1
            entry["retry"] = begun
        elif case == "native_preview_commit_prepare_failure":
            assert new == old, entry
        elif case == "native_preview_cancel_prepare_failure":
            assert new["revision"] == old["revision"] + 1 and not new["available"] and new["preview"] is None
        state = session.call("inspect", document=doc)
        if state["available"] and state["sketch_edit"]:
            session.call("sketch_cancel", document=doc, token="review")
        elif state["available"] and state["preview"]:
            session.call("preview_cancel", document=doc, token="gesture")
        session.call("close", document=doc)
        return entry
    finally:
        session.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    runtime = json.loads(args.runtime.read_text(encoding="utf-8"))
    backend = Path(runtime["script"]).resolve().parent
    wrapper = output / "fault_worker.py"
    wrapper.write_text(FAULT_WORKER.replace("BACKEND", repr(str(backend))), encoding="utf-8", newline="\n")
    runtime["script"] = str(wrapper)
    report = {case: workflow(args.host.resolve(), runtime, case, op, number) for case, op, number in CASES}
    (output / "session-lifecycle-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"passed": len(report), "snapshot_failure_recovery": True}))


if __name__ == "__main__":
    main()
