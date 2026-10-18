"""Verify twenty fresh project STEP exports using independent Open CASCADE."""
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / ".tools" / "verification"))
import gmsh

PROJECTS = (
    "adjustment-bracket", "bearing-block", "grooved-pulley",
    "instrument-enclosure", "cable-comb", "clevis-pair", "standoff-set",
    "swept-handle", "circular-reducer", "rectangular-duct",
)


def verify():
    directory = ROOT / "target" / "test-output" / "end-user-projects"
    results = []
    gmsh.initialize()
    try:
        gmsh.option.setNumber("General.Terminal", 0)
        for project in PROJECTS:
            for variant in ("baseline", "edited"):
                folder = directory / project
                fixture = json.loads((folder / f"{variant}.json").read_text(encoding="utf-8"))
                assert fixture["project"] == project and fixture["variant"] == variant
                assert fixture["step_exported"] is True and fixture["closed_brep_and_mesh"] is True
                expected = fixture["expected_volume_mm3"]
                assert math.isfinite(expected) and expected > 0
                gmsh.clear()
                gmsh.model.occ.importShapes(str(folder / f"{variant}.step"))
                gmsh.model.occ.synchronize()
                solids = gmsh.model.getEntities(3)
                actual = sum(gmsh.model.occ.getMass(dim, tag) for dim, tag in solids)
                assert len(solids) == fixture["expected_parts"], (project, variant, len(solids))
                assert math.isclose(actual, expected, rel_tol=1e-7, abs_tol=1e-5), (project, variant, actual, expected)
                boxes = [gmsh.model.occ.getBoundingBox(dim, tag) for dim, tag in solids]
                bounds = [[min(box[k] for box in boxes) for k in range(3)], [max(box[k + 3] for box in boxes) for k in range(3)]]
                for side in range(2):
                    for axis in range(3):
                        assert abs(bounds[side][axis] - fixture["expected_bounds_mm"][side][axis]) < 1e-5, (project, variant, bounds, fixture["expected_bounds_mm"])
                results.append({"project": project, "variant": variant, "parts": len(solids), "expected_volume_mm3": expected, "volume_mm3": actual, "bounds_mm": bounds})
    finally:
        gmsh.finalize()
    report = {"result": "passed", "verifier": "Gmsh/Open CASCADE, independent of the app kernel", "projects": len(PROJECTS), "step_files": len(results), "cases": results}
    (directory / "independent-step-check.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Independent project STEP checks passed for {len(results)} files across {len(PROJECTS)} projects.")


if __name__ == "__main__":
    verify()
