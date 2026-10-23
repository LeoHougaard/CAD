"""Native helper CLIs must retain the configured launcher and ordered arguments."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

TESTS_ROOT = Path(__file__).resolve().parent


class NativeInvocationCaptured(Exception):
    pass


class RuntimeArgumentTests(unittest.TestCase):
    def test_extrusion_helpers_preserve_launcher_arguments(self):
        runtime = {
            "executable": "/usr/bin/env",
            "args": ["PYTHONPATH=/portable runtime/lib", "/portable runtime/AppRun", "-u"],
            "script": "worker.py",
        }
        for name in ["extrusion_termination_workflows", "extrusion_task_retention_workflows",
                     "extrusion_expression_workflows"]:
            with self.subTest(helper=name), tempfile.TemporaryDirectory(prefix="cad-runtime-args-") as temporary:
                directory = Path(temporary)
                config = directory / "runtime.json"
                config.write_text(json.dumps(runtime), encoding="utf-8")
                path = TESTS_ROOT / (name + ".py")
                spec = importlib.util.spec_from_file_location(name, path)
                module = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(module)
                command = []

                def capture(arguments, **options):
                    command.extend(arguments)
                    self.assertTrue(options["check"])
                    raise NativeInvocationCaptured()

                arguments = [str(path), "--host", "unused-host", "--compiler", "unused-compiler",
                             "--runtime", str(config), "--output", str(directory / "outputs")]
                with patch.object(sys, "argv", arguments), patch.object(module.subprocess, "run", capture):
                    with self.assertRaises(NativeInvocationCaptured):
                        module.main()
                prefix = [runtime["executable"], *runtime["args"]]
                self.assertEqual(command[:len(prefix)], prefix)
                self.assertEqual(Path(command[len(prefix)]).name, "extrusion_termination_workflows.py")
                self.assertEqual(command[len(prefix) + 1], "--native-fixtures")


if __name__ == "__main__":
    unittest.main()
