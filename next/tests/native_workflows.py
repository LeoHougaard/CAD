"""Exercise real FreeCAD document workflows through the Rust process boundary.

No mocked geometry. Run setup_runtime.py first, then pass its JSON with --runtime.
Outputs and native documents belong in a disposable, caller-selected directory.
"""

import argparse
import json
import math
from pathlib import Path
import subprocess
import time


class Session:
    def __init__(self, host, runtime):
        command = [str(host), "--executable", runtime["executable"],
                   "--script", runtime["script"]]
        for arg in runtime["args"]:
            command.extend(["--arg", arg])
        start = time.perf_counter()
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, text=True, encoding="utf-8")
        self.timings = []
        self.revisions = {}
        self.capabilities = self.call("capabilities")
        self.startup_ms = (time.perf_counter() - start) * 1000

    def call(self, op, expect_error=None, **parameters):
        doc = parameters.get("document")
        if op in {"transaction", "undo", "redo", "save", "close"} and doc in self.revisions:
            parameters.setdefault("expected_revision", self.revisions[doc])
        start = time.perf_counter()
        self.process.stdin.write(json.dumps(dict(op=op, **parameters)) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise AssertionError(f"Native host exited: {self.process.poll()}")
        response = json.loads(line)
        self.timings.append({"op": op, "ms": (time.perf_counter() - start) * 1000})
        if expect_error:
            assert not response["ok"], response
            assert response["error"]["code"] == expect_error, response
            return response["error"]
        assert response["ok"], response
        result = response["result"]
        if "document" in result and "revision" in result:
            self.revisions[result["document"]] = result["revision"]
        if op == "close":
            self.revisions.pop(doc, None)
        return result

    def close(self):
        self.process.stdin.close()
        assert self.process.wait(timeout=10) == 0


def add(type_id, name, body=None):
    return dict(op="add_object", type=type_id, name=name, **({"body": body} if body else {}))


def set_value(obj, prop, value):
    return dict(op="set", object=obj, property=prop, value=value)


def link(obj, *subelements):
    return dict(link=obj, **({"subelements": list(subelements)} if subelements else {}))


def constraint(obj, kind, *args):
    return dict(op="constraint", object=obj, kind=kind, args=list(args))


def circle(obj, center, radius, index=0):
    return [dict(op="geometry", object=obj, kind="circle", center=center, radius=radius),
            constraint(obj, "DistanceX", index, 3, center[0]),
            constraint(obj, "DistanceY", index, 3, center[1]),
            constraint(obj, "Diameter", index, 2 * radius)]


def rectangle(obj, width=40, height=30):
    points = [[0, 0, 0], [width, 0, 0], [width, height, 0], [0, height, 0]]
    edits = [dict(op="geometry", object=obj, kind="line", start=points[i], end=points[(i+1) % 4])
             for i in range(4)]
    edits.extend(constraint(obj, "Coincident", i, 2, (i+1) % 4, 1) for i in range(4))
    edits.extend(constraint(obj, kind, i) for i, kind in enumerate(
        ["Horizontal", "Vertical", "Horizontal", "Vertical"]))
    edits.extend([constraint(obj, "Coincident", 0, 1, -1, 1),
                  constraint(obj, "Distance", 0, width), constraint(obj, "Distance", 1, height)])
    return edits


def objects(state):
    return {obj["name"]: obj for obj in state["objects"]}


def near(actual, expected, tolerance=1e-6):
    assert math.isclose(actual, expected, rel_tol=1e-8, abs_tol=tolerance), (actual, expected)


def solid(state, obj, volume=None):
    result = objects(state)[obj]["shape"]
    assert result["valid"] and result["solids"] == 1, (obj, result)
    if volume is not None:
        near(result["volume"], volume)
    return result


def transaction(session, doc, label, edits, **parameters):
    return session.call("transaction", document=doc, label=label, edits=edits, **parameters)["state"]


def sketch_workflow(session):
    doc = session.call("new", name="SketchEditing")["document"]
    state = transaction(session, doc, "Start empty sketch", [add("Sketcher::SketchObject", "Sketch")])
    assert objects(state)["Sketch"]["shape"]["null"]
    state = transaction(session, doc, "Underconstrained line", [
        dict(op="geometry", object="Sketch", kind="line", start=[0, 0, 0], end=[10, 0, 0]),
        constraint("Sketch", "Horizontal", 0)])
    assert objects(state)["Sketch"]["sketch"]["degrees_of_freedom"] == 3
    transaction(session, doc, "Drag line endpoint", [
        dict(op="move_geometry", object="Sketch", index=0, point=2, position=[20, 10, 0])])
    edge = session.call("topology", document=doc, object="Sketch")["edges"][0]
    near(edge["endpoints"][1][0], 20)
    near(edge["endpoints"][1][1], 10)
    near(edge["endpoints"][0][1], 10)
    state = transaction(session, doc, "Toggle construction", [dict(op="construction", object="Sketch", index=0)])
    assert objects(state)["Sketch"]["shape"]["null"]
    state = session.call("undo", document=doc)
    assert not objects(state)["Sketch"]["shape"]["null"]
    state = transaction(session, doc, "Delete back to empty", [dict(op="delete_geometry", object="Sketch", index=0)])
    assert objects(state)["Sketch"]["sketch"]["geometry_count"] == 0
    assert objects(state)["Sketch"]["shape"]["null"]
    state = transaction(session, doc, "Three point arc", [dict(op="geometry", object="Sketch", kind="arc",
        start=[0, 0, 0], mid=[10, 10, 0], end=[20, 0, 0])])
    edge = session.call("topology", document=doc, object="Sketch")["edges"][0]
    near(edge["length"], 10 * math.pi)
    return dict(degrees_of_freedom=objects(state)["Sketch"]["sketch"]["degrees_of_freedom"], arc_length=edge["length"])


def attachment_workflow(session, output):
    doc = session.call("new", name="AttachedPocket")["document"]
    state = transaction(session, doc, "Constrained base and pad", [
        add("PartDesign::Body", "Body"), add("Sketcher::SketchObject", "BaseSketch", "Body"),
        *rectangle("BaseSketch"), add("PartDesign::Pad", "Pad", "Body"),
        set_value("Pad", "Profile", link("BaseSketch")), set_value("Pad", "Length", 20)])
    solid(state, "Pad", 24000)
    assert objects(state)["BaseSketch"]["sketch"]["degrees_of_freedom"] == 0
    topology = session.call("topology", document=doc, object="Pad")
    face = next(face["subelement"] for face in topology["faces"]
                if face["normal"] and face["normal"][2] > .99)
    state = transaction(session, doc, "Attach pocket sketch to native face", [
        add("Sketcher::SketchObject", "PocketSketch", "Body"),
        set_value("PocketSketch", "AttachmentSupport", link("Pad", face)),
        set_value("PocketSketch", "MapMode", "FlatFace"),
        *circle("PocketSketch", [10, 10, 0], 3),
        add("PartDesign::Pocket", "Pocket", "Body"),
        set_value("Pocket", "Profile", link("PocketSketch")), set_value("Pocket", "Length", 5)])
    removed = math.pi * 9 * 5
    solid(state, "Pocket", 24000 - removed)
    near(objects(state)["PocketSketch"]["properties"]["Placement"]["position"][2], 20)
    state = transaction(session, doc, "Resize parent dimensions", [
        set_value("Pad", "Length", 30), dict(op="datum", object="BaseSketch", index=9, value="50 mm")])
    solid(state, "Pocket", 45000 - removed)
    near(objects(state)["PocketSketch"]["properties"]["Placement"]["position"][2], 30)
    session.call("transaction", document=doc, label="Wrong dimension units", edits=[
        dict(op="datum", object="BaseSketch", index=9, value="10 deg")], expect_error="dimension_mismatch")
    solid(session.call("inspect", document=doc), "Pocket", 45000 - removed)
    state = transaction(session, doc, "Native dimension expression", [
        dict(op="expression", object="BaseSketch", property="Constraints[10]", expression="Constraints[9] * 0.6")])
    solid(state, "Pocket", 45000 - removed)
    # An added inner wire changes the parent's number of faces and edges. The
    # original attached sketch must stay on the extrusion end through mapping.
    state = transaction(session, doc, "Add parent inner wire", circle("BaseSketch", [30, 15, 0], 2, 4))
    expected = 45000 - math.pi * 4 * 30 - removed
    solid(state, "Pocket", expected)
    assert objects(state)["Pad"]["shape"]["faces"] == 7
    near(objects(state)["PocketSketch"]["properties"]["Placement"]["position"][2], 30)
    session.call("save", document=doc, path=str(output / "attached-pocket.FCStd"))
    session.call("close", document=doc)
    state = session.call("open", path=str(output / "attached-pocket.FCStd"))
    doc = state["document"]
    solid(state, "Pocket", expected)
    state = transaction(session, doc, "Edit restored attached dimension", [
        dict(op="datum", object="PocketSketch", index=2, value="8 mm")])
    solid(state, "Pocket", 45000 - math.pi * 4 * 30 - math.pi * 16 * 5)
    near(objects(state)["PocketSketch"]["properties"]["Placement"]["position"][2], 30)
    assert objects(state)["PocketSketch"]["sketch"]["degrees_of_freedom"] == 0
    session.call("save", document=doc, path=str(output / "attached-pocket-edited.FCStd"))
    state = transaction(session, doc, "Move and rotate the owning Body", [
        set_value("Body", "Placement", dict(position=[100, 200, 300],
                  rotation=[0, 0, math.sqrt(.5), math.sqrt(.5)]))])
    body = solid(state, "Body")
    for actual, expected in zip(body["bounds"]["min"], [70, 200, 300]):
        near(actual, expected)
    for actual, expected in zip(body["bounds"]["max"], [100, 250, 330]):
        near(actual, expected)
    child = session.call("topology", document=doc, object="Pocket")
    assert child["world_from_shape"]["position"] == [100, 200, 300]
    parent = session.call("mesh", document=doc, object="Body")
    for coordinate in parent["world_from_shape"]["position"]:
        near(coordinate, 0)
    session.call("save", document=doc, path=str(output / "placed-body.FCStd"))
    return dict(document=doc, final_volume=objects(state)["Pocket"]["shape"]["volume"],
                support=objects(state)["PocketSketch"]["properties"]["AttachmentSupport"])


def fillet_workflow(session, output):
    doc = session.call("new", name="NativeFillets")["document"]
    state = transaction(session, doc, "Base block", [
        add("PartDesign::Body", "Body"), add("Sketcher::SketchObject", "Sketch", "Body"),
        *rectangle("Sketch"), add("PartDesign::Pad", "Pad", "Body"),
        set_value("Pad", "Profile", link("Sketch")), set_value("Pad", "Length", 20)])
    topo = session.call("topology", document=doc, object="Pad")
    edges = [edge["subelement"] for edge in topo["edges"]]
    state = transaction(session, doc, "Three-edge corners", [
        add("PartDesign::Fillet", "CornerFillet", "Body"),
        set_value("CornerFillet", "Base", link("Pad", *edges)),
        set_value("CornerFillet", "Radius", 2)])
    corner = solid(state, "CornerFillet")
    # Rounded cuboid = inset box + face slabs + quarter-cylinders + corner sphere.
    a, b, c, r = 36, 26, 16, 2
    expected = a*b*c + 2*r*(a*b + b*c + c*a) + math.pi*r*r*(a+b+c) + 4*math.pi*r**3/3
    near(corner["volume"], expected)
    assert corner["faces"] > 6
    session.call("save", document=doc, path=str(output / "three-edge-corners.FCStd"))
    # Undo the complete fillet feature, then redo it, including its links.
    undone = session.call("undo", document=doc)
    assert "CornerFillet" not in objects(undone)
    redone = session.call("redo", document=doc)
    near(solid(redone, "CornerFillet")["volume"], corner["volume"])
    revision = redone["revision"]
    session.call("transaction", document=doc, label="Impossible radius", edits=[
        set_value("CornerFillet", "Radius", 1000)], expect_error="invalid_model")
    restored = session.call("inspect", document=doc)
    assert restored["available"] and restored["revision"] > revision
    near(solid(restored, "CornerFillet")["volume"], corner["volume"])
    near(objects(restored)["CornerFillet"]["properties"]["Radius"], 2)
    # A different agent's stale command must not alter this document.
    session.call("transaction", document=doc, label="Stale edit", expected_revision=revision - 1,
                 edits=[set_value("CornerFillet", "Radius", 1)], expect_error="stale_revision")
    near(solid(session.call("inspect", document=doc), "CornerFillet")["volume"], corner["volume"])
    session.call("undo", document=doc)
    vertical = next(edge["subelement"] for edge in topo["edges"]
                    if edge["bounds"]["max"][2] - edge["bounds"]["min"][2] > 19.9)
    state = transaction(session, doc, "Vertical fillet ending at top", [
        add("PartDesign::Fillet", "SideFillet", "Body"),
        set_value("SideFillet", "Base", link("Pad", vertical)), set_value("SideFillet", "Radius", 3)])
    solid(state, "SideFillet", 24000 - (1 - math.pi / 4) * 9 * 20)
    topo = session.call("topology", document=doc, object="SideFillet")
    top = next(edge["subelement"] for edge in topo["edges"]
               if edge["bounds"]["min"][2] > 19.99 and edge["length"] > 25)
    state = transaction(session, doc, "Top fillet along tangent chain", [
        add("PartDesign::Fillet", "TopFillet", "Body"),
        set_value("TopFillet", "Base", link("SideFillet", top)), set_value("TopFillet", "Radius", 1.5)])
    result = solid(state, "TopFillet")
    assert result["volume"] < objects(state)["SideFillet"]["shape"]["volume"]
    topo = session.call("topology", document=doc, object="TopFillet")
    created = [face["bounds"] for face in topo["faces"]
               if face["normal"] is None and face["bounds"]["min"][2] > 18.49]
    # Propagation must reach BOTH straight runs joined by the existing tangent
    # corner. Merely reducing the volume along the selected edge is insufficient.
    assert any(b["max"][0] - b["min"][0] > 25 and b["max"][1] - b["min"][1] < 1.51 for b in created)
    assert any(b["max"][1] - b["min"][1] > 20 and b["max"][0] - b["min"][0] < 1.51 for b in created)
    session.call("save", document=doc, path=str(output / "tangent-fillet.FCStd"))
    mesh = session.call("mesh", document=doc, object="TopFillet", deflection=.1)
    assert mesh["positions"] and mesh["triangles"]
    assert len(mesh["faces"]) == result["faces"]
    assert all(max(tri) < len(mesh["positions"]) for tri in mesh["triangles"])
    return dict(document=doc, corner_volume=corner["volume"], tangent_volume=result["volume"],
                display_triangles=len(mesh["triangles"]))


def display_normals_workflow(session):
    doc = session.call("new", name="NativeDisplayNormals")["document"]
    transaction(session, doc, "Native normal reference shapes", [
        add("PartDesign::Body", "Body"), add("Sketcher::SketchObject", "Sketch", "Body"),
        dict(op="geometry", object="Sketch", kind="circle", center=[0, 0, 0], radius=5),
        add("PartDesign::Pad", "Pad", "Body"), set_value("Pad", "Profile", link("Sketch")),
        set_value("Pad", "Length", 10),
        add("Part::Cylinder", "Outer"), set_value("Outer", "Radius", 5), set_value("Outer", "Height", 10),
        add("Part::Cylinder", "Inner"), set_value("Inner", "Radius", 2), set_value("Inner", "Height", 10),
        add("Part::Cut", "Ring"), set_value("Ring", "Base", link("Outer")), set_value("Ring", "Tool", link("Inner")),
        add("Part::Sphere", "Sphere"), set_value("Sphere", "Radius", 5)])

    def checked_mesh(obj):
        mesh = session.call("mesh", document=doc, object=obj, deflection=.03)
        topo = session.call("topology", document=doc, object=obj, deflection=.03)
        assert len(mesh["positions"]) == len(mesh["normals"]) > 0
        for normal in mesh["normals"]:
            assert len(normal) == 3 and all(math.isfinite(value) for value in normal)
            near(sum(value * value for value in normal), 1)
        groups = []
        for face, native in zip(mesh["faces"], topo["faces"]):
            assert face["subelement"] == native["subelement"]
            first, count = face["first_triangle"], face["triangle_count"]
            vertices = {index for triangle in mesh["triangles"][first:first + count] for index in triangle}
            assert vertices
            if native["normal"] is not None:
                for index in vertices:
                    near(sum(a * b for a, b in zip(mesh["normals"][index], native["normal"])), 1)
            groups.append((native, vertices))
        return mesh, groups

    def cylindrical_normals(mesh, groups, placed=False):
        counts = {"outward": 0, "inward": 0}
        for face, vertices in groups:
            if face["geometry_type"] != "Part::GeomCylinder":
                continue
            for index in vertices:
                point = mesh["positions"][index]
                radial = [0, point[1] - 200, point[2] - 300] if placed else [point[0], point[1], 0]
                radius = math.sqrt(sum(value * value for value in radial))
                # Ring's inner cylindrical wall must point into the hole.
                sign = -1 if radius < 3 else 1
                expected = [sign * value / radius for value in radial]
                near(sum(a * b for a, b in zip(mesh["normals"][index], expected)), 1)
                counts["inward" if sign < 0 else "outward"] += 1
        assert counts["outward"]
        return counts

    mesh, groups = checked_mesh("Pad")
    cylindrical_normals(mesh, groups)
    mesh, groups = checked_mesh("Ring")
    ring = cylindrical_normals(mesh, groups)
    assert ring["inward"]
    sphere, _ = checked_mesh("Sphere")
    for point, normal in zip(sphere["positions"], sphere["normals"]):
        near(sum(a * b / 5 for a, b in zip(point, normal)), 1)
    coarse = session.call("topology", document=doc, object="Pad", deflection=.5)
    fine = session.call("topology", document=doc, object="Pad", deflection=.03)
    coarse_points = sum(len(edge["points"]) for edge in coarse["edges"])
    fine_points = sum(len(edge["points"]) for edge in fine["edges"])
    assert fine_points > coarse_points
    for op in ("mesh", "topology"):
        session.call(op, document=doc, object="Pad", deflection=0, expect_error="invalid_request")
    transaction(session, doc, "Place native normal reference Body", [
        set_value("Body", "Placement", dict(position=[100, 200, 300],
                  rotation=[0, math.sqrt(.5), 0, math.sqrt(.5)]))])
    child, groups = checked_mesh("Pad")
    cylindrical_normals(child, groups)
    transform = child["world_from_shape"]
    assert transform["position"] == [100, 200, 300]
    for actual, expected in zip(transform["rotation"], [0, math.sqrt(.5), 0, math.sqrt(.5)]):
        near(actual, expected)
    body, groups = checked_mesh("Body")
    cylindrical_normals(body, groups, placed=True)
    for value in body["world_from_shape"]["position"]:
        near(value, 0)
    # The Body mesh is already placed. The child requires one quaternion
    # rotation, and normals receive no translation. Check their world agreement.
    def world_normals(values):
        return {tuple(round(value, 6) for value in normal) for normal in values}
    rotated_child = [[normal[2], normal[1], -normal[0]] for normal in child["normals"]]
    assert world_normals(rotated_child) == world_normals(body["normals"])
    return dict(ring_vertices=len(mesh["positions"]), sphere_vertices=len(sphere["positions"]),
                inward_vertices=ring["inward"], placed_body_vertices=len(body["positions"]),
                coarse_edge_points=coarse_points, fine_edge_points=fine_points)


def unicode_document_workflow(session, out):
    # Session writes escaped JSON, but the production Rust host parses and
    # serializes it as actual UTF-8 before the native Python request stream.
    label = "L\u00e9o \u00f8 \u03bc \u5de5\u5177 \U0001f527"
    folder = out / "L\u00e9o \u00f8 \u5de5\u5177"
    folder.mkdir(exist_ok=True)
    path = folder / "pi\u00e8ce \U0001f527.FCStd"
    doc = session.call("new", name="UnicodeDocument")["document"]
    transaction(session, doc, "Create box", [add("Part::Box", "Box"),
                set_value("Box", "Length", 2), set_value("Box", "Width", 3),
                set_value("Box", "Height", 4)])
    state = transaction(session, doc, label, [set_value("Box", "Label", label)])
    assert objects(state)["Box"]["properties"]["Label"] == label
    state = session.call("undo", document=doc)
    assert objects(state)["Box"]["properties"]["Label"] == "Box"
    state = session.call("redo", document=doc)
    assert objects(state)["Box"]["properties"]["Label"] == label
    saved = session.call("save", document=doc, path=str(path))
    assert Path(saved["path"]) == path and path.is_file()
    session.call("close", document=doc)
    state = session.call("open", path=str(path))
    doc = state["document"]
    assert objects(state)["Box"]["properties"]["Label"] == label
    solid(state, "Box", 24)
    state = transaction(session, doc, "Edit reopened Unicode document", [set_value("Box", "Width", 6)])
    assert objects(state)["Box"]["properties"]["Label"] == label
    solid(state, "Box", 48)
    session.call("close", document=doc)
    return dict(utf8_label_undo_redo=True, unicode_path_reopen=True,
                reopened_parametric_edit_volume=48)


def canonical_expression_workflow(session, out):
    doc = session.call("new", name="CanonicalExpressions")["document"]
    state = transaction(session, doc, "Native attached sketch", [add("PartDesign::Body", "Body"),
        add("Sketcher::SketchObject", "Sketch", "Body"),
        set_value("Sketch", "AttachmentSupport", [link("XY_Plane", "")]),
        set_value("Sketch", "MapMode", "FlatFace"),
        dict(op="geometry", object="Sketch", kind="line", start=[0, 0, 0], end=[4, 3, 0]),
        dict(op="expression", object="Sketch", property="AttachmentOffset.Base.z", expression="2 mm")])
    canonical = ".AttachmentOffset.Base.z"
    assert objects(state)["Sketch"]["expressions"][canonical] == "2 mm"
    state = transaction(session, doc, "Edit canonical native expression", [
        dict(op="expression", object="Sketch", property=canonical, expression="5 mm")])
    near(objects(state)["Sketch"]["properties"]["Placement"]["position"][2], 5)
    state = session.call("undo", document=doc)
    near(objects(state)["Sketch"]["properties"]["Placement"]["position"][2], 2)
    state = session.call("redo", document=doc)
    assert objects(state)["Sketch"]["expressions"][canonical] == "5 mm"
    path = out / "canonical-expression.FCStd"
    session.call("save", document=doc, path=str(path))
    session.call("close", document=doc)
    state = session.call("open", path=str(path))
    doc = state["document"]
    assert objects(state)["Sketch"]["expressions"][canonical] == "5 mm"
    state = transaction(session, doc, "Clear reopened canonical expression", [
        dict(op="expression", object="Sketch", property=canonical, expression=None)])
    assert not objects(state)["Sketch"]["expressions"]
    state = transaction(session, doc, "Reposition after clearing expression", [
        set_value("Sketch", "AttachmentOffset", dict(position=[0, 0, 7], rotation=[0, 0, 0, 1]))])
    near(objects(state)["Sketch"]["properties"]["Placement"]["position"][2], 7)
    # A native self-relative path must not bypass computed/internal permissions,
    # nor admit an empty root through repeated dots.
    for prop, code in [(".Shape", "read_only_property"), ("._ElementMapVersion", "read_only_property"),
                       ("..AttachmentOffset.Base.z", "unsupported_property")]:
        session.call("transaction", document=doc, label="Keep native expression permissions", expect_error=code,
            edits=[dict(op="expression", object="Sketch", property=prop, expression="1")])
        state = session.call("inspect", document=doc)
        assert state["available"] and not objects(state)["Sketch"]["expressions"]
        near(objects(state)["Sketch"]["properties"]["Placement"]["position"][2], 7)
    session.call("close", document=doc)
    return dict(native_canonical_path=canonical, set_clear_undo_redo_reopen=True,
                underlying_readonly_guards=True, final_native_offset=7)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", required=True, type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    runtime = json.loads(args.runtime.read_text(encoding="utf-8-sig"))
    session = Session(args.host.resolve(), runtime)
    try:
        assert session.capabilities["freecad_version"][:3] == ["1", "1", "4"]
        result = dict(capabilities=session.capabilities,
                      sketch=sketch_workflow(session),
                      attachment=attachment_workflow(session, args.output.resolve()),
                      fillets=fillet_workflow(session, args.output.resolve()),
                      display_normals=display_normals_workflow(session),
                      unicode_document=unicode_document_workflow(session, args.output.resolve()),
                      canonical_expressions=canonical_expression_workflow(session, args.output.resolve()),
                      startup_ms=session.startup_ms, operations=session.timings)
        (args.output / "report.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({key: value for key, value in result.items() if key not in {"capabilities", "operations"}}, indent=2))
    finally:
        session.close()


if __name__ == "__main__":
    main()
