"""Native sketch tasks through the Rust host, including native history and solids."""

import argparse
import json
import math
from pathlib import Path

from native_workflows import add, constraint, link, near, objects, set_value, solid, transaction
from sketch_commands_workflows import Compiler, element
from sketch_editor_workflows import SketchSession


def sketch(state, name="Sketch"):
    return objects(state)[name]["sketch"]


def inspect(session, doc):
    return session.call("inspect", document=doc)


def task(compiler, state, kind, selection=(), name="Sketch", **parameters):
    return compiler.call(op="edit", snapshot=objects(state)[name], kind=kind,
                         selection=list(selection), parameters=parameters)


def begin(session, label, body=True):
    doc = session.call("new", name=label)["document"]
    transaction(session, doc, "Native sketch", ([add("PartDesign::Body", "Body")] if body else []) +
                [add("Sketcher::SketchObject", "Sketch", "Body" if body else None)])
    return doc


def pad(session, doc, name="Sketch", length=5):
    return transaction(session, doc, "Native Pad", [add("PartDesign::Pad", "Pad", "Body"),
        set_value("Pad", "Profile", link(name)), set_value("Pad", "Length", length)])


def reopen(session, doc, path):
    session.call("save", document=doc, path=str(path.resolve()))
    session.call("close", document=doc)
    return session.call("open", path=str(path.resolve()))


def corner(session, compiler, output, chamfer):
    doc = begin(session, "NativeChamfer" if chamfer else "NativeFillet")
    baseline = transaction(session, doc, "Rust rectangle", compiler.draw("rectangle", [[0, 0, 0], [10, 10, 0]]))
    # Dimension the surviving diagonal endpoints. Dimensions on the trimmed
    # corner itself are removed/retargeted by the native fillet task.
    for kind in ("DistanceX", "DistanceY"):
        baseline = transaction(session, doc, "Native overall rectangle dimension", compiler.call(op="constraint",
            snapshot=objects(baseline)["Sketch"], kind=kind, selection=[element(0, 1), element(1, 2)], value="10 mm"))
    before = baseline["undo_count"]
    token = "native_corner"
    def edits(radius):
        return task(compiler, baseline, "chamfer" if chamfer else "fillet", [element(0, 2)], radius=radius)
    session.call("preview", document=doc, token=token, edits=edits("1 mm"))
    session.call("preview", document=doc, token=token, edits=edits("2 mm"))
    state = session.call("preview_cancel", document=doc, token=token)
    assert sketch(state)["geometry_count"] == 4 and state["undo_count"] == before
    session.call("preview", document=doc, token=token, edits=edits("2 mm"))
    state = session.call("preview_commit", document=doc, token=token)
    assert state["undo_count"] == before + 1
    arc = next(geo for geo in sketch(state)["geometry"] if geo["kind"] == "arc")
    near(arc["radius"], 2)
    assert arc["construction"] == chamfer
    state = transaction(session, doc, "Native radius dimension", [constraint("Sketch", "Radius", arc["index"], 2)])
    datum = sketch(state)["constraint_count"] - 1
    state = pad(session, doc)
    volume = 490 if chamfer else (96 + math.pi) * 5
    near(solid(state, "Pad")["volume"], volume)
    session.call("undo", document=doc)
    session.call("redo", document=doc)
    state = reopen(session, doc, output / ("chamfer.FCStd" if chamfer else "fillet.FCStd"))
    doc = state["document"]
    state = transaction(session, doc, "Edit native corner radius after reopen", [dict(op="datum", object="Sketch", index=datum, value="1 mm")])
    edited = 497.5 if chamfer else (99 + math.pi / 4) * 5
    near(solid(state, "Pad")["volume"], edited)
    near(solid(session.call("undo", document=doc), "Pad")["volume"], volume)
    near(solid(session.call("redo", document=doc), "Pad")["volume"], edited)
    session.call("close", document=doc)
    return {"chamfer": chamfer, "volume": volume, "edited_volume": edited, "preview_cancel_history_reopen": True}


def arc_extension(session, compiler):
    doc = begin(session, "ArcExtensionHistory", body=False)
    state = transaction(session, doc, "Rust circular arc", compiler.draw("arc", [[5, 0, 0], [math.sqrt(12.5), math.sqrt(12.5), 0], [0, 5, 0]]))
    state = transaction(session, doc, "Native persistent arc projection", [add("Sketcher::SketchObject", "Follower"),
        dict(op="external_geometry", object="Follower", source="Sketch", subelement="Edge1")])
    baseline = sketch(state)["geometry"][0]["last_parameter"]
    reference = sketch(state, "Follower")["externals"][0]["persistent_reference"]
    frame = state
    def extension(value): return task(compiler, frame, "extend", [element(0, 2)], increment=value)
    def check(current, expected):
        near(sketch(current)["geometry"][0]["last_parameter"], expected)
        assert sketch(current, "Follower")["externals"][0]["persistent_reference"] == reference
    for increment in (0.1, 0.2):
        state = session.call("preview", document=doc, token="arc_extend", edits=extension(increment))["state"]
        check(state, baseline + increment)
    check(session.call("preview_cancel", document=doc, token="arc_extend"), baseline)
    state = session.call("preview", document=doc, token="arc_extend", edits=extension("30 deg"))["state"]
    check(state, baseline + math.pi / 6)
    state = session.call("preview_commit", document=doc, token="arc_extend")
    check(session.call("undo", document=doc), baseline)
    check(session.call("redo", document=doc), baseline + math.pi / 6)
    baseline += math.pi / 6
    session.call("transaction", document=doc, label="Abort native arc extension", expect_error="unsupported_property",
        edits=extension(0.25) + [set_value("Sketch", "MissingNativeProperty", 1)])
    check(inspect(session, doc), baseline)
    session.call("transaction", document=doc, label="Reject angle unit mismatch", expect_error="dimension_mismatch", edits=extension("2 mm"))
    check(inspect(session, doc), baseline)
    session.call("sketch_begin", document=doc, token="arc_session", object="Sketch")
    state = transaction(session, doc, "First native arc action", extension(0.1))
    check(state, baseline + 0.1)
    state = transaction(session, doc, "Second native arc action", extension(0.2))
    check(state, baseline + 0.3)
    check(session.call("undo", document=doc), baseline + 0.1)
    check(session.call("redo", document=doc), baseline + 0.3)
    for increment in (0.4, 0.5):
        state = session.call("preview", document=doc, token="session_extend", edits=extension(increment))["state"]
        check(state, baseline + 0.3 + increment)
    check(session.call("preview_cancel", document=doc, token="session_extend"), baseline + 0.3)
    check(session.call("sketch_cancel", document=doc, token="arc_session"), baseline)
    session.call("close", document=doc)
    return {"native_property_registration": True, "preview_replace_cancel": True, "native_session_replay_cancel": True,
            "persistent_reference_unchanged": True, "failed_batch_atomic": True}


def curve_pair_corner(session, compiler):
    doc = begin(session, "NativeTwoCurveFillet", body=False)
    state = transaction(session, doc, "Rust connected lines", compiler.draw("line", [[0, 0, 0], [10, 0, 0]]) +
        compiler.draw("line", [[10, 0, 0], [10, 10, 0]], index=1) + [constraint("Sketch", "Coincident", 0, 2, 1, 1)])
    state = transaction(session, doc, "Native two-curve preserve-corner fillet", task(compiler, state, "fillet", [element(0), element(1)],
        positions=[[8, 0, 0], [10, 2, 0]], radius="2 mm", preserve_corner=True))
    assert sketch(state)["geometry_count"] == 4
    near(next(geo for geo in sketch(state)["geometry"] if geo["kind"] == "arc")["radius"], 2)
    assert sum(c["type"] == "Tangent" for c in sketch(state)["constraints"]) == 2
    assert sketch(session.call("undo", document=doc))["geometry_count"] == 2
    assert sketch(session.call("redo", document=doc))["geometry_count"] == 4
    session.call("close", document=doc)
    return {"two_curve_native_pick_positions": True, "preserve_corner_native_history": True}


def split_and_extend(session, compiler):
    doc = begin(session, "SplitClosedProfile")
    state = transaction(session, doc, "Rust rectangle", compiler.draw("rectangle", [[0, 0, 0], [10, 10, 0]]))
    state = transaction(session, doc, "Native split", task(compiler, state, "split", [element(0)], position=[4, 0, 0]))
    assert sketch(state)["geometry_count"] == 5
    state = pad(session, doc)
    near(solid(state, "Pad")["volume"], 500)
    session.call("undo", document=doc)
    assert sketch(session.call("undo", document=doc))["geometry_count"] == 4
    assert sketch(session.call("redo", document=doc))["geometry_count"] == 5
    near(solid(session.call("redo", document=doc), "Pad")["volume"], 500)
    session.call("close", document=doc)
    doc = begin(session, "ExtendNativeLine", body=False)
    state = transaction(session, doc, "Rust line", compiler.draw("line", [[0, 0, 0], [10, 0, 0]]))
    state = transaction(session, doc, "Typed native line extension", task(compiler, state, "extend", [element(0, 2)], increment="2 cm"))
    near(sketch(state)["geometry"][0]["end"][0], 30)
    near(sketch(session.call("undo", document=doc))["geometry"][0]["end"][0], 10)
    near(sketch(session.call("redo", document=doc))["geometry"][0]["end"][0], 30)
    session.call("close", document=doc)
    return {"split_closed_profile_volume": 500, "typed_line_extension_undo_redo": True}


def carbon_copy(session, compiler, output):
    doc = begin(session, "CarbonCopyProfile")
    state = transaction(session, doc, "Rust source rectangle", compiler.draw("rectangle", [[0, 0, 0], [10, 8, 0]]))
    for index, value in [(0, "10 mm"), (1, "8 mm")]:
        edits = compiler.call(op="constraint", snapshot=objects(state)["Sketch"], kind="Distance", selection=[element(index)], value=value)
        state = transaction(session, doc, "Native source dimension", edits)
    source_datum = sketch(state)["constraint_count"] - 2
    state = transaction(session, doc, "Native copy target", [add("Sketcher::SketchObject", "Copy", "Body")])
    state = transaction(session, doc, "Native parametric Carbon Copy", task(compiler, state, "carbon_copy", name="Copy", source="Sketch", construction=False))
    assert objects(state)["Copy"]["expressions"]
    state = pad(session, doc, name="Copy")
    near(solid(state, "Pad")["volume"], 400)
    state = transaction(session, doc, "Parent dimension drives copy", [dict(op="datum", object="Sketch", index=source_datum, value="12 mm")])
    near(solid(state, "Pad")["volume"], 480)
    near(solid(session.call("undo", document=doc), "Pad")["volume"], 400)
    near(solid(session.call("redo", document=doc), "Pad")["volume"], 480)
    state = reopen(session, doc, output / "carbon-copy.FCStd")
    assert objects(state)["Copy"]["expressions"]
    near(solid(state, "Pad")["volume"], 480)
    session.call("close", document=state["document"])
    return {"native_expressions": True, "copied_profile_volume": 400, "parent_edit_volume": 480, "undo_redo_reopen": True}


def spline_tasks(session, compiler, output):
    poles = [[0, 0, 0], [2, 3, 0], [5, 3, 0], [7, 0, 0]]
    doc = begin(session, "NativeSplineTasks")
    transaction(session, doc, "Rust closed B-spline profile", compiler.draw("bspline", poles) +
        compiler.draw("line", [poles[-1], poles[0]], index=1) + [constraint("Sketch", "Coincident", 0, 2, 1, 1), constraint("Sketch", "Coincident", 0, 1, 1, 2)])
    state = pad(session, doc)
    near(solid(state, "Pad")["volume"], 54)
    assert not sketch(state)["spline_controls"]
    initial_undo = state["undo_count"]
    frame = state
    for height in (4, 5):
        candidate = session.call("preview", document=doc, token="pole_drag", edits=task(compiler, frame, "spline_pole", [element(0)], pole=1, position=[2, height, 0]))["state"]
        near(sketch(candidate)["geometry"][0]["poles"][1][1], height)
    state = session.call("preview_cancel", document=doc, token="pole_drag")
    near(solid(state, "Pad")["volume"], 54)
    assert sketch(state)["geometry_count"] == 2 and state["undo_count"] == initial_undo
    result = session.call("transaction", document=doc, label="Native pole task", edits=task(compiler, state, "spline_pole", [element(0)], pole=1, position=[2, 4, 0]))
    state, control = result["state"], result["edits"][0]["index"]
    assert len(sketch(state)["spline_controls"]) == 4
    assert {"spline_index": 0, "pole_index": 1, "control_index": control} in sketch(state)["spline_controls"]
    near(solid(state, "Pad")["volume"], 63)
    equal = next(c["index"] for c in sketch(state)["constraints"] if c["type"] == "Equal" and control in (c["first"], c["second"]))
    frame = state
    session.call("preview", document=doc, token="cancel_weight", edits=task(compiler, frame, "spline_weight", [element(0)], pole=1, value="1.5", release_constraints=[equal]))
    state = session.call("preview_cancel", document=doc, token="cancel_weight")
    near(solid(state, "Pad")["volume"], 63)
    assert sketch(state)["geometry"][0]["weights"] == [1, 1, 1, 1]
    for value in ("1.5", "2"):
        state = session.call("preview", document=doc, token="native_weight", edits=task(compiler, frame, "spline_weight", [element(0)], pole=1, value=value, release_constraints=[equal]))["state"]
        near(sketch(state)["geometry"][0]["weights"][1], float(value))
    state = session.call("preview_commit", document=doc, token="native_weight")
    assert sketch(state)["geometry"][0]["rational"]
    assert sketch(state)["geometry"][0]["weights"] == [1, 2, 1, 1]
    rational_volume = solid(state, "Pad")["volume"]
    assert abs(rational_volume - 63) > 0.1
    near(solid(session.call("undo", document=doc), "Pad")["volume"], 63)
    near(solid(session.call("redo", document=doc), "Pad")["volume"], rational_volume)
    session.call("transaction", document=doc, label="Reject non-Equal constraint release", expect_error="invalid_request",
        edits=task(compiler, state, "spline_weight", [element(0)], pole=1, value="3", release_constraints=[0]))
    near(solid(inspect(session, doc), "Pad")["volume"], rational_volume)
    state = reopen(session, doc, output / "spline-pole-weight.FCStd")
    near(solid(state, "Pad")["volume"], rational_volume)
    session.call("close", document=state["document"])
    doc = begin(session, "NativeSplineKnots", body=False)
    state = transaction(session, doc, "Rust B-spline", compiler.draw("bspline", poles))
    for edit, degree, knots, mults in [
        (dict(op="spline_degree", change=1), 4, [0, 1], [5, 5]),
        (dict(op="spline_knot", parameter=0.5, multiplicity=1), 4, [0, 0.5, 1], [5, 1, 5]),
        (dict(op="spline_knot", knot=2, change=1), 4, [0, 0.5, 1], [5, 2, 5]),
        (dict(op="spline_knot", knot=2, change=-2), 4, [0, 1], [5, 5])]:
        state = transaction(session, doc, "Native spline topology task", task(compiler, state, edit["op"], [element(0)], **{key: value for key, value in edit.items() if key != "op"}))
        curve = sketch(state)["geometry"][0]
        assert (curve["degree"], curve["knots"], curve["multiplicities"]) == (degree, knots, mults)
        session.call("undo", document=doc)
        restored = session.call("redo", document=doc)
        assert sketch(restored)["geometry"][0] == curve
    state = transaction(session, doc, "Native degree approximation", task(compiler, state, "spline_degree", [element(0)], change=-1))
    assert any(geo["kind"] == "bspline" and geo["degree"] == 3 for geo in sketch(state)["geometry"])
    assert any(geo["kind"] == "bspline" and geo["degree"] == 4 for geo in sketch(session.call("undo", document=doc))["geometry"])
    session.call("redo", document=doc)
    session.call("close", document=doc)
    return {"pole_edit_volume": 63, "rational_weight_volume": rational_volume, "degree_knots_native_history": True,
            "pole_preview_cancel": True, "weight_preview_commit": True, "invalid_release_atomic": True, "reopen": True}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", required=True, type=Path)
    parser.add_argument("--compiler", required=True, type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    runtime = json.loads(args.runtime.read_text())
    session, compiler = SketchSession(args.host, runtime), Compiler(args.compiler)
    try:
        report = {"corners": [corner(session, compiler, args.output, flag) for flag in (False, True)],
                  "two_curve_corner": curve_pair_corner(session, compiler),
                  "arc_extension": arc_extension(session, compiler), "split_extend": split_and_extend(session, compiler),
                  "carbon_copy": carbon_copy(session, compiler, args.output), "spline_tasks": spline_tasks(session, compiler, args.output)}
        (args.output / "sketch-operations-report.json").write_text(json.dumps(report, indent=2))
        print(json.dumps(report, indent=2))
    finally:
        compiler.close()
        session.close()


if __name__ == "__main__":
    main()
