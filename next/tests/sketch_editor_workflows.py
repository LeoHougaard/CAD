"""Exercise native sketch editing sessions, constraint states and trimming.

Every edit goes through the persistent Rust host into pinned native Sketcher.
"""
import argparse
import json
import math
from pathlib import Path

from native_workflows import add, circle, constraint, link, near, objects, rectangle, set_value, solid, transaction
from preview_workflows import PreviewSession


class SketchSession(PreviewSession):
    def __init__(self, *args):
        self.sketch_sessions = {}
        super().__init__(*args)

    def call(self, op, expect_error=None, **parameters):
        doc = parameters.get("document")
        if op in {"sketch_begin", "sketch_finish", "sketch_cancel"} and doc in self.revisions:
            parameters.setdefault("expected_revision", self.revisions[doc])
        if op in {"transaction", "undo", "redo", "preview", "preview_commit", "preview_cancel"} and doc in self.sketch_sessions:
            parameters.setdefault("sketch_token", self.sketch_sessions[doc])
        result = super().call(op, expect_error=expect_error, **parameters)
        if not expect_error:
            state = result.get("state", result)
            if "sketch_edit" in state:
                if state["sketch_edit"]:
                    self.sketch_sessions[state["document"]] = state["sketch_edit"]["token"]
                else:
                    self.sketch_sessions.pop(state["document"], None)
        return result


def flags_and_trim(session, output):
    doc = session.call("new", name="ConstraintStates")["document"]
    state = transaction(session, doc, "Dimensioned line and native point", [
        add("Sketcher::SketchObject", "Sketch"),
        dict(op="geometry", object="Sketch", kind="line", start=[0, 0, 0], end=[10, 0, 0]),
        dict(op="geometry", object="Sketch", kind="point", position=[2, 3, 0], construction=True),
        constraint("Sketch", "Distance", 0, 10), constraint("Sketch", "Horizontal", 0)])
    point = objects(state)["Sketch"]["sketch"]["geometry"][1]
    assert point["kind"] == "point" and point["position"] == [2, 3, 0] and point["construction"]
    baseline_dof = objects(state)["Sketch"]["sketch"]["degrees_of_freedom"]
    state = transaction(session, doc, "Reference dimension", [
        dict(op="constraint_state", object="Sketch", index=0, driving=False, name="Width", virtual=True)])
    sketch = objects(state)["Sketch"]["sketch"]
    assert not sketch["constraints"][0]["driving"] and sketch["constraints"][0]["virtual"]
    assert sketch["constraints"][0]["name"] == "Width" and sketch["degrees_of_freedom"] == baseline_dof + 1
    state = transaction(session, doc, "Move reference dimension", [
        dict(op="move_geometry", object="Sketch", index=0, point=2, position=[20, 0, 0])])
    solved = objects(state)["Sketch"]["sketch"]
    line = solved["geometry"][0]
    measured = math.dist(line["start"], line["end"])
    near(solved["constraints"][0]["value"], measured)
    assert measured > 10
    state = transaction(session, doc, "Driving dimension inactive", [
        dict(op="constraint_state", object="Sketch", index=0, driving=True, active=False, virtual=False)])
    assert not objects(state)["Sketch"]["sketch"]["constraints"][0]["active"]
    state = transaction(session, doc, "Reactivate dimension", [
        dict(op="constraint_state", object="Sketch", index=0, active=True),
        dict(op="datum", object="Sketch", index=0, value="25 mm")])
    near(objects(state)["Sketch"]["sketch"]["constraints"][0]["value"], 25)
    session.call("transaction", document=doc, edits=[dict(op="datum", object="Sketch", index=0, value="10 deg")],
                 expect_error="dimension_mismatch")
    state = session.call("inspect", document=doc)
    near(objects(state)["Sketch"]["sketch"]["constraints"][0]["value"], 25)
    session.call("transaction", document=doc, edits=[dict(op="constraint_state", object="Sketch", index=0, active="yes")],
                 expect_error="invalid_request")
    session.call("inspect", document=doc)
    transaction(session, doc, "Create trim intersections", [add("Sketcher::SketchObject", "Trim"),
        dict(op="geometry", object="Trim", kind="line", start=[-10, 0, 0], end=[10, 0, 0]),
        dict(op="geometry", object="Trim", kind="line", start=[0, -10, 0], end=[0, 10, 0])])
    state = transaction(session, doc, "Native trim", [dict(op="trim_geometry", object="Trim", index=0, position=[5, 0, 0])])
    near(objects(state)["Trim"]["sketch"]["geometry"][0]["end"][0], 0)
    state = session.call("undo", document=doc)
    near(objects(state)["Trim"]["sketch"]["geometry"][0]["end"][0], 10)
    state = session.call("redo", document=doc)
    near(objects(state)["Trim"]["sketch"]["geometry"][0]["end"][0], 0)
    path = output / "constraint-states.FCStd"
    freedoms = {name: obj["sketch"]["degrees_of_freedom"] for name, obj in objects(state).items()}
    session.call("save", document=doc, path=str(path))
    session.call("close", document=doc)
    state = session.call("open", path=str(path))
    doc = state["document"]
    for name, freedom in freedoms.items():
        assert objects(state)[name]["sketch"]["degrees_of_freedom"] == freedom
    assert objects(state)["Sketch"]["sketch"]["constraints"][0]["name"] == "Width"
    state = transaction(session, doc, "Edit restored named dimension", [dict(op="datum", object="Sketch", index=0, value="30 mm")])
    near(objects(state)["Sketch"]["sketch"]["constraints"][0]["value"], 30)
    return {"reference_and_driving": True, "trim_undo_redo": True, "native_points": True}


def consumed_profile(session, output):
    doc = session.call("new", name="ConsumedSketch")["document"]
    state = transaction(session, doc, "Constrained consumed profile", [add("PartDesign::Body", "Body"),
        add("Sketcher::SketchObject", "Sketch", "Body"), *rectangle("Sketch"),
        add("PartDesign::Pad", "Pad", "Body"), set_value("Pad", "Profile", link("Sketch")), set_value("Pad", "Length", 20)])
    face = next(face["subelement"] for face in session.call("topology", document=doc, object="Pad")["faces"]
                if face["normal"] and face["normal"][2] > .99)
    state = transaction(session, doc, "Attached constrained pocket", [add("Sketcher::SketchObject", "Hole", "Body"),
        set_value("Hole", "AttachmentSupport", link("Pad", face)), set_value("Hole", "MapMode", "FlatFace"),
        *circle("Hole", [10, 10, 0], 3), add("PartDesign::Pocket", "Pocket", "Body"),
        set_value("Pocket", "Profile", link("Hole")), set_value("Pocket", "Length", 5)])
    initial_undo = state["undo_count"]
    original_volume = 24000 - math.pi * 9 * 5
    solid(state, "Pocket", original_volume)
    token = "edit-consumed"
    begin = session.call("sketch_begin", document=doc, object="Sketch", token=token)
    assert begin["sketch_edit"] == {"token": token, "object": "Sketch"} and begin["undo_count"] == 0
    fixed = session.call("preview", document=doc, token="fixed-drag", edits=[
        dict(op="move_geometry", object="Sketch", index=0, point=2, position=[100, 100, 0])])["state"]
    assert objects(fixed)["Sketch"]["sketch"]["geometry"] == objects(begin)["Sketch"]["sketch"]["geometry"]
    session.call("preview_cancel", document=doc, token="fixed-drag")
    session.call("transaction", document=doc, edits=[constraint("Sketch", "Horizontal", 0)], expect_error="invalid_model")
    recovered = session.call("inspect", document=doc)
    assert recovered["undo_count"] == 0 and recovered["sketch_edit"]
    for op, args in [("save", {"path": str(output / "denied.FCStd")}), ("close", {}),
                     ("new", {"name": "DeniedDocument"})]:
        session.call(op, document=doc, expect_error="sketch_active", **args)
    session.call("transaction", document=doc, sketch_token="another-agent", edits=[dict(op="delete_geometry", object="Sketch", index=0)],
                 expect_error="sketch_owner")
    session.call("transaction", document=doc, edits=[set_value("Pad", "Length", 30)], expect_error="sketch_active")
    opened = transaction(session, doc, "Open consumed contour", [dict(op="delete_geometry", object="Sketch", index=0)])
    assert objects(opened)["Sketch"]["sketch"]["geometry_count"] == 3
    assert opened["undo_count"] == 1
    solid(opened, "Pocket", original_volume)
    context = session.call("mesh", document=doc, object="Pocket")
    assert context["revision"] == opened["revision"] and context["context_revision"] < context["revision"]
    session.call("sketch_finish", document=doc, token=token, expect_error="invalid_model")
    restored = session.call("inspect", document=doc)
    assert restored["sketch_edit"] and objects(restored)["Sketch"]["sketch"]["geometry_count"] == 3
    solid(restored, "Pocket", original_volume)
    state = session.call("undo", document=doc)
    assert objects(state)["Sketch"]["sketch"]["geometry_count"] == 4 and state["redo_count"] == 1
    state = session.call("redo", document=doc)
    assert objects(state)["Sketch"]["sketch"]["geometry_count"] == 3
    session.call("undo", document=doc)
    state = transaction(session, doc, "Correct with native width", [dict(op="datum", object="Sketch", index=9, value="50 mm")])
    assert state["undo_count"] == 1 and state["redo_count"] == 0
    # More than native FreeCAD's 20-entry undo limit remains cancelable because
    # the session keeps one native transaction and a bounded command journal.
    for width in range(51, 81):
        transaction(session, doc, "Adjust width", [dict(op="datum", object="Sketch", index=9, value=f"{width} mm")])
    state = session.call("sketch_finish", document=doc, token=token)
    assert state["sketch_edit"] is None and state["undo_count"] == initial_undo + 1
    solid(state, "Pocket", 80 * 30 * 20 - math.pi * 9 * 5)
    state = session.call("undo", document=doc)
    solid(state, "Pocket", original_volume)
    # Opening and canceling without edits preserves native redo exactly.
    count = state["redo_count"]
    session.call("sketch_begin", document=doc, object="Sketch", token=token)
    state = session.call("sketch_cancel", document=doc, token=token)
    assert state["redo_count"] == count
    state = session.call("redo", document=doc)
    solid(state, "Pocket", 80 * 30 * 20 - math.pi * 9 * 5)
    session.call("sketch_begin", document=doc, object="Sketch", token=token)
    for width in range(81, 111):
        transaction(session, doc, "Discard dimension edits", [dict(op="datum", object="Sketch", index=9, value=f"{width} mm")])
    transaction(session, doc, "Discard an open profile", [dict(op="delete_geometry", object="Sketch", index=0)])
    state = session.call("sketch_cancel", document=doc, token=token)
    assert state["undo_count"] == initial_undo + 1 and state["sketch_edit"] is None
    solid(state, "Pocket", 80 * 30 * 20 - math.pi * 9 * 5)
    path = output / "consumed-sketch-session.FCStd"
    session.call("save", document=doc, path=str(path))
    session.call("close", document=doc)
    state = session.call("open", path=str(path))
    doc = state["document"]
    session.call("sketch_begin", document=doc, object="Hole", token="edit-hole")
    transaction(session, doc, "Edit attached dimension after reopen", [dict(op="datum", object="Hole", index=2, value="8 mm")])
    state = session.call("sketch_finish", document=doc, token="edit-hole")
    solid(state, "Pocket", 80 * 30 * 20 - math.pi * 16 * 5)
    near(objects(state)["Hole"]["properties"]["Placement"]["position"][2], 20)
    return {"open_contour_finish_rejected": True, "session_actions": 31,
            "finish_native_undo_entries": 1, "reopened_attached_pocket_volume": objects(state)["Pocket"]["shape"]["volume"]}


def drag_session(session):
    doc = session.call("new", name="SketchDragSession")["document"]
    transaction(session, doc, "Sketch geometry", [add("Sketcher::SketchObject", "Sketch"),
        dict(op="geometry", object="Sketch", kind="line", start=[0, 0, 0], end=[10, 0, 0]),
        constraint("Sketch", "Horizontal", 0)])
    session.call("sketch_begin", document=doc, object="Sketch", token="drag-edit")
    def drag(x, y):
        return session.call("preview", document=doc, token="drag", edits=[
            dict(op="move_geometry", object="Sketch", index=0, point=2, position=[x, y, 0])])["state"]
    drag(20, 10)
    state = drag(30, 15)
    near(objects(state)["Sketch"]["sketch"]["geometry"][0]["end"][0], 30)
    assert state["undo_count"] == 0
    session.call("preview_cancel", document=doc, token="drag")
    state = session.call("inspect", document=doc)
    near(objects(state)["Sketch"]["sketch"]["geometry"][0]["end"][0], 10)
    drag(20, 10)
    state = session.call("preview_commit", document=doc, token="drag")
    assert state["undo_count"] == 1
    state = session.call("undo", document=doc)
    near(objects(state)["Sketch"]["sketch"]["geometry"][0]["end"][0], 10)
    state = session.call("redo", document=doc)
    near(objects(state)["Sketch"]["sketch"]["geometry"][0]["end"][0], 20)
    session.call("sketch_finish", document=doc, token="drag-edit")
    session.call("sketch_begin", document=doc, object="Sketch", token="drag-edit")
    drag(50, 50)
    state = session.call("sketch_cancel", document=doc, token="drag-edit")
    assert state["sketch_edit"] is None and state["preview"] is None
    near(objects(state)["Sketch"]["sketch"]["geometry"][0]["end"][0], 20)
    return {"preview_replace_commit_cancel": True, "native_journal_undo_redo": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", required=True, type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    session = SketchSession(args.host.resolve(), json.loads(args.runtime.read_text(encoding="utf-8-sig")))
    try:
        report = {"constraint_states": flags_and_trim(session, args.output.resolve()),
                  "consumed_profile": consumed_profile(session, args.output.resolve()),
                  "drag": drag_session(session), "operations": session.timings}
        (args.output / "sketch-editor-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({key: value for key, value in report.items() if key != "operations"}, indent=2))
    finally:
        session.close()


if __name__ == "__main__":
    main()
