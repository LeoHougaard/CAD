"""Read-only descriptor of the solved native Pad, in world coordinates.

No transactions, setters, geometry construction, or fallback dimensions.
Direction uses the native normal/direction/reference properties.
"""
import math


def describe(obj, revision):
    if obj.TypeId != 'PartDesign::Pad':
        return None
    try:
        # Only a finite length mode has a draggable endpoint. Other modes keep
        # their existing property/reference collectors and native semantics.
        if str(obj.Type) != 'Length':
            return None
        profile = obj.Profile[0]
        if profile is None or profile.Shape.isNull():
            return None
        length = float(obj.Length.Value)
        if not math.isfinite(length) or length < 0:
            return None
        # Shapes/Direction live in the containing coordinate system. Remove
        # the object's own placement before applying the complete parent frame.
        frame = obj.getGlobalPlacement().multiply(obj.Placement.inverse())
        profile_frame = profile.getGlobalPlacement().multiply(profile.Placement.inverse())
        origin = profile_frame.multVec(profile.Shape.CenterOfMass)
        normal = profile.getGlobalPlacement().Rotation.multVec(type(obj.Direction)(0, 0, 1))
        if obj.UseCustomVector:
            axis = frame.Rotation.multVec(obj.Direction)
        elif obj.ReferenceAxis and obj.ReferenceAxis[0] is not None:
            ref, subs = obj.ReferenceAxis
            if len(subs) != 1:
                return None
            sub = ref.getSubObject(subs[0])
            if sub is None or sub.ShapeType != 'Edge' or not sub.Curve.isDerivedFrom('Part::GeomLine'):
                return None
            ref_frame = ref.getGlobalPlacement().multiply(ref.Placement.inverse())
            axis = ref_frame.Rotation.multVec(sub.tangentAt(sub.FirstParameter))
        else:
            axis = normal
        norm = axis.Length
        if not math.isfinite(norm) or norm < 1e-9:
            return None
        axis = axis / norm
        if not all(math.isfinite(v) for v in (origin.x, origin.y, origin.z, axis.x, axis.y, axis.z)):
            return None
        side = str(getattr(obj, 'SideType', ''))
        if side not in ('', 'One side', 'Symmetric'):
            return None
        factor = 0.5 if bool(obj.Midplane) or side == 'Symmetric' else 1.0
        if obj.AlongSketchNormal:
            cosine = abs(axis.dot(normal))
            if cosine < 1e-6:
                return None
            factor /= cosine
        return dict(version=1, revision=revision, feature=obj.Name, feature_id=obj.ID,
                    profile=profile.Name, profile_id=profile.ID,
                    origin=[origin.x, origin.y, origin.z],
                    axis=[axis.x, axis.y, axis.z], factor=factor,
                    signed_length=-length if obj.Reversed else length)
    except (AttributeError, TypeError, ValueError, RuntimeError, IndexError):
        return None
