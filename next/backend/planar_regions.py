"""Associative bounded planar regions from unchanged native sketch boundaries.

Native bounded half-edge cells, adjoining loops and nested holes. Interior
intersections use derived GeneralFuse edges with native input/output history.
No source curve edits, ordinary external geometry, copied sketch or stored solid.
Finite planar native attachment faces can additionally bound the outer region.
Caller retains the existing worker's transactions, leases and revision checks.
"""
import json
import math
import re
import FreeCAD as App
import Part

V1_FIELDS = {'RegionSource', 'RegionSourceId', 'RegionDocumentUid', 'RegionBodyId',
          'RegionGeometryIds', 'RegionSelection', 'RegionVersion'}
SUPPORT_FIELDS = {'RegionSupport', 'RegionSupportIdentity'}
FIELDS = V1_FIELDS | SUPPORT_FIELDS
CREATE = {'op', 'name', 'source', 'source_id', 'body', 'body_id', 'regions', 'source_token'}
UPDATE = {'op', 'object', 'object_id', 'regions'}
NAME = re.compile(r'[A-Za-z_][A-Za-z0-9_]*\Z')

class RegionError(ValueError):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code

def require(condition, code, message):
    if not condition:
        raise RegionError(code, message)

def owner(obj):
    body = obj.getParentGeoFeatureGroup()
    return body if body is not None and body.TypeId == 'PartDesign::Body' and obj in body.Group else None

def current(doc, name, native_id, typ):
    require(isinstance(name, str) and NAME.fullmatch(name), 'invalid_request', 'Choose a native object name')
    obj = doc.getObject(name)
    require(obj is not None and obj.Document == doc and type(native_id) is int and obj.ID == native_id
            and obj.TypeId == typ, 'stale_reference', 'A planar-region source or owner changed')
    return obj

def _key(outer, holes):
    return json.dumps({'outer': list(outer), 'holes': [list(x) for x in sorted(holes)]}, separators=(',', ':'))

def edge_correspondence(source, access_mode='identity_matrix', trace=None):
    """Join native geometry subnames to native topological-name provenance.

    SketchObject::getSubObject explicitly gives lowercase edgeN geometry-index
    semantics. Uppercase EdgeN below addresses Shape topology only. Wire order,
    coordinate closeness, and equal curve shapes are never used to infer IDs.
    """
    shape = source.Shape
    require(shape.Tag == source.ID, 'stale_reference', 'Native sketch shape owner tag changed')
    names = {}
    geometry = []
    for index in range(source.GeometryCount):
        gid = source.getGeometryId(index)
        construction = source.getConstruction(index)
        geometry.append({'geometry_index': index, 'persistent_id': gid, 'construction': construction})
        if construction:
            continue
        subname = 'edge' + str(index + 1)
        if trace:
            trace('api_input', {'subname':subname, 'access_mode':access_mode, 'geometry':geometry[-1]})
        try:
            if access_mode == 'omit_matrix':
                native = source.getSubObject(subname, retType=0, transform=False)
            elif access_mode == 'identity_matrix':
                matrix = App.Matrix()
                if trace:
                    trace('matrix_argument', {'subname':subname, 'python_type':type(matrix).__module__+'.'+type(matrix).__name__, 'repr':repr(matrix)})
                native = source.getSubObject(subname, 0, matrix, False)
            else:
                raise RegionError('unsupported_regions', 'Unsupported native access mode')
        except TypeError as error:
            if trace:
                trace('api_type_error', {'subname':subname, 'message':str(error)})
            raise
        require(native is not None and native.ShapeType == 'Edge' and len(native.Edges) == 1,
                'unsupported_regions', 'Native geometry does not expose one edge')
        name = native.getElementMappedName('Edge1')
        if trace:
            trace('api_result', {'subname':subname, 'shape_type':native.ShapeType, 'native_geometry_name':name})
        require(name == 'g' + str(gid) and name not in names,
                'stale_reference', 'Native geometry subname disagrees with its persistent identity')
        names[name] = gid
        geometry[-1]['native_geometry_name'] = name
    mapped = {}
    records = []
    for index, edge in enumerate(shape.Edges, 1):
        indexed = 'Edge' + str(index)
        name = shape.getElementMappedName(indexed)
        require(bool(name) and shape.getElementIndexedName(name) == indexed,
                'stale_reference', 'Native topological name is absent or ambiguous')
        history = shape.getElementHistory(name)
        if trace:
            trace('topology_result', {'index':index, 'mapped_name':name, 'history':history})
        if history is None:
            # Sketch buildShape names an edge with the native geometry name,
            # then makeElementWires appends structural operation postfixes.
            # History can be absent for untagged geometry/structural names.
            origin = name
        else:
            require(isinstance(history, tuple) and len(history) == 3 and history[0] == source.ID,
                    'stale_reference', 'Native edge provenance belongs to another owner')
            origin = history[1]
        # The root before the native ';' operation separator is created by
        # SketchObject::convertSubName. Validate it against the geometry's own
        # native subobject name; never infer IDs from topology order or curves.
        root = origin.partition(';')[0]
        require(root in names and names[root] not in mapped,
                'stale_reference', 'Native edge provenance is stale or not one-to-one')
        gid = names[root]
        mapped[gid] = edge
        records.append({'topological_index': index, 'mapped_name': name, 'history': history,
                        'geometry_name': root, 'persistent_id': gid})
    require(set(mapped) == set(names.values()), 'unsupported_regions', 'Some native sketch geometries lack one topological edge')
    return mapped, {'source_id': source.ID, 'shape_tag': shape.Tag,
                    'geometry': geometry, 'edges': records}

def boundaries(source, include_support=True):
    require(source.TypeId == 'Sketcher::SketchObject' and source.isValid()
            and not source.ExternalGeometry and not source.ConflictingConstraints
            and not source.RedundantConstraints and not source.MalformedConstraints,
            'unsupported_regions', 'Choose a valid internal native sketch')
    ids = {source.getGeometryId(i) for i in range(source.GeometryCount) if not source.getConstruction(i)}
    shape = source.Shape
    require(not shape.isNull() and shape.isValid() and 0 < len(shape.Edges) <= 256
            and 0 < len(shape.Wires) <= 64, 'unsupported_regions', 'Bounded closed planar loops required')
    mapped, _ = edge_correspondence(source)
    require(set(mapped) == ids, 'unsupported_regions', 'Point or split geometry is unsupported')
    # Walk the native planar edge graph, including junctions and open tails.
    # Native vertex tolerances are queried, never changed.
    import math
    from region_graph import cycles
    vertices = []
    def vertex_id(v):
        for i,old in enumerate(vertices):
            if v.isSame(old) or (partition is None and v.distToShape(old)[0] <= max(v.getTolerance(1), old.getTolerance(1))):
                return i
        vertices.append(v); return len(vertices)-1
    def same_vertex(a,b):
        return a.isSame(b) or a.distToShape(b)[0] <= max(a.getTolerance(1),b.getTolerance(1))
    native_edges=list(mapped.values())
    needs_partition = False
    for i,a in enumerate(native_edges):
        for b in native_edges[i+1:]:
            section=a.section(b)
            require(not section.Edges, 'unsupported_regions', 'Overlapping native sketch edges')
            for vertex in section.Vertexes:
                if not (any(same_vertex(vertex,v) for v in a.Vertexes)
                        and any(same_vertex(vertex,v) for v in b.Vertexes)):
                    needs_partition = True
    partition = None
    if needs_partition:
        mapped, partition = derived_fragments(source, mapped)
    inverse = source.Placement.inverse()
    def angle(v):
        v = inverse.Rotation.multVec(v)
        return math.atan2(v.y, v.x)
    graph = []; closed = []
    for gid,edge in mapped.items():
        if edge.isClosed():
            closed.append([(gid,0)]); continue
        require(len(edge.Vertexes)==2, 'unsupported_regions', 'Native edge needs two endpoints')
        graph.append((gid, vertex_id(edge.firstVertex()), vertex_id(edge.lastVertex()),
                      angle(edge.tangentAt(edge.FirstParameter)),
                      angle(-edge.tangentAt(edge.LastParameter))))
    try:
        walks = cycles(graph) + closed
    except ValueError as error:
        raise RegionError('unsupported_regions', str(error))
    loops = {}
    for walk in walks:
        edges = []
        for gid,reverse in walk:
            edge = mapped[gid].copy()
            if reverse: edge.reverse()
            edges.append(edge)
        wire = Part.Wire(edges)
        require(wire.isClosed() and wire.isValid(), 'unsupported_regions', 'Invalid native bounded walk')
        if partition is None:
            points = [inverse.multVec(p) for p in wire.discretize(Deflection=0.01)]
            signed = sum(a.x*b.y-b.x*a.y for a,b in zip(points,points[1:]+points[:1]))
        else:
            normal = source.Placement.Rotation.multVec(App.Vector(0, 0, 1))
            signed = sum((-1 if reverse else 1) * analytic_area(mapped[gid], normal)
                         for gid, reverse in walk)
        if signed <= 0 and walk not in closed: continue # unbounded face
        members = tuple(sorted(g for g,d in walk))
        face = Part.Face(wire)
        require(face.isValid() and face.Area > 0 and face.Surface.TypeId == 'Part::GeomPlane',
                'unsupported_regions', 'Simple bounded planar faces required')
        require(members not in loops, 'unsupported_regions', 'Ambiguous bounded cell identity')
        loops[members] = {'wire':wire, 'face':face, 'containers':[]}
        if partition is not None:
            loops[members]['geometry_ids'] = sorted({partition['origins'][g] for g in members})
    require(bool(loops), 'unsupported_regions', 'No closed bounded native regions')
    ordered = sorted(loops)
    for i, a in enumerate(ordered):
        for b in ordered[i+1:]:
            aa, bb = loops[a], loops[b]
            common = aa['face'].common(bb['face'])
            if not common.Faces:
                continue
            a_in_b = not aa['face'].cut(bb['face']).Faces
            b_in_a = not bb['face'].cut(aa['face']).Faces
            require(a_in_b != b_in_a, 'unsupported_regions', 'Loops must be separate or strictly nested')
            loops[a if a_in_b else b]['containers'].append(b if a_in_b else a)
    for key, loop in loops.items():
        containers = loop['containers']
        # Strict containment orders candidates. Native face area only identifies
        # the immediate container, never a replacement geometry or region ID.
        loop['parent'] = min(containers, key=lambda k: loops[k]['face'].Area) if containers else None
    for key, loop in loops.items():
        loop['children'] = sorted(k for k, value in loops.items() if value['parent'] == key)
        loop['region'] = _key(key, loop['children'])
    if include_support:
        add_support_boundary(source, loops, ids)
    if partition is not None:
        bind_partition(source, loops, ids, partition)
    return loops, sorted(ids)

def derived_fragments(source, originals):
    """Detached native split; only the GFA history establishes parent IDs.

    Parameter rank names siblings, never infers parentage. Native topology order
    is irrelevant. A changed incidence/count invalidates saved keys. Smooth
    parameter edits retain identity while split order and incidence remain fixed.
    """
    from region_graph import junction_signature
    gids = sorted(originals)
    inputs = [originals[g].copy() for g in gids]
    require(all(e.Curve.TypeId in ('Part::GeomLine', 'Part::GeomCircle') for e in inputs),
            'unsupported_regions', 'Intersecting regions support native lines, arcs and circles')
    try:
        result, history = inputs[0].generalFuse(inputs[1:], 0.0)
        require(result.isValid() and len(history) == len(gids)
                and 0 < len(result.Edges) <= 1024,
                'unsupported_regions', 'Invalid or excessive native split history')
        fragments = {}; origins = {}; endpoints = {}; vertices = []
        def junction(v):
            # GFA owns conforming topology. Never join distinct result vertices
            # by coordinates, curve equality, tessellation or arbitrary epsilon.
            for index, old in enumerate(vertices):
                if v.isSame(old): return index
            vertices.append(v); return len(vertices)-1
        for gid, original, row in zip(gids, inputs, history):
            require(all(piece.ShapeType == 'Edge' for piece in row),
                    'unsupported_regions', 'Non-edge native split history')
            # Some supported native versions leave unchanged entries empty.
            # Only native identity in the actual result permits that fallback.
            pieces = list(row) if row else [e for e in result.Edges if e.isSame(original)]
            require(bool(pieces), 'unsupported_regions', 'Native split lost an original edge')
            ranked = []
            for piece in pieces:
                require(sum(piece.isSame(e) for e in result.Edges) == 1,
                        'unsupported_regions', 'Split history does not identify one result edge')
                for old in fragments.values():
                    require(not piece.isSame(old), 'unsupported_regions', 'Multiple source owners for a fragment')
                edge = piece
                require(edge.Orientation in ('Forward', 'Reversed'),
                        'unsupported_regions', 'Unsupported native edge orientation')
                if edge.Orientation == 'Reversed': edge = edge.reversed()
                mid = edge.valueAt((edge.FirstParameter + edge.LastParameter)/2)
                parameter = original.Curve.parameter(mid)
                if original.isClosed():
                    parameter = (parameter - original.FirstParameter) % (original.LastParameter-original.FirstParameter)
                require(math.isfinite(parameter), 'unsupported_regions', 'Invalid source curve parameter')
                ranked.append((parameter, edge))
            ranked.sort(key=lambda pair: pair[0])
            require(len({p for p,e in ranked}) == len(ranked),
                    'unsupported_regions', 'Ambiguous fragment parameter order')
            for rank, (_, edge) in enumerate(ranked):
                token = json.dumps([gid, rank, len(ranked)], separators=(',', ':'))
                fragments[token] = edge; origins[token] = gid
                endpoints[token] = (junction(edge.firstVertex()), junction(edge.lastVertex()))
        require(len(fragments) == len(result.Edges), 'unsupported_regions', 'Incomplete native fragment provenance')
        return fragments, {'origins': origins, 'junctions': junction_signature(endpoints)}
    except (AttributeError, TypeError, RuntimeError) as error:
        raise RegionError('unsupported_regions', 'Native partition API unavailable: ' + str(error)) from error


def analytic_area(edge, normal):
    """Exact analytic line integral, used only to reject the unbounded walk.

    Faces and booleans remain native. No discretized polygon decides a cell.
    Edge is Forward; the caller supplies the half-edge reversal separately.
    """
    a = edge.valueAt(edge.FirstParameter); b = edge.valueAt(edge.LastParameter)
    curve = edge.Curve
    if curve.TypeId == 'Part::GeomLine':
        return a.cross(b).dot(normal) / 2
    require(curve.TypeId == 'Part::GeomCircle', 'unsupported_regions', 'Unsupported partition curve')
    return (curve.Center.cross(b-a).dot(normal)
            + curve.Radius**2 * (edge.LastParameter-edge.FirstParameter) * curve.Axis.dot(normal)) / 2


def bind_partition(source, loops, ids, partition):
    support = support_face(source)
    binding = {'document': str(source.Document.Uid), 'source': source.Name,
               'source_id': source.ID, 'body_id': owner(source).ID,
               'geometry_ids': sorted(ids),
               'source_geometry_ids': [source.getGeometryId(i) for i in range(source.GeometryCount)],
               'support': support[1] if support is not None else None, 'frame': 'source-body',
               'junctions': partition['junctions']}
    for key, loop in loops.items():
        loop['region'] = json.dumps({'version': 3, 'binding': binding,
            'boundary': json.loads(_key(key, loop['children']))}, sort_keys=True, separators=(',', ':'))


def add_support_boundary(source, loops, ids):
    support = support_face(source)
    if support is not None:
        obj, identity, face = support
        for loop in loops.values():
            # Native booleans establish coplanarity and finite containment.
            remainder = loop['face'].copy().cut(face.copy())
            # A completely removed face may be an empty/null native result.
            require(not remainder.Faces,
                    'unsupported_regions', 'Closed contours must lie inside the planar support face')
        children = sorted(k for k, v in loops.items() if v['parent'] is None)
        # Empty membership denotes the support, never a fake geometry ID.
        loops[()] = {'face': face, 'children': children}
        binding = {'document': str(source.Document.Uid), 'source': source.Name,
                   'source_id': source.ID, 'body_id': owner(source).ID,
                   'geometry_ids': sorted(ids),
                   'source_geometry_ids': [source.getGeometryId(i) for i in range(source.GeometryCount)],
                   'support': identity, 'frame': 'source-body'}
        for key, loop in loops.items():
            loop['region'] = json.dumps({'version': 2, 'binding': binding,
                'boundary': json.loads(_key(key, loop['children']))}, sort_keys=True, separators=(',', ':'))


def support_face(source):
    try:
        return _support_face(source)
    except (AttributeError, TypeError, RuntimeError) as error:
        raise RegionError('unsupported_regions', 'Native support face API unavailable: ' + str(error)) from error


def _support_face(source):
    """Resolve one same-Body planar face on a detached shape.

    Both native Shapes use their shared Body's frame. Origin/datum planes
    retain internal-only regions; other ambiguous attachments fail closed.
    """
    support = list(source.AttachmentSupport)
    if not support:
        require(str(source.MapMode) == 'Deactivated', 'unsupported_regions', 'Missing native attachment support')
        return None
    require(len(support) == 1 and len(support[0]) == 2,
            'unsupported_regions', 'One native planar support is required')
    obj, subs = support[0]
    require(obj is not None and obj.Document == source.Document,
            'stale_reference', 'Foreign or missing support object')
    body = owner(source)
    if obj.isDerivedFrom('App::Plane') or obj.TypeId == 'PartDesign::Plane':
        require(body is not None and (owner(obj) == body or obj in body.Origin.OriginFeatures)
                and list(subs) in ([], ['']), 'unsupported_regions', 'Foreign datum support')
        return None
    require(body is not None and owner(obj) == body and obj != source
            and source not in obj.OutListRecursive and str(source.MapMode) == 'FlatFace',
            'stale_reference', 'Support ownership, attachment mode or dependency graph changed')
    require(len(subs) == 1 and isinstance(subs[0], str),
            'unsupported_regions', 'Choose one finite support face')
    shape = obj.Shape
    require(not shape.isNull() and shape.isValid() and shape.Tag == obj.ID,
            'stale_reference', 'Invalid support shape or native owner tag')
    sub = subs[0]
    indexed = sub if re.fullmatch(r'Face[1-9][0-9]*', sub) else shape.getElementIndexedName(sub)
    require(isinstance(indexed, str) and re.fullmatch(r'Face[1-9][0-9]*', indexed),
            'unsupported_regions', 'Attachment does not identify one native face')
    mapped = shape.getElementMappedName(indexed)
    require(bool(mapped) and shape.getElementIndexedName(mapped) == indexed,
            'stale_reference', 'Support face lacks unambiguous native mapped provenance')
    face = shape.getElement(indexed).copy()
    require(face.ShapeType == 'Face' and face.isValid() and math.isfinite(face.Area) and face.Area > 0
            and face.Wires and all(w.isClosed() for w in face.Wires)
            and face.Surface.TypeId == 'Part::GeomPlane',
            'unsupported_regions', 'Finite native planar support face required')
    normal = source.Placement.Rotation.multVec(App.Vector(0, 0, 1))
    if face.normalAt(0, 0).dot(normal) < 0:
        face.reverse()
    identity = {'object': obj.Name, 'native_id': obj.ID, 'mapped_face': mapped,
                'history': shape.getElementHistory(mapped), 'map_mode': str(source.MapMode)}
    return obj, identity, face


def _placement_record(placement):
    # Native display text rounds coordinates; bind the actual binary64 values.
    base = placement.Base
    return [[float(base.x), float(base.y), float(base.z)],
            [float(value) for value in placement.Rotation.Q]]


def source_token(source):
    import hashlib
    support = support_face(source)
    payload = [str(source.Document.Uid), source.Name, source.ID,
               owner(source).ID if owner(source) else None,
               [source.getGeometryId(i) for i in range(source.GeometryCount)],
               _placement_record(source.Placement), _placement_record(source.getGlobalPlacement()), source.Shape.exportBrepToString()]
    if support is not None:
        obj, identity, face = support
        payload.extend([identity, _placement_record(obj.Placement), _placement_record(obj.getGlobalPlacement()), obj.Shape.exportBrepToString()])
    return hashlib.sha256(json.dumps(payload, sort_keys=True).encode()).hexdigest()

def catalog(source):
    loops, ids = boundaries(source)
    result = []
    for key in sorted(loops):
        loop = loops[key]
        atom = loop['face'].copy()
        for child in loop['children']:
            atom = atom.cut(loops[child]['face'].copy())
        require(atom.isValid() and atom.Area > 0 and len(atom.Faces) == 1,
                'unsupported_regions', 'A bounded region must be one connected native face')
        result.append({'key': loop['region'], 'outer_geometry_ids': loops[key].get('geometry_ids', list(key)),
                       'hole_geometry_ids': [loops[c].get('geometry_ids', list(c)) for c in loop['children']], 'area': atom.Area, 'triangles': display_triangles(atom, source)})
    return {'source': source.Name, 'source_id': source.ID, 'source_token':source_token(source), 'geometry_ids': ids, 'regions': result}

def selection(loops, requested, frame=None):
    require(isinstance(requested, list) and requested and all(isinstance(k, str) for k in requested)
            and len(requested) == len(set(requested)), 'invalid_request', 'Select distinct bounded region keys')
    available = {v['region'] for v in loops.values()}
    require(set(requested).issubset(available), 'stale_reference', 'A selected region boundary changed')
    chosen = set(requested)
    # Cancel an edge shared by two selected adjacent atoms. Retain precisely
    # transitions between selected and unselected regions, including holes.
    atoms = []
    for loop in loops.values():
        if loop['region'] not in chosen: continue
        atom = loop['face'].copy()
        for child in loop['children']: atom = atom.cut(loops[child]['face'].copy())
        require(atom.isValid() and atom.Area > 0 and len(atom.Faces) == 1,
                'unsupported_regions', 'A bounded region must be one connected native face')
        atoms.append(atom)
    face = atoms[0]
    for atom in atoms[1:]: face = face.fuse(atom)
    face = face.removeSplitter()
    if frame is not None:
        # Copy the analytic geometry through a rigid transform. A location-only
        # inverse is overwritten when Part::Feature imposes its Placement.
        face = face.copy()
        face.transformShape(frame.inverse().toMatrix(), True)
    require(face.isValid() and face.Faces and not face.Solids and face.Area > 0,
            'invalid_model', 'Native selected-region face construction failed')
    return face

def claimed(obj):
    return any(k in obj.PropertiesList for k in FIELDS)

def protected(obj, prop):
    return claimed(obj) and prop in FIELDS | {'Shape', 'Placement', 'Proxy'}

def resolve(obj):
    require(obj.TypeId == 'Part::Part2DObjectPython' and V1_FIELDS.issubset(obj.PropertiesList)
            and obj.RegionVersion in (1, 2, 3) and obj.RegionDocumentUid == str(obj.Document.Uid),
            'stale_reference', 'Invalid planar profile identity or version')
    source = obj.RegionSource
    require(source is not None and source.ID == obj.RegionSourceId and source.Document == obj.Document
            and owner(obj) is not None and owner(source) == owner(obj) and owner(obj).ID == obj.RegionBodyId
            and obj not in source.OutListRecursive, 'stale_reference', 'Planar profile ownership graph changed')
    has_support = obj.RegionVersion == 2 or (obj.RegionVersion == 3 and bool(SUPPORT_FIELDS.intersection(obj.PropertiesList)))
    if has_support:
        require(SUPPORT_FIELDS.issubset(obj.PropertiesList), 'stale_reference', 'Missing support profile fields')
        require(obj.getTypeIdOfProperty('RegionSupport') == 'App::PropertyLink'
                and obj.getTypeIdOfProperty('RegionSupportIdentity') == 'App::PropertyString'
                and all('ReadOnly' in obj.getPropertyStatus(p) for p in SUPPORT_FIELDS),
                'stale_reference', 'Support profile property schema changed')
        support = support_face(source)
        require(support is not None and obj.RegionSupport == support[0]
                and obj.RegionSupportIdentity == json.dumps(support[1], sort_keys=True, separators=(',', ':'))
                and obj not in support[0].OutListRecursive,
                'stale_reference', 'Native support face identity or dependency changed')
    else:
        require(not SUPPORT_FIELDS.intersection(obj.PropertiesList), 'stale_reference', 'Unexpected version 1 support fields')
    loops, ids = boundaries(source, include_support=has_support)
    if obj.RegionVersion == 3:
        require(all(json.loads(k).get('version') == 3 for k in obj.RegionSelection),
                'stale_reference', 'Partition profile requires version 3 boundaries')
    require(ids == list(obj.RegionGeometryIds), 'stale_reference', 'Native source geometry identities changed')
    return source, loops

class RegionProfile:
    def execute(self, obj):
        source, loops = resolve(obj)
        # Derived planar face only. No prebuilt solid, sketch clone or source edit.
        face = selection(loops, list(obj.RegionSelection), source.Placement)
        # Part2DObject's Placement supplies the stock Pad normal. Native
        # Part::Feature::onChanged also imposes it on Shape during recompute.
        obj.Placement = source.Placement
        obj.Shape = face
    def __getstate__(self):
        return None
    def __setstate__(self, state):
        require(state is None, 'invalid_model', 'Unsupported planar profile proxy state')

def definition(obj):
    """Validate authoritative inputs before the derived Shape has executed."""
    source, loops = resolve(obj)
    require(isinstance(obj.Proxy, RegionProfile), 'invalid_model', 'Planar profile recompute handler unavailable')
    selection(loops, list(obj.RegionSelection))
    return {'source': source.Name, 'source_id': source.ID, 'body': owner(obj).Name,
            'keys': list(obj.RegionSelection), 'geometry_ids': list(obj.RegionGeometryIds), 'version': obj.RegionVersion}


def metadata(obj):
    result = definition(obj)
    require(not obj.Shape.isNull() and obj.Shape.isValid() and obj.Shape.Faces and not obj.Shape.Solids,
            'invalid_model', 'Planar profile has no valid derived faces')
    return result

def prepare(doc, edit):
    if edit.get('op') == 'create_planar_region_profile':
        require(set(edit) == CREATE, 'invalid_request', 'Unexpected planar profile fields')
        source = current(doc, edit['source'], edit['source_id'], 'Sketcher::SketchObject')
        body = current(doc, edit['body'], edit['body_id'], 'PartDesign::Body')
        require(owner(source) == body and isinstance(edit['name'], str) and NAME.fullmatch(edit['name'])
                and doc.getObject(edit['name']) is None, 'invalid_request', 'Choose same-Body source and unused profile name')
        require(edit['source_token'] == source_token(source), 'stale_reference', 'Source geometry changed since region discovery')
        loops, ids = boundaries(source)
        selection(loops, edit['regions'])
        return source, body, ids
    require(set(edit) == UPDATE, 'invalid_request', 'Unexpected planar profile update fields')
    obj = current(doc, edit['object'], edit['object_id'], 'Part::Part2DObjectPython')
    _, loops = resolve(obj)
    selection(loops, edit['regions'])
    return obj

def apply(doc, edit):
    ready = prepare(doc, edit)
    if edit['op'] == 'update_planar_region_profile':
        ready.RegionSelection = edit['regions']
        return {'object': ready.Name}
    source, body, ids = ready
    support = support_face(source)
    obj = body.newObject('Part::Part2DObjectPython', edit['name'])
    require(obj.Name == edit['name'], 'name_conflict', 'Native profile name changed')
    values = [('RegionSource','App::PropertyLink',source), ('RegionSourceId','App::PropertyInteger',source.ID),
              ('RegionDocumentUid','App::PropertyString',str(doc.Uid)), ('RegionBodyId','App::PropertyInteger',body.ID),
              ('RegionGeometryIds','App::PropertyIntegerList',ids), ('RegionSelection','App::PropertyStringList',edit['regions']),
              ('RegionVersion','App::PropertyInteger',3 if any(json.loads(k).get('version') == 3 for k in edit['regions']) else (2 if support is not None else 1))]
    if support is not None:
        values.extend([('RegionSupport', 'App::PropertyLink', support[0]),
                       ('RegionSupportIdentity', 'App::PropertyString',
                        json.dumps(support[1], sort_keys=True, separators=(',', ':')))])
    for name, typ, value in values:
        obj.addProperty(typ, name, 'Planar regions');setattr(obj, name, value)
        obj.setPropertyStatus(name, 'ReadOnly')
    obj.Proxy = RegionProfile()
    require(owner(obj) == body, 'invalid_model', 'Native profile must belong to the source Body')
    return {'object': obj.Name}

def preflight(doc, edits):
    for edit in edits:
        if not isinstance(edit, dict):
            continue
        if edit.get('op') in {'create_planar_region_profile','update_planar_region_profile'}:
            prepare(doc, edit)
        if edit.get('op') in {'set','expression'}:
            obj = doc.getObject(edit.get('object',''))
            prop = edit.get('property','').removeprefix('.').split('.')[0].split('[')[0]
            require(obj is None or not protected(obj, prop), 'read_only_property', 'Derived planar profile fields are protected')


def display_triangles(face, source):
    points, triangles = face.tessellate(0.1)
    delta = source.getGlobalPlacement().multiply(source.Placement.inverse())
    points = [delta.multVec(p) for p in points]
    return [[[points[i].x,points[i].y,points[i].z] for i in triangle] for triangle in triangles]
