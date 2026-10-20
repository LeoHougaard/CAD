"""Verify native gesture ownership, replacement, rollback and one-step history."""

import argparse
import json
from pathlib import Path

from native_workflows import Session, add, link, near, objects, rectangle, set_value, solid, transaction


class PreviewSession(Session):
    def call(self, op, expect_error=None, **parameters):
        if op in {"preview", "preview_commit", "preview_cancel"}:
            document = parameters.get("document")
            if document in self.revisions:
                parameters.setdefault("expected_revision", self.revisions[document])
        return super().call(op, expect_error=expect_error, **parameters)


def workflow(session, output):
    doc = session.call("new", name="GestureEditing")["document"]
    base = transaction(session, doc, "Base box", [add("Part::Box", "Box"),
        set_value("Box", "Length", 20), set_value("Box", "Width", 30),
        set_value("Box", "Height", 40)])
    solid(base, "Box", 24000)
    baseline_undo = base["undo_count"]
    other_doc = session.call("new", name="OtherAgent")["document"]
    token = "gesture-1"

    def preview(edits, **parameters):
        return session.call("preview", document=doc, token=token, label="Resize box",
                            edits=edits, **parameters)

    # Every replacement starts from the original baseline. The first temporary
    # object disappears even though it is absent from the replacement edit list.
    first = preview([add("Part::Sphere", "TemporarySphere"), set_value("Box", "Length", 50)])
    assert first["state"]["preview"] == {"token": token, "label": "Resize box"}
    state = preview([set_value("Box", "Length", 60)])["state"]
    assert "TemporarySphere" not in objects(state)
    solid(state, "Box", 72000)
    revision = state["revision"]
    session.call("preview", document=doc, token="other-agent", edits=[set_value("Box", "Width", 1)],
                 expect_error="preview_owner")
    session.call("preview_commit", document=doc, token="other-agent", expect_error="preview_owner")
    session.call("preview_cancel", document=doc, token=token, expected_revision=revision - 1,
                 expect_error="stale_revision")
    for op, args in [("transaction", {"edits": [set_value("Box", "Length", 1)]}),
                     ("undo", {}), ("redo", {}), ("save", {"path": str(output / "denied.FCStd")}),
                     ("close", {})]:
        session.call(op, document=doc, expect_error="preview_active", **args)
    assert not (output / "denied.FCStd").exists()
    session.call("transaction", document=other_doc, edits=[add("Part::Box", "Box")],
                 expect_error="preview_active")
    session.call("new", name="DeniedNewDocument", expect_error="preview_active")
    session.call("new", name="DeniedNewDocument", document=doc, expect_error="preview_active")
    session.call("open", path=str(output / "unused.FCStd"), document=doc, expect_error="preview_active")
    session.call("inspect", document=other_doc)

    # Invalid geometry is never exposed. The last valid candidate is recreated
    # inside the same native document. Revision advances because recreating
    # candidate objects can change persistent element aliases.
    preview([set_value("Box", "Length", 80), add("Part::Cut", "InvalidCut")], expect_error="invalid_model")
    restored = session.call("inspect", document=doc)
    assert restored["revision"] > revision and restored["preview"]["token"] == token
    session.call("preview_commit", document=doc, token=token, expected_revision=revision,
                 expect_error="stale_revision")
    revision = restored["revision"]
    solid(restored, "Box", 72000)
    assert "InvalidCut" not in objects(restored)
    for op in ("mesh", "topology"):
        display = session.call(op, document=doc, object="Box")
        assert display["revision"] == revision and display["preview"]["token"] == token
        if op == "topology":
            assert all(len(edge["points"]) == 2 for edge in display["edges"])

    committed = session.call("preview_commit", document=doc, token=token)
    assert committed["preview"] is None and committed["undo_count"] == baseline_undo + 1
    solid(committed, "Box", 72000)
    solid(session.call("undo", document=doc), "Box", 24000)
    before = session.call("inspect", document=doc)
    preview([set_value("Box", "Width", 90)], expect_error="preview_redo_conflict")
    after = session.call("inspect", document=doc)
    assert before["redo_count"] == after["redo_count"] == 1
    assert before["revision"] == after["revision"] and after["preview"] is None
    solid(session.call("redo", document=doc), "Box", 72000)
    preview([set_value("Box", "Length", 10)])
    canceled = session.call("preview_cancel", document=doc, token=token)
    assert canceled["preview"] is None and canceled["undo_count"] == baseline_undo + 1
    solid(canceled, "Box", 72000)
    solid(session.call("undo", document=doc), "Box", 24000)
    solid(session.call("redo", document=doc), "Box", 72000)
    session.call("preview_commit", document=doc, token=token, expect_error="no_preview")

    # Failure before any accepted preview leaves the document unowned and usable.
    revision = session.revisions[doc]
    preview([add("Part::Cut", "InvalidCut")], expect_error="invalid_model")
    state = session.call("inspect", document=doc)
    assert state["preview"] is None and state["revision"] > revision
    transaction(session, doc, "Add curved edge", [add("Sketcher::SketchObject", "Sketch"),
        dict(op="geometry", object="Sketch", kind="circle", center=[0, 0, 0], radius=3)])
    edge = session.call("topology", document=doc, object="Sketch")["edges"][0]
    assert edge["closed"] and len(edge["points"]) > 8
    near(edge["points"][0][0], 3)
    assert max(point[1] for point in edge["points"]) > 2.8

    path = output / "gesture.FCStd"
    session.call("save", document=doc, path=str(path))
    session.call("close", document=doc)
    reopened = session.call("open", path=str(path))
    doc = reopened["document"]
    assert reopened["preview"] is None
    solid(reopened, "Box", 72000)
    state = transaction(session, doc, "Continue after reopen", [set_value("Box", "Length", 70)])
    solid(state, "Box", 84000)
    feature_doc = session.call("new", name="FeatureGesture")["document"]
    feature_edits = [add("PartDesign::Body", "Body"),
        add("Sketcher::SketchObject", "Sketch", "Body"), *rectangle("Sketch"),
        add("PartDesign::Pad", "Pad", "Body"), set_value("Pad", "Profile", link("Sketch")),
        set_value("Pad", "Length", 20)]
    first = session.call("preview", document=feature_doc, token="feature", edits=feature_edits)
    old_topology = session.call("topology", document=feature_doc, object="Pad")
    session.call("preview", document=feature_doc, token="feature", edits=[add("Part::Cut", "Invalid")],
                 expect_error="invalid_model")
    restored = session.call("inspect", document=feature_doc)
    assert restored["revision"] > first["revision"]
    solid(restored, "Pad", 24000)
    assert objects(restored)["Sketch"]["sketch"]["degrees_of_freedom"] == 0
    new_topology = session.call("topology", document=feature_doc, object="Pad")
    assert len(old_topology["faces"]) == len(new_topology["faces"]) == 6
    canceled = session.call("preview_cancel", document=feature_doc, token="feature")
    assert not canceled["objects"] and canceled["undo_count"] == 0
    transaction(session, feature_doc, "Create branch baseline", [add("Part::Box", "Box")])
    transaction(session, feature_doc, "Resize before undo", [set_value("Box", "Length", 20)])
    baseline = session.call("undo", document=feature_doc)
    assert baseline["redo_count"] == 1
    revision = baseline["revision"]
    session.call("transaction", document=feature_doc, edits=[], expect_error="invalid_request")
    untouched = session.call("inspect", document=feature_doc)
    assert untouched["revision"] == revision and untouched["redo_count"] == 1
    session.call("transaction", document=feature_doc, label="Failed native branch",
                 edits=[set_value("Box", "Width", 100), add("Part::Cut", "Invalid")],
                 expect_error="invalid_model")
    restored = session.call("inspect", document=feature_doc)
    assert restored["revision"] > revision and restored["undo_count"] == baseline["undo_count"]
    # FreeCAD destroys redo on entering the attempted branch even if it aborts.
    # This remains an explicit native limitation, not a silent same-revision change.
    assert restored["redo_count"] == 0
    near(solid(restored, "Box")["volume"], solid(baseline, "Box")["volume"])
    assert objects(restored)["Box"]["properties"]["Width"] == objects(baseline)["Box"]["properties"]["Width"]
    session.call("transaction", document=feature_doc, expected_revision=revision,
                 edits=[set_value("Box", "Width", 50)], expect_error="stale_revision")
    state = transaction(session, feature_doc, "Continue after failed branch", [set_value("Box", "Width", 50)])
    solid(state, "Box", 5000)
    revision = state["revision"]
    session.call("close", document=other_doc)
    remaining = session.call("inspect", document=feature_doc)
    assert remaining["revision"] == revision
    solid(remaining, "Box", 5000)
    # Closing a different native document neither closes nor rewinds this one.
    # The closed name can be reused without affecting remaining open documents.
    replacement = session.call("new", name=other_doc)
    assert replacement["document"] == other_doc and not replacement["objects"]
    session.call("close", document=other_doc)
    solid(session.call("inspect", document=doc), "Box", 84000)
    return {"document": doc, "committed_volume": 72000, "reopened_edit_volume": 84000,
            "circle_display_points": len(edge["points"])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", required=True, type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    runtime = json.loads(args.runtime.read_text(encoding="utf-8-sig"))
    session = PreviewSession(args.host.resolve(), runtime)
    try:
        result = workflow(session, args.output.resolve())
        report = dict(result, startup_ms=session.startup_ms, operations=session.timings)
        (args.output / "preview-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(result, indent=2))
    finally:
        session.close()


if __name__ == "__main__":
    main()
