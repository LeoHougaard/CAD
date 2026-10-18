"""Check fresh selected-region STEP fixtures with independent Open CASCADE."""
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / ".tools" / "verification"))
import gmsh

CASES = ("lens", "circle-union", "divided-rectangle", "annulus")


def verify():
    directory = ROOT / "target" / "test-output" / "regions"
    results = []
    gmsh.initialize()
    try:
        gmsh.option.setNumber("General.Terminal", 0)
        for name in CASES:
            fixture = json.loads((directory / f"{name}.json").read_text(encoding="utf-8"))
            assert fixture["case"] == name and fixture["step_roundtrip_verified"] is True
            expected = fixture["expected_volume_mm3"]
            assert math.isfinite(expected) and expected > 0
            gmsh.clear()
            gmsh.model.occ.importShapes(str(directory / f"{name}.step"))
            gmsh.model.occ.synchronize()
            solids = gmsh.model.getEntities(3)
            actual = sum(gmsh.model.occ.getMass(dim, tag) for dim, tag in solids)
            assert len(solids) == 1, (name, len(solids))
            assert math.isclose(actual, expected, rel_tol=1e-7, abs_tol=1e-5), (name, actual, expected)
            results.append({"case": name, "expected_volume_mm3": expected, "volume_mm3": actual})
    finally:
        gmsh.finalize()
    report = {"result": "passed", "verifier": "Gmsh/Open CASCADE", "cases": results}
    (directory / "independent-step-check.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Independent selected-region STEP checks passed for {len(results)} fixtures.")


if __name__ == "__main__":
    verify()
