"""Public native datum admission, revision and unchanged-state regression."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import traceback


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("host", "runtime", "output"):
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    runtime = json.loads(args.runtime.read_text(encoding="utf8"))
    worker = Path(runtime["script"]).resolve()
    observer = Path(__file__).resolve().parent / "fixtures" / "datum_guard_observer.py"
    host = args.host.resolve()
    (out / "provenance.json").write_text(json.dumps({
        "host": str(host), "host_sha256": digest(host),
        "runtime": str(args.runtime.resolve()), "runtime_sha256": digest(args.runtime),
        "interpreter": str(Path(runtime["executable"]).resolve()),
        "interpreter_sha256": digest(runtime["executable"]), "interpreter_args": runtime["args"],
        "worker": str(worker), "worker_sha256": digest(worker),
        "observer": str(observer), "observer_sha256": digest(observer),
    }, indent=2) + "\n", encoding="utf8")
    command = [str(host), "--executable", runtime["executable"], "--script", str(observer)]
    for arg in runtime["args"]:
        command.extend(["--arg", arg])
    env = os.environ.copy()
    env.pop("PYTHONPATH", None)
    env.pop("PYTHONHOME", None)
    env.update(PYTHONDONTWRITEBYTECODE="1", PYTHONNOUSERSITE="1", PYTHONUTF8="1", PYTHONIOENCODING="utf8")
    for key, folder in (("HOME", "home"), ("USERPROFILE", "home"), ("FREECAD_USER_HOME", "config"),
                        ("APPDATA", "appdata"), ("LOCALAPPDATA", "localappdata"), ("TMP", "tmp"), ("TEMP", "tmp")):
        path = out / "private7" / folder
        path.mkdir(parents=True, exist_ok=True)
        env[key] = str(path)
    env.update(CAD_DATUM_WORKER=str(worker), CAD_DATUM_TRACE=str(out / "observer.jsonl"))
    events = []
    stderr = (out / "native-worker.log").open("w", encoding="utf8")
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr,
                               text=True, encoding="utf8", env=env,
                               creationflags=getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0))

    def call(op, error=None, **parameters):
        request = {"op": op, **parameters}
        process.stdin.write(json.dumps(request) + "\n")
        process.stdin.flush()
        line = process.stdout.readline()
        assert line, ("native host exited", process.poll())
        response = json.loads(line)
        events.append({"request": request, "response": response})
        (out / "events.json").write_text(json.dumps(events, indent=2) + "\n", encoding="utf8")
        assert response["ok"] == (error is None), response
        if error:
            assert response["error"]["code"] == error, response
            return response["error"]
        return response["result"]

    def admission_counts():
        entries = [json.loads(line) for line in (out / "observer.jsonl").read_text(encoding="utf8").splitlines()]
        return {kind: sum(item["event"] == kind for item in entries)
                for kind in ("preflight_enter", "apply_datum_entry")}

    def transaction(doc, revision, edits, **more):
        return call("transaction", document=doc, expected_revision=revision, sketch_token="owner",
                    label="datum test", edits=edits, **more)

    def datum(obj, index, value):
        return {"op": "datum", "object": obj, "index": index, "value": value}

    try:
        call("capabilities")
        state = call("new", name="DatumGuard")
        doc, revision = state["document"], state["revision"]
        edits = [{"op": "add_object", "type": "Sketcher::SketchObject", "name": name}
                 for name in ("Sketch", "Other")]
        edits += [{"op": "geometry", "object": name, "kind": "circle", "center": [0, 0, 0], "radius": 5}
                  for name in ("Sketch", "Other")]
        edits += [{"op": "constraint", "object": name, "kind": "Diameter", "args": [0, 10]}
                  for name in ("Sketch", "Other")]
        edits += [{"op": "geometry", "object": "Sketch", "kind": "line", "start": [5, 5, 0], "end": [10, 10, 0]},
                  {"op": "constraint", "object": "Sketch", "kind": "DistanceX", "args": [1, 1, 5]},
                  {"op": "constraint", "object": "Sketch", "kind": "DistanceY", "args": [1, 1, 5]}]
        state = call("transaction", document=doc, expected_revision=revision, label="fixture", edits=edits)["state"]
        call("sketch_begin", document=doc, object="Sketch", token="owner", expected_revision=state["revision"])
        before = call("inspect", document=doc)
        revision = before["revision"]
        counts = admission_counts()
        transaction(doc, revision - 1, [datum("Sketch", 0, 0)], error="stale_revision")
        assert admission_counts() == counts and call("inspect", document=doc) == before
        transaction(doc, revision, [datum("Other", 0, 0)], error="sketch_active")
        assert admission_counts() == counts and call("inspect", document=doc) == before
        call("transaction", document=doc, expected_revision=revision, sketch_token="wrong", label="wrong owner",
             edits=[datum("Sketch", 0, 0)], error="sketch_owner")
        assert admission_counts() == counts and call("inspect", document=doc) == before
        error = transaction(doc, revision, [datum("Sketch", 0, 0)], error="native_error")
        assert error["message"] == "Zero is not a valid datum for the constraint with index 0"
        after = call("inspect", document=doc)
        assert after["revision"] == revision + 1
        before_without_revision, after_without_revision = dict(before), dict(after)
        before_without_revision.pop("revision"); after_without_revision.pop("revision")
        assert after_without_revision == before_without_revision
        now = admission_counts()
        assert now["preflight_enter"] == counts["preflight_enter"] + 1
        assert now["apply_datum_entry"] == counts["apply_datum_entry"]
        revision = after["revision"]
        for index in (1, 2):
            for value in (0, -3):
                state = transaction(doc, revision, [datum("Sketch", index, value)])["state"]
                revision = state["revision"]
                sketch = next(item["sketch"] for item in state["objects"] if item["name"] == "Sketch")
                assert sketch["constraints"][index]["value"] == value
        state = call("sketch_cancel", document=doc, token="owner", expected_revision=revision)
        call("close", document=doc, expected_revision=state["revision"])
        process.stdin.close()
        assert process.wait(timeout=10) == 0
        (out / "passed.json").write_text(json.dumps({"cases": ["stale revision", "wrong sketch", "wrong token",
            "native zero rejection +1 revision and exact state", "DistanceX/Y zero and negative"]}, indent=2) + "\n", encoding="utf8")
    except BaseException:
        (out / "failure.txt").write_text(traceback.format_exc(), encoding="utf8")
        if process.poll() is None:
            process.kill()
            process.wait()
        raise
    finally:
        stderr.close()


if __name__ == "__main__":
    main()
