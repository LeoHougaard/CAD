"""Multi-document ownership controls through the Rust host and command compiler.

Use --observe-baseline to retain failures while exercising the same controls.
No active-document API, cloned document, or edited native history is used.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import traceback

from native_workflows import add, circle, link, near, objects, rectangle, set_value, transaction
from sketch_editor_workflows import SketchSession
from history_workflows import seed


class Recorded(SketchSession):
    def __init__(self, host, runtime, folder):
        self.folder = folder
        self.transcript = []
        super().__init__(host, runtime)

    def call(self, op, expect_error=None, **parameters):
        event = dict(op=op, parameters=parameters, expected_error=expect_error)
        self.transcript.append(event)
        try:
            result = super().call(op, expect_error=expect_error, **parameters)
            event["result"] = result
            return result
        except Exception as error:
            event["failure"] = str(error)
            raise
        finally:
            (self.folder/"transcript.json").write_text(json.dumps(self.transcript, indent=2)+"\n", newline="\n")


def compiled(compiler, state, **properties):
    request = dict(type="Part::Box", name="Box", properties=properties)
    command = [str(compiler), "--native-properties", "-"]
    response = subprocess.run(command, input=json.dumps(request), text=True,
                              encoding="utf-8", capture_output=True, timeout=15)
    assert response.returncode == 0, response.stderr
    return json.loads(response.stdout)


def box(s, name):
    doc = s.call("new", name=name)["document"]
    state = transaction(s, doc, "Seed local box", [add("Part::Box", "Box"),
        set_value("Box", "Length", 10), set_value("Box", "Width", 10), set_value("Box", "Height", 10)])
    return doc, state


def other(s, redo=False):
    doc, state = box(s, "Other")
    if redo:
        transaction(s, doc, "Other own undoable edit", [set_value("Box", "Width", 13)])
        state = s.call("undo", document=doc)
        assert state["redo_count"] == 1
    return doc, state


def content(state):
    # Every exposed native object field, including expressions and attachments.
    # Document revision and history are checked separately, without masking them.
    return state["objects"]


def check_other(s, doc, before, checks, step):
    after = s.call("inspect", document=doc)
    changed = [k for k in before if before[k] != after[k]]
    checks.append(dict(step=step, other_document=doc, unchanged=not changed,
                       changed_fields=changed,
                       before={k: before[k] for k in ("revision", "undo_count", "redo_count", "available")},
                       after={k: after[k] for k in ("revision", "undo_count", "redo_count", "available")}))


def property_edit(s, compiler, folder, checks, redo=False):
    a, initial = box(s, "Owner")
    b, before = other(s, redo)
    edits = compiled(compiler, initial, Width=17)
    state = transaction(s, a, "Inactive property owner", edits)
    near(objects(state)["Box"]["shape"]["volume"], 1700)
    assert state["undo_count"] == initial["undo_count"]+1
    check_other(s, b, before, checks, "inactive ordinary property transaction")


def rollback(s, compiler, folder, checks):
    a, initial = box(s, "Owner")
    b, before = other(s, True)
    s.call("transaction", document=a, label="Inactive rollback",
           edits=compiled(compiler, initial, Width=17)+[set_value("Box", "InvalidProperty", 2)],
           expect_error="unsupported_property")
    restored = s.call("inspect", document=a)
    assert content(restored) == content(initial)
    assert restored["revision"] > initial["revision"]
    assert restored["undo_count"] == initial["undo_count"] and restored["available"]
    check_other(s, b, before, checks, "inactive failed transaction rollback")


def previews(s, compiler, folder, checks, redo=False):
    a, initial = box(s, "Owner")
    b, before = other(s, redo)
    token = "local-preview"
    def preview(edits, **kw):
        return s.call("preview", document=a, token=token, label="Local preview", edits=edits, **kw)
    preview(compiled(compiler, initial, Width=11)+[set_value("Box", "InvalidProperty", 1)],
            expect_error="unsupported_property")
    failed_first = s.call("inspect", document=a)
    assert content(failed_first) == content(initial) and failed_first["preview"] is None
    assert failed_first["undo_count"] == initial["undo_count"]
    check_other(s, b, before, checks, "failed first inactive preview")
    first = preview([add("Part::Sphere", "Temporary")]+compiled(compiler, initial, Width=12))["state"]
    assert "Temporary" in objects(first)
    check_other(s, b, before, checks, "first inactive preview")
    replaced = preview(compiled(compiler, first, Width=14))["state"]
    assert "Temporary" not in objects(replaced)
    check_other(s, b, before, checks, "preview replacement")
    preview(compiled(compiler, replaced, Width=18)+[set_value("Box", "InvalidProperty", 1)],
            expect_error="unsupported_property")
    restored = s.call("inspect", document=a)
    assert content(restored) == content(replaced) and restored["preview"] == replaced["preview"]
    assert restored["revision"] > replaced["revision"] and restored["available"]
    check_other(s, b, before, checks, "failed preview replacement restoration")
    s.call("preview_cancel", document=a, token=token, expected_revision=replaced["revision"], expect_error="stale_revision")
    s.call("preview_cancel", document=a, token="wrong", expect_error="preview_owner")
    canceled = s.call("preview_cancel", document=a, token=token)
    assert content(canceled) == content(initial) and canceled["preview"] is None
    assert canceled["undo_count"] == initial["undo_count"]
    check_other(s, b, before, checks, "preview Cancel")
    preview(compiled(compiler, canceled, Width=16))
    committed = s.call("preview_commit", document=a, token=token)
    near(objects(committed)["Box"]["shape"]["volume"], 1600)
    assert committed["undo_count"] == initial["undo_count"]+1
    check_other(s, b, before, checks, "preview commit")
    undone = s.call("undo", document=a)
    assert content(undone) == content(initial)
    redone = s.call("redo", document=a)
    assert content(redone) == content(committed)
    check_other(s, b, before, checks, "committed preview undo and redo")


def history(s, compiler, folder, checks):
    a, initial = box(s, "Owner")
    edited = transaction(s, a, "Own undo entry", compiled(compiler, initial, Width=14))
    b, before = other(s, True)
    undone = s.call("undo", document=a)
    assert content(undone) == content(initial) and undone["redo_count"] == 1
    check_other(s, b, before, checks, "native undo of inactive target")
    # New C is an intentional public-API activation, making A inactive again.
    c, third = box(s, "Third")
    redone = s.call("redo", document=a)
    assert content(redone) == content(edited)
    check_other(s, b, before, checks, "native redo leaves B redo intact")
    check_other(s, c, third, checks, "native redo of inactive target leaves C intact")


def sketch(s, compiler, folder, checks):
    a = s.call("new", name="Owner")["document"]
    base = transaction(s, a, "Consumed source sketch", [add("PartDesign::Body", "Body"),
        add("Sketcher::SketchObject", "Sketch", "Body"), *rectangle("Sketch", 20, 10),
        add("PartDesign::Pad", "Pad", "Body"), set_value("Pad", "Profile", link("Sketch")),
        set_value("Pad", "Length", 5)])
    face = next(f["subelement"] for f in s.call("topology", document=a, object="Pad")["faces"]
                if f["normal"] and f["normal"][2] > .99)
    base = transaction(s, a, "Attached dependent pocket", [add("Sketcher::SketchObject", "Hole", "Body"),
        set_value("Hole", "AttachmentSupport", link("Pad", face)), set_value("Hole", "MapMode", "FlatFace"),
        *circle("Hole", [4, 4, 0], 1), add("PartDesign::Pocket", "Pocket", "Body"),
        set_value("Pocket", "Profile", link("Hole")), set_value("Pocket", "Length", 1)])
    b, before = other(s, True)
    begin = s.call("sketch_begin", document=a, object="Sketch", token="local-sketch")
    assert begin["undo_count"] == begin["redo_count"] == 0
    check_other(s, b, before, checks, "inactive sketch Begin without empty transaction")
    def width(value):
        return [dict(op="datum", object="Sketch", index=9, value=f"{value} mm")]
    action = transaction(s, a, "Sketch dimension", width(30))
    assert action["undo_count"] == 1
    check_other(s, b, before, checks, "sketch action")
    s.call("undo", document=a)
    s.call("redo", document=a)
    check_other(s, b, before, checks, "sketch journal undo and redo")
    first = s.call("preview", document=a, token="sketch-drag", edits=width(35))["state"]
    replaced = s.call("preview", document=a, token="sketch-drag", edits=width(36))["state"]
    s.call("preview", document=a, token="sketch-drag", edits=width(38)+[set_value("Sketch", "InvalidProperty", 1)],
           expect_error="unsupported_property")
    restored = s.call("inspect", document=a)
    assert content(restored) == content(replaced)
    check_other(s, b, before, checks, "sketch gesture first, replacement and failed replacement")
    s.call("preview_cancel", document=a, token="sketch-drag")
    s.call("preview", document=a, token="sketch-drag", edits=width(32))
    s.call("preview_commit", document=a, token="sketch-drag")
    check_other(s, b, before, checks, "sketch gesture Cancel and commit")
    # Worker-wide sketch ownership wins before another document's stale revision.
    s.call("transaction", document=b, expected_revision=-1, edits=[], expect_error="sketch_active")
    finished = s.call("sketch_finish", document=a, token="local-sketch")
    assert finished["sketch_edit"] is None and finished["undo_count"] == base["undo_count"]+1
    near(objects(finished)["Pad"]["shape"]["volume"], 32*10*5)
    assert objects(finished)["Hole"]["properties"]["AttachmentSupport"] == objects(base)["Hole"]["properties"]["AttachmentSupport"]
    check_other(s, b, before, checks, "sketch Finish updates own dependent native features")
    assert content(s.call("undo", document=a)) == content(base)
    assert content(s.call("redo", document=a)) == content(finished)
    s.call("sketch_begin", document=a, object="Sketch", token="cancel-sketch")
    transaction(s, a, "Discard source update", width(34))
    canceled = s.call("sketch_cancel", document=a, token="cancel-sketch")
    assert content(canceled) == content(finished)
    assert canceled["undo_count"] == finished["undo_count"]
    check_other(s, b, before, checks, "sketch Cancel restores source and dependencies")


def lifecycle(s, compiler, folder, checks):
    a, initial = box(s, "Owner")
    path = folder/"saved.FCStd"
    s.call("save", document=a, path=str(path))
    saved_hash = hashlib.sha256(path.read_bytes()).hexdigest()
    initial = s.call("inspect", document=a)
    b, before = other(s, True)
    for op, parameters, error in [
        ("new", dict(name="Other"), "name_conflict"),
        ("open", dict(path=str(folder/"missing.FCStd")), "not_found"),
        ("open", dict(path=str(path)), "already_open"),
    ]:
        s.call(op, expect_error=error, **parameters)
        check_other(s, b, before, checks, "failed "+op+" "+error)
    corrupt = folder/"corrupt.FCStd"
    corrupt.write_bytes(b"This is not a FreeCAD ZIP document.\n")
    s.call("open", path=str(corrupt), expect_error="native_error")
    check_other(s, b, before, checks, "failed native open cleanup")
    s.call("inspect", document="corrupt", expect_error="not_found")
    replacement = s.call("new", name="corrupt")
    s.call("close", document=replacement["document"])
    check_other(s, b, before, checks, "new and close empty document")
    assert content(s.call("inspect", document=a)) == content(initial)
    s.call("close", document=a)
    check_other(s, b, before, checks, "close other model")
    opened = s.call("open", path=str(path))
    assert content(opened) == content(initial)
    check_other(s, b, before, checks, "successful native open")
    transaction(s, opened["document"], "Edit reopened source", compiled(compiler, opened, Width=19))
    check_other(s, b, before, checks, "edit reopened native document")
    assert hashlib.sha256(path.read_bytes()).hexdigest() == saved_hash


def admission(s, compiler, folder, checks):
    a, initial = box(s, "Owner")
    b, before = other(s, True)
    for op, parameters, error in [
        ("transaction", dict(expected_revision=-1, edits=[]), "stale_revision"),
        ("transaction", dict(edits=[]), "invalid_request"),
        ("preview", dict(token="p", edits=[]), "invalid_request"),
        ("preview_cancel", dict(token="p"), "no_preview"),
        ("sketch_begin", dict(object="Box", token="s"), "invalid_request"),
        ("sketch_cancel", dict(token="s"), "no_sketch_edit"),
    ]:
        s.call(op, document=a, expect_error=error, **parameters)
        check_other(s, b, before, checks, "admission "+op+" "+error)
    after = s.call("inspect", document=a)
    assert after == initial
    # Reads of the inactive owner must not select it or open a transaction.
    for op, extra in [("inspect", {}), ("mesh", dict(object="Box")),
                      ("topology", dict(object="Box"))]:
        s.call(op, document=b, **extra)
    check_other(s, b, before, checks, "read-only requests")


def suppression(s, compiler, folder, checks):
    a, initial = seed(s, "Owner")
    b, before = seed(s, "Other")
    state = transaction(s, a, "Original inactive suppression proof", [set_value("A", "Suppressed", True)])
    assert objects(state)["A"]["properties"]["Suppressed"]
    check_other(s, b, before, checks, "original c6 native suppression reproduction")


def source_dependencies(s, compiler, folder, checks):
    a = s.call("new", name="Owner")["document"]
    initial = transaction(s, a, "Native source and copy", [add("Sketcher::SketchObject", "Source"),
        *circle("Source", [5, 5, 0], 2), add("Sketcher::SketchObject", "Copy")])
    initial = transaction(s, a, "Native Carbon Copy dependency", [
        dict(op="carbon_copy", object="Copy", source="Source", construction=False)])
    b, before = other(s, True)
    s.call("sketch_begin", document=a, object="Source", token="source-session")
    transaction(s, a, "Change source diameter", [dict(op="datum", object="Source", index=2, value="6 mm")])
    finished = s.call("sketch_finish", document=a, token="source-session")
    near(objects(finished)["Copy"]["sketch"]["constraints"][2]["value"], 6)
    assert objects(finished)["Copy"]["expressions"] == objects(initial)["Copy"]["expressions"]
    check_other(s, b, before, checks, "source session Finish updates own Carbon Copy dependency")
    s.call("sketch_begin", document=a, object="Source", token="cancel-source")
    transaction(s, a, "Discard source diameter", [dict(op="datum", object="Source", index=2, value="8 mm")])
    canceled = s.call("sketch_cancel", document=a, token="cancel-source")
    assert content(canceled) == content(finished)
    check_other(s, b, before, checks, "source session Cancel restores own dependency")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--observe-baseline", action="store_true")
    args = parser.parse_args()
    args.output.mkdir()
    runtime = json.loads(args.runtime.read_text())
    results = {}
    cases = dict(property_edit=lambda *x: property_edit(*x, redo=False),
                 property_edit_other_redo=lambda *x: property_edit(*x, redo=True),
                 failed_transaction=rollback,
                 preview=lambda *x: previews(*x, redo=False),
                 preview_other_redo=lambda *x: previews(*x, redo=True),
                 inactive_history=history, sketch=sketch, lifecycle=lifecycle, admission=admission,
                 suppression=suppression, source_dependencies=source_dependencies)
    for name, case in cases.items():
        folder = args.output/name
        folder.mkdir()
        checks = []
        s = Recorded(args.host, runtime, folder)
        error = None
        try:
            case(s, args.compiler, folder, checks)
        except Exception:
            error = traceback.format_exc()
            (folder/"failure.txt").write_text(error, newline="\n")
        finally:
            s.close()
        failures = [c for c in checks if not c["unchanged"]]
        results[name] = dict(passed=not error and not failures, checks=checks,
                             unexpected_error=error, changed_other_document=len(failures))
        (args.output/"report.json").write_text(json.dumps(results, indent=2)+"\n", newline="\n")
        print(name, "PASS" if results[name]["passed"] else "FAIL", "other changes", len(failures), flush=True)
    assert args.observe_baseline or all(r["passed"] for r in results.values()), results


if __name__ == "__main__":
    main()
