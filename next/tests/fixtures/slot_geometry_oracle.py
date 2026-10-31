"""Test-only native Slot history oracle with bracketed native-state observations."""
import io
import json
import os
from pathlib import Path
import zipfile

import FreeCAD as App
import Part

_saved = {}


def _xml(sketch, prop):
    with zipfile.ZipFile(io.BytesIO(bytes(sketch.dumpPropertyContent(prop)))) as archive:
        return archive.read("Persistence.xml").decode("utf8")


def _facts(worker, doc, sketch):
    shape = sketch.Shape
    return {
        "object": sketch.Name, "object_id": sketch.ID,
        "geometry_ids": [sketch.getGeometryId(i) for i in range(sketch.GeometryCount)],
        "construction": [sketch.getConstruction(i) for i in range(sketch.GeometryCount)],
        "geometry": _xml(sketch, "Geometry"), "constraints": _xml(sketch, "Constraints"),
        "expressions": _xml(sketch, "ExpressionEngine"),
        "geometry_content": [g.Content for g in sketch.Geometry],
        "constraint_content": [c.Content for c in sketch.Constraints],
        "placement": [[sketch.Placement.Base.x, sketch.Placement.Base.y, sketch.Placement.Base.z],
                      list(sketch.Placement.Rotation.Q)],
        "history": [doc.UndoCount, doc.RedoCount], "session_history": worker.history_counts(doc),
        "session": worker.sketch_metadata(doc),
        "map": {kind + str(i): shape.getElementMappedName(kind + str(i))
                for kind, count in (("Edge", len(shape.Edges)), ("Vertex", len(shape.Vertexes)))
                for i in range(1, count + 1)},
        "counts": [len(shape.Edges), len(shape.Vertexes), len(shape.Wires), len(shape.Faces)],
        "valid": shape.isValid(), "null": shape.isNull(),
    }


def _capture(worker, doc, sketch, output, phase):
    before = _facts(worker, doc, sketch)
    raw = sketch.Shape.exportBrepToString()
    shape = sketch.Shape.copy()
    after = _facts(worker, doc, sketch)
    (output / (phase + ".brep")).write_bytes(raw.encode("utf8"))
    (output / (phase + ".json")).write_text(json.dumps({"before": before, "after": after}, indent=2) + "\n", encoding="utf8")
    assert before == after, "Slot shape observation changed the native document"
    return shape, before


def _solid_and_line(shape):
    assert not shape.isNull() and shape.isValid() and len(shape.Edges) == 5
    closed = [w for w in shape.Wires if w.isClosed() and w.isValid() and len(w.Edges) == 4]
    opened = [w for w in shape.Wires if not w.isClosed() and w.isValid() and len(w.Edges) == 1]
    assert len(closed) == len(opened) == 1
    face = Part.Face(closed[0])
    assert face.isValid() and len(face.Faces) == 1 and face.Area > 0
    solid = face.extrude(App.Vector(0, 0, 10))
    assert solid.isValid() and len(solid.Solids) == 1 and solid.Volume > 0
    return solid, opened[0]


def _compare(before, after, original, current, output):
    metrics = {"exact_fields": {}, "edges": [], "vertices": []}
    def save():
        (output / "comparison.json").write_text(json.dumps(metrics, indent=2, allow_nan=False) + "\n", encoding="utf8")
    exact = ("object", "object_id", "geometry_ids", "construction", "constraints", "expressions",
             "constraint_content", "placement", "history", "session_history", "session", "map",
             "counts", "valid", "null")
    metrics["exact_fields"] = {field: before[field] == after[field] for field in exact}
    metrics["raw_geometry_xml_equal"] = before["geometry"] == after["geometry"]
    save()
    assert all(metrics["exact_fields"].values()), "Slot native references or history changed"
    assert before["map"] and all(isinstance(value, str) and value for value in before["map"].values())
    assert set(before["map"]) == {"Edge" + str(i) for i in range(1, 6)} | {"Vertex" + str(i) for i in range(1, 7)}
    first, line_first = _solid_and_line(original)
    second, line_second = _solid_and_line(current)
    ab, ba = first.cut(second), second.cut(first)
    metrics["solid"] = {"before_minus_after": ab.Volume, "after_minus_before": ba.Volume,
                        "common": first.common(second).Volume, "before": first.Volume, "after": second.Volume}
    xy, yx = line_first.cut(line_second), line_second.cut(line_first)
    metrics["centerline"] = {"before_minus_after": xy.Length, "after_minus_before": yx.Length,
                             "common": line_first.common(line_second).Length}
    save()
    assert ab.Volume == ba.Volume == xy.Length == yx.Length == 0.0, "Slot native geometry changed"
    assert len(original.Edges) == len(current.Edges) == 5
    assert len(original.Vertexes) == len(current.Vertexes) == 6
    for index, (a, b) in enumerate(zip(original.Edges, current.Edges), 1):
        row = {"index": index, "mapping": before["map"]["Edge" + str(index)],
               "types": [type(a.Curve).__name__, type(b.Curve).__name__],
               "before_minus_after": a.cut(b).Length, "after_minus_before": b.cut(a).Length}
        metrics["edges"].append(row); save()
        assert row["types"][0] == row["types"][1] and row["before_minus_after"] == row["after_minus_before"] == 0.0
    for index, (a, b) in enumerate(zip(original.Vertexes, current.Vertexes), 1):
        intersection = a.common(b)
        row = {"index": index, "mapping": before["map"]["Vertex" + str(index)],
               "intersection_vertices": len(intersection.Vertexes), "null": intersection.isNull()}
        metrics["vertices"].append(row); save()
        assert row["intersection_vertices"] == 1 and not row["null"]
    shifted = second.copy(); shifted.translate(App.Vector(0.125, 0, 0))
    wrong_a, wrong_b = first.cut(shifted).Volume, shifted.cut(first).Volume
    a, b = original.Edges[0], current.Edges[1]
    metrics["negative_controls"] = {"shifted_before_minus_after": wrong_a, "shifted_after_minus_before": wrong_b,
        "swapped_maps_differ": before["map"]["Edge1"] != after["map"]["Edge2"],
        "swapped_before_minus_after": a.cut(b).Length, "swapped_after_minus_before": b.cut(a).Length}
    save()
    assert wrong_a > 0 and wrong_b > 0 and metrics["negative_controls"]["swapped_maps_differ"]
    assert metrics["negative_controls"]["swapped_before_minus_after"] > 0 and metrics["negative_controls"]["swapped_after_minus_before"] > 0


def observe(worker, doc, action):
    sketch = doc.getObject("Sketch")
    assert sketch is not None and sketch.isDerivedFrom("Sketcher::SketchObject")
    if action == "capture":
        assert doc.Name not in _saved
        output = Path(os.environ["CAD_LIFECYCLE_OUTPUT"]) / ("slot-oracle-" + str(os.getpid()))
        output.mkdir()
        shape, facts = _capture(worker, doc, sketch, output, "accepted")
        _saved[doc.Name] = (doc, shape, facts, output)
    elif action == "compare":
        owner, shape, facts, output = _saved.pop(doc.Name)
        assert owner is doc
        current, after = _capture(worker, doc, sketch, output, "redo")
        _compare(facts, after, shape, current, output)
    else:
        raise ValueError("Unknown Slot geometry oracle action")
