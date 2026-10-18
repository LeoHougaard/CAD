"""Independently check freshly exported assembly solids, placements and volume."""
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / ".tools" / "verification"))
import gmsh


def verify(folder=None, names=("bearing-blocks", "analytic-rings", "context-bound")):
    if folder is None:
        folder = ROOT / "target" / "test-output" / "assembly-performance"
    results = []
    gmsh.initialize()
    try:
        gmsh.option.setNumber("General.Terminal", 0)
        for name in names:
            spec = json.loads((folder / f"{name}.json").read_text(encoding="utf-8"))
            assert spec["case"] == name and spec["step_exported"] is True
            gmsh.clear()
            gmsh.model.occ.importShapes(str(folder / f"{name}.step"))
            gmsh.model.occ.synchronize()
            solids = gmsh.model.getEntities(3)
            expected = list(spec["expected_solids"])
            assert len(solids) == len(expected), (name, len(solids), len(expected))
            actual = []
            for dim, tag in solids:
                mass = gmsh.model.occ.getMass(dim, tag)
                box = gmsh.model.occ.getBoundingBox(dim, tag)
                bounds = [list(box[:3]), list(box[3:])]
                matches = [i for i, e in enumerate(expected) if math.isclose(mass, e["volume_mm3"], rel_tol=1e-7, abs_tol=1e-5) and all(abs(bounds[s][a] - e["bounds_mm"][s][a]) < 1e-5 for s in range(2) for a in range(3))]
                assert len(matches) == 1, (name, mass, bounds, expected)
                expected.pop(matches[0])
                actual.append({"volume_mm3": mass, "bounds_mm": bounds})
            assert not expected, (name, expected)
            results.append({"case": name, "solids": actual})
    finally:
        gmsh.finalize()
    report = {"result": "passed", "verifier": "Gmsh/Open CASCADE, independent of the app kernel", "step_files": len(results), "solids": sum(len(r["solids"]) for r in results), "cases": results}
    (folder / "independent-step-check.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Independent STEP checks passed for {report['solids']} positioned solids in {len(results)} files.")


if __name__ == "__main__":
    verify()
