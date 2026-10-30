"""Launch the source-bound headless Editor adapter. Never inject desktop input."""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

from .core import TraceError, compile_plan, digest, read_json, write_json


SELECTOR = "editor::sketch::lifecycle_tests::video_replay::tutorial_replay_from_json"


def validate_plan(plan):
    if plan.get("version") != 1 or not isinstance(plan.get("actions"), list):
        raise TraceError("unsupported replay plan")
    events, overrides = [], {}
    for original in plan["actions"]:
        event = {k: v for k, v in original.items() if k != "target"}
        event["status"] = "approved"
        if "target" in original:
            overrides[event["id"]] = original["target"]
        events.append(event)
    compile_plan({"version": 1, "source": plan["source"], "events": events},
                 {"version": 1, "target": plan.get("target", {}), "overrides": overrides})


def replay(plan_file, executable, expected_sha256, runtime_file, output, timeout=100):
    plan_file, executable, runtime_file = (Path(p).resolve() for p in (plan_file, executable, runtime_file))
    plan = read_json(plan_file)
    validate_plan(plan)
    if digest(executable) != expected_sha256:
        raise TraceError("Editor test executable hash mismatch")
    if not 0 < timeout <= 3600:
        raise TraceError("timeout must be between 0 and 3600 seconds")
    folder = Path(output).resolve()
    folder.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1", PYTHONNOUSERSITE="1", PYTHONUTF8="1",
               PYTHONIOENCODING="utf-8", CAD_TUTORIAL_PLAN=str(plan_file),
               CAD_LIFECYCLE_RUNTIME=str(runtime_file), CAD_LIFECYCLE_OUTPUT=str(folder / "output"),
               COMPOSED_EDITOR_RUN=str(folder), WORKSPACE_UNITS_OUTPUT=str(folder))
    for key, sub in [("HOME", "home"), ("USERPROFILE", "home"), ("FREECAD_USER_HOME", "config"),
                     ("APPDATA", "appdata"), ("LOCALAPPDATA", "localappdata"), ("TMP", "tmp"), ("TEMP", "tmp")]:
        env[key] = str(folder / sub)
        Path(env[key]).mkdir(exist_ok=True)
    (folder / "output").mkdir()
    env.pop("PYTHONPATH", None)
    env.pop("PYTHONHOME", None)
    command = [str(executable), SELECTOR, "--exact", "--ignored", "--test-threads=1", "--nocapture"]
    pins = {str(p): digest(p) for p in (plan_file, executable, runtime_file)}
    runtime = read_json(runtime_file)
    for name in ("executable", "script", "test_worker"):
        if runtime.get(name) and Path(runtime[name]).is_file():
            pins[str(Path(runtime[name]).resolve())] = digest(runtime[name])
    write_json(folder / "request.json", {"command": command, "pins": pins,
               "profiles": {key: env[key] for key in ("HOME", "USERPROFILE", "FREECAD_USER_HOME", "APPDATA", "LOCALAPPDATA", "TMP", "TEMP")},
               "physical_desktop": False})
    flags = getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0) | getattr(subprocess, "CREATE_NO_WINDOW", 0)
    with (folder / "test.log").open("wb") as log:
        process = subprocess.Popen(command, cwd=folder, env=env, stdout=log, stderr=subprocess.STDOUT, creationflags=flags)
        write_json(folder / "process.json", {"pid": process.pid})
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            # Preserve the live identity. The caller must reconcile this owned host
            # and its worker, rather than claiming that a forced host kill cleaned up.
            write_json(folder / "result.json", {"exit_code": None, "timeout": True, "host_pid": process.pid,
                       "process_cleanup_required": True, "tutorial_pass": False})
            raise TraceError(f"replay timed out; owned host {process.pid} needs cleanup, see {folder}")
    exact = all(digest(p) == h for p, h in pins.items())
    log = (folder / "test.log").read_text(encoding="utf-8", errors="replace")
    count = re.search(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed;", log)
    events_file = folder / "output/tutorial-replay/events.json"
    events = read_json(events_file) if events_file.exists() else []
    played = [row["replay_action"]["id"] for row in events if "replay_action" in row]
    complete = played == [row["id"] for row in plan["actions"]]
    passed = code == 0 and count is not None and count.groups() == ("1", "0") and exact and complete
    result = {"exit_code": code, "passed": passed, "all_action_ids_in_order": complete,
              "played_actions": played, "pins_exact_after": exact, "physical_desktop": False,
              "tutorial_pass": False, "native_graceful_os_exit": "uncertified"}
    write_json(folder / "result.json", result)
    return result
