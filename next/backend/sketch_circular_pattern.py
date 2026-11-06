"""Constraint-coupled native circular pattern: points, lines and circles only.

Caller owns the existing sketch preview/transaction/solve lifecycle. Geometry
coordinates initialize the solver; Coincident/Equal/Angle remain the association.
"""
import math
import FreeCAD as App
import Part
import Sketcher
import sketch_geometry


def plan(sketch, edit):
    from sketch_operations import OperationError, integer, quantity, vector
    allowed = {'op', 'object', 'indices', 'source_ids', 'center', 'angle', 'count'}
    if set(edit) != allowed or edit['object'] != sketch.Name:
        raise OperationError('Invalid circular pattern request')
    count = integer(edit['count'], 2, 99, 'Count')
    center = vector(edit['center'])
    angle = quantity(edit['angle'], 'deg')
    if center.z != 0 or not 0 < angle <= 360:
        raise OperationError('Use a sketch-plane center and total angle greater than 0 up to 360 deg')
    indices, identities = edit['indices'], edit['source_ids']
    if not isinstance(indices, list) or not indices or len(set(indices)) != len(indices):
        raise OperationError('Select distinct internal points, lines or circles')
    if not isinstance(identities, list) or len(identities) != len(indices):
        raise OperationError('Native source identity is missing')
    sources = []
    for index, identity in zip(indices, identities):
        integer(index, 0, sketch.GeometryCount - 1, 'Source index')
        if sketch.getGeometryId(index) != identity:
            raise OperationError('Native source identity changed; select again', 'stale_selection')
        geo = sketch.Geometry[index]
        if isinstance(geo, Part.Point):
            handles = [(1, App.Vector(geo.X, geo.Y, geo.Z))]
        elif isinstance(geo, Part.LineSegment):
            handles = [(1, geo.StartPoint), (2, geo.EndPoint)]
        elif isinstance(geo, Part.Circle):
            handles = [(3, geo.Center)]
        else:
            raise OperationError('Circular pattern supports only native points, line segments and circles')
        for _, p in handles:
            if p.z != 0 or not all(math.isfinite(v) for v in (p.x, p.y, p.z)):
                raise OperationError('Source must be finite sketch-plane geometry')
            if not math.isfinite((p - center).Length):
                raise OperationError('Native pattern radius exceeds the finite range')
        # Zero-length spokes cannot express rotation. Fail before changing anything.
        if any((p - center).Length == 0 for _, p in handles):
            raise OperationError('A selected handle lies at the pattern center; this pattern scope requires nonzero radii')
        sources.append((index, geo, handles, sketch.getConstruction(index)))
    step = math.radians(angle / (count if angle == 360 else count - 1))
    # Each handle owns one source spoke and one spoke for each generated instance.
    # Geometry and constraint IDs in the native API are signed ints.
    handles_count = sum(len(h) for _, _, h, _ in sources)
    if sketch.GeometryCount + 1 + handles_count * count + len(sources) * (count - 1) > 2147483647:
        raise OperationError('Pattern exceeds native geometry capacity')
    if sketch.ConstraintCount + 2 + handles_count * (2 + 4 * (count - 1)) + len(sources) * (count - 1) > 2147483647:
        raise OperationError('Pattern exceeds native constraint capacity')
    # Check all initialization arithmetic before the first native mutation.
    for k in range(1, count):
        for _, _, handles, _ in sources:
            for _, p in handles:
                rotated(center, p, step * k)
    return center, count, step, sources


def rotated(center, p, angle):
    from sketch_operations import OperationError
    c, s = math.cos(angle), math.sin(angle)
    d = p - center
    x, y = center.x + c * d.x - s * d.y, center.y + s * d.x + c * d.y
    if not math.isfinite(x) or not math.isfinite(y) or (App.Vector(x, y, 0) - center).Length == 0:
        raise OperationError('Rotated native geometry exceeds usable finite range')
    return App.Vector(x, y, 0)


def apply(sketch, edit):
    from sketch_operations import OperationError
    center, count, step, sources = plan(sketch, edit)

    def geometry(geo, construction):
        index = sketch.addGeometry(geo, construction)
        if type(index) is not int or index < 0:
            raise OperationError('Native pattern geometry creation failed', 'invalid_geometry')
        return index

    def constraint(kind, *args):
        sketch_geometry.validate_constraint(kind, list(args))
        index = sketch.addConstraint(Sketcher.Constraint(kind, *args))
        if type(index) is not int or index < 0:
            raise OperationError('Native pattern constraint creation failed', 'invalid_geometry')
        return index

    center_id = geometry(Part.Point(center), True)
    # Signed native dimensions for nonzero coordinates; point-on-axis for zero.
    for value, kind, axis in [(center.x, 'DistanceX', -2), (center.y, 'DistanceY', -1)]:
        if value == 0:
            constraint('PointOnObject', center_id, 1, axis)
        else:
            constraint(kind, center_id, 1, value)
    spokes = {}
    for source, _, handles, _ in sources:
        for point, p in handles:
            spoke = geometry(Part.LineSegment(center, p), True)
            constraint('Coincident', spoke, 1, center_id, 1)
            constraint('Coincident', spoke, 2, source, point)
            spokes[source, point] = spoke
    master = None
    for k in range(1, count):
        for source, geo, handles, construction in sources:
            positions = [rotated(center, p, step * k) for _, p in handles]
            if isinstance(geo, Part.Point):
                copied = Part.Point(positions[0])
            elif isinstance(geo, Part.LineSegment):
                copied = Part.LineSegment(*positions)
            else:
                copied = Part.Circle(positions[0], geo.Axis, geo.Radius)
            target = geometry(copied, construction)
            if isinstance(geo, Part.Circle):
                constraint('Equal', source, target)
            for (point, _), p in zip(handles, positions):
                spoke = geometry(Part.LineSegment(center, p), True)
                constraint('Coincident', spoke, 1, center_id, 1)
                constraint('Coincident', spoke, 2, target, point)
                constraint('Equal', spokes[source, point], spoke)
                angle_id = constraint('Angle', spokes[source, point], spoke, step * k)
                if master is None:
                    master = angle_id
                    sketch.renameConstraint(master, 'CircularPattern' + str(center_id) + 'Step')
                else:
                    # One editable native driving step, all other angles associated.
                    sketch.setExpression('Constraints[' + str(angle_id) + ']',
                                         str(k) + ' * Constraints[' + str(master) + ']')
    return {'object': sketch.Name}
