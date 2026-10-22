"""Match native preview getter ordering with actual FreeCAD and a getter-fault proxy."""
import argparse
import json
import math
from pathlib import Path
import subprocess

WORKER = r'''import sys
sys.dont_write_bytecode = True
sys.path.insert(0, BACKEND)
import freecad_worker as worker
original_document = worker.document
original_dispatch = worker.dispatch
track = False
fault = None
events = []

class Document:
    def __init__(self, doc):
        self.doc = doc
    def __getattr__(self, key):
        if key == "RedoCount":
            caller = sys._getframe(1).f_code.co_name
            if caller in {"preview", "sketch_preview", "preview_ready"}:
                phase = "admission"
            elif caller == "history_counts":
                phase = "history"
            else:
                raise RuntimeError("Unexpected native redo getter caller: " + caller)
            events.append(dict(getter=key, phase=phase))
            if fault == "all" or fault == phase:
                raise RuntimeError("Private review native RedoCount read failure")
        return getattr(self.doc, key)

def document(request, writable=False):
    doc = original_document(request, writable)
    return Document(doc) if track else doc

def dispatch(request):
    global track, fault, events
    if request.get("op") == "test_native_redo_reads":
        return {"reads": events}
    track = request.get("track_native_redo", False)
    fault = request.get("native_redo_fault")
    events = []
    return original_dispatch(request)

worker.document = document
worker.dispatch = dispatch
worker.main()
'''


def differences(a, b):
    if type(a) is not type(b):
        return True
    if isinstance(a, dict):
        return a.keys() != b.keys() or any(differences(a[k], b[k]) for k in a)
    if isinstance(a, list):
        return len(a) != len(b) or any(differences(x, y) for x, y in zip(a, b))
    if isinstance(a, float):
        return not math.isclose(a, b, rel_tol=1e-10, abs_tol=1e-9)
    return a != b


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ["host", "runtime", "output"]:
        parser.add_argument("--" + key, type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    runtime = json.loads(args.runtime.read_text(encoding="utf-8"))
    wrapper = output / "getter_fault_worker.py"
    wrapper.write_text(WORKER.replace("BACKEND", repr(str(Path(runtime["script"]).parent))), encoding="utf-8", newline="\n")
    command = [str(args.host.resolve()), "--executable", runtime["executable"], "--script", str(wrapper)]
    for value in runtime["args"]:
        command.extend(["--arg", value])
    records, cases = [], []
    stderr = (output / "stderr.txt").open("w", encoding="utf-8")
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True, encoding="utf-8")

    def call(op, error=None, **parameters):
        request = dict(version=1, id=len(records)+1, op=op, **parameters)
        process.stdin.write(json.dumps(request)+"\n")
        process.stdin.flush()
        line = process.stdout.readline()
        assert line, (request, process.poll())
        response = json.loads(line)
        records.append(dict(request=request, response=response))
        assert type(response["id"]) is int and response["id"] == request["id"]
        if error:
            assert response["ok"] is False and response["error"]["code"] == error, response
            return response["error"]
        assert response["ok"] is True, response
        return response["result"]

    def inspect():
        return call("inspect", document="NativeFacts")

    def write(op, error=None, **parameters):
        return call(op, error, document="NativeFacts", expected_revision=inspect()["revision"], **parameters)

    def geometry(y):
        return dict(op="geometry", object="Sketch", kind="line", start=[0,y,0], end=[10,y,0])

    def probe(name, parameters, expected, phases, unchanged=False, restored=False):
        before = inspect()
        result = write("preview", expected, track_native_redo=True, **parameters)
        reads = call("test_native_redo_reads")["reads"]
        assert [v["phase"] for v in reads] == phases, (name, reads)
        after = inspect()
        if unchanged:
            assert not differences(before, after), name
        if restored:
            assert after["revision"] == before["revision"] + 1
            assert not differences(before | dict(revision=after["revision"]), after), name
        cases.append(dict(case=name, before=before, result=result, after=after, native_reads=reads))

    def invalid_labels(prefix, params):
        for label in [None, False, 17, [], {}]:
            probe(prefix + "_label_" + type(label).__name__, params | dict(label=label, native_redo_fault="all"),
                  "invalid_request", [], unchanged=True)

    try:
        call("new", name="NativeFacts")
        write("transaction", edits=[dict(op="add_object", type="Part::Box", name="Box"),
              dict(op="add_object", type="Sketcher::SketchObject", name="Sketch"), geometry(0)])
        write("transaction", edits=[geometry(1)])
        state = write("undo")
        assert state["redo_count"] == 1
        feature = dict(token="p", edits=[dict(op="set",object="Box",property="Length",value=20)])
        invalid_labels("feature_first", feature)
        probe("feature_required_redo_failure", feature | dict(native_redo_fault="all"), "native_error", ["admission"], unchanged=True)
        probe("feature_real_redo_conflict", feature, "preview_redo_conflict", ["admission"], unchanged=True)
        write("sketch_begin", token="s", object="Sketch")
        sketch = dict(token="p", sketch_token="s", edits=[geometry(2)])
        invalid_labels("sketch_unopened", sketch)
        probe("sketch_required_redo_failure", sketch | dict(native_redo_fault="all"), "native_error", ["admission"], unchanged=True)
        probe("sketch_real_redo_conflict", sketch, "preview_redo_conflict", ["admission"], unchanged=True)
        write("sketch_cancel", token="s")
        write("redo")
        probe("feature_first_zero_redo", feature | dict(label="Original label"), None, ["admission","history","history"])
        invalid_labels("feature_replacement", feature)
        probe("feature_replacement_avoids_admission_getter", feature | dict(native_redo_fault="admission"), None, ["history","history"])
        probe("feature_replacement_retains_required_snapshot_read", feature | dict(native_redo_fault="all"), "native_error", ["history"], restored=True)
        write("preview_cancel", token="p")
        write("sketch_begin", token="s", object="Sketch")
        probe("sketch_unopened_zero_redo", sketch, None, ["admission"])
        write("preview_cancel", token="p", sketch_token="s")
        write("transaction", sketch_token="s", edits=[geometry(3)])
        invalid_labels("sketch_opened", sketch)
        probe("sketch_opened_does_not_read_redo", sketch | dict(native_redo_fault="all"), None, [])
        probe("sketch_replacement_does_not_read_redo", sketch | dict(edits=[geometry(4)], native_redo_fault="all"), None, [])
        write("preview_cancel", token="p", sketch_token="s")
        write("sketch_cancel", token="s")
        write("close")
    finally:
        process.stdin.close()
        assert process.wait(timeout=15) == 0
        stderr.close()
        (output/"transcript.json").write_text(json.dumps(records,indent=2)+"\n",encoding="utf-8")
        (output/"cases.json").write_text(json.dumps(cases,indent=2)+"\n",encoding="utf-8")
    print(json.dumps(dict(cases=len(cases),responses=len(records),native_getter_order=True)))


if __name__ == "__main__":
    main()
