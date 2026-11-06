"""PROSPECTIVE REAL NATIVE TEST: UNRUN. Run only in an authorized FreeCAD runtime.

No compiler, GUI or worker is launched here. No input document is saved.
Optional argv[1] is the completed R297/run2 FCStd (verified SHA below).
Worker preview/lease/revision and GUI qualification remain separate gates.
"""
import hashlib
import json
import math
from pathlib import Path
import sys
import FreeCAD as App
import Part
import Sketcher
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import planar_regions as R


def qualify(doc, source):
    body = R.owner(source)
    originals = {o.Name:o.Shape.exportBrepToString() for o in doc.Objects
                 if o.TypeId != 'PartDesign::Body' and 'Shape' in o.PropertiesList and not o.Shape.isNull()}
    before = R.catalog(source)
    outside = [r for r in before['regions'] if json.loads(r['key'])['boundary']['outer'] == []]
    assert len(outside) == 1
    support = R.support_face(source)
    assert math.isclose(sum(r['area'] for r in before['regions']), support[2].Area, rel_tol=1e-10)
    # Force tessellation again, checking exact unchanged original BRep bytes.
    assert R.catalog(source) == before
    assert all(doc.getObject(n).Shape.exportBrepToString() == b for n,b in originals.items())
    names = set(o.Name for o in doc.Objects)
    doc.openTransaction('Support bounded profile and stock Pad')
    R.apply(doc, dict(op='create_planar_region_profile', name='SupportRegion', source=source.Name,
        source_id=source.ID, body=body.Name, body_id=body.ID,
        regions=[outside[0]['key']], source_token=before['source_token']))
    doc.recompute()
    profile = doc.getObject('SupportRegion')
    assert profile.isValid() and profile.Shape.isValid() and profile.RegionVersion == 2
    assert math.isclose(profile.Shape.Area, outside[0]['area'], rel_tol=1e-10)
    assert profile.Placement == source.Placement
    pad = body.newObject('PartDesign::Pad', 'SupportPad')
    pad.Profile = profile
    pad.Length = 3
    doc.recompute()
    assert pad.isValid() and pad.Shape.isValid() and len(pad.Shape.Solids) == 1
    assert all(doc.getObject(n).Shape.exportBrepToString() == b for n,b in originals.items())
    doc.commitTransaction()
    result = pad.Shape.exportBrepToString()
    doc.undo(); doc.recompute()
    assert set(o.Name for o in doc.Objects) == names
    doc.redo(); doc.recompute()
    assert doc.getObject('SupportPad').Shape.exportBrepToString() == result
    return {'regions':len(before['regions']), 'outside_area':outside[0]['area'], 'stock_pad':True}


def fixture():
    doc = App.newDocument('SupportFaceRegionsProspective')
    body = doc.addObject('PartDesign::Body', 'Body')
    base = body.newObject('PartDesign::AdditiveBox', 'Base')
    base.Length = 120; base.Width = 90; base.Height = 12
    doc.recompute()
    face_index = max(range(1,len(base.Shape.Faces)+1), key=lambda i:base.Shape.Faces[i-1].CenterOfMass.z)
    source = body.newObject('Sketcher::SketchObject', 'Source')
    source.AttachmentSupport = [(base, ['Face'+str(face_index)])]
    source.MapMode = 'FlatFace'; doc.recompute()
    local = base.Shape.Faces[face_index-1].copy().transformed(matrix=source.Placement.inverse().toMatrix())
    x,y = local.CenterOfMass.x,local.CenterOfMass.y
    radius = min(local.BoundBox.XLength,local.BoundBox.YLength)/10
    source.addGeometry(Part.Circle(App.Vector(x,y,0), App.Vector(0,0,1), radius), False)
    source.addGeometry(Part.Circle(App.Vector(x,y,0), App.Vector(0,0,1), radius/2), False)
    # Independent internal polygon plus a harmless open tail.
    points = [(x+2*radius,y),(x+3*radius,y),(x+3*radius,y+radius),(x+2*radius,y+radius)]
    for a,b in zip(points,points[1:]+points[:1]):
        source.addGeometry(Part.LineSegment(App.Vector(*a,0),App.Vector(*b,0)),False)
    source.addGeometry(Part.LineSegment(App.Vector(x,y+2*radius,0),App.Vector(x+radius,y+2*radius,0)),False)
    doc.recompute()
    return doc,source


if __name__ == '__main__':
    doc = None
    try:
        if len(sys.argv) > 1:
            path = Path(sys.argv[1])
            assert hashlib.sha256(path.read_bytes()).hexdigest() == '407af624f18f7acc3aff38d239bacc481cab60640e8b6c7d4d38fa8437944bfb'
            doc = App.openDocument(str(path.resolve()))
            source = doc.getObject('Sketch29')
        else:
            doc,source = fixture()
        print(json.dumps(qualify(doc,source), sort_keys=True))
    finally:
        if doc is not None: App.closeDocument(doc.Name)
