"""Read-only finite-domain capability and strict optional datum policy.

The primitive owns geometry qualification. Callers retain native transactions,
quantity parsing, solving and rollback. No native imports occur here.
"""
from types import SimpleNamespace
import first_scale as Primitive


def validate(edit):
    if "first_scale" in edit and (edit.get("op") != "datum"
                                   or type(edit["first_scale"]) is not bool):
        raise ValueError("first_scale must be a boolean on a datum edit")


def qualified(sketch, index):
    if not Primitive.eligible(sketch, index):
        return False
    try:
        Primitive.qualify_lines(sketch, index)
    except Primitive.Unqualified:
        return False
    return True


def capability(sketch):
    constraints = sketch.Constraints
    existing = [i for i in range(len(constraints)) if qualified(sketch, i)]
    creators = []
    # Describe prospective native line-length constraints without adding one.
    # All geometry/metadata/reference qualification remains in the primitive.
    for i in range(sketch.GeometryCount):
        candidate = SimpleNamespace(Type="Distance", First=i, FirstPos=0,
                                    Second=-2000, SecondPos=0, Third=-2000,
                                    ThirdPos=0, Driving=True, IsActive=True)
        view = SimpleNamespace(Geometry=sketch.Geometry,
                               ExternalGeometry=sketch.ExternalGeometry,
                               ExpressionEngine=sketch.ExpressionEngine,
                               Constraints=[*constraints, candidate])
        if qualified(view, len(constraints)):
            creators.append({"kind": "Distance", "args": [i]})
    return {"constraints": existing, "creators": creators}


def apply(sketch, index, quantity, edit):
    validate(edit)
    if not edit.get("first_scale", False):
        return False
    # Recheck native state after index_from resolution and all preceding edits.
    # Explicit stale/unsupported intent is an error, never a silent fallback.
    if not qualified(sketch, index):
        raise Primitive.Unqualified("First scaling is unsupported by the current native sketch")
    return Primitive.scale_before_datum(sketch, index, quantity)
