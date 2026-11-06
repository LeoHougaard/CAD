"""Opaque native object checkpoints. Never parse or rebuild geometry/IDs."""
import base64
import checkpoint_native

capabilities = checkpoint_native.capabilities
require_runtime = checkpoint_native.require_runtime

def capture(sketch):
    return base64.b64encode(bytes(sketch.dumpContent())).decode("ascii")

def restore(sketch, payload):
    # Whole-object native restore also runs onDocumentRestored. Restoring the
    # Geometry/Constraints/ExpressionEngine properties separately loses formulas.
    checkpoint_native.restore_bytes(sketch, base64.b64decode(payload, validate=True))
