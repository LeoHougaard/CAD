"""Native Part::Helix authoring. No proxy, cached shape or recompute observer.

Two root SubShapeBinders retain mapped native face/edge references. Native
expressions derive cylindrical radius and height from their measured shapes.
The native attachment engine owns the axis and frame.
"""
import math
import re
import FreeCAD as App
import Part
import document_units as DocumentUnits

class HelixError(ValueError):
    code = "invalid_helix_reference"


def require(test, message):
    if not test:
        raise HelixError(message)


def initialize(obj):
    obj.addProperty("App::PropertyFloat", "Turns", "Helix", "Number of revolutions")
    obj.Turns = 1.0
    obj.addProperty("App::PropertyAngle", "StartAngle", "Helix", "Start angle around the reference axis")
    obj.StartAngle = 0.0


def reference(doc, edit):
    source = doc.getObject(edit.get("source", ""))
    require(source is not None and str(source.ID) == edit.get("source_id"),
            "Cylinder source identity changed; select it again")
    sub = edit.get("subelement", "")
    require(re.fullmatch(r"(?:Face|Edge)[1-9][0-9]*", sub) is not None,
            "Select a cylindrical face or a complete circular edge")
    require(edit.get("mapped_name") and source.Shape.getElementMappedName(sub) == edit["mapped_name"],
            "Selected native topology changed; select it again")
    shape = source.Shape
    if sub.startswith("Face"):
        face = shape.getElement(sub)
        require(isinstance(face.Surface, Part.Cylinder), "Select a cylindrical face")
    else:
        edge = shape.getElement(sub)
        require(isinstance(edge.Curve, Part.Circle) and edge.isClosed(), "Select a complete circular edge")
        faces = [f for f in shape.ancestorsOfType(edge, Part.Face) if isinstance(f.Surface, Part.Cylinder)]
        require(len(faces) == 1, "Circular edge must bound exactly one cylindrical face")
        face = faces[0]
    circles = [e for e in face.Edges if not e.Degenerated and isinstance(e.Curve, Part.Circle) and e.isClosed()]
    require(len(circles) == 2, "Reference requires a complete, finite cylindrical side face")
    axis = face.Surface.Axis
    circles.sort(key=lambda e: e.Curve.Center.dot(axis))
    edge = circles[0]
    radius = edge.Curve.Radius
    height = abs((circles[1].Curve.Center - edge.Curve.Center).dot(axis))
    require(radius > 1e-7 and height > 1e-7 and
            math.isclose(face.Area, 2 * math.pi * radius * height, rel_tol=1e-9, abs_tol=1e-7),
            "Reference must be a full cylindrical side face")
    face_key = next("Face" + str(i) for i, f in enumerate(shape.Faces, 1) if f.isSame(face))
    edge_key = next("Edge" + str(i) for i, e in enumerate(shape.Edges, 1) if e.isSame(edge))
    require(all(shape.getElementMappedName(k) for k in (face_key, edge_key)), "Reference has no mapped native identity")
    return source, face_key, edge_key, edge.Curve.Axis.dot(axis) < 0


def preflight(doc, edits):
    for edit in edits:
        if edit.get("op") == "set" and edit.get("property") in ("Turns", "StartAngle"):
            obj = doc.getObject(edit.get("object", ""))
            created = any(e.get("op") == "create_standalone_helix" and e.get("name") == edit.get("object") for e in edits)
            if created or (obj is not None and obj.TypeId == "Part::Helix"):
                value = DocumentUnits.value(edit["value"], "" if edit["property"] == "Turns" else "deg")
                require(math.isfinite(value) and (edit["property"] != "Turns" or 0 < value <= 10000),
                        "Turns must be positive and at most 10000; angle must be finite")
        if edit.get("op") == "create_standalone_helix":
            reference(doc, edit)
            keys = [edit.get(k) for k in ("name", "face_binder", "edge_binder")]
            require(len(set(keys)) == 3 and all(isinstance(k, str) and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", k) for k in keys),
                    "Choose distinct native feature names")


def create(doc, edit):
    source, face_key, edge_key, reversed_axis = reference(doc, edit)
    keys = [edit[k] for k in ("name", "face_binder", "edge_binder")]
    require(not any(doc.getObject(k) for k in keys), "Helix dependency name is already in use")
    binders = []
    for key, sub in zip(keys[1:], (face_key, edge_key)):
        binder = doc.addObject("PartDesign::SubShapeBinder", key)
        binder.Support = [(source, [sub])]
        binder.Visibility = False
        binders.append(binder)
    doc.recompute()
    obj = doc.addObject("Part::Helix", keys[0])
    initialize(obj)
    obj.Style = "New style"
    obj.AttachmentSupport = [(binders[1], ["Edge1"])]
    obj.MapMode = "Concentric"
    obj.MapReversed = reversed_axis
    obj.AttachmentOffset = App.Placement(App.Vector(), App.Rotation(App.Vector(0, 0, 1), 1))
    obj.setExpression("AttachmentOffset.Rotation.Angle", "StartAngle")
    obj.addProperty("App::PropertyLink", "CylinderFace", "Helix reference")
    obj.CylinderFace = binders[0]
    obj.setEditorMode("CylinderFace", 1)
    # TopoShape's Python Area and Length attributes are unitless mm^2/mm.
    obj.setExpression("Radius", keys[2] + ".Shape.Length / (2 * pi) * 1 mm")
    obj.setExpression("Height", keys[1] + ".Shape.Area / " + keys[2] + ".Shape.Length * 1 mm")
    obj.setExpression("Pitch", "Height / Turns")
    return {"object": obj.Name}


def validate(obj):
    if obj.TypeId == "Part::Helix" and "Turns" in obj.PropertiesList:
        require(obj.CylinderFace is not None and len(obj.CylinderFace.Shape.Faces) == 1
                and isinstance(obj.CylinderFace.Shape.Faces[0].Surface, Part.Cylinder),
                "Helix reference is no longer cylindrical")
        require(math.isfinite(obj.Turns) and 0 < obj.Turns <= 10000,
                "Turns must be positive and at most 10000")
