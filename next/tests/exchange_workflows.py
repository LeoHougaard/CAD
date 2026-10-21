"""Real native STEP/IGES exchange, rollback, history and subsequent feature edits."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import tempfile

from native_workflows import Session, add, link, near, objects, set_value, solid, transaction


class ExchangeSession(Session):
    def call(self, op, expect_error=None, **parameters):
        if op == "export_exchange" and parameters.get("document") in self.revisions:
            parameters.setdefault("expected_revision", self.revisions[parameters["document"]])
        return super().call(op, expect_error=expect_error, **parameters)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def import_file(session, doc, path, *following):
    return transaction(session, doc, "Import native exchange geometry", [
        {"op": "import_exchange", "path": str(path)}, *following])


def imported_shapes(state):
    return [obj for obj in state["objects"] if obj["type"] == "Part::Feature"]


def refreshed(session, doc):
    return session.call("inspect", document=doc)


def export_file(session, doc, path, names, **parameters):
    return session.call("export_exchange", document=doc, path=str(path), objects=names, **parameters)


def workflow(session, output):
    assert "export_exchange" in session.capabilities["operations"]
    assert "import_exchange" in session.capabilities["edit_operations"]
    schema = session.call("feature_schema", type="Part::Feature")
    assert schema["property_types"]["Placement"] == "App::PropertyPlacement"
    source = session.call("new", name="ExchangeSource")["document"]
    q = math.sqrt(0.5)
    source_state = transaction(session, source, "Create placed native source parts", [
        add("App::Part", "Assembly"), add("Part::Box", "Box"), add("Part::Cylinder", "Cylinder"),
        set_value("Box", "Length", 10), set_value("Box", "Width", 5), set_value("Box", "Height", 3),
        set_value("Box", "Placement", {"position": [2, 3, 4]}),
        set_value("Assembly", "Group", [link("Box")]),
        set_value("Assembly", "Placement", {"position": [10, 20, 30], "rotation": [0, 0, q, q]}),
        set_value("Cylinder", "Radius", 2), set_value("Cylinder", "Height", 4),
        set_value("Cylinder", "Placement", {"position": [20, 0, 0]})])
    native_original = output / "source-original.FCStd"
    session.call("save", document=source, path=str(native_original))
    original_hash = digest(native_original)
    total_volume = 150 + 16 * math.pi
    total_area = 190 + 24 * math.pi
    format_reports = []
    exports = {}
    for extension in ("step", "iges"):
        path = output / ("source." + extension)
        before = refreshed(session, source)
        result = export_file(session, source, path, ["Assembly", "Cylinder"])
        assert result["exported_objects"] == ["Assembly", "Cylinder"]
        assert result["placement"] == "selected_root_local"
        assert result["revision"] == before["revision"]
        assert refreshed(session, source) == before, "Export changed the native source document"
        assert digest(native_original) == original_hash
        exports[extension] = (path, digest(path))
        target = session.call("new", name="Import" + extension.upper())["document"]
        baseline = transaction(session, target, "Preexisting native object", [add("Part::Box", "Existing")])
        state = import_file(session, target, path)
        added = imported_shapes(state)
        assert added and all(obj["shape"]["valid"] for obj in added)
        near(sum(obj["shape"]["volume"] for obj in added), total_volume, 1e-5)
        near(sum(obj["shape"]["area"] for obj in added), total_area, 1e-5)
        assert state["undo_count"] == baseline["undo_count"] + 1
        if extension == "step":
            assert len(added) == 2 and all(obj["shape"]["solids"] == 1 for obj in added)
            assembly = next(obj for obj in state["objects"] if obj["type"] == "App::Part" and obj["label"] == "Assembly")
            for key in ("min", "max"):
                for actual, expected in zip(assembly["shape"]["bounds"][key], objects(source_state)["Assembly"]["shape"]["bounds"][key]):
                    near(actual, expected)
        else:
            assert len(added) == 1 and added[0]["shape"]["solids"] == 0
            name = added[0]["name"]
            old = added[0]["shape"]["bounds"]["min"][0]
            moved = transaction(session, target, "Place imported IGES surfaces", [set_value(name, "Placement", {"position": [10, 0, 0]})])
            near(objects(moved)[name]["shape"]["bounds"]["min"][0], old + 10)
            session.call("undo", document=target)
        undone = session.call("undo", document=target)
        assert set(objects(undone)) == set(objects(baseline))
        solid(undone, "Existing", 1000)
        restored = session.call("redo", document=target)
        assert len(imported_shapes(restored)) == len(added)
        # The importer must not commit a surrounding multi-edit transaction.
        session.call("transaction", expect_error="not_found", document=target, label="Fail after import", edits=[
            {"op": "import_exchange", "path": str(path)}, set_value("Existing", "Length", 20),
            set_value("MissingObject", "Length", 1)])
        rolled_back = refreshed(session, target)
        assert set(objects(rolled_back)) == set(objects(restored))
        solid(rolled_back, "Existing", 1000)
        assert digest(path) == exports[extension][1]
        format_reports.append({"format": extension, "bytes": path.stat().st_size,
                               "imported_shapes": len(added), "faces": sum(obj["shape"]["faces"] for obj in added),
                               "solids": sum(obj["shape"]["solids"] for obj in added), "volume": total_volume})

    # Selecting a leaf deliberately retains its native local placement. Selecting
    # its enclosing Part above retained the native hierarchy and parent placement.
    leaf_file = output / "leaf-local.stp"
    export_file(session, source, leaf_file, ["Box"])
    leaf_hash = digest(leaf_file)
    edited_doc = session.call("new", name="EditImportedSTEP")["document"]
    state = import_file(session, edited_doc, leaf_file)
    imported = imported_shapes(state)[0]
    near(imported["shape"]["bounds"]["min"][0], 2)
    near(imported["shape"]["bounds"]["min"][1], 3)
    near(imported["shape"]["bounds"]["min"][2], 4)
    name = imported["name"]
    state = transaction(session, edited_doc, "Cut imported STEP with native feature", [
        add("Part::Box", "Tool"), set_value("Tool", "Length", 2), set_value("Tool", "Width", 5),
        set_value("Tool", "Height", 3), set_value("Tool", "Placement", {"position": [2, 3, 4]}),
        add("Part::Cut", "Cut"), set_value("Cut", "Base", link(name)), set_value("Cut", "Tool", link("Tool")),
        set_value(name, "Visibility", False), set_value("Tool", "Visibility", False)])
    solid(state, "Cut", 120)
    solid(transaction(session, edited_doc, "Edit native cut tool", [set_value("Tool", "Length", 3)]), "Cut", 105)
    saved = output / "edited-import.FCStd"
    session.call("save", document=edited_doc, path=str(saved))
    session.call("close", document=edited_doc)
    reopened = session.call("open", path=str(saved))
    edited_doc = reopened["document"]
    solid(reopened, "Cut", 105)
    solid(transaction(session, edited_doc, "Edit reopened native cut", [set_value("Tool", "Length", 4)]), "Cut", 90)
    assert digest(leaf_file) == leaf_hash

    # Rejections protect output files and use the same revision/ownership rules.
    path = exports["step"][0]
    old_hash = digest(path)
    export_file(session, source, path, ["Assembly"], expect_error="file_exists")
    assert digest(path) == old_hash
    protected = output / "protected.step"
    protected.write_bytes(b"existing user-owned destination sentinel")
    protected_hash = digest(protected)
    export_file(session, source, protected, ["Missing"], overwrite=True, expect_error="not_found")
    assert digest(protected) == protected_hash
    transaction(session, source, "Hide assembly geometry for rejected export", [set_value("Box", "Visibility", False)])
    export_file(session, source, protected, ["Assembly"], overwrite=True, expect_error="no_shape")
    assert digest(protected) == protected_hash, "Failed native export overwrote the existing destination"
    transaction(session, source, "Restore assembly visibility", [set_value("Box", "Visibility", True)])
    export_file(session, source, output / "unsupported.obj", ["Box"], expect_error="unsupported_format")
    export_file(session, source, output / "empty.step", [], expect_error="invalid_request")
    export_file(session, source, output / "duplicate.step", ["Box", "Box"], expect_error="invalid_request")
    export_file(session, source, output / "stale.step", ["Box"], expected_revision=0, expect_error="stale_revision")
    export_file(session, source, output / "missing-revision.step", ["Box"], expected_revision=None, expect_error="missing_revision")
    assert not (output / "stale.step").exists() and not (output / "missing-revision.step").exists()
    session.call("transaction", expect_error="unsupported_format", document=edited_doc,
                 edits=[{"op": "import_exchange", "path": str(output / "legacy.cadpart")}])
    refreshed(session, edited_doc)
    corrupted = output / "corrupt.step"
    corrupted.write_text("This is not STEP geometry", encoding="utf-8")
    before = refreshed(session, edited_doc)
    session.call("transaction", expect_error="exchange_failed", document=edited_doc,
                 edits=[{"op": "import_exchange", "path": str(corrupted)}])
    after = refreshed(session, edited_doc)
    assert set(objects(before)) == set(objects(after))
    solid(after, "Cut", 90)
    session.call("transaction", expect_error="stale_revision", document=edited_doc, expected_revision=before["revision"],
                 edits=[{"op": "import_exchange", "path": str(leaf_file)}])

    # Native preview candidates and open sketch edits are never exported/imported.
    session.call("preview", document=source, expected_revision=session.revisions[source], token="exchange-preview",
                 label="Pending source edit", edits=[set_value("Box", "Length", 11)])
    export_file(session, source, output / "preview.step", ["Box"], expect_error="preview_active")
    export_file(session, edited_doc, output / "other-preview.step", ["Cut"], expect_error="preview_active")
    session.call("transaction", expect_error="preview_active", document=edited_doc,
                 edits=[{"op": "import_exchange", "path": str(leaf_file)}])
    session.call("preview_cancel", document=source, expected_revision=session.revisions[source], token="exchange-preview")
    transaction(session, source, "Native sketch for ownership check", [add("Sketcher::SketchObject", "Sketch")])
    session.call("sketch_begin", document=source, expected_revision=session.revisions[source], object="Sketch", token="exchange-sketch")
    export_file(session, source, output / "sketch.step", ["Box"], expect_error="sketch_active")
    session.call("transaction", expect_error="sketch_active", document=edited_doc,
                 edits=[{"op": "import_exchange", "path": str(leaf_file)}])
    session.call("sketch_cancel", document=source, expected_revision=session.revisions[source], token="exchange-sketch")
    assert not any((output / name).exists() for name in ("preview.step", "other-preview.step", "sketch.step"))
    transaction(session, source, "Change source for deliberate replacement", [set_value("Box", "Length", 12)])
    export_file(session, source, path, ["Assembly", "Cylinder"], overwrite=True)
    assert digest(path) != old_hash and digest(native_original) == original_hash
    replaced_doc = session.call("new", name="ReplacedSTEP")["document"]
    replaced = import_file(session, replaced_doc, path)
    near(sum(obj["shape"]["volume"] for obj in imported_shapes(replaced)), 180 + 16 * math.pi)
    assert not list(output.glob(".cad-exchange-*")), "Native export staging files leaked"
    return {"formats": format_reports, "native_cut_volumes": [120, 105, 90],
            "native_source_sha256": original_hash, "output": str(output)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", required=True, type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix="exchange-", dir=args.output.resolve()))
    session = ExchangeSession(args.host.resolve(), json.loads(args.runtime.read_text(encoding="utf-8-sig")))
    try:
        report = workflow(session, output)
        report.update(startup_ms=session.startup_ms, operations=session.timings)
        (output / "exchange-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({key: value for key, value in report.items() if key != "operations"}, indent=2))
    finally:
        session.close()


if __name__ == "__main__":
    main()
