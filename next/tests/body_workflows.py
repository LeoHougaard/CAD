"""Verify native multi-Body Boolean history through the Rust host and packages."""
import argparse
import json
import math
from pathlib import Path
import subprocess
import sys

from native_workflows import add, link, near, objects, rectangle, set_value, solid, transaction
from package_workflows import compile_package
from preview_workflows import PreviewSession


def placement_near(actual, expected):
    for a, b in zip(actual["position"], expected["position"]):
        near(a, b)
    near(abs(sum(a * b for a, b in zip(actual["rotation"], expected["rotation"]))), 1)


def visible(state, expected):
    found = {obj["body_state"]["display_object"] for obj in state["objects"]
             if obj["type"] == "PartDesign::Body" and obj["body_state"]["display_object"]}
    assert found == set(expected), (found, expected)


def seed(session, operation, transformed):
    doc = session.call("new", name=operation + ("Placed" if transformed else "Bodies"))["document"]
    edits = [add("PartDesign::Body", "Empty")]
    poses = {}
    for name, width, offset in [("Target", 20, 0), ("Tool", 10, 15), ("Alternate", 6, 5)]:
        pose = dict(position=[100, 200 + offset, 300] if transformed else [offset, 0, 0],
                    rotation=[0, 0, math.sqrt(.5), math.sqrt(.5)] if transformed else [0, 0, 0, 1])
        poses[name] = pose
        edits.extend([add("PartDesign::Body", name), add("Sketcher::SketchObject", name + "Sketch", name),
                      *rectangle(name + "Sketch", width, 20), add("PartDesign::Pad", name + "Pad", name),
                      set_value(name + "Pad", "Profile", link(name + "Sketch")),
                      set_value(name + "Pad", "Length", 10), set_value(name, "Placement", pose)])
    state = transaction(session, doc, "Independent native Bodies", edits)
    visible(state, ["TargetPad", "ToolPad", "AlternatePad"])
    assert objects(state)["Empty"]["body_state"] == {"tip": None, "consumed_by": [], "display_object": None}
    for name in poses:
        assert objects(state)[name]["body"] == name
        assert objects(state)[name + "Sketch"]["body"] == name
        assert objects(state)[name + "Pad"]["body"] == name
        assert objects(state)[name]["body_state"]["tip"] == name + "Pad"
        placement_near(objects(state)[name]["global_placement"], poses[name])
    return doc, state, poses


def expected(operation, target_width=20, tool_width=10, target_height=10, tool_height=10, offset=15):
    base = target_width * 20 * target_height
    tool = tool_width * 20 * tool_height
    overlap = max(0, min(target_width, offset + tool_width) - max(0, offset)) * 20 * min(target_height, tool_height)
    return {"Fuse": base + tool - overlap, "Cut": base - overlap, "Common": overlap}[operation]


def check_boolean(state, operation, volume, poses):
    native = objects(state)
    solid(state, "Boolean", volume)
    solid(state, "Target", volume)
    assert native["Boolean"]["body"] == "Target"
    assert native["Boolean"]["properties"]["Type"] == operation
    assert native["Boolean"]["properties"]["BaseFeature"]["link"] == "TargetPad"
    assert native["Boolean"]["properties"]["Group"] == [link("Tool")]
    assert native["Target"]["body_state"] == {"tip": "Boolean", "consumed_by": [], "display_object": "Boolean"}
    assert native["Tool"]["body_state"] == {"tip": "ToolPad", "consumed_by": ["Boolean"], "display_object": None}
    assert native["ToolPad"]["body"] == "Tool" and native["ToolSketch"]["body"] == "Tool"
    visible(state, ["Boolean", "AlternatePad"])
    for name in poses:
        placement_near(native[name]["global_placement"], poses[name])


def workflow(session, compiler, output, operation):
    doc, initial, poses = seed(session, operation, False)
    undo_count = initial["undo_count"]
    edits = compile_package(compiler, "body.boolean", dict(name="Boolean", target="Target", tool="Tool", operation=operation))
    state = session.call("preview", document=doc, token="boolean", label="Native " + operation, edits=edits)["state"]
    check_boolean(state, operation, expected(operation), poses)
    state = session.call("preview_cancel", document=doc, token="boolean")
    assert "Boolean" not in objects(state) and state["undo_count"] == undo_count
    visible(state, ["TargetPad", "ToolPad", "AlternatePad"])
    for name in poses:
        placement_near(objects(state)[name]["global_placement"], poses[name])
    session.call("preview", document=doc, token="boolean", label="Native " + operation, edits=edits)
    state = session.call("preview_commit", document=doc, token="boolean")
    assert state["undo_count"] == undo_count + 1
    check_boolean(state, operation, expected(operation), poses)
    state = session.call("undo", document=doc)
    assert "Boolean" not in objects(state)
    visible(state, ["TargetPad", "ToolPad", "AlternatePad"])
    for name in poses:
        placement_near(objects(state)[name]["global_placement"], poses[name])
    check_boolean(session.call("redo", document=doc), operation, expected(operation), poses)

    # With the target at identity, native reparenting preserves world placement.
    # Replacing an existing feature's operation and tool changes BOTH Bodies.
    # Cancel restores
    # its previous Group, operation, tip, consumption and placements together.
    state = session.call("preview", document=doc, token="replace", edits=[
        set_value("Boolean", "Group", [link("Alternate")]), set_value("Boolean", "Type", "Cut")])["state"]
    divided = objects(state)["Boolean"]["shape"]
    assert divided["valid"] and divided["solids"] == 2
    near(divided["volume"], 2800)
    assert objects(state)["Tool"]["body_state"]["consumed_by"] == []
    assert objects(state)["Alternate"]["body_state"]["consumed_by"] == ["Boolean"]
    visible(state, ["Boolean", "ToolPad"])
    for name in poses:
        placement_near(objects(state)[name]["global_placement"], poses[name])
    state = session.call("preview", document=doc, token="replace", edits=[
        set_value("Boolean", "Group", [link("Alternate")]), set_value("Boolean", "Type", "Fuse")])["state"]
    solid(state, "Boolean", 4000)
    check_boolean(session.call("preview_cancel", document=doc, token="replace"), operation, expected(operation), poses)

    # Disjoint intersection is native non-null empty topology. It must still
    # fail before publishing a final solid or consuming different tool Bodies.
    session.call("preview", document=doc, token="invalid", edits=[
        set_value("Tool", "Placement", dict(position=[50, 0, 0], rotation=[0, 0, 0, 1])),
        set_value("Boolean", "Type", "Common")], expect_error="invalid_model")
    state = session.call("inspect", document=doc)
    assert state["available"] and state["preview"] is None
    check_boolean(state, operation, expected(operation), poses)
    for tools in [[link("Target")], [link("Tool"), link("Tool")], [link("TargetPad")], []]:
        session.call("transaction", document=doc, edits=[set_value("Boolean", "Group", tools)], expect_error="invalid_request")
        check_boolean(session.call("inspect", document=doc), operation, expected(operation), poses)
    session.call("transaction", document=doc, edits=[add("PartDesign::Boolean", "Reuse", "Alternate"),
        set_value("Reuse", "Group", [link("Tool")])], expect_error="invalid_request")
    state = session.call("inspect", document=doc)
    assert "Reuse" not in objects(state)
    session.call("transaction", document=doc, edits=[add("PartDesign::Boolean", "Cycle", "Tool"),
        set_value("Cycle", "Group", [link("Target")])], expect_error="invalid_request")
    state = session.call("inspect", document=doc)
    assert "Cycle" not in objects(state) and state["available"]
    check_boolean(state, operation, expected(operation), poses)

    if operation == "Fuse":
        transaction(session, doc, "Single-solid target policy", [set_value("Target", "AllowCompound", False)])
        session.call("preview", document=doc, token="disjoint", edits=[
            set_value("Tool", "Placement", dict(position=[50, 0, 0], rotation=[0, 0, 0, 1]))], expect_error="invalid_model")
        state = session.call("inspect", document=doc)
        check_boolean(state, operation, expected(operation), poses)
    state = transaction(session, doc, "Change target's constrained parent sketch", [
        dict(op="datum", object="TargetSketch", index=9, value="25 mm")])
    check_boolean(state, operation, expected(operation, target_width=25), poses)
    state = transaction(session, doc, "Edit retained consumed tool's parent sketch", [
        dict(op="datum", object="ToolSketch", index=9, value="12 mm")])
    check_boolean(state, operation, expected(operation, target_width=25, tool_width=12), poses)
    path = output / f"{operation.lower()}-bodies.FCStd"
    session.call("save", document=doc, path=str(path))
    session.call("close", document=doc)
    state = session.call("open", path=str(path))
    doc = state["document"]
    check_boolean(state, operation, expected(operation, target_width=25, tool_width=12), poses)
    assert objects(state)["ToolSketch"]["sketch"]["degrees_of_freedom"] == 0
    state = transaction(session, doc, "Edit consumed tool after native reopen", [set_value("ToolPad", "Length", 12)])
    value = expected(operation, target_width=25, tool_width=12, tool_height=12)
    check_boolean(state, operation, value, poses)
    mesh = session.call("mesh", document=doc, object="Boolean", deflection=.03)
    assert mesh["positions"] and len(mesh["positions"]) == len(mesh["normals"])
    session.call("close", document=doc)
    return dict(operation=operation, transformed=False, initial_volume=expected(operation),
                final_volume=value, native_tool_history=True, retained_tool_placement=True)


def native_reference(parameters):
    """Run only with the pinned native Python, independently of the worker."""
    import FreeCAD as App
    import PartDesign
    import Part
    import Sketcher
    doc = App.newDocument("RawNativeBoolean")
    bodies = {}
    specifications = [("Target", parameters.get("target_width", 20), 0, 10),
                      ("Tool", parameters.get("tool_width", 10), 15, parameters.get("tool_height", 10))]
    if parameters.get("multiple"):
        specifications.append(("Alternate", parameters.get("alternate_width", 6), 18, 10))
    for name, width, offset, height in specifications:
        body = doc.addObject("PartDesign::Body", name)
        sketch = body.newObject("Sketcher::SketchObject", name + "Sketch")
        for edit in rectangle(name + "Sketch", width, 20):
            if edit["op"] == "geometry":
                sketch.addGeometry(Part.LineSegment(App.Vector(*edit["start"]), App.Vector(*edit["end"])))
            else:
                sketch.addConstraint(Sketcher.Constraint(edit["kind"], *edit["args"]))
        pad = body.newObject("PartDesign::Pad", name + "Pad")
        pad.Profile = sketch
        pad.Length = height
        body.Placement = (App.Placement(App.Vector(100, 200 + offset, 300), App.Rotation(0, 0, math.sqrt(.5), math.sqrt(.5)))
                          if parameters.get("transformed", True) else App.Placement(App.Vector(offset, 0, 0), App.Rotation()))
        bodies[name] = body
    doc.recompute()
    feature = bodies["Target"].newObject("PartDesign::Boolean", "Boolean")
    feature.Group = [bodies["Tool"], bodies["Alternate"]] if parameters.get("multiple") else [bodies["Tool"]]
    feature.Type = parameters["operation"]
    doc.recompute()
    def placement(value):
        return {"position": [value.Base.x, value.Base.y, value.Base.z], "rotation": list(value.Rotation.Q)}
    shape = feature.Shape
    result = {"valid": shape.isValid(), "solids": len(shape.Solids), "volume": shape.Volume,
              "bounds": {"min": [shape.BoundBox.XMin, shape.BoundBox.YMin, shape.BoundBox.ZMin],
                         "max": [shape.BoundBox.XMax, shape.BoundBox.YMax, shape.BoundBox.ZMax]} if len(shape.Vertexes) else None,
              "placements": {key: placement(body.getGlobalPlacement()) for key, body in bodies.items()}}
    print(json.dumps(result))


def reference(runtime, operation, **parameters):
    command = [runtime["executable"], *runtime["args"], str(Path(__file__).resolve()),
               "--native-reference", json.dumps(dict(operation=operation, **parameters))]
    result = subprocess.run(command, capture_output=True, encoding="utf-8", timeout=30)
    assert result.returncode == 0, result.stderr
    return json.loads(next(line for line in reversed(result.stdout.splitlines()) if line.startswith("{")))


def transformed_reference_workflow(session, compiler, output, runtime, operation):
    doc, initial, poses = seed(session, operation, True)
    edits = compile_package(compiler, "body.boolean", dict(name="Boolean", target="Target", tool="Tool", operation=operation))
    baseline = reference(runtime, operation)
    if baseline["solids"] == 0:
        session.call("preview", document=doc, token="native-placed", edits=edits, expect_error="invalid_model")
        state = session.call("inspect", document=doc)
        assert state["available"] and "Boolean" not in objects(state)
        visible(state, ["TargetPad", "ToolPad", "AlternatePad"])
        for name in poses:
            placement_near(objects(state)[name]["global_placement"], poses[name])
        session.call("close", document=doc)
        return dict(operation=operation, transformed=True, raw_native=baseline, empty_result_rejected=True)

    def matched(state, native):
        shape = objects(state)["Boolean"]["shape"]
        assert shape["valid"] == native["valid"] and shape["solids"] == native["solids"]
        near(shape["volume"], native["volume"])
        for end in ["min", "max"]:
            for actual, expected_value in zip(shape["bounds"][end], native["bounds"][end]):
                near(actual, expected_value)
        for name, pose in native["placements"].items():
            placement_near(objects(state)[name]["global_placement"], pose)
        assert objects(state)["Tool"]["body_state"]["consumed_by"] == ["Boolean"]
        visible(state, ["Boolean", "AlternatePad"])

    matched(session.call("preview", document=doc, token="native-placed", edits=edits)["state"], baseline)
    state = session.call("preview_cancel", document=doc, token="native-placed")
    assert "Boolean" not in objects(state)
    for name in poses:
        placement_near(objects(state)[name]["global_placement"], poses[name])
    session.call("preview", document=doc, token="native-placed", edits=edits)
    matched(session.call("preview_commit", document=doc, token="native-placed"), baseline)
    state = session.call("undo", document=doc)
    assert "Boolean" not in objects(state)
    matched(session.call("redo", document=doc), baseline)
    state = transaction(session, doc, "Native transformed parent edits", [
        dict(op="datum", object="TargetSketch", index=9, value="25 mm"),
        dict(op="datum", object="ToolSketch", index=9, value="12 mm"), set_value("ToolPad", "Length", 12)])
    changed = reference(runtime, operation, target_width=25, tool_width=12, tool_height=12)
    matched(state, changed)
    path = output / f"{operation.lower()}-raw-native-placed.FCStd"
    session.call("save", document=doc, path=str(path))
    session.call("close", document=doc)
    state = session.call("open", path=str(path))
    matched(state, changed)
    session.call("close", document=state["document"])
    return dict(operation=operation, transformed=True, raw_native=baseline,
                changed_native=changed, matched_native_group_semantics=True)


def multiple_tools_workflow(session, output, runtime, operation):
    doc, _, poses = seed(session, "Multiple" + operation, False)
    poses["Alternate"] = dict(position=[18, 0, 0], rotation=[0, 0, 0, 1])
    transaction(session, doc, "Overlapping second native tool", [set_value("Alternate", "Placement", poses["Alternate"])])
    edits = [add("PartDesign::Boolean", "Boolean", "Target"),
             set_value("Boolean", "Group", [link("Tool"), link("Alternate")]),
             set_value("Boolean", "Type", operation)]
    # Native Common intersects the base with the combined tool regions and can
    # preserve touching partitions as separate solids. It is not A∩B∩C.
    expected_volume = {"Fuse": 5000, "Cut": 3000, "Common": 1000}[operation]
    baseline = reference(runtime, operation, transformed=False, multiple=True)
    near(baseline["volume"], expected_volume)
    def check(state, volume, native=baseline):
        shape = objects(state)["Boolean"]["shape"]
        assert shape["valid"] and shape["solids"] == native["solids"]
        near(shape["volume"], volume)
        near(shape["volume"], native["volume"])
        for end in ["min", "max"]:
            for actual, reference_value in zip(shape["bounds"][end], native["bounds"][end]):
                near(actual, reference_value)
        native = objects(state)
        assert native["Boolean"]["properties"]["Group"] == [link("Tool"), link("Alternate")]
        for tool in ["Tool", "Alternate"]:
            assert native[tool]["body_state"]["consumed_by"] == ["Boolean"]
            assert native[tool + "Sketch"]["body"] == tool
            placement_near(native[tool]["global_placement"], poses[tool])
        visible(state, ["Boolean"])
    check(session.call("preview", document=doc, token="multi", edits=edits)["state"], expected_volume)
    state = session.call("preview_cancel", document=doc, token="multi")
    visible(state, ["TargetPad", "ToolPad", "AlternatePad"])
    session.call("preview", document=doc, token="multi", edits=edits)
    check(session.call("preview_commit", document=doc, token="multi"), expected_volume)
    state = session.call("undo", document=doc)
    assert "Boolean" not in objects(state)
    visible(state, ["TargetPad", "ToolPad", "AlternatePad"])
    check(session.call("redo", document=doc), expected_volume)
    path = output / f"{operation.lower()}-multiple-tools.FCStd"
    session.call("save", document=doc, path=str(path))
    session.call("close", document=doc)
    state = session.call("open", path=str(path))
    doc = state["document"]
    check(state, expected_volume)
    state = transaction(session, doc, "Edit second consumed tool after reopen", [
        dict(op="datum", object="AlternateSketch", index=9, value="8 mm")])
    changed = 5200 if operation == "Fuse" else expected_volume
    changed_native = reference(runtime, operation, transformed=False, multiple=True, alternate_width=8)
    check(state, changed, changed_native)
    session.call("save", document=doc, path=str(output / f"{operation.lower()}-multiple-tools-edited.FCStd"))
    session.call("close", document=doc)
    return dict(operation=operation, transformed=False, tools=2, initial_volume=expected_volume,
                final_volume=changed, native_tool_history=True, raw_native=baseline,
                changed_native=changed_native, matched_native_group_semantics=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    runtime = json.loads(args.runtime.read_text(encoding="utf-8-sig"))
    session = PreviewSession(args.host.resolve(), runtime)
    try:
        cases = [workflow(session, args.compiler.resolve(), args.output.resolve(), op)
                 for op in ["Fuse", "Cut", "Common"]]
        cases.extend(transformed_reference_workflow(session, args.compiler.resolve(), args.output.resolve(), runtime, op)
                     for op in ["Fuse", "Cut", "Common"])
        cases.extend(multiple_tools_workflow(session, args.output.resolve(), runtime, op) for op in ["Fuse", "Cut", "Common"])
        report = dict(cases=cases, operations=session.timings, capabilities=session.capabilities)
        (args.output / "body-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps([dict(operation=case["operation"], transformed=case["transformed"],
                               tools=case.get("tools", 1),
                               volume=case.get("initial_volume", case.get("raw_native", {}).get("volume")),
                               native_group_matched=case.get("matched_native_group_semantics", False),
                               empty_result_rejected=case.get("empty_result_rejected", False)) for case in cases], indent=2))
    finally:
        session.close()


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--native-reference":
        native_reference(json.loads(sys.argv[2]))
    else:
        main()
