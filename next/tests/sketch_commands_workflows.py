"""Verify Rust-generated sketch commands against the real native solver.

The persistent compiler and native worker are separate processes. Every drawing
and authored constraint in these checks comes from the shipped Rust compiler.
"""

import argparse
import json
import math
from pathlib import Path
import subprocess

from native_workflows import Session, add, near, objects, transaction


class Compiler:
    def __init__(self, executable):
        self.process = subprocess.Popen([str(executable)], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, text=True, encoding="utf-8")

    def call(self, expect_error=False, **request):
        self.process.stdin.write(json.dumps(request) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        assert line, f"Sketch compiler exited: {self.process.poll()}"
        result = json.loads(line)
        if expect_error:
            assert not result["ok"], result
            assert result["error"]["code"] == "invalid_sketch_command", result
            return result
        assert result["ok"], result
        return result["edits"]

    def draw(self, tool, points, index=0, construction=False):
        return self.call(op="draw", object="Sketch", tool=tool, points=points,
                         index=index, construction=construction)

    def close(self):
        self.process.stdin.close()
        assert self.process.wait(timeout=10) == 0


def element(index, point=0):
    return dict(index=index, point=point)


def constraint_case(session, compiler, label, draws, kind, selection, reduction,
                    value=None, expected_value=None):
    doc = session.call("new", name="CompilerConstraint")["document"]
    edits = [add("Sketcher::SketchObject", "Sketch")]
    index = 0
    for tool, points in draws:
        generated = compiler.draw(tool, points, index)
        index += sum(edit["op"] == "geometry" for edit in generated)
        edits.extend(generated)
    state = transaction(session, doc, "Draw with Rust", edits)
    snapshot = objects(state)["Sketch"]
    before = snapshot["sketch"]["degrees_of_freedom"]
    edits = compiler.call(op="constraint", snapshot=snapshot, kind=kind,
                          selection=selection, value=value)
    state = transaction(session, doc, label, edits)
    sketch = objects(state)["Sketch"]["sketch"]
    after = sketch["degrees_of_freedom"]
    assert before - after == reduction, (label, before, after, reduction)
    assert not sketch["conflicting_constraints"], (label, sketch)
    assert not sketch["redundant_constraints"], (label, sketch)
    native = sketch["constraints"][snapshot["sketch"]["constraint_count"]:]
    if kind == "Block" and selection[0]["point"] != 0:
        assert [constraint["type"] for constraint in native] == ["DistanceX", "DistanceY"]
    else:
        assert native[0]["type"] == kind, (label, native)
    if expected_value is not None:
        near(native[0]["value"], expected_value)
    session.call("close", document=doc)
    return dict(label=label, freedom_before=before, freedom_after=after,
                constraint_type=kind, typed_value=value)


def constraint_workflows(session, compiler):
    line = ("line", [[1, 2, 0], [5, 5, 0]])
    line2 = ("line", [[15, 2, 0], [19, 12, 0]])
    horizontal = ("line", [[0, 0, 0], [10, 0, 0]])
    vertical = ("line", [[0, 0, 0], [0, 10, 0]])
    circle = ("circle", [[5, 5, 0], [10, 5, 0]])
    arc = ("arc", [[10, 0, 0], [15, 5, 0], [10, 10, 0]])
    results = []

    def check(label, draws, kind, selection, reduction=1, value=None, expected=None):
        results.append(constraint_case(session, compiler, label, draws, kind,
                                       selection, reduction, value, expected))

    for kind in ["Horizontal", "Vertical"]:
        check(kind, [line], kind, [element(0)])
        check(kind + " points", [line], kind, [element(0, 1), element(0, 2)])
    check("Coincident", [line, line2], "Coincident", [element(0, 2), element(1, 1)], 2)
    check("Coincident origin", [line], "Coincident", [element(0, 1), element(-1, 1)], 2)
    for kind in ["Parallel", "Perpendicular", "Equal"]:
        check(kind, [line, line2], kind, [element(0), element(1)])
    check("Tangent entities", [horizontal, circle], "Tangent", [element(0), element(1)])
    check("Tangent endpoints", [horizontal, arc], "Tangent", [element(0, 2), element(1, 1)], 3)
    check("Perpendicular endpoints", [horizontal, ("line", [[10, 0, 0], [10, 10, 0]])],
          "Perpendicular", [element(0, 2), element(1, 1)], 3)
    check("Equal circles", [circle, ("circle", [[20, 20, 0], [25, 20, 0]])],
          "Equal", [element(0), element(1)])
    check("Symmetry about axis", [("line", [[-5, 0, 0], [5, 0, 0]]),
                                  ("line", [[0, -10, 0], [0, 10, 0]])],
          "Symmetric", [element(0, 1), element(0, 2), element(1)], 2)
    check("Symmetry about point", [("line", [[-5, 0, 0], [5, 0, 0]]),
                                   ("circle", [[0, 0, 0], [2, 0, 0]])],
          "Symmetric", [element(0, 1), element(0, 2), element(1, 3)], 2)
    for kind in ["Distance", "DistanceX", "DistanceY"]:
        check(kind + " line", [line], kind, [element(0)], value="12 mm", expected=12)
        check(kind + " point to origin", [line], kind, [element(0, 1)], value="12 mm", expected=12)
        check(kind + " between points", [line, line2], kind,
              [element(0, 2), element(1, 1)], value="12 mm", expected=12)
    check("Distance point to line", [line, line2], "Distance", [element(0, 2), element(1)],
          value="12 mm", expected=12)
    check("Circle radius", [circle], "Radius", [element(0)], value="8 mm", expected=8)
    check("Arc diameter", [arc], "Diameter", [element(0)], value="12 mm", expected=12)
    check("Line angle", [line], "Angle", [element(0)], value="35 deg", expected=math.radians(35))
    check("Two line angle", [line, line2], "Angle", [element(0), element(1)],
          value="60 deg", expected=math.radians(60))
    check("Block line", [line], "Block", [element(0)], 4)
    check("Block circle", [circle], "Block", [element(0)], 3)
    check("Fix point", [line], "Block", [element(0, 1)], 2)
    check("Point on object", [line, line2], "PointOnObject", [element(0, 2), element(1)])
    check("Angle from zero", [horizontal], "Angle", [element(0)], value="35 deg", expected=math.radians(35))
    check("Two line angle from zero", [horizontal, ("line", [[20, 0, 0], [30, 0, 0]])],
          "Angle", [element(0), element(1)], value="35 deg", expected=math.radians(35))
    check("DistanceX from zero", [vertical], "DistanceX", [element(0)], value="12 mm", expected=12)
    check("DistanceY from zero", [horizontal], "DistanceY", [element(0)], value="12 mm", expected=12)
    return results


def drawing_workflows(session, compiler):
    cases = [
        ("line", [[1, 2, 0], [5, 5, 0]], 1, 4),
        ("polyline", [[0, 0, 0], [5, 0, 0], [5, 5, 0]], 2, 6),
        ("rectangle", [[-5, 3, 0], [35, 33, 0]], 4, 4),
        ("circle", [[5, 5, 0], [10, 5, 0]], 1, 3),
        ("arc", [[10, 0, 0], [15, 5, 0], [10, 10, 0]], 1, 5),
        ("point", [[4, 6, 0]], 1, 2),
    ]
    results = []
    for tool, points, count, freedom in cases:
        doc = session.call("new", name="CompilerDrawing")["document"]
        state = transaction(session, doc, "Draw " + tool, [
            add("Sketcher::SketchObject", "Sketch"),
            *compiler.draw(tool, points, construction=True)])
        sketch = objects(state)["Sketch"]["sketch"]
        assert sketch["geometry_count"] == count, (tool, sketch)
        assert sketch["degrees_of_freedom"] == freedom, (tool, sketch)
        assert all(geo["construction"] for geo in sketch["geometry"]), (tool, sketch)
        assert objects(state)["Sketch"]["shape"]["null"], (tool, state)
        if tool == "rectangle":
            assert not any(c["type"] in {"Distance", "DistanceX", "DistanceY", "Block"}
                           for c in sketch["constraints"])
        results.append(dict(tool=tool, geometry=count, freedom=freedom, construction=True))
        session.call("close", document=doc)
    compiler.call(expect_error=True, op="draw", object="Sketch", tool="arc",
                  points=[[0, 0, 0], [1, 1, 0], [2, 2, 0]])
    compiler.call(expect_error=True, op="draw", object="Sketch", tool="line",
                  points=[[0, 0, 0], [0, 0, 0]])
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    compiler = Compiler(args.compiler.resolve())
    session = Session(args.host.resolve(), json.loads(args.runtime.read_text(encoding="utf-8-sig")))
    try:
        result = dict(drawings=drawing_workflows(session, compiler),
                      constraints=constraint_workflows(session, compiler))
        (args.output / "report.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(dict(drawings=len(result["drawings"]), constraints=len(result["constraints"]))))
    finally:
        session.close()
        compiler.close()


if __name__ == "__main__":
    main()
