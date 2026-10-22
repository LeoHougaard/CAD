"""Native Body Tip and reorder workflows using the production Rust compiler."""

import argparse
import json
import math
from pathlib import Path
import subprocess

from native_workflows import add, link, near, objects, set_value, transaction
from sketch_editor_workflows import SketchSession
import transform_workflows as transforms


def compile_edits(compiler, flag, parameters):
    result = subprocess.run([str(compiler), flag, "-"], input=json.dumps(parameters),
                            text=True, encoding="utf-8", capture_output=True, timeout=15)
    assert result.returncode == 0, result.stderr
    return json.loads(result.stdout)


def history(compiler, state, **request):
    return compile_edits(compiler, "--native-history", {"state": state, "request": request})


def primitive(name, body, size, position):
    return [add("PartDesign::AdditiveBox", name, body), set_value(name, "Length", size[0]),
            set_value(name, "Width", size[1]), set_value(name, "Height", size[2]),
            set_value(name, "Placement", {"position": position, "rotation": [0, 0, 0, 1]})]


def seed(session, name, transformed=False):
    doc = session.call("new", name=name)["document"]
    edits = [add("PartDesign::Body", "Body")]
    for name, size, position in [("Base", [10, 10, 10], [0, 0, 0]),
                                 ("A", [2, 2, 4], [1, 1, 10]), ("B", [3, 3, 3], [6, 6, 10])]:
        edits += primitive(name, "Body", size, position)
    if transformed:
        edits.append(set_value("Body", "Placement", {"position": [100, 200, 300],
                    "rotation": [0, 0, math.sqrt(0.5), math.sqrt(0.5)]}))
    state = transaction(session, doc, "Native parametric history", edits)
    chain(state, ["Base", "A", "B"], "B", 1043)
    assert all(objects(state)[name]["history_flags"]["can_tip"] for name in ["Base", "A", "B"])
    assert not objects(state)["Body"]["history_flags"]["can_tip"]
    return doc, state


def chain(state, order, tip, volume, body="Body"):
    native = objects(state)
    assert [entry["link"] for entry in native[body]["properties"]["Group"]] == order
    assert native[body]["body_state"]["tip"] == tip
    assert native[body]["valid"] and state["available"]
    near(native[body]["shape"]["volume"], volume)
    previous = None
    for name in order:
        if "BaseFeature" in native[name]["properties"]:
            assert native[name]["properties"]["BaseFeature"] == (link(previous) if previous else None), (name, native[name]["properties"]["BaseFeature"], previous)
            previous = name


def comparable_shape(obj):
    shape = obj.get("shape")
    if shape is None:
        return None
    shape = dict(shape)
    # Volume is meaningful for solids. OCCT returns unstable pseudo-volumes
    # for unbounded origin planes (XZ_Plane on ARM64 after native Redo).
    # Keep their topology, validity, bounds and area in the comparison.
    if not shape["solids"]:
        shape.pop("volume", None)
    return shape


def fingerprint(state):
    fields = {"Group", "Tip", "BaseFeature", "Profile", "AttachmentSupport", "MapMode", "Placement", "Visibility", "Length", "Width", "Height"}
    return {obj["name"]: {"type": obj["type"], "valid": obj["valid"], "shape": comparable_shape(obj),
            "properties": {key: value for key, value in obj["properties"].items() if key in fields},
            "expressions": obj["expressions"]} for obj in state["objects"]}


def same(actual, expected, path="state"):
    if isinstance(expected, dict):
        assert actual.keys() == expected.keys(), (path, actual.keys(), expected.keys())
        for key, value in expected.items():
            same(actual[key], value, path + "." + str(key))
    elif isinstance(expected, list):
        assert len(actual) == len(expected), (path, len(actual), len(expected))
        for index, (left, right) in enumerate(zip(actual, expected)):
            same(left, right, path + "." + str(index))
    elif isinstance(expected, (int, float)) and not isinstance(expected, bool):
        assert math.isclose(actual, expected, rel_tol=1e-8, abs_tol=1e-6), (path, actual, expected)
    else:
        assert actual == expected, (path, actual, expected)


def tip_workflow(session, compiler, output):
    doc, baseline = seed(session, "NativeBodyTip")
    before = baseline["undo_count"]
    state = transaction(session, doc, "Rollback native Tip", history(compiler, baseline, op="body_tip", object="Body", tip="Base"))
    chain(state, ["Base", "A", "B"], "Base", 1000)
    assert state["undo_count"] == before + 1
    assert objects(state)["Body"]["body_state"]["display_object"] == "Base"
    assert [objects(state)[name]["properties"]["Visibility"] for name in ["Base", "A", "B"]] == [True, False, False]
    same(fingerprint(session.call("undo", document=doc)), fingerprint(baseline))
    state = session.call("redo", document=doc)
    chain(state, ["Base", "A", "B"], "Base", 1000)
    before_noop = state
    state = transaction(session, doc, "Already native Tip", history(compiler, state, op="body_tip", object="Body", tip="Base"))
    assert state["undo_count"] == before_noop["undo_count"]
    same(fingerprint(state), fingerprint(before_noop))
    rollback_path = str((output / "rolled-back-tip.FCStd").resolve())
    session.call("save", document=doc, path=rollback_path)
    state = transaction(session, doc, "Explicit native empty Body Tip", history(compiler, state, op="body_tip", object="Body", tip=None))
    chain(state, ["Base", "A", "B"], None, 0)
    assert objects(state)["Body"]["shape"]["null"]
    assert objects(state)["Body"]["body_state"]["display_object"] is None
    assert all(not objects(state)[name]["shape"]["null"] for name in ["Base", "A", "B"])
    path = str((output / "null-tip.FCStd").resolve())
    session.call("save", document=doc, path=path)
    session.call("close", document=doc)
    state = session.call("open", path=path)
    doc = state["document"]
    chain(state, ["Base", "A", "B"], None, 0)
    state = transaction(session, doc, "Restore native Tip after reopen", history(compiler, state, op="body_tip", object="Body", tip="B"))
    chain(state, ["Base", "A", "B"], "B", 1043)
    baseline = fingerprint(state)
    revision = state["revision"]
    session.call("transaction", document=doc, label="Stale history write", expected_revision=revision - 1,
                 expect_error="stale_revision", edits=history(compiler, state, op="body_tip", object="Body", tip="A"))
    same(fingerprint(session.call("inspect", document=doc)), baseline)
    session.call("transaction", document=doc, label="Abort Tip and native view changes", expect_error="unsupported_property",
                 edits=history(compiler, state, op="body_tip", object="Body", tip="A") + [set_value("A", "InvalidProperty", 1)])
    restored = session.call("inspect", document=doc)
    same(fingerprint(restored), baseline)
    assert restored["revision"] > revision and restored["available"]
    session.call("close", document=doc)
    state = session.call("open", path=rollback_path)
    chain(state, ["Base", "A", "B"], "Base", 1000)
    assert [objects(state)[name]["properties"]["Visibility"] for name in ["Base", "A", "B"]] == [True, False, False]
    session.call("close", document=state["document"])
    return {"tip_rollback": True, "explicit_null_tip_reopen": True, "retained_parametric_successors": True,
            "stale_revision_guard": True, "failed_tip_view_state_atomic": True,
            "native_gui_visibility_persisted": True, "same_tip_no_native_undo_entry": True}


def reorder_workflow(session, compiler, output, transformed):
    doc, baseline = seed(session, "PlacedHistory" if transformed else "NativeReorder", transformed)
    state = transaction(session, doc, "Native reorder and explicit final Tip", history(compiler, baseline,
        op="body_reorder", object="A", target="B", after=True, tip="A"))
    chain(state, ["Base", "B", "A"], "A", 1043)
    near(objects(state)["B"]["shape"]["volume"], 1027)
    assert objects(state)["Body"]["body_state"]["display_object"] == "A"
    changed = fingerprint(state)
    same(fingerprint(session.call("undo", document=doc)), fingerprint(baseline))
    same(fingerprint(session.call("redo", document=doc)), changed)
    path = str((output / ("placed-reordered.FCStd" if transformed else "reordered.FCStd")).resolve())
    session.call("save", document=doc, path=path)
    session.call("close", document=doc)
    state = session.call("open", path=path)
    doc = state["document"]
    chain(state, ["Base", "B", "A"], "A", 1043)
    state = transaction(session, doc, "Edit reordered native primitive after reopen", [set_value("A", "Height", 5)])
    chain(state, ["Base", "B", "A"], "A", 1047)
    chain(session.call("undo", document=doc), ["Base", "B", "A"], "A", 1043)
    chain(session.call("redo", document=doc), ["Base", "B", "A"], "A", 1047)
    session.call("close", document=doc)
    return {"transformed": transformed, "native_rewired_chain": ["Base", "B", "A"], "baseline_volume": 1043,
            "reopened_edit_volume": 1047, "undo_redo": True}


def insertion_and_native_defaults(session, compiler):
    doc, baseline = seed(session, "NativeHistoryInsertion")
    state = transaction(session, doc, "Keep native remove/insert Tip behavior", history(compiler, baseline,
        op="body_reorder", object="A", target="B", after=True))
    chain(state, ["Base", "B", "A"], "B", 1027)
    state = session.call("undo", document=doc)
    state = transaction(session, doc, "Native insert-before reorder", history(compiler, state,
        op="body_reorder", object="B", target="A", after=False, tip="A"))
    chain(state, ["Base", "B", "A"], "A", 1043)
    state = session.call("undo", document=doc)
    state = transaction(session, doc, "Native null-target beginning", history(compiler, state,
        op="body_reorder", object="A", target=None, after=True, tip="B"))
    chain(state, ["A", "Base", "B"], "B", 1043)
    state = session.call("undo", document=doc)
    state = transaction(session, doc, "Native null-target end", history(compiler, state,
        op="body_reorder", object="A", target=None, after=False, tip="A"))
    chain(state, ["Base", "B", "A"], "A", 1043)
    state = session.call("undo", document=doc)
    state = transaction(session, doc, "Rollback for insertion", history(compiler, state, op="body_tip", object="Body", tip="Base"))
    state = transaction(session, doc, "Insert native feature at rolled-back Tip", compile_edits(compiler, "--native-properties", {
        "type": "PartDesign::AdditiveBox", "name": "Inserted", "body": "Body", "create": True,
        "properties": {"Length": 2, "Width": 2, "Height": 2,
                       "Placement": {"position": [3, 3, 10], "rotation": [0, 0, 0, 1]}}}))
    chain(state, ["Base", "Inserted", "A", "B"], "Inserted", 1008)
    near(objects(state)["B"]["shape"]["volume"], 1051)
    chain(session.call("undo", document=doc), ["Base", "A", "B"], "Base", 1000)
    state = session.call("redo", document=doc)
    chain(state, ["Base", "Inserted", "A", "B"], "Inserted", 1008)
    state = transaction(session, doc, "Resume native successor Tip", history(compiler, state, op="body_tip", object="Body", tip="B"))
    chain(state, ["Base", "Inserted", "A", "B"], "B", 1051)
    session.call("close", document=doc)
    return {"null_target_native_semantics": True, "absent_tip_preserves_native_result": True,
            "rollback_insert_preserves_successors": True, "inserted_volume": 1008, "resumed_successor_volume": 1051}


def failed_dependencies(session, compiler):
    doc = session.call("new", name="NativeHistoryDependencies")["document"]
    state = transaction(session, doc, "Native attachment parents", [add("PartDesign::Body", "Body")] +
        primitive("Base", "Body", [20, 20, 10], [0, 0, 0]) + primitive("Boss", "Body", [5, 5, 4], [5, 5, 10]))
    faces = session.call("topology", document=doc, object="Boss")["faces"]
    face = max(faces, key=lambda item: item["center"][2])["subelement"]
    state = transaction(session, doc, "Native face-attached successor", [add("Sketcher::SketchObject", "Profile", "Body"),
        set_value("Profile", "AttachmentSupport", [link("Boss", face)]), set_value("Profile", "MapMode", "FlatFace"),
        dict(op="geometry", object="Profile", kind="circle", center=[1, 1, 0], radius=1),
        add("PartDesign::Pad", "Finish", "Body"), set_value("Finish", "Profile", link("Profile")), set_value("Finish", "Length", 2)])
    assert objects(state)["Profile"]["history_flags"] == {"can_tip": False, "can_reorder": True, "transform_step": False}
    baseline = fingerprint(state)
    revision = state["revision"]
    session.call("transaction", document=doc, label="Reject native dependency reorder", expect_error="history_dependency",
        edits=history(compiler, state, op="body_reorder", object="Boss", target="Finish", after=True, tip="Finish"))
    state = session.call("inspect", document=doc)
    assert state["available"] and state["revision"] > revision
    same(fingerprint(state), baseline)
    # The native Tip setter accepts a member link first; recomputation decides
    # that a Sketch cannot be a PartDesign Body Tip. Probe the backend guard too.
    session.call("transaction", document=doc, label="Reject native non-feature Tip", expect_error="invalid_model",
        edits=[dict(op="body_tip", object="Body", tip="Profile")])
    same(fingerprint(session.call("inspect", document=doc)), baseline)
    state = transaction(session, doc, "Native expression dependency", [dict(op="expression", object="Boss", property="Height", expression="Base.Height / 2")])
    baseline = fingerprint(state)
    session.call("transaction", document=doc, label="Reject native expression cycle", expect_error="history_dependency",
        edits=history(compiler, state, op="body_reorder", object="Base", target="Boss", after=True, tip="Finish"))
    same(fingerprint(session.call("inspect", document=doc)), baseline)
    session.call("close", document=doc)
    return {"face_attached_dependency_rejected": True, "expression_cycle_rejected": True,
            "native_tip_eligibility_recompute": True, "atomic_restoration_with_current_revision": True}


def reference_and_transform_members(session, compiler, output):
    doc, state = seed(session, "NativeHistoryReferences", transformed=True)
    state = transaction(session, doc, "Construction sketch and native datum", [
        add("Sketcher::SketchObject", "Guide", "Body"),
        dict(op="geometry", object="Guide", kind="line", start=[0, 0, 0], end=[3, 4, 0], construction=True),
        add("PartDesign::Plane", "Plane", "Body")])
    baseline = fingerprint(state)
    for name in ["Guide", "Plane"]:
        assert objects(state)[name]["history_flags"] == {
            "can_tip": False, "can_reorder": True, "transform_step": False}
        state = transaction(session, doc, "Insert native reference after base", history(compiler, state,
            op="body_reorder", object=name, target="Base", after=True, tip="B"))
        native = objects(state)
        assert [member["link"] for member in native["Body"]["properties"]["Group"]][:2] == ["Base", name]
        assert native["Body"]["body_state"] == {"tip": "B", "display_object": "B", "consumed_by": []}
        assert native["A"]["properties"]["BaseFeature"] == link("Base")
        assert native["B"]["properties"]["BaseFeature"] == link("A")
        near(native["Body"]["shape"]["volume"], 1043)
        changed = fingerprint(state)
        same(fingerprint(session.call("undo", document=doc)), baseline)
        same(fingerprint(session.call("redo", document=doc)), changed)
        state = session.call("undo", document=doc)
    session.call("close", document=doc)

    doc = session.call("new", name="NativeHistoryTransformChild")["document"]
    state = transaction(session, doc, "Native pattern originals", [add("PartDesign::Body", "Body"),
        *transforms.feature(compiler, "Base", "AdditiveBox", Length=20, Width=20, Height=2,
                            Placement=transforms.placement(-10, -10)),
        *transforms.feature(compiler, "Protrusion", "AdditiveBox", Length=2, Width=2, Height=4,
                            Placement=transforms.placement(2, 2, 2))])
    state = transaction(session, doc, "Native transformation steps", transforms.compile_task(compiler,
        transforms.task([transforms.linear(True), transforms.mirror(True)], create=True)))
    baseline = fingerprint(state)
    expected_flags = {"can_tip": False, "can_reorder": True, "transform_step": True}
    assert objects(state)["Linear"]["history_flags"] == expected_flags
    state = transaction(session, doc, "Move child row within native Body history", history(compiler, state,
        op="body_reorder", object="Linear", target="Base", after=False, tip="Result"))
    transforms.check(state, 864, ("Linear", "Mirror"))
    assert objects(state)["Body"]["properties"]["Group"][0] == link("Linear")
    assert objects(state)["Linear"]["history_flags"] == expected_flags
    changed = fingerprint(state)
    same(fingerprint(session.call("undo", document=doc)), baseline)
    state = session.call("redo", document=doc)
    same(fingerprint(state), changed)
    transforms.check(state, 864, ("Linear", "Mirror"))
    path = str((output / "reordered-transform-child.FCStd").resolve())
    session.call("save", document=doc, path=path)
    session.call("close", document=doc)
    state = session.call("open", path=path)
    same(fingerprint(state), changed)
    transforms.check(state, 864, ("Linear", "Mirror"))
    session.call("close", document=state["document"])
    return {"moved_body_sketch_and_datum_insertion": True, "solid_chain_preserved": True,
            "native_transform_child_flags": expected_flags, "transform_step_order_preserved": True,
            "transform_volume": 864, "undo_redo_and_reopen": True}


def leases(session, compiler):
    doc, baseline = seed(session, "NativeHistoryLeases")
    state = transaction(session, doc, "Native unfinished sketch", [add("Sketcher::SketchObject", "Sketch", "Body")])
    edits = history(compiler, state, op="body_tip", object="Body", tip="Base")
    revision = state["revision"]
    session.call("preview", document=doc, token="history_preview", edits=edits, expect_error="history_busy")
    aliases = [{"kind": edit["op"], **{key: value for key, value in edit.items() if key != "op"}} for edit in edits]
    session.call("preview", document=doc, token="history_alias_preview", edits=aliases, expect_error="history_busy")
    state = session.call("inspect", document=doc)
    assert state["revision"] == revision and state["preview"] is None
    preview_state = session.call("preview", document=doc, token="native_shape_preview", edits=[set_value("Base", "Length", 11)])["state"]
    session.call("transaction", document=doc, label="History during another preview", edits=edits, expect_error="preview_active")
    session.call("preview", document=doc, token="native_shape_preview", edits=edits, expect_error="history_busy")
    session.call("preview", document=doc, token="native_shape_preview", edits=aliases, expect_error="history_busy")
    retained = session.call("inspect", document=doc)
    assert retained["revision"] == preview_state["revision"] and retained["preview"] == preview_state["preview"]
    same(fingerprint(retained), fingerprint(preview_state))
    state = session.call("preview_cancel", document=doc, token="native_shape_preview")
    near(objects(state)["Body"]["shape"]["volume"], 1043)
    session.call("sketch_begin", document=doc, token="native_sketch_session", object="Sketch")
    session.call("transaction", document=doc, label="History during unfinished sketch", edits=edits, expect_error="sketch_active")
    session.call("preview", document=doc, token="history_sketch_preview", edits=edits, expect_error="sketch_active")
    state = session.call("sketch_cancel", document=doc, token="native_sketch_session")
    near(objects(state)["Body"]["shape"]["volume"], 1043)
    state = transaction(session, doc, "Native history alias", aliases)
    assert objects(state)["Body"]["body_state"]["tip"] == "Base"
    near(objects(state)["Body"]["shape"]["volume"], 1000)
    session.call("close", document=doc)
    return {"history_preview_denied_before_mutation": True, "active_preview_lease": True,
            "sketch_write_lease": True, "history_alias_guard_and_dispatch": True}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    session = SketchSession(args.host, json.loads(args.runtime.read_text()))
    try:
        report = {"tip": tip_workflow(session, args.compiler, args.output),
                  "reorder": [reorder_workflow(session, args.compiler, args.output, transformed) for transformed in (False, True)],
                  "insertion": insertion_and_native_defaults(session, args.compiler),
                  "dependencies": failed_dependencies(session, args.compiler),
                  "members": reference_and_transform_members(session, args.compiler, args.output),
                  "leases": leases(session, args.compiler)}
        (args.output / "history-report.json").write_text(json.dumps(report, indent=2))
        print(json.dumps(report, indent=2))
    finally:
        session.close()


if __name__ == "__main__":
    main()
