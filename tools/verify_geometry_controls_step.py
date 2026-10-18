"""Verify fresh exact blend and planar-face extrusion exports independently."""
from verify_assembly_step import ROOT, verify

if __name__ == "__main__":
    verify(ROOT / "target" / "test-output" / "geometry-controls", (
        "chamfer-plate-hole", "fillet-plate-hole", "face-extrude", "revolved-cylinder",
        "trimmed-semicircle",
    ))
