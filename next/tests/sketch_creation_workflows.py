"""Native sketch support creation through the same Rust builder as the editor."""
import argparse
import json
import math
from pathlib import Path
import subprocess

from history_workflows import compile_edits, fingerprint, same
from native_workflows import add, link, near, objects, rectangle, set_value, solid, transaction
from sketch_editor_workflows import SketchSession


def creation(compiler, state, name, body, support, create_body=False):
    return compile_edits(compiler, "--native-sketch-create", {"state": state, "request": {
        "name": name, "body": body, "create_body": create_body, "support": support}})


def origin(role):
    return dict(kind="origin", role=role)


def reference(obj, subelement="", mode="FlatFace"):
    return dict(kind="reference", object=obj, subelements=[subelement], mode=mode)


def feature(compiler, name, kind, **properties):
    return compile_edits(compiler, "--native-properties", dict(
        type="PartDesign::" + kind, name=name, body="Body", create=True, properties=properties))


def check_support(state, name, source, subelement, mode):
    sketch = objects(state)[name]
    assert sketch["valid"] and state["available"]
    assert sketch["properties"]["AttachmentSupport"] == [link(source, subelement)]
    assert sketch["properties"]["MapMode"] == mode


def origin_workflows(session, compiler, output):
    results = []
    for role in ["XY_Plane", "XZ_Plane", "YZ_Plane"]:
        doc = session.call("new", name="CreatedOn" + role)["document"]
        state = transaction(session, doc, "Reserve native origin names", [add("PartDesign::Body", "ExistingBody")])
        original_names = {obj["name"] for obj in state["objects"]}
        state = transaction(session, doc, "Body and attached sketch in one native transaction",
            creation(compiler, state, "Sketch", "Body", origin(role), create_body=True))
        source = objects(state)["Sketch"]["properties"]["AttachmentSupport"][0]["link"]
        assert source != role  # ExistingBody has already reserved the unsuffixed name.
        assert objects(state)[source]["properties"]["Role"] == role
        assert objects(state)[source]["body"] == "Body"
        check_support(state, "Sketch", source, "", "FlatFace")
        state = session.call("undo", document=doc)
        assert {obj["name"] for obj in state["objects"]} == original_names
        state = session.call("redo", document=doc)
        check_support(state, "Sketch", source, "", "FlatFace")
        state = transaction(session, doc, "Native closed profile and Pad", [*rectangle("Sketch", 4, 3),
            *feature(compiler, "Pad", "Pad", Profile=link("Sketch"), Length=5),
            set_value("Body", "Placement", dict(position=[100, 200, 300],
                      rotation=[0, 0, math.sqrt(.5), math.sqrt(.5)]))])
        solid(state, "Body", 60)
        native = objects(state)
        for actual, expected in zip(native["Sketch"]["global_placement"]["position"], [100, 200, 300]):
            near(actual, expected)
        assert native["Body"]["body_state"]["tip"] == "Pad"
        baseline = fingerprint(state)
        state = session.call("undo", document=doc)
        assert "Pad" not in objects(state) and objects(state)["Sketch"]["sketch"]["geometry_count"] == 0
        state = session.call("redo", document=doc)
        same(fingerprint(state), baseline)
        path = str((output / (role + ".FCStd")).resolve())
        session.call("save", document=doc, path=path)
        session.call("close", document=doc)
        state = session.call("open", path=path)
        doc = state["document"]
        check_support(state, "Sketch", source, "", "FlatFace")
        solid(state, "Body", 60)
        state = transaction(session, doc, "Edit native profile dimension after reopening", [
            dict(op="datum", object="Sketch", index=9, value="6 mm")])
        solid(state, "Body", 90)
        solid(session.call("undo", document=doc), "Body", 60)
        solid(session.call("redo", document=doc), "Body", 90)
        session.call("close", document=doc)
        results.append(dict(role=role, native_support=source, moved_body=True,
                            atomic_body_creation_undo_redo=True, reopened_edit_volume=90))
    return results


def support_workflows(session, compiler, output):
    results = []
    for selected, subelement, mode in [("Datum", "", "FlatFace"), ("Datum", "Face1", "FlatFace"),
                                       ("SourceSketch", "", "ObjectXY"), ("Body", "top", "FlatFace")]:
        doc = session.call("new", name="NativeSketchSupport" + selected + ("Face" if subelement else "Whole"))["document"]
        state = transaction(session, doc, "Native support parents", [add("PartDesign::Body", "Body"),
            *feature(compiler, "Base", "AdditiveBox", Length=20, Width=20, Height=10),
            set_value("Body", "Placement", dict(position=[100, 200, 300],
                      rotation=[0, 0, math.sqrt(.5), math.sqrt(.5)]))])
        face = max(session.call("topology", document=doc, object="Base")["faces"], key=lambda f: f["center"][2])["subelement"]
        state = transaction(session, doc, "Attach native datum to parent face",
            feature(compiler, "Datum", "Plane", AttachmentSupport=[link("Base", face)], MapMode="FlatFace"))
        state = transaction(session, doc, "Native source sketch frame",
            creation(compiler, state, "SourceSketch", "Body", reference("Datum")))
        if subelement == "top":
            subelement = face
        state = transaction(session, doc, "Create sketch on chosen native support",
            creation(compiler, state, "Sketch", "Body", reference(selected, subelement, mode)))
        actual_source = "Base" if selected == "Body" else selected
        check_support(state, "Sketch", actual_source, subelement, mode)
        state = transaction(session, doc, "Closed native profile and supported Pad", [*rectangle("Sketch", 4, 3),
            *feature(compiler, "Pad", "Pad", Profile=link("Sketch"), Length=5)])
        solid(state, "Body", 4060)
        state = transaction(session, doc, "Edit support parent dimension", [set_value("Base", "Height", 12)])
        solid(state, "Body", 4860)
        near(objects(state)["Sketch"]["properties"]["Placement"]["position"][2], 12)
        baseline = fingerprint(state)
        path = str((output / (selected + ("Face" if subelement else "Whole") + ".FCStd")).resolve())
        session.call("save", document=doc, path=path)
        session.call("close", document=doc)
        state = session.call("open", path=path)
        doc = state["document"]
        same(fingerprint(state), baseline)
        state = transaction(session, doc, "Edit reopened native support parent", [set_value("Base", "Height", 14)])
        solid(state, "Body", 5660)
        near(objects(state)["Sketch"]["properties"]["Placement"]["position"][2], 14)
        solid(session.call("undo", document=doc), "Body", 4860)
        solid(session.call("redo", document=doc), "Body", 5660)
        session.call("close", document=doc)
        results.append(dict(selected=selected, native_support=actual_source, subelement=subelement, mode=mode,
                            parent_volume=4860, reopened_parent_volume=5660))
    return results


def guards(session, compiler):
    doc = session.call("new", name="NativeSketchCreationGuards")["document"]
    state = transaction(session, doc, "Native support ownership", [add("PartDesign::Body", "Body"),
        add("PartDesign::Line", "Axis", "Body"), add("PartDesign::Body", "OtherBody"),
        add("PartDesign::Plane", "ForeignPlane", "OtherBody")])
    before, revision, count = fingerprint(state), state["revision"], state["undo_count"]
    for support in [reference("ForeignPlane"), reference("Axis"), origin("XY_Plane001")]:
        result = subprocess.run([str(compiler), "--native-sketch-create", "-"], text=True, capture_output=True,
            input=json.dumps(dict(state=state, request=dict(name="Rejected", body="Body", create_body=False, support=support))), timeout=15)
        assert result.returncode != 0 and result.stderr
    state = session.call("inspect", document=doc)
    assert state["revision"] == revision and state["undo_count"] == count
    same(fingerprint(state), before)
    for bad in [dict(op="add_object", type="Sketcher::SketchObject", name="BadSketch", body="Body", origin_plane="bad"),
                dict(op="add_object", type="Part::Box", name="BadBox", body="Body", origin_plane="XY_Plane"),
                dict(op="add_object", type="Sketcher::SketchObject", name="BadSketch", origin_plane="XY_Plane")]:
        session.call("transaction", document=doc, label="Reject malformed native origin attachment", edits=[bad], expect_error="invalid_request")
        state = session.call("inspect", document=doc)
        same(fingerprint(state), before)
        assert state["available"]
    # A later invalid edit must also remove a newly created Body, its generated
    # Origin and its attached Sketch when the one native transaction aborts.
    compiled = creation(compiler, state, "NewSketch", "NewBody", origin("XZ_Plane"), create_body=True)
    session.call("transaction", document=doc, label="Abort body and attached sketch creation", expect_error="unsupported_property",
        edits=compiled + [set_value("NewSketch", "Missing", 1)])
    same(fingerprint(session.call("inspect", document=doc)), before)
    session.call("close", document=doc)
    doc = session.call("new", name="NativePlanarityGuard")["document"]
    state = transaction(session, doc, "Native cylindrical support", [add("PartDesign::Body", "Body"),
        *feature(compiler, "Cylinder", "AdditiveCylinder", Radius=5, Height=10)])
    face = next(f["subelement"] for f in session.call("topology", document=doc, object="Cylinder")["faces"]
                if "Cylinder" in f["geometry_type"])
    before = fingerprint(state)
    session.call("transaction", document=doc, label="Native FlatFace rejects nonplanar support", expect_error="invalid_model",
        edits=creation(compiler, state, "NonplanarSketch", "Body", reference("Cylinder", face)))
    restored = session.call("inspect", document=doc)
    assert restored["available"]
    same(fingerprint(restored), before)
    session.call("close", document=doc)
    return dict(no_silent_xy_fallback=True, native_origin_payload_guard=True,
                atomic_failed_new_body=True, native_planarity_guard=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["host", "compiler", "runtime", "output"]:
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    session = SketchSession(args.host, json.loads(args.runtime.read_text(encoding="utf-8-sig")))
    try:
        report = dict(origins=origin_workflows(session, args.compiler, args.output),
                      supports=support_workflows(session, args.compiler, args.output), guards=guards(session, args.compiler))
        (args.output / "sketch-creation-report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(report, indent=2))
    finally:
        session.close()


if __name__ == "__main__":
    main()
