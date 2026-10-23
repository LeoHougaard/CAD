"""Native helper CLIs must retain the configured launcher and ordered arguments."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
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
                     "extrusion_expression_workflows", "extrusion_whole_object_workflows"]:
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

    def test_whole_object_preparation_and_worker_preserve_runtime(self):
        runtime = {
            "executable": "/usr/bin/env",
            "args": ["PYTHONPATH=/portable runtime/squashfs-root/usr/lib",
                     "/portable runtime/squashfs-root/AppRun", "python", "-u"],
            "script": "/portable runtime/native worker.py",
        }
        for boundary in ["native-prepare", "worker-runtime"]:
            with self.subTest(boundary=boundary), tempfile.TemporaryDirectory(prefix="cad runtime args ") as temporary:
                directory = Path(temporary)
                config = directory / "runtime with spaces.json"
                config.write_text(json.dumps(runtime), encoding="utf-8")
                fixture = directory / "synthetic input.FCStd"
                fixture.write_bytes(b"Not opened: dispatch guard stops before modeling")
                fixtures = directory / "fixture manifest.json"
                fixtures.write_text(json.dumps({"Pad": {"path": str(fixture)}}), encoding="utf-8")
                output = directory / "output with spaces"
                path = TESTS_ROOT / "extrusion_whole_object_workflows.py"
                spec = importlib.util.spec_from_file_location("whole_object_runtime_guard", path)
                module = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(module)
                commands = []
                workers = []

                def capture_native(arguments, **options):
                    commands.append(list(arguments))
                    if boundary == "native-prepare":
                        raise NativeInvocationCaptured()
                    return subprocess.CompletedProcess(arguments, 0)

                def capture_worker(host, worker_runtime):
                    workers.append((host, worker_runtime, os.environ.get("PYTHONDONTWRITEBYTECODE")))
                    raise NativeInvocationCaptured()

                arguments = [str(path), "--host", "host with spaces", "--compiler", "unused compiler",
                             "--runtime", str(config), "--fixtures", str(fixtures), "--output", str(output)]
                import preview_workflows
                with patch.dict(os.environ), patch.object(sys, "argv", arguments), \
                        patch.object(module.subprocess, "run", capture_native), \
                        patch.object(preview_workflows, "PreviewSession", capture_worker):
                    with self.assertRaises(NativeInvocationCaptured):
                        module.main()
                self.assertEqual(len(commands), 1)
                if boundary == "native-prepare":
                    self.assertEqual(commands[0], [runtime["executable"], *runtime["args"], str(path.resolve()),
                                                  "--native-prepare", str(fixtures.resolve()), str(output.resolve())])
                    self.assertEqual(workers, [])
                else:
                    self.assertEqual(workers, [(Path("host with spaces").resolve(), runtime, "1")])
                self.assertEqual(json.loads(config.read_text(encoding="utf-8")), runtime)


if __name__ == "__main__":
    unittest.main()
