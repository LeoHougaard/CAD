"""Pinned Sketcher Copy/Clone/Move/array, through native application methods."""
import math

import FreeCAD as App
import Part
import sketch_geometry


def _checked(values):
    from sketch_operations import OperationError
    result = tuple(values)
    if not all(math.isfinite(v) for v in result):
        raise OperationError('Translated geometry exceeds the native finite range')
    return result


def _add(first, second):
    return _checked(a + b for a, b in zip(first, second))


def _xyz(point):
    return _checked((point.x, point.y, point.z))


def _locations(geo):
    # These are precisely the coordinates modified by addCopy's nine branches:
    # both line ends, circle/conic centers, every spline pole, and GeomPoint.
    if isinstance(geo, Part.LineSegment):
        return [_xyz(geo.StartPoint), _xyz(geo.EndPoint)]
    if isinstance(geo, Part.BSplineCurve):
        return [_xyz(p) for p in geo.getPoles()]
    if isinstance(geo, Part.Point):
        return [_checked((geo.X, geo.Y, geo.Z))]
    if isinstance(geo, (Part.Circle, Part.ArcOfCircle, Part.Ellipse,
                        Part.ArcOfEllipse, Part.ArcOfHyperbola, Part.ArcOfParabola)):
        return [_xyz(geo.Center)]
    return []  # Native addCopy also skips unsupported geometry.


def _length(delta):
    from sketch_operations import OperationError
    # Use Base::Vector3d::Length, as native separation dimensions do. hypot
    # would hide the native squared-length overflow/underflow at this boundary.
    length = App.Vector(*delta).Length
    if not math.isfinite(length) or length == 0:
        raise OperationError('Native separation length must be finite and nonzero')


def _reference_after(geo, locations):
    if isinstance(geo, (Part.LineSegment, Part.Point, Part.Circle, Part.Ellipse)):
        return locations[0]
    # Native arc references are evaluated AFTER setCenter; spline references
    # AFTER setPoles. Evaluate the same native curve on a detached copy, not an
    # approximated translated endpoint. No sketch/property/ID is modified here.
    detached = geo.copy()
    if isinstance(geo, Part.BSplineCurve):
        # Python exposes native SetPole, not the C++ bulk setPoles wrapper.
        # Omitting weight preserves the existing rational weights exactly.
        for index, p in enumerate(locations, 1):
            detached.setPole(index, App.Vector(*p))
        return _xyz(detached.StartPoint)
    detached.Center = App.Vector(*locations[0])
    # addCopy calls getStartPoint(true) for all four arc classes. Pinned
    # GeomArcOfConic emulates CCW by taking EndPoint when Axis.Z is negative.
    return _xyz(detached.EndPoint if detached.Axis.z < 0 else detached.StartPoint)


def _validate_arithmetic(sketch, indices, displacement, columns, rows, scale, constrained):
    delta = _xyz(displacement)
    # Native precomputes these products even for one row. Reject infinity now,
    # before a later 0 * infinity could make an otherwise unused term NaN.
    perpendicular = _checked((scale * delta[1], scale * -delta[0], 0.0))
    col = [_checked(x * v for v in delta) for x in range(columns)]
    row = [_checked(y * v for v in perpendicular) for y in range(rows)]
    geometries = sketch.Geometry
    sources = [(geometries[i], _locations(geometries[i])) for i in indices]
    if constrained:
        _length(delta)
        if rows > 1:
            _length(perpendicular)
        first = geometries[indices[0]]
        point = 3 if isinstance(first, (Part.Circle, Part.Ellipse)) else 1
        reference = _xyz(sketch.getPoint(indices[0], point))
    for y in range(rows):
        for x in range(columns):
            if x == 0 and y == 0:
                continue
            first_locations = None
            for geo, locations in sources:
                # Preserve C++ left association: (source + column) + row.
                translated = [_add(_add(p, col[x]), row[y]) for p in locations]
                if first_locations is None:
                    first_locations = translated
                if isinstance(geo, Part.LineSegment):
                    _length(_checked(b - a for a, b in zip(translated[0], translated[1])))
            if constrained and first_locations:
                # Separation uses DIFFERENT grouping: ref + (column + row).
                cx, ry = (x, y - 1) if x == 0 else (x - 1, y)
                start = _add(reference, _add(col[cx], row[ry]))
                end = _reference_after(first, first_locations)
                _length(_checked(b - a for a, b in zip(start, end)))


def apply(sketch, edit):
    from sketch_operations import OperationError, flag, integer, quantity, vector
    op = edit['op']
    array = op == 'rectangular_array'
    allowed = {'op', 'object', 'indices', 'all_geometry', 'displacement'}
    if array:
        allowed.update(('columns', 'rows', 'clone', 'constrain_displacement', 'perpendicular_scale'))
    if set(edit) - allowed:
        raise OperationError('Unsupported native copy task parameter')
    indices = edit.get('indices')
    all_geometry = flag(edit, 'all_geometry', False)
    if not isinstance(indices, list) or all_geometry and indices:
        raise OperationError('Choose selected geometry or all geometry')
    if all_geometry:
        indices = list(range(sketch.GeometryCount))
    if not indices:
        raise OperationError('Select internal native geometries or choose all geometry')
    # Legacy SketcherCopy::activate and CmdSketcherRectangularArray admit only
    # non-external edges and standalone GeomPoint vertices. Never pass negative
    # IDs to addMove: C++ indexes newgeoVals[*it] without bounds checking.
    indices = [integer(i, 0, sketch.GeometryCount - 1, 'Geometry index') for i in indices]
    if len(set(indices)) != len(indices):
        raise OperationError('Select each source geometry once')
    displacement = vector(edit.get('displacement'))
    if displacement.z != 0:
        raise OperationError('Displacement must lie in the sketch plane')
    clone = op == 'clone_geometry' or array and flag(edit, 'clone', False)
    columns = integer(edit.get('columns'), 2, 99, 'Columns') if array else 2
    rows = integer(edit.get('rows'), 1, 99, 'Rows') if array else 1
    constrained = flag(edit, 'constrain_displacement', False) if array else False
    scale = quantity(edit.get('perpendicular_scale', 1), '') if array else 1
    # Use native InternalAlignment links, not curve coordinates or guessed IDs.
    # This is the pinned getListOfSelectedGeoIds(true) policy for conic/spline
    # parent selections, also used by the existing Mirror task.
    for index in list(indices):
        geo = sketch.Geometry[index]
        if isinstance(geo, (Part.Ellipse, Part.ArcOfEllipse, Part.ArcOfHyperbola,
                            Part.ArcOfParabola, Part.BSplineCurve)):
            for constraint in sketch.Constraints:
                if constraint.Type == 'InternalAlignment' and constraint.Second == index and constraint.First not in indices:
                    indices.append(integer(constraint.First, 0, sketch.GeometryCount - 1, 'Internal geometry index'))
    # Native curve IDs and constraint-list sizes are signed ints. Arithmetic
    # stays in Python until the proven GUI dimensions and safe sizes pass.
    cells = columns * rows
    if sketch.GeometryCount + (cells - 1) * (len(indices) + int(constrained)) > 2147483647:
        raise OperationError('Array exceeds the native geometry index range')
    if sketch.ConstraintCount * cells + (cells - 1) * 5 > 2147483647:
        raise OperationError('Array exceeds the native constraint index range')
    _validate_arithmetic(sketch, indices, displacement, columns, rows, scale, constrained)
    if op == 'move_geometries':
        # addMove mutates the original geometry before Geometry.setValues, so
        # register the native property BEFORE that mutation. Pinned GeomPoint
        # Python wrappers omit extensions; recover them in exact property order.
        sketch.Geometry = sketch_geometry.restore_point_extensions(sketch, 'Geometry', sketch.Geometry)
        sketch.addMove(indices, displacement)
    elif array:
        # GUI passes Columns THEN Rows; PyImp misleadingly names them rows/cols.
        sketch.addRectangularArray(indices, displacement, clone, columns, rows, constrained, scale)
    else:
        sketch.addCopy(indices, displacement, clone)
    # Do not infer result IDs from addCopy's tuple. Native snapshots own IDs.
    return {'object': sketch.Name}
