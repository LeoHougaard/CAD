"""Check freshly generated sweep/loft STEP solids with independent Open CASCADE.

Run the Rust release workspace tests first. Expected volumes come from their
analytic fixture reports, never from the app's measured volume. This check keeps
the strict 1e-7 relative/1e-5 mm3 absolute limit even for numerical torus mass
estimates in the Rust kernel.
"""
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / ".tools" / "verification"))
import gmsh

CASES = (
    "straight-annular",
    "straight-annular-edited",
    "circular-annular",
    "circular-annular-edited",
    "mitred-elbow",
    "mitred-elbow-edited",
    "loft-two-circles",
    "loft-two-circles-edited",
    "loft-three-circles",
    "loft-three-circles-edited",
    "loft-two-rectangles",
    "loft-two-rectangles-edited",
    "loft-three-rectangles",
    "loft-three-rectangles-edited",
    "loft-bilinear-rotated",
    "loft-bilinear-rotated-edited",
    "loft-antiparallel-add",
)


def verify():
    directory = ROOT / "output" / "sweep-loft"
    results = []
    gmsh.initialize()
    try:
        gmsh.option.setNumber("General.Terminal", 0)
        for name in CASES:
            fixture = json.loads((directory / f"{name}.json").read_text(encoding="utf-8"))
            if fixture.get("case") != name or fixture.get("step_roundtrip_verified") is not True:
                raise AssertionError(f"Missing or inconsistent STEP fixture report: {name}")
            expected = fixture["expected_volume_mm3"]
            if not math.isfinite(expected) or expected <= 0:
                raise AssertionError(f"Invalid analytic expected volume: {name}")
            gmsh.clear()
            gmsh.model.occ.importShapes(str(directory / f"{name}.step"))
            gmsh.model.occ.synchronize()
            solids = gmsh.model.getEntities(3)
            measured = sum(gmsh.model.occ.getMass(dim, tag) for dim, tag in solids)
            if len(solids) != 1:
                raise AssertionError((name, "solid count", len(solids), 1))
            if not math.isclose(measured, expected, rel_tol=1e-7, abs_tol=1e-5):
                raise AssertionError((name, "volume_mm3", measured, expected))
            results.append({
                "fixture": name,
                "solids": len(solids),
                "expected_volume_mm3": expected,
                "volume_mm3": measured,
                "absolute_error_mm3": abs(measured - expected),
            })
    finally:
        gmsh.finalize()
    report = {
        "verifier": "Gmsh/Open CASCADE, independent of the app kernel",
        "relative_tolerance": 1e-7,
        "absolute_tolerance_mm3": 1e-5,
        "result": "passed",
        "cases": results,
    }
    (directory / "ci-independent-step-check.json").write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8"
    )
    print(f"Independent sweep/loft STEP checks passed for {len(results)} fixtures.")


if __name__ == "__main__":
    verify()
