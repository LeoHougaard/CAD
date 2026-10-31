"""Bounded native New extrusion to a selected vertex.

The caller owns the preview transaction, document lease and revision checks.
This module does not copy geometry or resolve a vertex by coordinates.
"""
import re
import FreeCAD as App
import Part

OWNER_TAGS = ("VertexExtrusionOwner", "VertexExtrusionOwnerId", "VertexExtrusionDocumentUid")


class VertexExtrusionError(ValueError):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def require(condition, code, message):
    if not condition:
        raise VertexExtrusionError(code, message)


def actual_object(doc, name, native_id, type_id):
    require(isinstance(name, str) and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name),
            "invalid_request", "A current native object name is required")
    obj = doc.getObject(name)
    require(obj is not None and obj.Document == doc and type(native_id) is int
            and obj.ID == native_id and obj.TypeId == type_id,
            "stale_reference", "A native source object changed")
    return obj


def source_plane(source, body, owner_body, *, validate_shape=True):
    """Resolve the existing same-Body datum attachment without changing it."""
    try:
        support = source.AttachmentSupport
        if len(support) != 1 or support[0][0] is None:
            return None
        plane, subelements = support[0][0], list(support[0][1])
        if plane.TypeId != "PartDesign::Plane" or owner_body(plane) != body:
            return None
        if subelements in ([], [""]):
            return plane
        if subelements != ["Face1"] or str(source.MapMode) != "FlatFace":
            return None
        if not validate_shape:
            return plane
        shape = plane.Shape
        if shape.isNull() or not shape.isValid() or len(shape.Faces) != 1:
            return None
        face = shape.getElement(subelements[0])
        return plane if face.ShapeType == "Face" and face.isValid() and face.isSame(shape.Faces[0]) else None
    except Exception:
        return None


def boundary(sketch):
    """Recognize one selected-line slot with independent normal points allowed.

    This deliberately does not claim arbitrary sketch region selection. All
    edges come from the live sketch and retain its mapped identities.
    """
    require(sketch.isValid(),
            "invalid_geometry", "The source sketch is not valid")
    normal = [(i, g.TypeId) for i, g in enumerate(sketch.Geometry) if not sketch.getConstruction(i)]
    kinds = [kind for _, kind in normal]
    require(kinds.count("Part::GeomLineSegment") == 3
            and kinds.count("Part::GeomArcOfCircle") == 2
            and all(kind in ("Part::GeomLineSegment", "Part::GeomArcOfCircle", "Part::GeomPoint") for kind in kinds),
            "invalid_geometry", "The source sketch must have one bounded slot region")
    shape = sketch.Shape
    require(not shape.isNull() and len(shape.Edges) == 5,
            "invalid_geometry", "The source sketch does not have five live curve edges")
    wires = [w for w in shape.Wires if w.isClosed() and len(w.Edges) == 4]
    require(len(wires) == 1, "invalid_geometry", "Select exactly one closed four-edge region")
    wire = wires[0]
    selected = [("Edge" + str(i), edge) for i, edge in enumerate(shape.Edges, 1)
                if any(edge.isSame(member) for member in wire.Edges)]
    other = [edge for edge in shape.Edges if not any(edge.isSame(member) for member in wire.Edges)]
    require(len(selected) == 4 and len(other) == 1 and len(other[0].Vertexes) == 2,
            "invalid_geometry", "The closed region boundary is ambiguous")
    maps = [shape.getElementMappedName(name) for name, _ in selected]
    require(all(maps) and len(set(maps)) == 4, "stale_reference", "Slot edge mappings are unavailable")
    native_maps = {"g" + str(sketch.getGeometryId(i)) + ";SKT": kind
                   for i, kind in normal if kind != "Part::GeomPoint"}
    boundary_kinds = [native_maps.get(name.lstrip(";")) for name in maps]
    require(boundary_kinds.count("Part::GeomArcOfCircle") == 2
            and boundary_kinds.count("Part::GeomLineSegment") == 2,
            "invalid_geometry", "The closed boundary is not the two-arc slot region")
    interior = [name for name in native_maps if name not in {m.lstrip(";") for m in maps}]
    require(len(interior) == 1 and native_maps[interior[0]] == "Part::GeomLineSegment"
            and any(shape.getElementMappedName("Edge" + str(i)).lstrip(";") == interior[0]
                    and edge.isSame(other[0]) for i, edge in enumerate(shape.Edges, 1)),
            "invalid_geometry", "The retained interior curve is not the original native line")
    # Ensure the retained normal line stays strictly inside the selected face.
    face = Part.Face(wire)
    section = wire.section(other[0])
    require(face.isValid() and len(section.Vertexes) == 0 and len(section.Edges) == 0
            and all(face.distToShape(v)[0] < 1e-7 for v in other[0].Vertexes),
            "invalid_geometry", "The interior line intersects or leaves the region")
    return [name for name, _ in selected], maps


def prepare_create(doc, edit, owner_body, effective_visibility):
    fields = {"op", "source", "source_id", "body", "body_id", "target", "target_id",
              "vertex", "vertex_map", "new_body", "binder", "plane", "name", "mode", "length"}
    require(isinstance(edit, dict) and set(edit) == fields,
            "invalid_request", "Unexpected vertex extrusion request")
    source = actual_object(doc, edit["source"], edit["source_id"], "Sketcher::SketchObject")
    body = actual_object(doc, edit["body"], edit["body_id"], "PartDesign::Body")
    require(owner_body(source) == body and body.getParentGeoFeatureGroup() is None,
            "invalid_request", "The source Body must be a native root Body")
    mode = edit["mode"]
    require(mode in ("Blind", "Up to vertex"), "invalid_request", "Unsupported end condition")
    target = None
    point = None
    vertex, mapped = edit["vertex"], edit["vertex_map"]
    if mode == "Up to vertex":
        target = actual_object(doc, edit["target"], edit["target_id"], "Sketcher::SketchObject")
        require(source != target and owner_body(source) == body and owner_body(target) == body
                and effective_visibility(target), "stale_reference", "Choose a visible vertex in the source Body")
        require(source not in target.OutListRecursive and body not in target.OutListRecursive,
                "invalid_request", "The target would create a native dependency cycle")
        require(isinstance(vertex, str) and re.fullmatch(r"Vertex[1-9][0-9]*", vertex)
                and isinstance(mapped, str) and mapped and not target.Shape.isNull(),
                "stale_reference", "The selected native vertex is unavailable")
        try:
            require(target.Shape.getElementMappedName(vertex) == mapped,
                    "stale_reference", "The selected native vertex changed")
            point = target.Shape.getElement(vertex).Point
        except VertexExtrusionError:
            raise
        except Exception as error:
            raise VertexExtrusionError("stale_reference", "The selected native vertex changed") from error
    else:
        require(all(edit[k] is None for k in ("target", "target_id", "vertex", "vertex_map"))
                and isinstance(edit["length"], (int, float)) and not isinstance(edit["length"], bool)
                and 0 < edit["length"] < 1e9, "invalid_request", "Blind length must be positive")
    sketch_plane = source_plane(source, body, owner_body)
    require(sketch_plane is not None,
            "invalid_geometry", "The sketch must be attached to a native plane in its Body")
    require(source not in sketch_plane.OutListRecursive and (target is None or source not in target.OutListRecursive),
            "invalid_request", "The selected references form a dependency cycle")
    edges, edge_maps = boundary(source)
    direction = source.getGlobalPlacement().Rotation.multVec(App.Vector(0, 0, 1))
    distance = (point - source.getGlobalPlacement().Base).dot(direction) if point is not None else edit["length"]
    require(abs(distance) > 1e-7, "invalid_geometry", "The extrusion end lies in the sketch plane")
    return source, body, target, sketch_plane, edges, edge_maps, distance, mapped, vertex, direction


def create(doc, edit, owner_body, effective_visibility):
    source, body, target, sketch_plane, edges, edge_maps, distance, mapped, vertex, direction = prepare_create(
        doc, edit, owner_body, effective_visibility)
    names = [edit[k] for k in ("new_body", "binder", "plane", "name")]
    require(len(set(names)) == 4 and all(isinstance(n, str) and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", n)
            and doc.getObject(n) is None for n in names),
            "name_conflict", "New extrusion object names must be unused")
    document_uid = str(doc.Uid)
    require(bool(document_uid), "invalid_request", "The native document identity is unavailable")
    old_tip = body.Tip
    new_body = doc.addObject("PartDesign::Body", edit["new_body"])
    binder = new_body.newObject("PartDesign::SubShapeBinder", edit["binder"])
    binder.Support = [(source, edges)]
    doc.recompute()
    plane = None
    if target is not None:
        plane = new_body.newObject("PartDesign::Plane", edit["plane"])
        plane.AttachmentSupport = [(sketch_plane, [""]), (target, [vertex])]
        plane.MapMode = "ParallelPlane"
        doc.recompute()
    pad = new_body.newObject("PartDesign::Pad", edit["name"])
    pad.Profile = (binder, [""])
    if plane is not None:
        pad.Type = "UpToFace"
        pad.UpToFace = (plane, [""])
        pad.Reversed = distance < 0
    else:
        pad.Type = "Length"
        pad.Length = distance
    # Scalar identity tags avoid helper-to-feature link cycles. Ownership also
    # requires the actual native links, Body and current Pad identity.
    for item in (pad, binder, plane) if plane is not None else (pad, binder):
        for prop, type_id, value in ((OWNER_TAGS[0], "App::PropertyString", pad.Name),
                                     (OWNER_TAGS[1], "App::PropertyInteger", pad.ID),
                                     (OWNER_TAGS[2], "App::PropertyString", document_uid)):
            item.addProperty(type_id, prop, "Internal")
            setattr(item, prop, value)
            item.setPropertyStatus(prop, "ReadOnly")
            item.setPropertyStatus(prop, "Hidden")
    binder.Visibility = False
    if plane is not None:
        plane.Visibility = False
    doc.recompute()
    require(body.Tip == old_tip and owner_body(source) == body and (target is None or owner_body(target) == body)
            and new_body.Tip == pad and all(owner_body(o) == new_body for o in ((binder, plane, pad) if plane is not None else (binder, pad)))
            and pad.isValid() and not pad.Shape.isNull() and pad.Shape.isValid()
            and len(pad.Shape.Solids) == 1,
            "invalid_model", "Native vertex extrusion did not create one independent solid")
    distances = [(v.Point - source.getGlobalPlacement().Base).dot(direction) for v in pad.Shape.Vertexes]
    require(distances and abs(min(distances) - min(0., distance)) < 1e-6
            and abs(max(distances) - max(0., distance)) < 1e-6,
            "invalid_model", "The native solid does not terminate at the selected vertex plane")
    return {"object": pad.Name, "body": new_body.Name, "binder": binder.Name,
            "plane": plane.Name if plane is not None else None, "edge_mappings": edge_maps, "vertex_mapping": mapped}


def _helper_shell(pad, owner_body):
    """Resolve only this feature's two native helpers, or return no claim."""
    if pad is None or pad.TypeId != "PartDesign::Pad" or str(pad.Type) not in ("UpToFace", "Length"):
        return None
    profile, face = pad.Profile, pad.UpToFace
    if not profile or not profile[0] or (str(pad.Type) == "UpToFace" and (not face or not face[0])):
        return None
    binder, plane = profile[0], face[0] if str(pad.Type) == "UpToFace" else None
    body = owner_body(pad)
    if (body is None or binder.TypeId != "PartDesign::SubShapeBinder"
            or owner_body(binder) != body
            or not valid_tag(pad, pad) or not valid_tag(binder, pad)
            or list(profile[1]) != [""]):
        return None
    if plane is not None and (plane.TypeId != "PartDesign::Plane" or owner_body(plane) != body
                              or binder == plane or not valid_tag(plane, pad)
                              or list(face[1]) != [""]):
        return None
    return body, binder, plane


def helper_owner(obj, owner_body):
    name = getattr(obj, "VertexExtrusionOwner", None)
    if not isinstance(name, str) or not name:
        return None
    pad = obj.Document.getObject(name)
    owned = owned_helpers(pad, owner_body)
    return pad if owned and obj in owned[1:] else None


def valid_tag(obj, pad):
    try:
        return (obj.Document == pad.Document and all(prop in obj.PropertiesList for prop in OWNER_TAGS)
                and obj.VertexExtrusionOwner == pad.Name and type(obj.VertexExtrusionOwnerId) is int
                and obj.VertexExtrusionOwnerId == pad.ID
                and obj.VertexExtrusionDocumentUid == str(pad.Document.Uid)
                and all({"ReadOnly", "Hidden"}.issubset(set(obj.getPropertyStatus(prop))) for prop in OWNER_TAGS))
    except Exception:
        return False


def declares_helpers(pad, owner_body):
    if pad is None or pad.TypeId != "PartDesign::Pad" or owner_body(pad) is None:
        return False
    if any(prop in pad.PropertiesList for prop in OWNER_TAGS):
        return True
    # A malformed partial tag on an actually referenced helper must not cause
    # the Pad to be treated as an unrelated ordinary feature.
    links = []
    for prop in ("Profile", "UpToFace"):
        value = getattr(pad, prop, None)
        if value and value[0] is not None:
            links.append(value[0])
    return any(any(prop in obj.PropertiesList for prop in OWNER_TAGS) for obj in links)


def _owned_graph(pad, owner_body):
    """Require the complete live source/helper graph before claiming objects."""
    try:
        shell = _helper_shell(pad, owner_body)
        if shell is None:
            return None
        body, binder, plane = shell
        support = binder.Support
        if len(support) != 1 or support[0][0] is None or len(support[0][1]) != 4:
            return None
        source = support[0][0]
        source_body = owner_body(source)
        if (source.Document != pad.Document or source.Document.getObject(source.Name) is not source
                or source.TypeId != "Sketcher::SketchObject" or source_body is None
                or source_body == body or source_body.getParentGeoFeatureGroup() is not None
                or body.getParentGeoFeatureGroup() is not None
                or pad in source.OutListRecursive or body in source.OutListRecursive):
            return None
        sketch_plane = source_plane(source, source_body, owner_body, validate_shape=False)
        if sketch_plane is None or source in sketch_plane.OutListRecursive:
            return None
        target, vertex, mapped = None, None, None
        if plane is None:
            if pad.UpToFace and pad.UpToFace[0] is not None:
                return None
            if not 0 < pad.Length.Value < 1e9:
                return None
        else:
            attached = plane.AttachmentSupport
            if (str(plane.MapMode) != "ParallelPlane" or len(attached) != 2
                    or attached[0][0] != sketch_plane
                    or list(attached[0][1]) not in ([""], [])
                    or attached[1][0] is None or len(attached[1][1]) != 1):
                return None
            target, vertex = attached[1][0], attached[1][1][0]
            if (target.Document != pad.Document or target.Document.getObject(target.Name) is not target
                    or target.TypeId != "Sketcher::SketchObject" or target == source
                    or owner_body(target) != source_body or source in target.OutListRecursive
                    or pad in target.OutListRecursive or body in target.OutListRecursive
                    or not isinstance(vertex, str) or re.fullmatch(r"Vertex[1-9][0-9]*", vertex) is None
                    or target.Shape.isNull()):
                return None
            mapped = target.Shape.getElementMappedName(vertex)
            target.Shape.getElement(vertex).Point
            if not mapped:
                return None
        # Reject malformed references before geometry queries can build native
        # validation or tessellation caches on an otherwise unrelated source.
        if source_plane(source, source_body, owner_body) != sketch_plane:
            return None
        linked_maps = [source.Shape.getElementMappedName(sub) for sub in support[0][1]]
        if not all(linked_maps) or len(set(linked_maps)) != 4:
            return None
        _, actual_maps = boundary(source)
        if sorted(linked_maps) != sorted(actual_maps):
            return None
        return (*shell, source, target, vertex, mapped)
    except Exception:
        return None


def owned_helpers(pad, owner_body):
    graph = _owned_graph(pad, owner_body)
    return graph[:3] if graph is not None else None


def feature_metadata(pad, owner_body):
    graph = _owned_graph(pad, owner_body)
    if graph is None or graph[2] is None:
        return None
    body, binder, plane, source, target, vertex, mapped = graph
    return {"pad": pad.Name, "pad_id": pad.ID, "body": body.Name, "body_id": body.ID,
            "binder": binder.Name, "binder_id": binder.ID, "plane": plane.Name, "plane_id": plane.ID,
            "source": source.Name, "source_id": source.ID, "target": target.Name,
            "target_id": target.ID, "vertex": vertex, "vertex_map": mapped}


def prepare_update(doc, edit, owner_body, effective_visibility):
    fields = {"op", "pad", "pad_id", "body", "body_id", "binder", "binder_id", "plane", "plane_id",
              "source", "source_id", "target", "target_id", "vertex", "vertex_map"}
    require(isinstance(edit, dict) and set(edit) == fields, "invalid_request", "Unexpected extrusion edit")
    pad = actual_object(doc, edit["pad"], edit["pad_id"], "PartDesign::Pad")
    meta = feature_metadata(pad, owner_body)
    require(meta is not None and all(meta[k] == edit[k] for k in
            ("pad", "pad_id", "body", "body_id", "binder", "binder_id", "plane", "plane_id", "source", "source_id")),
            "stale_reference", "The extrusion ownership changed")
    source = actual_object(doc, edit["source"], edit["source_id"], "Sketcher::SketchObject")
    target = actual_object(doc, edit["target"], edit["target_id"], "Sketcher::SketchObject")
    source_body = owner_body(source)
    require(target != source and owner_body(target) == source_body and effective_visibility(target)
            and source not in target.OutListRecursive and pad not in target.OutListRecursive,
            "stale_reference", "Choose a visible independent vertex in the source Body")
    vertex, mapped = edit["vertex"], edit["vertex_map"]
    require(isinstance(vertex, str) and re.fullmatch(r"Vertex[1-9][0-9]*", vertex)
            and isinstance(mapped, str) and mapped and not target.Shape.isNull(),
            "stale_reference", "The selected vertex is unavailable")
    try:
        require(target.Shape.getElementMappedName(vertex) == mapped, "stale_reference", "The selected vertex changed")
        point = target.Shape.getElement(vertex).Point
    except VertexExtrusionError:
        raise
    except Exception as error:
        raise VertexExtrusionError("stale_reference", "The selected vertex changed") from error
    boundary(source)
    direction = source.getGlobalPlacement().Rotation.multVec(App.Vector(0, 0, 1))
    distance = (point - source.getGlobalPlacement().Base).dot(direction)
    require(abs(distance) > 1e-7, "invalid_geometry", "The vertex lies in the sketch plane")
    return pad, source, source_body, target, vertex, mapped, distance, direction


def update(doc, edit, owner_body, effective_visibility):
    pad, source, source_body, target, vertex, mapped, distance, direction = prepare_update(
        doc, edit, owner_body, effective_visibility)
    original_tip = source_body.Tip
    plane = doc.getObject(edit["plane"])
    plane.AttachmentSupport = [(source.AttachmentSupport[0][0], [""]), (target, [vertex])]
    pad.Reversed = distance < 0
    doc.recompute()
    distances = [(v.Point - source.getGlobalPlacement().Base).dot(direction) for v in pad.Shape.Vertexes]
    require(source_body.Tip == original_tip and feature_metadata(pad, owner_body) is not None
            and pad.isValid() and pad.Shape.isValid() and len(pad.Shape.Solids) == 1 and distances
            and abs(min(distances) - min(0., distance)) < 1e-6
            and abs(max(distances) - max(0., distance)) < 1e-6,
            "invalid_model", "The edited extrusion is not valid at the selected vertex")
    return {"object": pad.Name, "vertex_mapping": mapped}


def owned_origin(body):
    """Check the native origin objects that Body deletion also removes."""
    try:
        origin = body.Origin
        children = list(origin.OriginFeatures)
        kinds = [obj.TypeId for obj in children]
        valid = (origin is not None and origin.TypeId == "App::Origin"
                 and origin.Document == body.Document
                 and body.Document.getObject(origin.Name) is origin
                 and set(origin.InList) == {body}
                 and len(set(children)) == 7 and len(children) == 7
                 and kinds.count("App::Line") == 3 and kinds.count("App::Plane") == 3
                 and kinds.count("App::Point") == 1
                 and set(origin.OutList) == set(children)
                 and all(obj.Document == body.Document
                         and body.Document.getObject(obj.Name) is obj
                         and not obj.OutList
                         and origin in obj.InList
                         and all(parent in (origin, body) for parent in obj.InList)
                         for obj in children))
    except Exception:
        valid = False
    require(valid, "dependent_objects", "Native origin ownership or dependents prevent removing the Body")
    return origin, children


def remove_owned(doc, pad, owner_body):
    """Remove the whole created part only when no other native owner uses it."""
    owned = owned_helpers(pad, owner_body)
    if owned is None:
        require(not declares_helpers(pad, owner_body), "dependent_objects",
                "The extrusion helper ownership is incomplete")
        return False
    body, binder, plane = owned
    helpers = (binder, plane) if plane is not None else (binder,)
    require(body.Tip == pad and set(body.Group) == {*helpers, pad}
            and body.getParentGeoFeatureGroup() is None,
            "dependent_objects", "The new Body contains other native history or a parent")
    for obj, allowed in [(pad, {body}), *((helper, {body, pad}) for helper in helpers)]:
        require(all(item in allowed for item in obj.InList), "dependent_objects",
                "Native dependents prevent removing the extrusion")
    require(not body.InList, "dependent_objects", "Native dependents prevent removing the Body")
    owned_origin(body)
    for obj in (pad, *reversed(helpers), body):
        doc.removeObject(obj.Name)
    return True
