"""Generic direct-native immutable source fixtures, no candidate adapter."""
import hashlib, json, math, sys, zipfile
from pathlib import Path
import FreeCAD as App
import Part, PartDesign, Sketcher, Assembly, JointObject
out = Path(sys.argv[1]); out.mkdir()
doc = App.newDocument('Component')
box = doc.addObject('Part::Box','Block'); box.Length=12; box.Width=6; box.Height=4
box.setExpression('Width','Length / 2')
sketch=doc.addObject('Sketcher::SketchObject','ConstrainedSketch')
sketch.addGeometry(Part.Circle(App.Vector(2,3,0),App.Vector(0,0,1),2),False)
sketch.addConstraint(Sketcher.Constraint('Diameter',0,4))
sketch.renameConstraint(0,'Diameter')
nested = doc.addObject('App::Link','Nested'); nested.setLink(box)
nested.LinkTransform=True; nested.LinkPlacement=App.Placement(App.Vector(7,8,9),App.Rotation(App.Vector(0,0,1),25))
nested.ScaleVector=App.Vector(1.2,0.8,1.5)
ass=doc.addObject('Assembly::AssemblyObject','SourceAssembly');ass.Type='Assembly'
group=ass.newObject('Assembly::JointGroup','SourceJoints')
part=ass.newObject('App::Link','SourcePart');part.setLink(box)
ground=group.newObject('App::FeaturePython','SourceGround');JointObject.GroundedJoint(ground,part)
doc.recompute()
doc.saveAs(str(out/'component.FCStd'))
identity=dict(document_uid=str(doc.Uid),object_id=box.ID,nested_id=nested.ID)
box.Length=18;doc.recompute();doc.saveAs(str(out/'revision.FCStd'))
App.closeDocument(doc.Name)
# A matching label/name does not establish native identity.
wrong=App.newDocument('Component');b=wrong.addObject('Part::Box','Block');wrong.recompute();wrong.saveAs(str(out/'wrong.FCStd'));App.closeDocument(wrong.Name)
# Independent native Hole with serialized custom inputs and an intentionally
# stale cached shape. Loaders must regenerate rather than trust that cache.
doc=App.newDocument('HoleSource');body=doc.addObject('PartDesign::Body','Body')
base=body.newObject('PartDesign::AdditiveBox','Base');base.Length=40;base.Width=30;base.Height=15
sketch=body.newObject('Sketcher::SketchObject','Profile');sketch.Placement.Base=App.Vector(0,0,15)
sketch.addGeometry(Part.Circle(App.Vector(10,10,0),App.Vector(0,0,1),2),False)
sketch.addConstraint(Sketcher.Constraint('Diameter',0,4))
doc.recompute()
hole=body.newObject('PartDesign::Hole','Hole');hole.Profile=sketch;hole.Diameter=4;hole.Depth=5
hole.HoleCutType='Counterbore';hole.HoleCutCustomValues=True;hole.HoleCutDiameter=9;hole.HoleCutDepth=2
doc.recompute(); assert hole.isValid() and hole.Shape.isValid(),hole.getStatusString()
doc.saveAs(str(out/'hole.FCStd')); App.closeDocument(doc.Name)
def edit_archive(src,dst,transform):
    with zipfile.ZipFile(src) as z,zipfile.ZipFile(dst,'w',zipfile.ZIP_DEFLATED) as target:
        for n in z.namelist():
            data=z.read(n)
            if n=='Document.xml':data=transform(data.decode()).encode()
            target.writestr(n,data)
import xml.etree.ElementTree as ET
def depth(value):
    def change(text):
        xml=ET.fromstring(text);xml.find("./ObjectData/Object[@name='Hole']/Properties/Property[@name='Depth']/Float").set('value',str(value));return ET.tostring(xml,encoding='unicode')
    return change
edit_archive(out/'hole.FCStd',out/'hole-stale.FCStd',depth(8))
edit_archive(out/'hole.FCStd',out/'hole-invalid.FCStd',depth(-5))
def broken(text): return text.replace('JointObject','MissingAssemblyProxyModule')
edit_archive(out/'component.FCStd',out/'broken-proxy.FCStd',broken)
files={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in out.glob('*.FCStd')}
(out/'manifest.json').write_text(json.dumps(dict(files=files,identity=identity),indent=2))
print(json.dumps(dict(files=list(files),identity=identity)))
