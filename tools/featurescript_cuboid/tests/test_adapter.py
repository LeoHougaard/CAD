"""Portable contract regressions; no CAD runtime or private evidence required."""

import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("cuboid_adapter", ROOT / "adapter.py")
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)
SOURCE = (ROOT / "bounded-cuboid.fs").read_text(encoding="utf8")
QUANTITIES = json.loads((ROOT / "quantities.json").read_text(encoding="utf8"))
CANONICAL = json.loads((ROOT / "tests/canonical-package.json").read_text(encoding="utf8"))


class AdapterTests(unittest.TestCase):
    def test_exact_canonical_package_and_native_mapping(self):
        package = adapter.adapt(SOURCE, QUANTITIES)
        self.assertEqual(package, CANONICAL)
        # Independent, concrete expected native edits, with no geometry stand-in.
        self.assertEqual(package["edits"], [
            {"op": "add_object", "type": "Part::Box", "name": {"$param": "name"}},
            {"op": "set", "object": {"$param": "name"},
             "property": "Length", "value": {"$param": "length"}},
            {"op": "set", "object": {"$param": "name"},
             "property": "Width", "value": {"$param": "width"}},
            {"op": "set", "object": {"$param": "name"},
             "property": "Height", "value": {"$param": "height"}},
        ])
        self.assertEqual([package["parameters"][k]["default"]
                          for k in ("length", "width", "height")], [20, 12.7, 30])
        for key in ("length", "width", "height"):
            self.assertEqual(package["parameters"][key]["unit"], "mm")
            self.assertNotIn("maximum", package["parameters"][key])

    def test_documented_ascii_whitespace(self):
        for separator in (" ", "\t", "\r", "\n", " \t\r\n"):
            with self.subTest(separator=repr(separator)):
                source = SOURCE.replace("FeatureScript 2500", "FeatureScript" + separator + "2500")
                self.assertEqual(adapter.adapt(source, QUANTITIES), CANONICAL)
        self.assertEqual(adapter.adapt(SOURCE.replace("\n", "\r\n\t"), QUANTITIES), CANONICAL)

    def test_review_defect_separators_and_other_controls_reject(self):
        # The first four were actually accepted before the reviewed lexical fix.
        forbidden = "\u00a0\u2003\u2028\x1c"
        forbidden += "".join(chr(i) for i in range(33) if chr(i) not in " \t\r\n")
        forbidden += "\x7f\x85\u1680\u2000\u2001\u2002\u2004\u2005\u2006\u2007\u2008\u2009\u200a\u2029\u202f\u205f\u3000\ufeff"
        for separator in dict.fromkeys(forbidden):
            with self.subTest(codepoint=hex(ord(separator))):
                source = SOURCE.replace("FeatureScript 2500", "FeatureScript" + separator + "2500")
                with self.assertRaises(ValueError):
                    adapter.adapt(source, QUANTITIES)

    def test_unsupported_source_rejects(self):
        sources = [
            SOURCE + 'println("x");', SOURCE + "// comment", SOURCE + "/* comment */",
            SOURCE.replace("vector(0, 0, 0)", "vector(1, 0, 0)"),
            SOURCE.replace("2500.0", "2501.0"),
            SOURCE.replace("fCuboid", "fCylinder"),
            SOURCE.replace("export const", "exportconst"),
            SOURCE.replace("fCuboid", "f Cuboid"),
            SOURCE.replace('"Bounded cuboid"', '"Bounded\\u0020cuboid"'),
            SOURCE.replace("definition.length,", "definition.length * 2,"),
            SOURCE + " " * 16385, "",
        ]
        for source in sources:
            with self.subTest(source=source[-60:]):
                with self.assertRaises(ValueError):
                    adapter.adapt(source, QUANTITIES)

    def test_units_and_inclusive_input_bounds(self):
        for unit, value in (("mm", 25.4), ("cm", 2.54), ("m", .0254), ("in", 1)):
            for dimension in ("length", "width", "height"):
                with self.subTest(unit=unit, dimension=dimension):
                    quantities = copy.deepcopy(QUANTITIES)
                    quantities[dimension] = {"value": value, "unit": unit}
                    parameter = adapter.adapt(SOURCE, quantities)["parameters"][dimension]
                    self.assertAlmostEqual(parameter["default"], 25.4)
                    self.assertEqual(parameter["unit"], "mm")
        for value in (1e-6, 1e6):
            quantities = dict(QUANTITIES, length={"value": value, "unit": "mm"})
            self.assertEqual(adapter.adapt(SOURCE, quantities)["parameters"]["length"]["default"], value)

    def test_invalid_quantities_reject(self):
        bad = [0, -1, True, False, "2", None, float("nan"), float("inf"),
               -float("inf"), 1e-7, 1e6 + 1, 10**400]
        for value in bad:
            with self.subTest(value=repr(value)[:40]):
                quantities = dict(QUANTITIES, length={"value": value, "unit": "mm"})
                # The sealed adapter reports huge integers as OverflowError.
                with self.assertRaises((ValueError, OverflowError)):
                    adapter.adapt(SOURCE, quantities)
        for unit in ("deg", "MM", "inch", "", None, 1, []):
            with self.subTest(unit=unit):
                with self.assertRaises(ValueError):
                    adapter.adapt(SOURCE, dict(QUANTITIES, width={"value": 1, "unit": unit}))
        for quantity in (1, None, {}, {"value": 1}, {"unit": "mm"},
                         {"value": 1, "unit": "mm", "expression": "x"}):
            with self.subTest(quantity=quantity):
                with self.assertRaises(ValueError):
                    adapter.adapt(SOURCE, dict(QUANTITIES, height=quantity))
        for quantities in (None, [], {}, dict(QUANTITIES, extra=1),
                           {k: v for k, v in QUANTITIES.items() if k != "width"}):
            with self.subTest(quantities=quantities):
                with self.assertRaises(ValueError):
                    adapter.adapt(SOURCE, quantities)
        with self.assertRaises(ValueError):
            adapter.adapt(SOURCE, dict(QUANTITIES, length={"value": 1e6, "unit": "m"}))

    def test_names_are_data_and_must_be_native_identifiers(self):
        for name in ("AnotherBox", "_Box2"):
            package = adapter.adapt(SOURCE, QUANTITIES, name)
            self.assertEqual(package["parameters"]["name"]["default"], name)
            self.assertEqual(package["edits"], CANONICAL["edits"])
        for name in ("Box;exec()", "Box\n", "\u00e9Box", "1Box", "", None):
            with self.subTest(name=name):
                with self.assertRaises(ValueError):
                    adapter.adapt(SOURCE, QUANTITIES, name)


class CliTests(unittest.TestCase):
    def run_cli(self, source, quantities, output, name="Cuboid"):
        return subprocess.run([sys.executable, "-B", str(ROOT / "adapter.py"),
                               str(source), str(quantities), str(output), "--name", name],
                              cwd=output.parent, capture_output=True, timeout=15)

    def test_export_from_unrelated_cwd_and_complete_replacement(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "cuboid.json"
            for existing in (False, True):
                with self.subTest(existing=existing):
                    if existing:
                        output.write_bytes(b"old output\x00")
                    result = self.run_cli(ROOT / "bounded-cuboid.fs", ROOT / "quantities.json", output)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(json.loads(output.read_bytes()), CANONICAL)
                    self.assertEqual(output.read_text(encoding="utf8"), json.dumps(CANONICAL, indent=2) + "\n")
                    self.assertFalse(output.with_suffix(".tmp").exists())

    def test_rejections_preserve_existing_output_and_create_nothing(self):
        cases = [
            (SOURCE.replace("FeatureScript 2500", "FeatureScript\u00a02500"), json.dumps(QUANTITIES), "Cuboid"),
            (SOURCE + "// unsupported", json.dumps(QUANTITIES), "Cuboid"),
            (SOURCE, json.dumps(dict(QUANTITIES, length={"value": 0, "unit": "mm"})), "Cuboid"),
            (SOURCE, "{broken JSON", "Cuboid"),
            (SOURCE, json.dumps(QUANTITIES), "Box;exec()"),
        ]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, quantities, output = root / "input.fs", root / "input.json", root / "output.json"
            sentinel = b"preserve this output\r\n\x00\xff"
            for text, data, name in cases:
                for existing in (False, True):
                    with self.subTest(source=text[-20:], data=data[:25], name=name, existing=existing):
                        source.write_text(text, encoding="utf8")
                        quantities.write_text(data, encoding="utf8")
                        if existing:
                            output.write_bytes(sentinel)
                        else:
                            output.unlink(missing_ok=True)
                        before = {p.name: p.read_bytes() for p in root.iterdir()}
                        result = self.run_cli(source, quantities, output, name)
                        self.assertNotEqual(result.returncode, 0)
                        self.assertTrue(result.stderr)
                        self.assertEqual({p.name: p.read_bytes() for p in root.iterdir()}, before)


if __name__ == "__main__":
    unittest.main()
