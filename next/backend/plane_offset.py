"""Read-only FlatFace offset descriptor. No native discovery mutations."""
import math


def describe(obj, revision):
    try:
        if obj.TypeId != 'PartDesign::Plane' or str(obj.MapMode) != 'FlatFace':
            return None
        support = obj.AttachmentSupport
        if len(support) != 1 or list(support[0][1]) not in ([], ['']):
            return None
        source = support[0][0]
        if source.TypeId not in ('App::Plane', 'PartDesign::Plane'):
            return None
        offset = obj.AttachmentOffset
        rotation = tuple(float(v) for v in offset.Rotation.Q)
        if (offset.Base.x != 0 or offset.Base.y != 0
                or rotation not in ((0.0, 0.0, 0.0, 1.0), (0.0, 0.0, 0.0, -1.0))):
            return None
        length = float(offset.Base.z)
        if not math.isfinite(length) or length < 0:
            return None
        frame = obj.getGlobalPlacement().multiply(offset.inverse())
        axis = frame.Rotation.multVec(type(offset.Base)(0, 0, 1))
        origin = frame.Base
        values = [origin.x, origin.y, origin.z, axis.x, axis.y, axis.z]
        if not all(math.isfinite(v) for v in values):
            return None
        return dict(version=1, revision=revision, feature=obj.Name, feature_id=obj.ID,
                    support=source.Name, support_id=source.ID, origin=values[:3],
                    axis=values[3:], length=length, flipped=bool(obj.MapReversed),
                    frame=list(frame.Matrix.A))
    except (AttributeError, TypeError, ValueError, RuntimeError, IndexError):
        return None
