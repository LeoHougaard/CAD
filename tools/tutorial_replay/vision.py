"""Silent video analysis. Detections are proposals until a person reviews them."""

from __future__ import annotations

import math
import shutil
import subprocess
from pathlib import Path

from .core import MODIFIERS, TraceError, digest, read_json, write_json


def ffmpeg_path(explicit=None):
    if explicit:
        return str(Path(explicit).resolve())
    found = shutil.which("ffmpeg")
    if found:
        return found
    try:
        import imageio_ffmpeg
        return imageio_ffmpeg.get_ffmpeg_exe()
    except ImportError as exc:
        raise TraceError("Pass --ffmpeg or install imageio-ffmpeg") from exc


def libraries():
    try:
        import cv2
        import numpy as np
    except ImportError as exc:
        raise TraceError("Extraction needs numpy and opencv-python-headless; see requirements.txt") from exc
    cv2.setNumThreads(1)
    return cv2, np


def _read_exact(stream, size):
    result = bytearray()
    while len(result) < size:
        part = stream.read(size - len(result))
        if not part:
            break
        result.extend(part)
    if result and len(result) != size:
        raise TraceError("truncated decoder frame")
    return bytes(result)


def frames(video, profile, start, end, fps, ffmpeg, log):
    width, height = profile["width"], profile["height"]
    # Do not decode or play audio. Output dimensions must match the calibration.
    command = [ffmpeg, "-hide_banner", "-nostdin", "-loglevel", "error", "-threads", "1",
               "-ss", str(start), "-i", str(Path(video).resolve()), "-t", str(end - start),
               "-map", "0:v:0", "-an", "-sn", "-dn", "-vf", f"fps={fps}",
               "-pix_fmt", "rgb24", "-threads:v", "1", "-f", "rawvideo", "pipe:1"]
    cv2, np = libraries()
    capture = cv2.VideoCapture(str(video))
    actual = (int(capture.get(cv2.CAP_PROP_FRAME_WIDTH)), int(capture.get(cv2.CAP_PROP_FRAME_HEIGHT)))
    capture.release()
    if actual != (width, height):
        raise TraceError(f"video {actual} differs from profile {(width, height)}; recalibrate instead of scaling")
    flags = getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0) | getattr(subprocess, "CREATE_NO_WINDOW", 0)
    with Path(log).open("wb") as errors:
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=errors, creationflags=flags)
        try:
            index = 0
            while True:
                data = _read_exact(process.stdout, width * height * 3)
                if not data:
                    break
                yield start + index / fps, np.frombuffer(data, dtype=np.uint8).reshape(height, width, 3)
                index += 1
            code = process.wait(timeout=15)
            if code:
                raise TraceError(f"ffmpeg exited {code}; see {log}")
        finally:
            process.stdout.close()
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)


class CursorTracker:
    def __init__(self, profile, folder):
        self.cv2, self.np = libraries()
        self.config = profile.get("cursor", {})
        self.templates = []
        for item in self.config.get("templates", []):
            path = folder / item["file"]
            if item.get("sha256") and digest(path) != item["sha256"]:
                raise TraceError(f"cursor template hash differs from calibration: {path}")
            rgba = self.cv2.imread(str(path), self.cv2.IMREAD_UNCHANGED)
            if rgba is None or rgba.shape[2] != 4:
                raise TraceError(f"cursor template must be an RGBA image: {path}")
            gray = self.cv2.cvtColor(rgba[:, :, :3], self.cv2.COLOR_BGR2GRAY)
            self.templates.append((item, gray, rgba[:, :, 3]))

    def locate(self, rgb):
        if not self.templates:
            return None
        cv, np = self.cv2, self.np
        gray = cv.cvtColor(rgb, cv.COLOR_RGB2GRAY)
        x0, y0, x1, y1 = self.config.get("search", [0, 0, gray.shape[1], gray.shape[0]])
        search = gray[y0:y1, x0:x1]
        candidates = []
        for item, template, mask in self.templates:
            result = cv.matchTemplate(search, template, cv.TM_SQDIFF_NORMED, mask=mask)
            result = np.where(np.isfinite(result), result, 1.0)
            _, _, location, _ = cv.minMaxLoc(result)
            x, y = location
            score = max(0., min(1., 1. - float(result[y, x])))
            radius = self.config.get("exclude_radius", 20)
            other = result.copy()
            other[max(0, y-radius):y+radius+1, max(0, x-radius):x+radius+1] = 1.
            margin = score - (1. - max(0., min(1., float(other.min()))))
            hx, hy = item["hotspot"]
            candidates.append({"xy": [x0+x+hx, y0+y+hy], "score": round(score, 6),
                               "margin": round(margin, 6), "template": item["file"]})
        best = max(candidates, key=lambda item: item["score"])
        best["accepted_detection"] = (best["score"] >= self.config.get("min_score", .92)
                                      and best["margin"] >= self.config.get("min_margin", .02))
        # accepted_detection means a detector threshold passed, not human verification.
        return best


def extract(video, profile_file, output, start, end, fps=30, ffmpeg=None, cursor=True):
    if not (math.isfinite(start) and math.isfinite(end) and 0 <= start < end and 0 < fps <= 120):
        raise TraceError("invalid interval or sample rate")
    folder = Path(output)
    folder.mkdir(parents=True, exist_ok=False)
    evidence = folder / "frames"
    evidence.mkdir()
    profile_file = Path(profile_file).resolve()
    profile = read_json(profile_file)
    cv, np = libraries()
    tracker = CursorTracker(profile if cursor else {}, profile_file.parent)
    threshold = profile.get("pressed_threshold", 140)
    release = profile.get("release_threshold", threshold)
    margin = profile.get("key_inset", 3)
    keys = profile["key_rois"]
    for name, roi in keys.items():
        if len(roi) != 4 or not (0 <= roi[0] < roi[2] <= profile["width"] and 0 <= roi[1] < roi[3] <= profile["height"]):
            raise TraceError(f"invalid key region {name}")
    active, events, signals, positions = set(), [], [], []
    count = 0
    try:
        for t, rgb in frames(video, profile, start, end, fps, ffmpeg_path(ffmpeg), folder / "decoder.log"):
            gray = cv.cvtColor(rgb, cv.COLOR_RGB2GRAY)
            current = set()
            for key, (left, top, right, bottom) in keys.items():
                patch = gray[top+margin:bottom-margin, left+margin:right-margin]
                if not patch.size:
                    raise TraceError(f"{key}: inset removes the entire key region")
                level = float(patch.mean())
                if level > (release if key in active else threshold):
                    current.add(key)
            point = tracker.locate(rgb)
            if point:
                positions.append({"t": round(t, 6), **point})
            added, removed = current-active, active-current
            if added or removed or count == 0:
                signals.append({"t": round(t, 6), "bright": sorted(current), "onsets": sorted(added),
                                "fade_below_threshold": sorted(removed), "physical_release_known": False})
            # Treat simultaneous modifier/key signals as a proposal, never a proven chord.
            for key in sorted(added):
                eid = f"input-{len(events)+1:05d}"
                name = f"frames/{eid}.jpg"
                cv.imwrite(str(folder / name), cv.cvtColor(rgb, cv.COLOR_RGB2BGR))
                event = {"id": eid, "t": round(t, 6), "kind": "click" if key in ("M1", "M2", "M3") else "key",
                         "status": "pending", "evidence": name, "detected_overlay": key,
                         "reason": "Review click versus hold/drag" if key.startswith("M") and key[1:].isdigit()
                         else "Overlay onset; physical release/repeat is not encoded",
                         "detected_modifiers": sorted(current & MODIFIERS), "timing": "visual onset"}
                if event["kind"] == "click":
                    event["button"] = {"M1": "left", "M2": "right", "M3": "middle"}[key]
                    if point and point["accepted_detection"]:
                        event["xy"] = point["xy"]
                        event["cursor_score"] = point["score"]
                else:
                    event["key"] = key
                events.append(event)
            active = current
            count += 1
        trace = {"version": 1, "source": {"path": str(Path(video).resolve()), "sha256": digest(video),
                  "width": profile["width"], "height": profile["height"], "sample_fps": fps,
                  "url": profile.get("reference_url"), "interval": [start, end]},
                 "calibration_sha256": digest(profile_file), "events": events, "signals": signals,
                 "cursor": positions, "gaps": [], "extraction": {"frames": count, "audio_decoded": False,
                  "physical_key_up_known": False, "repeated_presses_may_be_merged": True,
                  "cursor_detection_is_reviewed": False}}
        write_json(folder / "trace.json", trace)
        write_json(folder / "result.json", {"exit_code": 0, "frames": count, "proposals": len(events),
                                           "source_sha256": trace["source"]["sha256"]})
        return trace
    except BaseException as exc:
        write_json(folder / "result.json", {"exit_code": 1, "error": str(exc), "frames": count})
        raise
