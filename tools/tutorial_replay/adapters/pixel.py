"""Translate ONE reviewed, freshly mapped input to the existing Pixel agent API.

This module performs no device IO. The caller must retain the sole lease, check
Android readiness/owned identity, inspect a fresh screenshot and guard its hash.
"""

from ..core import TraceError, xy


KEYS = {"NumEnter": "KP_Enter", "Enter": "Return", "Escape": "Escape", "Space": "space",
        "Backspace": "BackSpace", "Delete": "Delete", "Tab": "Tab", "Num.": "KP_Decimal",
        "Num+": "KP_Add", "Num-": "KP_Subtract", "Num*": "KP_Multiply", "Num/": "KP_Divide"}
MODS = {"CtrlLeft": "Control_L", "CtrlRight": "Control_R", "AltLeft": "Alt_L",
        "AltRight": "Alt_R", "ShiftLeft": "Shift_L", "ShiftRight": "Shift_R"}
FUNCTION_KEYS = frozenset(f"F{number}" for number in range(1, 25))


def to_action(event, window, coordinate_space="desktop"):
    """Return an agent action. Never batch inputs across new screenshots."""
    kind = event["kind"]
    if kind == "key":
        source = event["key"]
        if source in MODS:
            raise TraceError("Pixel API cannot preserve a standalone modifier hold")
        if source in KEYS:
            key = KEYS[source]
        elif source.startswith("Num") and len(source) == 4 and source[-1].isdigit():
            key = "KP_" + source[-1]
        elif len(source) == 1 and source.isascii() and source.isalnum():
            # Uppercase overlay labels name physical letter keys. Uppercase X11
            # keysyms would accidentally add Shift to S/N/etc.
            key = source.lower()
        elif source in FUNCTION_KEYS or source in ("Left", "Right", "Up", "Down", "Home", "End", "Page_Up", "Page_Down"):
            key = source
        else:
            raise TraceError(f"unsupported Pixel key: {source}")
        mods = event.get("modifiers", [])
        if any(name not in MODS for name in mods):
            raise TraceError("unsupported Pixel modifier")
        return {"action": "key", "window": window["id"], "key": "+".join([*(MODS[name] for name in mods), key])}
    if kind not in ("click", "move"):
        raise TraceError(f"Pixel step adapter does not implement {kind}; no input substitution")
    if event.get("modifiers"):
        raise TraceError("Pixel pointer API cannot preserve held modifiers")
    target = event.get("target", {})
    if target.get("kind") != "pixel":
        raise TraceError("Pixel input needs a point mapped from the freshly inspected frame")
    x, y = xy(target.get("xy"))
    if coordinate_space == "window-client":
        x, y = x + window["x"], y + window["y"]
    elif coordinate_space != "desktop":
        raise TraceError("unknown Pixel coordinate space")
    if not (window["x"] <= x < window["x"] + window["width"] and window["y"] <= y < window["y"] + window["height"]):
        raise TraceError("Pixel point outside the owned client")
    result = {"action": kind, "window": window["id"], "x": round(x), "y": round(y)}
    if not (window["x"] <= result["x"] < window["x"] + window["width"] and window["y"] <= result["y"] < window["y"] + window["height"]):
        raise TraceError("rounded Pixel point outside the owned client")
    if kind == "click":
        try:
            result["button"] = {"left": 1, "middle": 2, "right": 3}[event["button"]]
        except KeyError as exc:
            raise TraceError("unsupported Pixel mouse button") from exc
    return result


def preflight(events, window, coordinate_space="desktop"):
    """Reject an unsupported fragment before the caller sends its first input."""
    return [to_action(event, window, coordinate_space) for event in events]
