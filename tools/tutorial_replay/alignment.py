"""Least-squares alignment of matched 2D image points, in pixels.

These fits describe image alignment only. They do not measure CAD geometry or
establish which navigation inputs were observed in a recording.
"""

import math
import numbers


def _number(value):
    if isinstance(value, bool) or not isinstance(value, numbers.Real):
        raise ValueError("coordinates and tolerances must be finite numbers")
    try:
        value = float(value)
    except (OverflowError, ValueError):
        raise ValueError("coordinates and tolerances must be finite numbers") from None
    if not math.isfinite(value):
        raise ValueError("coordinates and tolerances must be finite numbers")
    return value


def _point(value):
    if not isinstance(value, (list, tuple)) or len(value) != 2:
        raise ValueError("each point must contain exactly two coordinates")
    return (_number(value[0]), _number(value[1]))


def _matches(source, target):
    if not isinstance(source, (list, tuple)) or not isinstance(target, (list, tuple)):
        raise ValueError("point collections must be lists or tuples")
    if len(source) != len(target) or len(source) < 2:
        raise ValueError("at least two matched points are required")
    source, target = list(map(_point, source)), list(map(_point, target))
    if len(set(source)) < 2 or len(set(target)) < 2:
        raise ValueError("both point sets must be nondegenerate")
    return source, target


def _center(points):
    return tuple(math.fsum(p[i] / len(points) for p in points) for i in (0, 1))


def transform_point(transform, point):
    """Apply a fit's transform to a point and return an (x, y) tuple."""
    x, y = _point(point)
    a, b = transform["a"], transform["b"]
    tx, ty = transform["translation"]
    return (a * x - b * y + tx, b * x + a * y + ty)


def inverse_transform_point(transform, point):
    """Apply the inverse of a fit's transform to a point."""
    x, y = _point(point)
    a, b = transform["a"], transform["b"]
    tx, ty = transform["translation"]
    denominator = a * a + b * b
    if not math.isfinite(denominator) or denominator == 0:
        raise ValueError("transform is not invertible")
    x, y = x - tx, y - ty
    return ((a * x + b * y) / denominator, (-b * x + a * y) / denominator)


def _result(source, target, a, b, tx, ty):
    scale = math.hypot(a, b)
    if not all(map(math.isfinite, (a, b, tx, ty, scale))) or scale == 0:
        raise ValueError("fit is numerically degenerate")
    transform = {"a": a, "b": b, "scale": scale,
                 "rotation_radians": math.atan2(b, a), "translation": [tx, ty]}
    residuals = []
    for p, q in zip(source, target):
        x, y = transform_point(transform, p)
        dx, dy = x - q[0], y - q[1]
        distance = math.hypot(dx, dy)
        if not math.isfinite(distance):
            raise ValueError("fit is numerically degenerate")
        residuals.append({"dx": dx, "dy": dy, "distance": distance})
    # hypot avoids squaring large distances before the average.
    rmse = math.hypot(*(r["distance"] / math.sqrt(len(residuals)) for r in residuals))
    return {"transform": transform, "residuals": residuals,
            "rmse_pixels": rmse, "max_residual_pixels": max(r["distance"] for r in residuals)}


def fit_translation(source, target):
    """Fit a translation to all matched points and report pixel residuals."""
    source, target = _matches(source, target)
    try:
        s, t = _center(source), _center(target)
        return _result(source, target, 1.0, 0.0, t[0] - s[0], t[1] - s[1])
    except (OverflowError, ZeroDivisionError):
        raise ValueError("fit is numerically degenerate") from None


def fit_similarity(source, target):
    """Fit an orientation-preserving uniform scale, rotation and translation."""
    source, target = _matches(source, target)
    try:
        s, t = _center(source), _center(target)
        centered_s = [(x - s[0], y - s[1]) for x, y in source]
        centered_t = [(x - t[0], y - t[1]) for x, y in target]
        denominator = math.fsum(x * x + y * y for x, y in centered_s)
        a = math.fsum(x * u + y * v for (x, y), (u, v) in zip(centered_s, centered_t)) / denominator
        b = math.fsum(x * v - y * u for (x, y), (u, v) in zip(centered_s, centered_t)) / denominator
        return _result(source, target, a, b, t[0] - a * s[0] + b * s[1],
                       t[1] - b * s[0] - a * s[1])
    except (OverflowError, ZeroDivisionError):
        raise ValueError("fit is numerically degenerate") from None


def propose_camera_step(source, target, pointer_anchor, rotation_tolerance_radians=1e-9):
    """Propose inferred wheel and post-zoom middle-drag motion in screen pixels.

    The wheel value is smooth scroll delta, using exp(delta * .002). Scale
    clamping, input smoothing and 3D orbit are outside this calculation. A fit
    with rotation outside the supplied tolerance returns no proposed input. Within
    the tolerance, the reported fit retains its rotation but the proposed step
    ignores it; fit residuals are not residuals measured after that step.
    """
    tolerance = _number(rotation_tolerance_radians)
    if tolerance < 0:
        raise ValueError("rotation tolerance must be nonnegative")
    anchor = _point(pointer_anchor)
    fit = fit_similarity(source, target)
    transform = fit["transform"]
    if abs(transform["rotation_radians"]) > tolerance:
        return {"status": "unsupported_rotation", "fit": fit}
    scale = transform["scale"]
    tx, ty = transform["translation"]
    pan = [tx - (1 - scale) * anchor[0], ty - (1 - scale) * anchor[1]]
    if not all(map(math.isfinite, pan)):
        raise ValueError("camera step is numerically degenerate")
    return {"status": "proposed", "input_basis": "inferred", "wheel_delta": math.log(scale) / .002,
            "pointer_anchor": list(anchor), "post_zoom_pan": pan,
            "ignored_rotation_radians": transform["rotation_radians"], "fit": fit}

