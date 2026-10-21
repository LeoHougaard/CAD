"""Native advanced curves, solver handles and projected parametric profiles.

Run through the Rust host after the sketch_geometry worker integration. The
geometry, constraints, history and saved files are real FreeCAD objects.
"""

import argparse
import json
import math
from pathlib import Path

from native_workflows import Session, add, constraint, link, near, objects, set_value, solid, transaction
from sketch_commands_workflows import Compiler, element


CURVES = [
    (dict(kind="ellipse", center=[0, 0, 0], major=[3, 0, 0], minor=[0, 2, 0]), 5, 6 * math.pi),
    (dict(kind="ellipse_arc", center=[0, 0, 0], major=[3, 0, 0], minor=[0, 2, 0],
          start_parameter=0, end_parameter=math.pi), 7, 3 * math.pi),
    (dict(kind="bspline", poles=[[0, 0, 0], [2, 3, 0], [5, 3, 0], [7, 0, 0]]), 12, 10.8),
    (dict(kind="hyperbola_arc", center=[0, 0, 0], major=[2, 0, 0], minor=[0, 1, 0],
          start_parameter=-1, end_parameter=1), 7, 2 * (math.sinh(1) * math.cosh(1) - 1)),
    (dict(kind="parabola_arc", center=[0, 0, 0], focal=1,
          start_parameter=-1, end_parameter=1), 6, 1 / 3),
]


def sketch(state, name="Sketch"):
    return objects(state)[name]["sketch"]


def draw_curve(compiler, case):
    kind = case["kind"]
    if kind == "bspline":
        return compiler.draw("periodic_bspline" if case.get("periodic") else "bspline", case["poles"])
    if kind == "parabola_arc":
        f = case["focal"]
        points = [case["center"], [f, 0, 0]] + [[t * t / (4 * f), t, 0] for t in (case["start_parameter"], case["end_parameter"])]
    else:
        points = [case["center"], case["major"], case["minor"]]
        if kind == "ellipse_arc":
            points += [[3 * math.cos(t), 2 * math.sin(t), 0] for t in (case["start_parameter"], case["end_parameter"])]
        elif kind == "hyperbola_arc":
            points += [[2 * math.cosh(t), math.sinh(t), 0] for t in (case["start_parameter"], case["end_parameter"])]
    return compiler.draw(kind, points)


def author_constraint(compiler, state, kind, selection, value=None, name="Sketch"):
    return compiler.call(op="constraint", snapshot=objects(state)[name], kind=kind,
                         selection=selection, value=value)


def begin(session, name):
    doc = session.call("new", name=name)["document"]
    transaction(session, doc, "Create native sketch", [add("PartDesign::Body", "Body"),
                add("Sketcher::SketchObject", "Sketch", "Body")])
    return doc


def inspect(session, doc):
    return session.call("inspect", document=doc)


def roundtrip(session, doc, path, feature, volume):
    session.call("save", document=doc, path=str(path.resolve()))
    session.call("close", document=doc)
    state = session.call("open", path=str(path.resolve()))
    near(solid(state, feature)["volume"], volume)
    return state


def advanced_curve(session, compiler, case, expected_dof, area, output):
    kind = case["kind"]
    doc = begin(session, "Curve_" + kind)
    state = transaction(session, doc, "Native " + kind, draw_curve(compiler, case))
    curve = sketch(state)["geometry"][0]
    assert curve["kind"] == kind and 2 <= len(curve["points"]) <= 2048
    assert sketch(state)["degrees_of_freedom"] == expected_dof
    assert all(math.isfinite(v) for point in curve["points"] for v in point)
    assert all(handle["index"] == 0 and handle["draggable"] for handle in curve["handles"])
    handle = next(h for h in curve["handles"] if h["point"] == (1 if kind == "bspline" else 3))
    before = state["undo_count"]
    token = "curve_drag"
    for offset in (0.25, 0.5):
        target = [handle["position"][0] + offset, handle["position"][1] + offset, 0]
        state = session.call("preview", document=doc, expected_revision=session.revisions[doc], token=token,
                 edits=[dict(op="move_geometry", object="Sketch", index=0, point=handle["point"],
                             position=target, relative=False)])["state"]
        actual = next(h for h in sketch(state)["geometry"][0]["handles"] if h["point"] == handle["point"])
        near(actual["position"][0], target[0])
        near(actual["position"][1], target[1])
    state = session.call("preview_commit", document=doc, expected_revision=session.revisions[doc], token=token)
    assert state["undo_count"] == before + 1
    session.call("undo", document=doc)
    session.call("redo", document=doc)
    state = session.call("undo", document=doc)
    state = transaction(session, doc, "Constrain native curve", author_constraint(compiler, state, "Block", [element(0)]))
    assert sketch(state)["degrees_of_freedom"] == 0
    position = 1 if kind == "bspline" else 3
    session.call("transaction", document=doc, label="Conflicting curve dimension", expect_error="invalid_model",
                 edits=[constraint("Sketch", "DistanceX", 0, position, 1000),
                        constraint("Sketch", "DistanceX", 0, position, 2000)])
    state = inspect(session, doc)
    assert state["available"] and sketch(state)["degrees_of_freedom"] == 0
    assert sketch(state)["constraint_count"] == 1
    if kind != "ellipse":
        handles = sketch(state)["geometry"][0]["handles"]
        start = next(h["position"] for h in handles if h["point"] == 1)
        end = next(h["position"] for h in handles if h["point"] == 2)
        state = transaction(session, doc, "Close native profile", compiler.draw("line", [end, start], index=1))
        edits = author_constraint(compiler, state, "Coincident", [element(1, 1), element(0, 2)])
        edits += author_constraint(compiler, state, "Coincident", [element(1, 2), element(0, 1)])
        transaction(session, doc, "Constrain profile closure", edits)
    state = transaction(session, doc, "Pad native advanced profile", [add("PartDesign::Pad", "Pad", "Body"),
                set_value("Pad", "Profile", link("Sketch")), set_value("Pad", "Length", 5)])
    near(solid(state, "Pad")["volume"], area * 5, tolerance=2e-5)
    restored = roundtrip(session, doc, output / (kind + ".FCStd"), "Pad", area * 5)
    doc = restored["document"]
    assert sketch(restored)["geometry"][0]["kind"] == kind
    assert sketch(restored)["degrees_of_freedom"] == 0
    state = transaction(session, doc, "Edit reopened native extrusion", [set_value("Pad", "Length", 7)])
    near(solid(state, "Pad")["volume"], area * 7, tolerance=2e-5)
    near(solid(session.call("undo", document=doc), "Pad")["volume"], area * 5, tolerance=2e-5)
    near(solid(session.call("redo", document=doc), "Pad")["volume"], area * 7, tolerance=2e-5)
    session.call("close", document=doc)
    return {"kind": kind, "native_dof": expected_dof, "pad_volume": area * 5, "drag_single_undo": True}


def native_spline_controls(session, compiler):
    doc = begin(session, "NativeSplineControls")
    state = transaction(session, doc, "B-spline", draw_curve(compiler, CURVES[2][0]))
    poles = sketch(state)["geometry"][0]["poles"]
    state = transaction(session, doc, "Expose native control geometry", [dict(op="expose_internal", object="Sketch", index=0)])
    controls = [geo for geo in sketch(state)["geometry"] if geo["kind"] == "circle" and geo["construction"]]
    assert len(controls) == len(poles)
    control = next(geo for geo in controls if math.dist(geo["center"], poles[1]) < 1e-6)
    state = transaction(session, doc, "Move native control point", [dict(op="move_geometry", object="Sketch",
                index=control["index"], point=3, position=[2, 4, 0], relative=False)])
    near(sketch(state)["geometry"][0]["poles"][1][1], 4)
    state = session.call("undo", document=doc)
    near(sketch(state)["geometry"][0]["poles"][1][1], 3)
    state = session.call("redo", document=doc)
    near(sketch(state)["geometry"][0]["poles"][1][1], 4)
    session.call("close", document=doc)
    return {"native_control_circles": len(controls), "control_move_undo_redo": True}


def ellipse_axis_dimensions(session, compiler, output):
    doc = begin(session, "EllipseAxisDimensions")
    state = transaction(session, doc, "Ellipse and native axes", draw_curve(compiler, CURVES[0][0]) +
                 [dict(op="expose_internal", object="Sketch", index=0)])
    lines = [geo for geo in sketch(state)["geometry"] if geo["kind"] == "line" and geo["construction"]]
    major = next(geo["index"] for geo in lines if math.isclose(math.dist(geo["start"], geo["end"]), 6))
    minor = next(geo["index"] for geo in lines if math.isclose(math.dist(geo["start"], geo["end"]), 4))
    datum = sketch(state)["constraint_count"]
    state = transaction(session, doc, "Dimension native major axis", author_constraint(compiler, state, "Distance", [element(major)], "8 mm"))
    state = transaction(session, doc, "Dimension native minor axis", author_constraint(compiler, state, "Distance", [element(minor)], "3 mm"))
    near(sketch(state)["geometry"][0]["major_radius"], 4)
    near(sketch(state)["geometry"][0]["minor_radius"], 1.5)
    state = transaction(session, doc, "Typed native major diameter", [dict(op="datum", object="Sketch", index=datum, value="10 mm")])
    near(sketch(state)["geometry"][0]["major_radius"], 5)
    near(sketch(session.call("undo", document=doc))["geometry"][0]["major_radius"], 4)
    near(sketch(session.call("redo", document=doc))["geometry"][0]["major_radius"], 5)
    state = transaction(session, doc, "Pad dimensioned ellipse", [add("PartDesign::Pad", "Pad", "Body"),
                 set_value("Pad", "Profile", link("Sketch")), set_value("Pad", "Length", 5)])
    near(solid(state, "Pad")["volume"], 37.5 * math.pi)
    state = roundtrip(session, doc, output / "dimensioned-ellipse.FCStd", "Pad", 37.5 * math.pi)
    near(sketch(state)["geometry"][0]["major_radius"], 5)
    session.call("close", document=state["document"])
    return {"native_axis_diameter_dimensions": True, "typed_unit_undo_redo_reopen": True}


def constructor_preflight(session, compiler):
    doc = begin(session, "ConstructorPreflight")
    state = transaction(session, doc, "Native ellipse", draw_curve(compiler, CURVES[0][0]))
    state = session.call("undo", document=doc)
    revision, redo = state["revision"], state["redo_count"]
    for kind, args in [("RadiusMajor", [0, 3]), ("Distance", [0]), ("Coincident", [0.0, 1, 0, 2]),
                       ("Tangent", [0, 1, 2, 1.5]), ("Block", [2**40])]:
        session.call("transaction", document=doc, label="Rejected native constructor", expect_error="invalid_request",
                     edits=[dict(op="constraint", object="Sketch", kind=kind, args=args)])
        state = inspect(session, doc)
        assert state["available"] and state["revision"] == revision and state["redo_count"] == redo
    assert session.call("capabilities")["process_id"] == session.capabilities["process_id"]
    state = session.call("redo", document=doc)
    assert sketch(state)["geometry"][0]["kind"] == "ellipse"
    session.call("close", document=doc)
    return {"native_constructor_crash_paths_rejected": True, "revision_and_redo_preserved": True}


def spline_parameters(session, compiler):
    doc = begin(session, "NativeRationalSpline")
    rational = dict(kind="bspline", poles=[[0, 0, 0], [2, 3, 0], [5, 3, 0], [7, 0, 0]],
                    degree=3, weights=[1, 2, 1, 1], knots=[0, 1], multiplicities=[4, 4])
    state = transaction(session, doc, "Native rational spline", [dict(op="geometry", object="Sketch", **rational)])
    curve = sketch(state)["geometry"][0]
    assert curve["rational"] and curve["weights"] == [1, 2, 1, 1]
    assert curve["knots"] == [0, 1] and curve["multiplicities"] == [4, 4]
    session.call("transaction", document=doc, label="Invalid native knot multiplicities", expect_error="invalid_geometry",
                 edits=[dict(op="geometry", object="Sketch", **{**rational, "multiplicities": [1, 1]})])
    state = inspect(session, doc)
    assert state["available"] and sketch(state)["geometry_count"] == 1
    session.call("close", document=doc)
    doc = begin(session, "NativePeriodicSpline")
    state = transaction(session, doc, "Native periodic spline", compiler.draw("periodic_bspline",
                [[0, 0, 0], [5, 0, 0], [5, 5, 0], [0, 5, 0]]))
    assert sketch(state)["geometry"][0]["periodic"]
    assert not sketch(state)["geometry"][0]["handles"]
    state = transaction(session, doc, "Pad native periodic profile", [add("PartDesign::Pad", "Pad", "Body"),
                set_value("Pad", "Profile", link("Sketch")), set_value("Pad", "Length", 5)])
    assert solid(state, "Pad")["volume"] > 0
    session.call("close", document=doc)
    return {"rational_parameters_preserved": True, "invalid_knots_atomic": True, "periodic_profile_pad": True}


def native_intersection(session):
    doc = session.call("new", name="NativeIntersection")["document"]
    transaction(session, doc, "Native reference solid", [add("Part::Box", "Source")])
    transaction(session, doc, "Intersection sketch", [add("Sketcher::SketchObject", "Sketch"),
                set_value("Sketch", "Placement", {"position": [0, 0, 5], "rotation": [0, 0, 0, 1]})])
    state = transaction(session, doc, "Project native face", [dict(op="external_geometry", object="Sketch", source="Source", subelement="Face1")])
    assert sketch(state)["externals"][0]["projection"]
    state = transaction(session, doc, "Native intersection mode", [dict(op="external_state", object="Sketch", index=0, intersection=True, defining=True)])
    assert sketch(state)["externals"][0]["intersection"] and sketch(state)["externals"][0]["defining"]
    state = session.call("undo", document=doc)
    assert sketch(state)["externals"][0]["projection"] and not sketch(state)["externals"][0]["defining"]
    state = session.call("redo", document=doc)
    assert sketch(state)["externals"][0]["intersection"]
    owner = "intersection_edit"
    session.call("sketch_begin", document=doc, expected_revision=session.revisions[doc], token=owner, object="Sketch")
    state = transaction(session, doc, "Owned native external edit", [dict(op="external_state", object="Sketch", index=0, defining=False)], sketch_token=owner)
    assert not sketch(state)["externals"][0]["defining"]
    state = session.call("undo", document=doc, sketch_token=owner)
    assert sketch(state)["externals"][0]["defining"]
    state = session.call("redo", document=doc, sketch_token=owner)
    assert not sketch(state)["externals"][0]["defining"]
    state = session.call("sketch_cancel", document=doc, expected_revision=session.revisions[doc], token=owner)
    assert sketch(state)["externals"][0]["intersection"] and sketch(state)["externals"][0]["defining"]
    session.call("close", document=doc)
    return {"native_intersection_mode": True, "external_session_undo_redo_cancel": True}


def projected_profile(session, compiler, output):
    doc = session.call("new", name="ProjectedProfile")["document"]
    points = [[0, 0, 0], [10, 0, 0], [0, 10, 0]]
    edits = [add("PartDesign::Body", "Body"), add("Sketcher::SketchObject", "Source", "Body")]
    edits += [dict(op="geometry", object="Source", kind="line", start=points[i], end=points[(i + 1) % 3]) for i in range(3)]
    edits += [constraint("Source", "Coincident", i, 2, (i + 1) % 3, 1) for i in range(3)]
    state = transaction(session, doc, "Parametric source profile", edits)
    transaction(session, doc, "Block native source edge", author_constraint(compiler, state, "Block", [element(1)], name="Source"))
    transaction(session, doc, "Target sketch", [add("Sketcher::SketchObject", "Sketch", "Body"),
        dict(op="geometry", object="Sketch", kind="line", start=[0, 0, 0], end=[0, 5, 0], construction=True)])
    state = transaction(session, doc, "Project native edges", [dict(op="external_geometry", object="Sketch",
                source="Source", subelement="Edge" + str(i + 1)) for i in range(3)])
    external = sketch(state)["externals"]
    assert len(external) == 3
    assert [entry["geometry_index"] for entry in external] == [-3, -4, -5]
    assert [entry["source"] for entry in external] == [link("Source", "Edge" + str(i + 1)) for i in range(3)]
    assert all(not entry["defining"] and entry["projection"] and entry["persistent_reference"] for entry in external)
    assert all(not handle["draggable"] for entry in external for handle in entry["geometry"]["handles"])
    references = [entry["persistent_reference"] for entry in external]
    state = transaction(session, doc, "Constrain to projected endpoint", author_constraint(compiler, state, "Coincident", [element(0, 1), element(-3, 1)]))
    state = transaction(session, doc, "Constrain projected alignment", author_constraint(compiler, state, "Vertical", [element(0)]))
    state = transaction(session, doc, "Dimension projected alignment", author_constraint(compiler, state, "Distance", [element(0)], "5 mm"))
    assert sketch(state)["degrees_of_freedom"] == 0
    transaction(session, doc, "Delete external and its native constraints", [dict(op="delete_external", object="Sketch", index=0)])
    state = session.call("undo", document=doc)
    assert len(sketch(state)["externals"]) == 3 and sketch(state)["constraint_count"] == 3
    session.call("redo", document=doc)
    state = session.call("undo", document=doc)
    state = transaction(session, doc, "Define profile with native projections", [dict(op="external_state", object="Sketch", index=i, defining=True) for i in range(3)])
    assert all(entry["defining"] for entry in sketch(state)["externals"])
    state = transaction(session, doc, "Pad defining external profile", [add("PartDesign::Pad", "Pad", "Body"),
                 set_value("Pad", "Profile", link("Sketch")), set_value("Pad", "Length", 5)])
    near(solid(state, "Pad")["volume"], 250)
    state = transaction(session, doc, "Edit source constrained vertex", [dict(op="move_geometry", object="Source", index=0, point=1, position=[2, 0, 0], relative=False)])
    near(solid(state, "Pad")["volume"], 200)
    near(sketch(state)["geometry"][0]["start"][0], 2)
    assert [entry["persistent_reference"] for entry in sketch(state)["externals"]] == references
    session.call("transaction", document=doc, label="Reject broken final projected solid", expect_error="invalid_model",
                 edits=[dict(op="delete_external", object="Sketch", index=0)])
    state = inspect(session, doc)
    assert state["available"] and len(sketch(state)["externals"]) == 3
    near(solid(state, "Pad")["volume"], 200)
    restored = roundtrip(session, doc, output / "projected-profile.FCStd", "Pad", 200)
    assert all(entry["defining"] for entry in sketch(restored)["externals"])
    assert sketch(restored)["degrees_of_freedom"] == 0
    session.call("close", document=restored["document"])
    return {"projected_edges": 3, "native_negative_ids": True, "parent_edit_volume": 200}


def reference_dimensions(session, compiler, output):
    doc = session.call("new", name="ProjectedDimensions")["document"]
    state = transaction(session, doc, "Native measurement source", [add("Sketcher::SketchObject", "Source"),
        dict(op="geometry", object="Source", kind="line", start=[0, 0, 0], end=[10, 0, 0]),
        dict(op="geometry", object="Source", kind="line", start=[10, 0, 0], end=[10, 10, 0]),
        dict(op="geometry", object="Source", kind="circle", center=[5, 5, 0], radius=2),
        add("Sketcher::SketchObject", "Sketch")])
    state = transaction(session, doc, "Project native measurement references", [dict(op="external_geometry", object="Sketch",
        source="Source", subelement="Edge" + str(i + 1)) for i in range(3)])
    for kind, selection in [("Distance", [element(-3)]), ("Radius", [element(-5)]),
                            ("Angle", [element(-3), element(-4)])]:
        edits = author_constraint(compiler, state, kind, selection)
        assert all(edit["op"] != "datum" for edit in edits), edits
        edits.append(dict(op="constraint_state", object="Sketch", index=sketch(state)["constraint_count"], driving=False))
        state = transaction(session, doc, "Native projected reference " + kind, edits)
        compiler.call(expect_error=True, op="constraint", snapshot=objects(state)["Sketch"],
                      kind=kind, selection=selection, value="5 mm" if kind != "Angle" else "45 deg")
    def measurements(current, expected):
        native = sketch(current)
        assert native["degrees_of_freedom"] == 0
        assert all(not item["driving"] for item in native["constraints"])
        for item, value in zip(native["constraints"], expected):
            near(item["value"], value)
    measurements(state, [10, 2, math.pi / 2])
    state = transaction(session, doc, "Change projected measured geometry", [
        dict(op="move_geometry", object="Source", index=0, point=2, position=[12, 0, 0], relative=False),
        dict(op="move_geometry", object="Source", index=1, point=2, position=[20, 10, 0], relative=False),
        dict(op="move_geometry", object="Source", index=2, point=0, position=[8, 5, 0], relative=False)])
    measurements(state, [12, 3, math.pi / 4])
    measurements(session.call("undo", document=doc), [10, 2, math.pi / 2])
    measurements(session.call("redo", document=doc), [12, 3, math.pi / 4])
    path = str((output / "projected-reference-dimensions.FCStd").resolve())
    session.call("save", document=doc, path=path)
    session.call("close", document=doc)
    state = session.call("open", path=path)
    measurements(state, [12, 3, math.pi / 4])
    session.call("close", document=state["document"])
    return {"compiled_reference_dimensions": ["Distance", "Radius", "Angle"], "parent_edit_undo_redo_reopen": True}


def projected_face_carriers(session):
    doc = session.call("new", name="ProjectedFaceCarriers")["document"]
    transaction(session, doc, "Native face sources", [add("Part::Box", "First"), add("Part::Box", "Second"),
        set_value("Second", "Placement", {"position": [20, 0, 0]}), add("Sketcher::SketchObject", "Sketch")])
    state = transaction(session, doc, "Two native projected faces", [dict(op="external_geometry", object="Sketch",
        source=name, subelement="Face6") for name in ("First", "Second")])
    baseline = sketch(state)["externals"]
    assert len(baseline) == 8 and [entry["source_index"] for entry in baseline] == [0] * 4 + [1] * 4
    state = transaction(session, doc, "Define one native carrier", [dict(op="external_state", object="Sketch", index=1, defining=True)])
    assert [entry["defining"] for entry in sketch(state)["externals"]] == [False, True] + [False] * 6
    state = session.call("undo", document=doc)
    assert not any(entry["defining"] for entry in sketch(state)["externals"])
    state = transaction(session, doc, "Delete cached carrier of second reference", [dict(op="delete_external", object="Sketch", index=5)])
    assert len(sketch(state)["externals"]) == 4
    assert all(entry["source"]["link"] == "First" for entry in sketch(state)["externals"])
    state = session.call("undo", document=doc)
    assert len(sketch(state)["externals"]) == 8
    state = session.call("redo", document=doc)
    assert len(sketch(state)["externals"]) == 4
    state = session.call("undo", document=doc)
    # The UI deduplicates selected carriers by native persistent reference.
    # Descending cached indices keep the remaining references' indices valid.
    by_reference = {entry["persistent_reference"]: entry["index"] for entry in sketch(state)["externals"]}
    state = transaction(session, doc, "Delete two native reference groups", [dict(op="delete_external", object="Sketch", index=index)
        for index in sorted(by_reference.values(), reverse=True)])
    assert not sketch(state)["externals"]
    state = session.call("undo", document=doc)
    assert len(sketch(state)["externals"]) == 8
    for flag, value in [("defining", "false"), ("intersection", 1), ("defining", None)]:
        session.call("transaction", document=doc, label="Reject invalid native external flag", expect_error="invalid_geometry", edits=[
            dict(op="external_state", object="Sketch", index=0, defining=True),
            dict(op="external_geometry", object="Sketch", source="First", subelement="Face1", **{flag: value})])
        restored = sketch(inspect(session, doc))["externals"]
        assert len(restored) == 8 and not any(entry["defining"] for entry in restored)
        assert [entry["persistent_reference"] for entry in restored] == [entry["persistent_reference"] for entry in baseline]
    session.call("close", document=doc)
    return {"carriers": 8, "native_reference_groups": 2, "defining_per_carrier": True,
            "delete_per_reference_undo_redo": True, "invalid_flags_atomic": True}


def conic_equality(session, compiler):
    cases = [
        ("mixed_ellipse", [("ellipse", [[0, 0, 0], [3, 0, 0], [0, 2, 0]]),
                           ("ellipse_arc", [[10, 0, 0], [14, 0, 0], [10, 3, 0], [14, 0, 0], [6, 0, 0]])],
         ["major_radius", "minor_radius"], 2),
        ("hyperbola", [("hyperbola_arc", [[x, 0, 0], [x + a, 0, 0], [x, b, 0],
                           [x + a * math.cosh(1), -b * math.sinh(1), 0],
                           [x + a * math.cosh(1), b * math.sinh(1), 0]]) for x, a, b in [(0, 2, 1), (10, 2.1, 1.05)]],
         ["major_radius", "minor_radius"], 2),
        ("parabola", [("parabola_arc", [[x, 0, 0], [x + f, 0, 0],
                          [x + 1 / (4 * f), -1, 0], [x + 1 / (4 * f), 1, 0]]) for x, f in [(0, 1), (10, 1.1)]],
         ["focal"], 1),
    ]
    results = []
    for label, draws, properties, reduction in cases:
        doc = begin(session, "Equal_" + label)
        edits = []
        for index, (tool, points) in enumerate(draws):
            edits.extend(compiler.draw(tool, points, index=index))
        state = transaction(session, doc, "Native conics", edits)
        before = sketch(state)["degrees_of_freedom"]
        state = transaction(session, doc, "Compiled conic equality", author_constraint(compiler, state, "Equal", [element(0), element(1)]))
        assert sketch(state)["degrees_of_freedom"] == before - reduction
        first, second = sketch(state)["geometry"]
        for prop in properties:
            near(first[prop], second[prop])
        assert sketch(session.call("undo", document=doc))["degrees_of_freedom"] == before
        assert sketch(session.call("redo", document=doc))["degrees_of_freedom"] == before - reduction
        session.call("close", document=doc)
        results.append(label)
    return {"native_equal_families": results, "native_solver_undo_redo": True}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", required=True, type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--compiler", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    session = Session(args.host, json.loads(args.runtime.read_text()))
    compiler = Compiler(args.compiler)
    try:
        report = {"curves": [advanced_curve(session, compiler, case, dof, area, args.output) for case, dof, area in CURVES],
                  "spline_controls": native_spline_controls(session, compiler), "spline_parameters": spline_parameters(session, compiler),
                  "ellipse_dimensions": ellipse_axis_dimensions(session, compiler, args.output), "constructor_preflight": constructor_preflight(session, compiler),
                  "intersection": native_intersection(session), "external_profile": projected_profile(session, compiler, args.output),
                  "reference_dimensions": reference_dimensions(session, compiler, args.output), "conic_equality": conic_equality(session, compiler),
                  "face_carriers": projected_face_carriers(session)}
        (args.output / "sketch-curves-report.json").write_text(json.dumps(report, indent=2))
        print(json.dumps(report, indent=2))
    finally:
        compiler.close()
        session.close()


if __name__ == "__main__":
    main()
