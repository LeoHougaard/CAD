"""Trace validation and explicit source-to-target input mapping. No OS input."""

from __future__ import annotations

import copy
import hashlib
import json
import math
from pathlib import Path


class TraceError(ValueError):
    pass


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def write_json(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for part in iter(lambda: f.read(1024 * 1024), b""):
            h.update(part)
    return h.hexdigest()


def number(value, name):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise TraceError(f"{name} must be a finite number")
    return value


def xy(value, name="point"):
    if not isinstance(value, list) or len(value) != 2:
        raise TraceError(f"{name} must be [x, y]")
    return [number(v, name) for v in value]


def rectangle(value):
    if not isinstance(value, list) or len(value) != 4:
        raise TraceError("rectangle must be [x, y, width, height]")
    values = [number(v, "rectangle") for v in value]
    if values[2] <= 0 or values[3] <= 0:
        raise TraceError("rectangle has no area")
    return values


KINDS = {"move", "click", "drag", "key", "key_down", "key_up", "text", "scroll", "checkpoint", "gap"}
POINTER = {"move", "click", "drag", "scroll"}
MODIFIERS = {"ShiftLeft", "ShiftRight", "CtrlLeft", "CtrlRight", "AltLeft", "AltRight"}


def validate(trace):
    if trace.get("version") != 1:
        raise TraceError("unsupported trace version")
    source = trace.get("source", {})
    for name in ("width", "height"):
        if number(source.get(name), f"source.{name}") <= 0:
            raise TraceError(f"invalid source.{name}")
    if not isinstance(trace.get("events"), list):
        raise TraceError("events must be a list")
    for gap in trace.get("gaps", []):
        if "resolved" in gap and type(gap["resolved"]) is not bool:
            raise TraceError("gap.resolved must be a JSON boolean")
        begin, end = number(gap.get("start"), "gap.start"), number(gap.get("end"), "gap.end")
        if begin < 0 or end < begin or not gap.get("reason"):
            raise TraceError("gap needs an ordered interval and reason")
    seen = set()
    last = -math.inf
    for event in trace.get("events", []):
        eid = event.get("id")
        if not isinstance(eid, str) or not eid or eid in seen:
            raise TraceError("event IDs must be unique nonempty strings")
        seen.add(eid)
        t = number(event.get("t"), f"{eid}.t")
        if t < 0 or t < last:
            raise TraceError(f"{eid}: events must be in source-time order")
        last = t
        if event.get("kind") not in KINDS:
            raise TraceError(f"{eid}: unknown input kind")
        if event.get("status") not in ("pending", "approved", "skipped"):
            raise TraceError(f"{eid}: status must be pending, approved or skipped")
        if event.get("status") == "skipped" and not event.get("reason"):
            raise TraceError(f"{eid}: skipped inputs need a reason")
        path = event.get("path", [])
        if not isinstance(path, list):
            raise TraceError(f"{eid}: path must be a list")
        for point in path:
            p = xy(point.get("xy"), eid)
            if not (0 <= p[0] < source["width"] and 0 <= p[1] < source["height"]):
                raise TraceError(f"{eid}: source path point outside video")
            if number(point.get("dt"), f"{eid}.path.dt") < 0:
                raise TraceError(f"{eid}: negative path time")
        if path and (path[0]["dt"] != 0 or any(b["dt"] <= a["dt"] for a, b in zip(path, path[1:]))):
            raise TraceError(f"{eid}: path times must increase from zero")
        if "xy" in event:
            p = xy(event["xy"], eid)
            if not (0 <= p[0] < source["width"] and 0 <= p[1] < source["height"]):
                raise TraceError(f"{eid}: source point outside video")
        if event["kind"] in {"key", "key_down", "key_up"} and not event.get("key"):
            raise TraceError(f"{eid}: key is missing")
        if event["kind"] == "text" and not isinstance(event.get("text"), str):
            raise TraceError(f"{eid}: text is missing")
        if "modifiers" in event and (not isinstance(event["modifiers"], list) or any(k not in MODIFIERS for k in event["modifiers"])):
            raise TraceError(f"{eid}: modifiers must list canonical modifier keys")
        if "expect" in event:
            expected = event["expect"]
            if not isinstance(expected, dict) or not expected or any(not isinstance(k, str) or not k.startswith("/") for k in expected):
                raise TraceError(f"{eid}: expect must be a nonempty JSON-pointer object")
        if event["kind"] == "checkpoint" and not event.get("expect"):
            raise TraceError(f"{eid}: checkpoint needs an explicit expectation")
    return trace


def _solve3(matrix, values):
    a = [list(map(float, row)) + [float(v)] for row, v in zip(matrix, values)]
    for col in range(3):
        pivot = max(range(col, 3), key=lambda row: abs(a[row][col]))
        if abs(a[pivot][col]) < 1e-10:
            raise TraceError("affine calibration points are collinear")
        a[col], a[pivot] = a[pivot], a[col]
        factor = a[col][col]
        a[col] = [v / factor for v in a[col]]
        for row in range(3):
            if row != col:
                factor = a[row][col]
                a[row] = [v - factor * w for v, w in zip(a[row], a[col])]
    return [a[i][3] for i in range(3)]


def map_point(point, binding):
    """Map each recorded sample; never replace a curved path with its endpoints."""
    x, y = xy(point)
    kind = binding.get("kind")
    if kind == "rect":
        sx, sy, sw, sh = rectangle(binding["source"])
        tx, ty, tw, th = rectangle(binding["target"])
        if not (sx <= x <= sx + sw and sy <= y <= sy + sh):
            raise TraceError("source point outside calibrated region")
        return [tx + (x - sx) / sw * tw, ty + (y - sy) / sh * th]
    if kind == "affine":
        pairs = binding.get("pairs", [])
        if len(pairs) != 3:
            raise TraceError("affine mapping requires exactly three calibration pairs")
        matrix = [[*xy(p["source"]), 1] for p in pairs]
        ax = _solve3(matrix, [xy(p["target"])[0] for p in pairs])
        ay = _solve3(matrix, [xy(p["target"])[1] for p in pairs])
        return [ax[0] * x + ax[1] * y + ax[2], ay[0] * x + ay[1] * y + ay[2]]
    raise TraceError(f"{kind}: cannot map pointer path")


def _target(event, mapping):
    eid = event["id"]
    override = mapping.get("overrides", {}).get(eid)
    if override:
        binding = copy.deepcopy(override)
    else:
        name = event.get("binding")
        if not name or name not in mapping.get("bindings", {}):
            raise TraceError(f"{eid}: missing explicit target binding")
        binding = copy.deepcopy(mapping["bindings"][name])
    kind = binding.get("kind")
    if kind in ("rect", "affine"):
        if event.get("path"):
            path = [{"dt": p["dt"], "xy": map_point(p["xy"], binding)} for p in event["path"]]
            if path[0]["dt"] != 0 or any(b["dt"] <= a["dt"] for a, b in zip(path, path[1:])):
                raise TraceError(f"{eid}: path times must increase from zero")
            return {"kind": "path", "points": path}
        if "xy" not in event:
            raise TraceError(f"{eid}: source point required for mapping")
        return {"kind": "pixel", "xy": map_point(event["xy"], binding)}
    if kind == "pixel":
        xy(binding.get("xy"), eid)
    elif kind == "path":
        points = binding.get("points", [])
        if len(points) < 2:
            raise TraceError(f"{eid}: path needs at least two points")
        for p in points:
            xy(p.get("xy"), eid)
            number(p.get("dt"), eid)
        if points[0]["dt"] != 0 or any(b["dt"] <= a["dt"] for a, b in zip(points, points[1:])):
            raise TraceError(f"{eid}: target path times must increase from zero")
    elif kind == "label":
        if not isinstance(binding.get("text"), str) or not binding["text"]:
            raise TraceError(f"{eid}: target label missing")
    elif kind == "plane":
        if binding.get("role") not in ("front", "top", "right"):
            raise TraceError(f"{eid}: unsupported standard plane")
    elif kind == "sketch":
        xy(binding.get("xy"), eid)
    elif kind == "sketch-element":
        if "object_label" in binding and (not isinstance(binding["object_label"], str) or not binding["object_label"]):
            raise TraceError(f"{eid}: scene sketch label must be nonempty")
        if "construction_axis" in binding and (type(binding["construction_axis"]) is not bool or "object_label" not in binding):
            raise TraceError(f"{eid}: construction axis requires a scene sketch label")
        if type(binding.get("persistent_id")) is not int or binding["persistent_id"] <= 0:
            raise TraceError(f"{eid}: positive native persistent ID required")
        if type(binding.get("point", 0)) is not int or binding.get("point", 0) not in (0, 1, 2):
            raise TraceError(f"{eid}: line point must be 0, 1 or 2")
        fraction = number(binding.get("fraction", .5), eid)
        if not 0 <= fraction <= 1:
            raise TraceError(f"{eid}: line fraction outside segment")
        if binding.get("construction_axis") and binding.get("point", 0) != 0:
            raise TraceError(f"{eid}: construction axis must target a line interior")
    else:
        raise TraceError(f"{eid}: unknown target mapping {kind}")
    return binding


def compile_plan(trace, mapping, start=None, end=None):
    validate(trace)
    if mapping.get("version") != 1:
        raise TraceError("unsupported mapping version")
    start = 0 if start is None else number(start, "start")
    end = math.inf if end is None else number(end, "end")
    if end < start:
        raise TraceError("end precedes start")
    for gap in trace.get("gaps", []):
        if not gap.get("resolved") and gap["end"] >= start and gap["start"] <= end:
            raise TraceError(f"unresolved source interval: {gap['reason']}")
    actions, skipped = [], []
    previous_end = -math.inf
    held = set()
    for event in trace["events"]:
        if not start <= event["t"] <= end:
            continue
        eid, kind = event["id"], event["kind"]
        if event["status"] == "skipped":
            skipped.append({"id": eid, "reason": event["reason"]})
            continue
        if event["status"] != "approved" or kind == "gap":
            raise TraceError(f"{eid}: input has not been reviewed")
        if event["t"] < previous_end:
            raise TraceError(f"{eid}: overlapping path/input intervals require interleaved replay, which is not supported")
        action = {k: copy.deepcopy(v) for k, v in event.items() if k not in ("status", "binding", "xy", "path")}
        if kind in POINTER:
            action["target"] = _target(event, mapping)
            if kind == "drag" and (action["target"]["kind"] != "path" or len(action["target"].get("points", [])) < 2):
                raise TraceError(f"{eid}: drag requires the reviewed full pointer path")
            if kind in ("click", "drag"):
                action.setdefault("button", "left")
                if action["button"] not in ("left", "right", "middle"):
                    raise TraceError(f"{eid}: invalid mouse button")
            if kind == "scroll":
                xy(event.get("delta"), f"{eid}.delta")
            if action["target"]["kind"] == "path" and kind not in ("move", "drag"):
                raise TraceError(f"{eid}: only move/drag inputs accept paths")
            bounds = mapping.get("target", {})
            if action["target"]["kind"] in ("pixel", "path"):
                width, height = number(bounds.get("width"), "target.width"), number(bounds.get("height"), "target.height")
                points = [action["target"]["xy"]] if action["target"]["kind"] == "pixel" else [p["xy"] for p in action["target"]["points"]]
                if any(not (0 <= p[0] < width and 0 <= p[1] < height) for p in points):
                    raise TraceError(f"{eid}: mapped point outside target dimensions")
        durations = [p["dt"] for p in event.get("path", [])]
        durations += [p["dt"] for p in action.get("target", {}).get("points", [])]
        event_end = event["t"] + max(durations, default=0)
        previous_end = event_end
        if event_end > end:
            raise TraceError(f"{eid}: path extends outside selected replay interval")
        for gap in trace.get("gaps", []):
            if not gap.get("resolved") and gap["end"] >= event["t"] and gap["start"] <= event_end:
                raise TraceError(f"{eid}: path crosses unresolved source interval: {gap['reason']}")
        if kind in ("key_down", "key_up"):
            key = event["key"]
            if kind == "key_down":
                if key in held:
                    raise TraceError(f"{eid}: key already held")
                held.add(key)
            else:
                if key not in held:
                    raise TraceError(f"{eid}: key-up without key-down")
                held.remove(key)
        if "wait_for" in action and (not isinstance(action["wait_for"], list) or not all(isinstance(op, str) for op in action["wait_for"])):
            raise TraceError(f"{eid}: wait_for must list native operation names")
        actions.append(action)
    if held:
        raise TraceError(f"unreleased keys at end of replay: {sorted(held)}")
    return {"version": 1, "source": copy.deepcopy(trace["source"]), "target": mapping.get("target", {}),
            "range": [start, None if end == math.inf else end], "actions": actions, "skipped": skipped,
            "claims": {"physical_timing_exact": False, "geometry_verified": False, "tutorial_pass": False}}


def record_plan(plan):
    """Resolve static mappings into a deterministic input ledger for adapter tests."""
    rows = []
    for action in plan["actions"]:
        row = copy.deepcopy(action)
        target = row.get("target", {})
        if target.get("kind") not in (None, "pixel", "path"):
            row["requires_live_resolution"] = True
        rows.append(row)
    return {"version": 1, "mode": "dry-run", "executed_ui": False, "actions": rows}
