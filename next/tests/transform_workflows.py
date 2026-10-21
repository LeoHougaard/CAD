"""Atomic MultiTransform tasks through the same Rust compiler as the editor."""
import argparse
import json
import math
from pathlib import Path
import subprocess

from native_workflows import add, link, objects, set_value, solid, transaction
from preview_workflows import PreviewSession


def compile_task(compiler, parameters):
    completed = subprocess.run([str(compiler), "--native-properties", "-"],
                               input=json.dumps(parameters), capture_output=True,
                               text=True, check=True)
    return json.loads(completed.stdout)


def placement(x, y, z=0):
    return dict(position=[x, y, z], rotation=[0, 0, 0, 1])


def feature(compiler, name, kind, **properties):
    return compile_task(compiler, dict(type="PartDesign::" + kind, name=name,
                                      body="Body", create=True, properties=properties))


def linear(create=False, **properties):
    if create:
        properties = dict(Direction=link("X_Axis", ""), Length=4, Occurrences=2, **properties)
    return dict(type="PartDesign::LinearPattern", name="Linear", create=create,
                properties=properties, expressions={})


def mirror(create=False):
    return dict(type="PartDesign::Mirrored", name="Mirror", create=create,
                properties=dict(MirrorPlane=link("XZ_Plane", "")) if create else {}, expressions={})


def task(steps, create=False, tip="Result", visible=True, removed=()):
    return dict(type="PartDesign::MultiTransform", name="Result", body="Body",
                create=create, properties=dict(Originals=[link("Protrusion")]) if create else {},
                expressions={}, transformations=steps, removed_transformations=list(removed),
                restore_tip=tip, restore_tip_visibility=visible)


def check(state, volume, order=("Linear",), tip="Result", visible=True):
    native = objects(state)
    solid(state, tip, volume)
    assert native["Body"]["body_state"]["tip"] == tip
    assert native["Body"]["body_state"]["display_object"] == (tip if visible else None)
    assert native[tip]["properties"]["Visibility"] == visible
    assert native["Result"]["properties"]["BaseFeature"] == link("Protrusion")
    assert native["Result"]["properties"]["Transformations"] == [link(name) for name in order]
    for name in order:
        assert native[name]["properties"]["BaseFeature"] is None
        assert native[name]["properties"]["Originals"] == []


def preview(session, document, compiler, parameters, token):
    return session.call("preview", document=document, token=token,
                        label="Edit transformation steps",
                        edits=compile_task(compiler, parameters))["state"]


def workflows(session, compiler, output):
    document = session.call("new", name="AtomicTransforms")["document"]
    initial = transaction(session, document, "Base and protrusion", [
        add("PartDesign::Body", "Body"),
        *feature(compiler, "Base", "AdditiveBox", Length=20, Width=20, Height=2,
                 Placement=placement(-10, -10)),
        *feature(compiler, "Protrusion", "AdditiveBox", Length=2, Width=2, Height=4,
                 Placement=placement(2, 2, 2))])
    count = initial["undo_count"]
    check(preview(session, document, compiler, task([linear(True)], create=True), "create"), 832)
    check(preview(session, document, compiler, task([linear(True), mirror(True)], create=True), "create"),
          864, ("Linear", "Mirror"))
    check(preview(session, document, compiler, task([linear(True)], create=True), "create"), 832)
    assert "Mirror" not in objects(session.call("inspect", document=document))
    invalid = task([linear(True)], create=True)
    invalid["transformations"][0]["properties"]["Direction"] = link("MissingAxis", "")
    session.call("preview", document=document, token="create",
                 edits=compile_task(compiler, invalid), expect_error="not_found")
    check(session.call("inspect", document=document), 832)
    cancelled = session.call("preview_cancel", document=document, token="create")
    assert not {"Result", "Linear"}.intersection(objects(cancelled))
    assert cancelled["undo_count"] == count
    assert objects(cancelled)["Body"]["body_state"]["display_object"] == "Protrusion"
    preview(session, document, compiler, task([linear(True)], create=True), "create")
    committed = session.call("preview_commit", document=document, token="create")
    check(committed, 832)
    assert committed["undo_count"] == count + 1
    undone = session.call("undo", document=document)
    assert not {"Result", "Linear"}.intersection(objects(undone))
    assert objects(undone)["Body"]["body_state"]["display_object"] == "Protrusion"
    check(session.call("redo", document=document), 832)

    # One native task owns creation, all child edits and cancellation together.
    check(preview(session, document, compiler, task([linear(), mirror(True)]), "add"),
          864, ("Linear", "Mirror"))
    check(session.call("preview_cancel", document=document, token="add"), 832)
    assert "Mirror" not in objects(session.call("inspect", document=document))
    preview(session, document, compiler, task([linear(), mirror(True)]), "add")
    check(session.call("preview_commit", document=document, token="add"), 864, ("Linear", "Mirror"))
    reordered = transaction(session, document, "Reorder steps",
                            compile_task(compiler, task([mirror(), linear()])))
    check(reordered, 864, ("Mirror", "Linear"))
    check(session.call("undo", document=document), 864, ("Linear", "Mirror"))
    check(session.call("redo", document=document), 864, ("Mirror", "Linear"))
    removed = transaction(session, document, "Remove step",
                          compile_task(compiler, task([linear()], removed=["Mirror"])))
    check(removed, 832)
    assert "Mirror" not in objects(removed)
    check(session.call("undo", document=document), 864, ("Mirror", "Linear"))
    check(session.call("redo", document=document), 832)

    # Editing an earlier transformation must preserve a downstream solid's
    # ownership, visibility and Tip, even though adding children moves native Tip.
    state = transaction(session, document, "Downstream feature", feature(
        compiler, "After", "AdditiveBox", Length=2, Width=2, Height=3,
        Placement=placement(-8, -8, 2)))
    check(state, 844, tip="After")
    params = task([linear(), mirror(True)], tip="After")
    check(preview(session, document, compiler, params, "upstream"), 876,
          ("Linear", "Mirror"), tip="After")
    check(session.call("preview_cancel", document=document, token="upstream"), 844, tip="After")
    preview(session, document, compiler, params, "upstream")
    check(session.call("preview_commit", document=document, token="upstream"), 876,
          ("Linear", "Mirror"), tip="After")
    check(session.call("undo", document=document), 844, tip="After")
    check(session.call("redo", document=document), 876, ("Linear", "Mirror"), tip="After")
    hidden = transaction(session, document, "Hide downstream result", [set_value("After", "Visibility", False)])
    check(hidden, 876, ("Linear", "Mirror"), tip="After", visible=False)
    edited = transaction(session, document, "Edit hidden upstream child", compile_task(
        compiler, task([linear(Occurrences=3), mirror()], tip="After", visible=False)))
    check(edited, 908, ("Linear", "Mirror"), tip="After", visible=False)
    check(session.call("undo", document=document), 876, ("Linear", "Mirror"), tip="After", visible=False)
    check(session.call("redo", document=document), 908, ("Linear", "Mirror"), tip="After", visible=False)

    path = str((output / "atomic-transforms.FCStd").resolve())
    session.call("save", document=document, path=path)
    session.call("close", document=document)
    restored = session.call("open", path=path)
    document = restored["document"]
    check(restored, 908, ("Linear", "Mirror"), tip="After", visible=False)
    state = transaction(session, document, "Edit reopened child", compile_task(
        compiler, task([linear(Occurrences=2), mirror()], tip="After", visible=False)))
    check(state, 876, ("Linear", "Mirror"), tip="After", visible=False)
    session.call("save", document=document, path=path)
    session.call("close", document=document)
    restored = session.call("open", path=path)
    check(restored, 876, ("Linear", "Mirror"), tip="After", visible=False)
    session.call("close", document=restored["document"])

    document = session.call("new", name="AllTransformSteps")["document"]
    transaction(session, document, "Seed four native step types", [
        add("PartDesign::Body", "Body"),
        *feature(compiler, "Base", "AdditiveBox", Length=20, Width=20, Height=2,
                 Placement=placement(-10, -10)),
        *feature(compiler, "Protrusion", "AdditiveBox", Length=2, Width=2, Height=4,
                 Placement=placement(2, 2, 2))])
    polar = dict(type="PartDesign::PolarPattern", name="Polar", create=True,
                 properties=dict(Axis=link("Z_Axis", ""), Angle=180, Occurrences=2))
    scaled = dict(type="PartDesign::Scaled", name="Scaled", create=True,
                  properties=dict(Factor=1.1, Occurrences=2))
    order = ("Linear", "Polar", "Mirror", "Scaled")
    state = transaction(session, document, "Four native transformation types", compile_task(
        compiler, task([linear(True), polar, mirror(True), scaled], create=True)))
    check(state, 929.312, order)
    polar.update(create=False, properties={})
    scaled.update(create=False, properties=dict(Factor=1.2))
    edited = transaction(session, document, "Edit scaled child", compile_task(
        compiler, task([linear(), polar, mirror(), scaled])))
    volume = solid(edited, "Result")["volume"]
    assert not math.isclose(volume, 929.312, rel_tol=1e-7)
    assert objects(edited)["Scaled"]["properties"]["Factor"] == 1.2
    check(session.call("undo", document=document), 929.312, order)
    check(session.call("redo", document=document), volume, order)
    path = str((output / "all-transform-steps.FCStd").resolve())
    session.call("save", document=document, path=path)
    session.call("close", document=document)
    restored = session.call("open", path=path)
    check(restored, volume, order)
    assert objects(restored)["Scaled"]["properties"]["Factor"] == 1.2
    session.call("close", document=restored["document"])
    return dict(create_cancel_commit=True, preview_replacement_and_failed_candidate=True,
                ordered_child_editing=True, atomic_child_removal=True,
                undo_redo=True, upstream_preserves_tip_and_visibility=True, reopened_edit=True,
                all_four_native_step_types=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    session = PreviewSession(args.host, json.loads(args.runtime.read_text(encoding="utf-8")))
    try:
        report = dict(ok=True, checks=workflows(session, args.compiler, args.output))
    finally:
        session.close()
    (args.output / "transform-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
