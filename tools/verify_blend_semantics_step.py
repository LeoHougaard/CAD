"""Check both ends and both material sides of exact circular rim fillets."""
from itertools import product
from verify_assembly_step import ROOT, verify

if __name__ == "__main__":
    names = tuple(
        f"rim-{str(reverse).lower()}-{str(symmetric).lower()}-{str(inner).lower()}-{str(end).lower()}"
        for reverse, symmetric, inner, end in product((False, True), repeat=4)
    ) + ("boolean-history-8", "boolean-history-10") + tuple(
        f"prism-{mode}-{str(reverse).lower()}"
        for mode, reverse in product(("Add", "Remove", "Intersect"), (False, True))
    )
    verify(ROOT / "target" / "test-output" / "blend-semantics", names)
    verify(ROOT / "target" / "test-output" / "original-enclosure-order", ("height-25", "height-30"))
