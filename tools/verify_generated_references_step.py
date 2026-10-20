"""Independently verify multi-step features attached to generated Sweep/Loft sides."""
from verify_assembly_step import ROOT, verify

if __name__ == "__main__":
    verify(ROOT / "target" / "test-output" / "generated-references", (
        "loft-side-cut", "loft-side-cut-edited",
        "mitred-side-cut", "mitred-side-cut-edited",
    ))
