"""Verify that local feature packages produce editable native document history.

Run after cargo build and setup_runtime.py. This uses the same package compiler
as the editor, then sends its output through the persistent native host.
"""

import argparse
import json
import math
from pathlib import Path
import subprocess

from native_workflows import Session, add, link, near, objects, set_value, solid, transaction


def compile_package(executable, package_id, parameters, directory=None, expect_error=False):
    command = [str(executable), "--id", package_id, "--parameters", json.dumps(parameters)]
    if directory is not None:
        command.extend(["--package-dir", str(directory)])
    result = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", timeout=15)
    if expect_error:
        assert result.returncode != 0, result.stdout
        return
    assert result.returncode == 0, result.stderr
    edits = json.loads(result.stdout)
    assert isinstance(edits, list) and edits
    return edits


def run(session, compiler, package_directory, output):
    rectangle_parameters = dict(object="Sketch", x=-5, y=3, width=40, height=30)
    rectangle = compile_package(compiler, "sketch.rectangle", rectangle_parameters)
    assert rectangle == compile_package(compiler, "sketch.rectangle", rectangle_parameters, package_directory)
    # Invalid package input is rejected before opening a native transaction.
    compile_package(compiler, "sketch.rectangle", dict(rectangle_parameters, width=0), expect_error=True)
    compile_package(compiler, "sketch.rectangle", dict(rectangle_parameters, script="exec()"), expect_error=True)

    origin_doc = session.call("new", name="PackageOrigin")["document"]
    origin = transaction(session, origin_doc, "Default origin and appended geometry", [
        add("Sketcher::SketchObject", "OriginSketch"),
        *compile_package(compiler, "sketch.rectangle", dict(object="OriginSketch", width=12, height=8)),
        *compile_package(compiler, "sketch.circle", dict(object="OriginSketch", index=4, radius=2))])
    origin_sketch = objects(origin)["OriginSketch"]["sketch"]
    assert origin_sketch["degrees_of_freedom"] == 0
    assert origin_sketch["geometry_count"] == 5
    session.call("close", document=origin_doc)

    doc = session.call("new", name="PackagePartStudio")["document"]
    state = transaction(session, doc, "Package rectangle and pad", [
        add("PartDesign::Body", "Body"), add("Sketcher::SketchObject", "Sketch", "Body"),
        *rectangle,
        *compile_package(compiler, "feature.pad", dict(name="Pad", body="Body", profile="Sketch", length=20))])
    solid(state, "Pad", 24000)
    assert objects(state)["Sketch"]["sketch"]["degrees_of_freedom"] == 0
    assert objects(state)["Sketch"]["sketch"]["constraint_count"] == 12
    bounds = objects(state)["Sketch"]["shape"]["bounds"]
    for actual, expected in zip(bounds["min"], [-5, 3, 0]):
        near(actual, expected)
    for actual, expected in zip(bounds["max"], [35, 33, 0]):
        near(actual, expected)

    top = next(face["subelement"] for face in session.call("topology", document=doc, object="Pad")["faces"]
               if face["normal"] and face["normal"][2] > .99)
    state = transaction(session, doc, "Package face sketch and pocket", [
        add("Sketcher::SketchObject", "HoleSketch", "Body"),
        set_value("HoleSketch", "AttachmentSupport", link("Pad", top)),
        set_value("HoleSketch", "MapMode", "FlatFace"),
        *compile_package(compiler, "sketch.circle", dict(object="HoleSketch", x=10, y=10, radius=3)),
        *compile_package(compiler, "feature.pocket", dict(name="Pocket", body="Body", profile="HoleSketch", length=5))])
    removed = math.pi * 9 * 5
    solid(state, "Pocket", 24000 - removed)
    assert objects(state)["HoleSketch"]["sketch"]["degrees_of_freedom"] == 0

    state = transaction(session, doc, "Native edit of package width", [
        dict(op="datum", object="Sketch", index=10, value="50 mm")])
    solid(state, "Pocket", 30000 - removed)
    near(objects(state)["HoleSketch"]["properties"]["Placement"]["position"][2], 20)
    topology = session.call("topology", document=doc, object="Pocket")
    vertical = next(edge["subelement"] for edge in topology["edges"]
                    if edge["bounds"]["max"][2] - edge["bounds"]["min"][2] > 19.9)
    fillet = compile_package(compiler, "feature.fillet", dict(name="Fillet", body="Body", source="Pocket", edge=vertical, radius=2))
    state = transaction(session, doc, "Package native fillet", fillet)
    expected = 30000 - removed - (1 - math.pi / 4) * 4 * 20
    solid(state, "Fillet", expected)
    undone = session.call("undo", document=doc)
    assert "Fillet" not in objects(undone)
    solid(session.call("redo", document=doc), "Fillet", expected)
    failed = compile_package(compiler, "feature.fillet", dict(name="Impossible", body="Body", source="Fillet", edge=vertical, radius=1000))
    session.call("transaction", document=doc, label="Reject package geometry", edits=failed, expect_error="invalid_model")
    assert "Impossible" not in objects(session.call("inspect", document=doc))

    path = output / "package-part-studio.FCStd"
    session.call("save", document=doc, path=str(path))
    session.call("close", document=doc)
    state = session.call("open", path=str(path))
    doc = state["document"]
    solid(state, "Fillet", expected)
    state = transaction(session, doc, "Edit native radius after reopen", [
        dict(op="datum", object="HoleSketch", index=2, value="4 mm")])
    expected_after_edit = 30000 - math.pi * 16 * 5 - (1 - math.pi / 4) * 4 * 20
    solid(state, "Fillet", expected_after_edit)
    session.call("save", document=doc, path=str(output / "package-part-studio-edited.FCStd"))
    return dict(document=doc, packages=5, volume=expected_after_edit,
                sketch_constraints=12, hole_constraints=3, native_history=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", required=True, type=Path)
    parser.add_argument("--compiler", required=True, type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--package-dir", type=Path, default=Path(__file__).resolve().parents[1] / "packages")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    runtime = json.loads(args.runtime.read_text(encoding="utf-8-sig"))
    session = Session(args.host.resolve(), runtime)
    try:
        result = run(session, args.compiler.resolve(), args.package_dir.resolve(), args.output.resolve())
        result["operations"] = session.timings
        (args.output / "report.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({key: value for key, value in result.items() if key != "operations"}, indent=2))
    finally:
        session.close()


if __name__ == "__main__":
    main()
