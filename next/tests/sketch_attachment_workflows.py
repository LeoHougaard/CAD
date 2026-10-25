"""Native sketch attachment options through the editor's shared Rust compiler."""
import argparse
import json
import math
import os
from pathlib import Path
import subprocess
import sys

from history_workflows import compile_edits, fingerprint, same
from native_workflows import add, objects, rectangle, set_value, transaction
from sketch_editor_workflows import SketchSession


def signature(state):
    sketch = objects(state)["Sketch"]
    props = sketch["properties"]
    return dict(placement=props["Placement"], global_placement=sketch["global_placement"],
                offset=props["AttachmentOffset"], reversed=props["MapReversed"],
                mode=props["MapMode"], support=props["AttachmentSupport"],
                expressions=sketch["expressions"], body=sketch["body"],
                pad_volume=objects(state).get("Pad", {}).get("shape", {}).get("volume"))


def equal(actual, expected, path="root"):
    if isinstance(actual, (int, float)) and not isinstance(actual, bool):
        assert isinstance(expected, (int, float)) and math.isclose(actual, expected, rel_tol=1e-9, abs_tol=1e-8), (path, actual, expected)
    elif isinstance(actual, dict):
        assert actual.keys() == expected.keys(), (path, actual.keys(), expected.keys())
        for key in actual:
            equal(actual[key], expected[key], path + "." + key)
    elif isinstance(actual, list):
        assert len(actual) == len(expected), (path, actual, expected)
        for i, (a, b) in enumerate(zip(actual, expected)):
            equal(a, b, path + "[" + str(i) + "]")
    else:
        assert actual == expected, (path, actual, expected)


def native_controls(manifest, output):
    import FreeCAD as App
    import Part, PartDesign, Sketcher
    def place(p):
        return App.Placement(App.Vector(*p["position"]), App.Rotation(*p["rotation"]))
    def enc(p):
        return dict(position=list(p.Base), rotation=list(p.Rotation.Q))
    results = []
    for case in json.loads(manifest.read_text(encoding="utf-8")):
        doc = App.openDocument(case["baseline"])
        body = doc.getObject("Body")
        request = case["request"]
        sketch = body.newObject("Sketcher::SketchObject", "Sketch")
        support = request["support"]
        if support["kind"] == "origin":
            source = next(o for o in body.Origin.OriginFeatures if getattr(o, "Role", None) == support["role"])
            subs = [""]
            mode = "FlatFace"
        else:
            source = doc.getObject(support["object"])
            subs = support["subelements"]
            mode = support["mode"]
        sketch.AttachmentSupport = [(source, subs)]
        sketch.MapMode = mode
        if "reversed" in request:
            sketch.MapReversed = request["reversed"]
        if "offset" in request:
            sketch.AttachmentOffset = place(request["offset"])
        if "offset_expression" in request:
            sketch.setExpression("AttachmentOffset.Base.z", request["offset_expression"])
        doc.recompute()
        if request.get("detach"):
            sketch.positionBySupport()
            frozen = sketch.Placement.copy()
            sketch.MapMode = "Deactivated"
            sketch.AttachmentSupport = []
            sketch.Placement = frozen
        if case.get("parent_height") is not None:
            doc.getObject("Base").Height = case["parent_height"]
        pad_volume = None
        if case.get("pad"):
            for start, end in [((0,0,0),(4,0,0)),((4,0,0),(4,3,0)),
                               ((4,3,0),(0,3,0)),((0,3,0),(0,0,0))]:
                sketch.addGeometry(Part.LineSegment(App.Vector(*start), App.Vector(*end)), False)
            pad = body.newObject("PartDesign::Pad", "Pad")
            pad.Profile = (sketch, [])
            pad.Length = 5
            body.Tip = pad
        doc.recompute()
        if case.get("pad"):
            assert pad.Shape.isValid() and len(pad.Shape.Solids) == 1
            pad_volume = pad.Shape.Volume
        measured = dict(placement=enc(sketch.Placement), global_placement=enc(sketch.getGlobalPlacement()),
                        offset=enc(sketch.AttachmentOffset), reversed=sketch.MapReversed,
                        mode=str(sketch.MapMode), support=[dict(link=o.Name, subelements=list(s)) for o, s in sketch.AttachmentSupport],
                        expressions=dict(sketch.ExpressionEngine), body=body.Name, pad_volume=pad_volume)
        equal(case["actual"], measured, case["name"])
        results.append(dict(name=case["name"], matched=True, native=measured))
        App.closeDocument(doc.Name)
    output.write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    runtime = json.loads(args.runtime.read_text(encoding="utf-8-sig"))
    session = SketchSession(args.host.resolve(), runtime)
    controls = []
    try:
        assert "detach_sketch" in session.capabilities["edit_operations"]
        cases = [(role, reversed, detach) for role in ["XY_Plane", "XZ_Plane", "YZ_Plane"]
                 for reversed in [False, True] for detach in [False, True]]
        cases += [("face", False, detach) for detach in [False, True]]
        cases += [("formula", True, detach) for detach in [False, True]]
        for number, (role, reversed, detach) in enumerate(cases):
            name = f"attachment{number}"
            doc = session.call("new", name=name)["document"]
            state = transaction(session, doc, "Native support and transformed Body", [
                add("PartDesign::Body", "Body"),
                add("PartDesign::AdditiveBox", "Base", "Body") if role == "face" else add("Part::Box", "Base"),
                set_value("Base", "Length", 10), set_value("Base", "Width", 8), set_value("Base", "Height", 6),
                set_value("Body", "Placement", dict(position=[10, 20, 30], rotation=[0, 0, math.sin(.3), math.cos(.3)]))])
            baseline = out / (name + "-input.FCStd")
            session.call("save", document=doc, path=str(baseline))
            before = fingerprint(session.call("inspect", document=doc))
            support = dict(kind="origin", role=role if role.endswith("Plane") else "XY_Plane")
            if role == "face":
                face = max(session.call("topology", document=doc, object="Base")["faces"], key=lambda f: f["center"][2])["subelement"]
                support = dict(kind="reference", object="Base", subelements=[face], mode="FlatFace")
            request = dict(name="Sketch", body="Body", create_body=False, support=support,
                           reversed=reversed, detach=detach,
                           offset=dict(position=[1, 2, 3], rotation=[0, 0, math.sin(math.pi/12), math.cos(math.pi/12)]))
            if role == "formula":
                request["offset_expression"] = "Base.Height / 2"
            edits = compile_edits(args.compiler, "--native-sketch-create", dict(state=state, request=request))
            state = transaction(session, doc, "Create configured sketch", edits)
            created = signature(state)
            same(fingerprint(session.call("undo", document=doc)), before)
            state = session.call("redo", document=doc)
            equal(signature(state), created)
            changed = transaction(session, doc, "Parent change follows native attachment policy", [set_value("Base", "Height", 12)])
            if role != "face":
                pad = compile_edits(args.compiler, "--native-properties", dict(type="PartDesign::Pad", name="Pad", body="Body", create=True,
                    properties=dict(Profile=dict(link="Sketch"), Length=5)))
                changed = transaction(session, doc, "Use configured native sketch for a solid", [*rectangle("Sketch", 4, 3), *pad])
                assert abs(objects(changed)["Pad"]["shape"]["volume"]-60) < 1e-7
            controls.append(dict(name=name, baseline=str(baseline), request=request, actual=signature(changed), parent_height=12, pad=role != "face"))
            path = out / (name + "-result.FCStd")
            session.call("save", document=doc, path=str(path))
            session.call("close", document=doc)
            reopened = session.call("open", path=str(path))
            equal(signature(reopened), signature(changed))
            session.call("close", document=reopened["document"])
        # A failed expression after actual object creation must restore the
        # entire native document, not leave a partially configured sketch.
        doc = session.call("new", name="AttachmentFailure")["document"]
        state = transaction(session, doc, "Body baseline", [add("PartDesign::Body", "Body")])
        before = fingerprint(state)
        request = dict(name="Sketch", body="Body", create_body=False,
                       support=dict(kind="origin", role="XY_Plane"), offset_expression="Missing.Height", detach=True)
        edits = compile_edits(args.compiler, "--native-sketch-create", dict(state=state, request=request))
        try:
            session.call("transaction", document=doc, label="Rejected attachment expression", edits=edits)
        except AssertionError as error:
            assert "Missing" in str(error) or "expression" in str(error), error
        else:
            raise AssertionError("Undefined native expression unexpectedly accepted")
        state = session.call("inspect", document=doc)
        same(fingerprint(state), before)
        request["offset_expression"] = "0.5 in"
        edits = compile_edits(args.compiler, "--native-sketch-create", dict(state=state, request=request))
        state = transaction(session, doc, "Correct native unit expression", edits)
        assert abs(objects(state)["Sketch"]["properties"]["AttachmentOffset"]["position"][2]-12.7) < 1e-8
        session.call("close", document=doc)
    finally:
        session.close()
    manifest = out / "native-controls.json"
    manifest.write_text(json.dumps(controls, indent=2) + "\n", encoding="utf-8")
    native_config = out / "native-config"
    native_config.mkdir()
    with (out / "native-control.log").open("x", encoding="utf-8") as log:
        result = subprocess.run([runtime["executable"], *runtime["args"], str(Path(__file__).resolve()),
                                 "--native-controls", str(manifest), str(out / "native-comparison.json")],
                                stdout=log, stderr=subprocess.STDOUT, timeout=90,
                                env=dict(os.environ, FREECAD_USER_HOME=str(native_config), PYTHONDONTWRITEBYTECODE="1"))
    assert result.returncode == 0, (out / "native-control.log").read_text()[-4000:]
    report = dict(native_comparisons=len(controls), failed_creation_rollback=True, corrected_native_units=True, passed=True)
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report))


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--native-controls":
        native_controls(Path(sys.argv[2]), Path(sys.argv[3]))
    else:
        main()
