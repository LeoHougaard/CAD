"""Unqualified App-only candidate. Caller owns transaction, solve and rollback.

No imports of FreeCAD until prepare_origin_scale is called. Never auto-enabled.
"""
import math
import xml.etree.ElementTree as ET


class Unqualified(ValueError):
    pass


SCALE_TYPES = frozenset(('Distance', 'DistanceX', 'DistanceY', 'Radius',
                         'Diameter', 'SnellsLaw', 'Weight'))


def eligible(sketch, index):
    """Explicit caller intent substitutes for GUI preference/viewer policy only."""
    constraints = sketch.Constraints
    if type(index) is not int or not 0 <= index < len(constraints):
        raise Unqualified('Invalid constraint index')
    # ExternalGeometry contains projected links, not the two intrinsic axes.
    if sketch.ExternalGeometry or any(c.Type == 'Block' for c in constraints):
        return False
    dimensions = [i for i, c in enumerate(constraints) if c.Type in SCALE_TYPES]
    c = constraints[index]
    return (dimensions == [index] and c.Driving and c.IsActive
            and not sketch.ExpressionEngine)


def extension_content(geometry):
    # Content is used for metadata comparison ONLY. Coordinates are never parsed
    # from it: FreeCAD Content can print coordinates with insufficient precision.
    root = ET.fromstring('<root>' + geometry.Content + '</root>')
    extension = root.find('GeoExtensions')
    if extension is None:
        raise Unqualified('Missing native geometry extensions')
    children = list(extension)
    if (extension.get('count') != '1' or len(children) != 1
            or children[0].get('type') != 'Sketcher::SketchGeometryExtension'
            or children[0].get('internalGeometryType') != '0'):
        raise Unqualified('Unqualified geometry metadata')
    return ET.tostring(extension, encoding='unicode')


def qualify_lines(sketch, edited):
    """Finite domain, not a claim to all Sketcher transform combinations."""
    geometry = sketch.Geometry
    n = len(geometry)
    if not n or any(g.TypeId != 'Part::GeomLineSegment' for g in geometry):
        raise Unqualified('Only native line segments are qualified for preparation')
    if sketch.ExternalGeometry or sketch.ExpressionEngine:
        raise Unqualified('Projected geometry and expressions are unqualified')
    for g in geometry:
        extension_content(g)
        if any(not math.isfinite(v) for p in (g.StartPoint, g.EndPoint)
               for v in (p.x, p.y, p.z)) or g.StartPoint.z != 0 or g.EndPoint.z != 0:
            raise Unqualified('Only finite sketch-local planar lines')

    def internal(i):
        return 0 <= i < n

    def point(i, p):
        return (internal(i) and p in (1, 2)) or (i == -1 and p == 1)

    for i, c in enumerate(sketch.Constraints):
        absent = c.Third == -2000 and c.ThirdPos == 0
        if c.Type in ('Horizontal', 'Vertical'):
            ok = (internal(c.First) and c.FirstPos == 0
                  and c.Second == -2000 and c.SecondPos == 0 and absent)
        elif c.Type == 'Coincident':
            ok = point(c.First, c.FirstPos) and point(c.Second, c.SecondPos) and absent
        elif c.Type == 'Symmetric':
            ok = (point(c.First, c.FirstPos) and point(c.Second, c.SecondPos)
                  and point(c.Third, c.ThirdPos))
        elif i == edited and c.Type == 'Distance':
            ok = (internal(c.First) and c.FirstPos == 0
                  and c.Second == -2000 and c.SecondPos == 0 and absent)
        else:
            ok = False
        if not ok:
            raise Unqualified('Unqualified constraint type/reference pattern')


def prepare_origin_scale(sketch, factor, edited):
    """Prepare detached native clones; no document mutation, no reconstruction."""
    if not math.isfinite(factor) or factor <= 1e-7:
        raise Unqualified('Factor must exceed pinned native Precision::Confusion')
    qualify_lines(sketch, edited)
    import FreeCAD as App
    matrix = App.Matrix()
    matrix.A11 = matrix.A22 = matrix.A33 = factor
    original = sketch.Geometry
    prepared = []
    for g in original:
        clone = g.clone()  # copy() would create a fresh native tag
        clone.transform(matrix)  # native gp_Trsf -> Geom_TrimmedCurve::Transform
        if clone.Tag != g.Tag or extension_content(clone) != extension_content(g):
            raise Unqualified('Native clone lost identity or metadata')
        prepared.append(clone)
    return prepared


def scale_before_datum(sketch, index, quantity):
    """Explicit opt-in policy + primitive. Does NOT setDatum/solve/commit.

    On True caller must immediately use its existing native setDatum and solve.
    On any exception caller must roll back its complete owning transaction.
    False means ordinary dimension behavior continues unchanged.
    """
    if not eligible(sketch, index):
        return False
    old = sketch.getDatum(index)
    if quantity.Unit != old.Unit:
        raise Unqualified('Native datum unit mismatch')
    if not math.isfinite(old.Value) or old.Value == 0 or not math.isfinite(quantity.Value):
        raise Unqualified('Invalid native datum ratio')
    prepared = prepare_origin_scale(sketch, quantity.Value / old.Value, index)
    ids = [sketch.getGeometryId(i) for i in range(sketch.GeometryCount)]
    tags = [g.Tag for g in sketch.Geometry]
    constraints = [c.Content for c in sketch.Constraints]
    sketch.Geometry = prepared  # typed PropertyGeometryList native clone setter
    if ([sketch.getGeometryId(i) for i in range(sketch.GeometryCount)] != ids
            or [g.Tag for g in sketch.Geometry] != tags
            or [c.Content for c in sketch.Constraints] != constraints):
        raise Unqualified('Typed setter changed identity or constraints; abort owner')
    return True
