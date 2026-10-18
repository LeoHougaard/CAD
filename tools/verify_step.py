"""Independent STEP check using Gmsh's Open CASCADE importer, not the app kernel.

Install gmsh into your Python environment, or put its Python module and DLL in
.tools/verification. Run after the Rust modeling tests. Nothing here is shipped.
"""
import json
import math
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(root / ".tools" / "verification"))
import gmsh

fixtures = {
    "plate": (1, (80 * 50 - 4 * math.pi * 9) * 8),
    "chamfer": (1, 23322.666666666667),
    "shell": (1, 40 * 30 * 20 - 36 * 26 * 18),
    "revolve": (1, math.pi * (225 - 100) * 20),
    "mirror": (2, 48000),
    "pattern": (3, 72000),
    "transform": (1, 24000),
}
results = []
gmsh.initialize()
try:
    gmsh.option.setNumber("General.Terminal", 0)
    for name, (count, volume) in fixtures.items():
        gmsh.clear()
        path = root / "target" / "test-output" / f"{name}.step"
        gmsh.model.occ.importShapes(str(path))
        gmsh.model.occ.synchronize()
        solids = gmsh.model.getEntities(3)
        measured = sum(gmsh.model.occ.getMass(dim, tag) for dim, tag in solids)
        assert len(solids) == count, (name, "solid count", len(solids), count)
        assert math.isclose(measured, volume, rel_tol=1e-7, abs_tol=1e-5), (name, measured, volume)
        results.append({"fixture": name, "solids": len(solids), "volume": measured, "expected": volume})
    # Fillet corner construction is kernel-specific. Import independently and
    # require a positive solid smaller than the original box; Rust tests also
    # validate its topology. This is not an analytic fillet accuracy proof.
    gmsh.clear()
    gmsh.model.occ.importShapes(str(root / "target/test-output/fillet.step"))
    gmsh.model.occ.synchronize()
    solids = gmsh.model.getEntities(3)
    measured = sum(gmsh.model.occ.getMass(dim, tag) for dim, tag in solids)
    assert len(solids) == 1 and 22000 < measured < 24000
    results.append({"fixture": "fillet", "solids": 1, "volume": measured, "check": "import and volume bounds"})
finally:
    gmsh.finalize()

output = root / "output" / "independent-step-check.json"
output.parent.mkdir(exist_ok=True)
output.write_text(json.dumps(results, indent=2), encoding="utf-8")
print(f"Independent STEP checks passed for {len(results)} fixtures.")
