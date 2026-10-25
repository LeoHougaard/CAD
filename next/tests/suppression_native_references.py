"""Native profile/dress-up suppression with live persistent followers."""
import json
from pathlib import Path
import sys
import FreeCAD as App
import Part
import Sketcher

here=Path(__file__).resolve().parent
sys.path.insert(0,str(here.parent/'backend'))
from suppression import register
out=Path(sys.argv[1]);out.mkdir();observer=register()
report={}


def check(d):
    assert d.recompute() is not False
    observer.check()
    for name in ['Base','Sketch','Pad','Fillet','Tail','Follower','Body']:
        obj=d.getObject(name)
        assert obj.isValid(),(name,obj.getStatusString())


def create(name,repaired):
    d=App.newDocument(name);d.UndoMode=1
    if repaired:observer.attach(d)
    b=d.addObject('PartDesign::Body','Body')
    base=b.newObject('PartDesign::AdditiveBox','Base');base.Length=20;base.Width=20;base.Height=10;d.recompute()
    sk=b.newObject('Sketcher::SketchObject','Sketch')
    for a,z in [((0,0),(4,0)),((4,0),(4,4)),((4,4),(0,4)),((0,4),(0,0))]:
        sk.addGeometry(Part.LineSegment(App.Vector(*a,0),App.Vector(*z,0)))
    sk.Placement.Base=App.Vector(1,1,10);sk.setExpression('Placement.Base.z','Base.Height')
    pad=b.newObject('PartDesign::Pad','Pad');pad.Profile=sk;pad.Length=6;d.recompute()
    edge=max([(i,e) for i,e in enumerate(pad.Shape.Edges,1) if e.BoundBox.ZLength>1],key=lambda item:item[1].CenterOfMass.z)[0]
    fillet=b.newObject('PartDesign::Fillet','Fillet');fillet.Base=(pad,['Edge'+str(edge)]);fillet.Radius=.5;d.recompute()
    assert fillet.isValid(),fillet.getStatusString()
    tail=b.newObject('PartDesign::AdditiveBox','Tail');tail.Length=3;tail.Width=3;tail.Height=3
    tail.Placement.Base=App.Vector(15,15,10);d.recompute()
    follower=d.addObject('Sketcher::SketchObject','Follower')
    face=min(enumerate(fillet.Shape.Faces,1),key=lambda item:item[1].CenterOfMass.x)[0]
    follower.AttachmentSupport=[(fillet,['Face'+str(face)])];follower.MapMode='FlatFace'
    follower.addGeometry(Part.LineSegment(App.Vector(),App.Vector(1,0,0)))
    follower.addExternal('Fillet','Edge1');check(d)
    return d


def follower_state(d):
    f=d.Follower
    return dict(placement=list(f.Placement.Base)+list(f.Placement.Rotation.Q),
                ids=[f.getGeometryId(i) for i in range(f.GeometryCount)],
                support=[(o.Name,list(subs)) for o,subs in f.AttachmentSupport],
                external=[(o.Name,sub) for o,sub in f.ExternalGeometry],
                expressions=list(d.Sketch.ExpressionEngine),
                geometry=[g.Content for g in f.Geometry],external_geometry=[g.Content for g in f.ExternalGeo])


try:
    for target in ['Fillet','Pad']:
        control_shape=None;control_follow=None
        for repaired in [False,True]:
            d=create(target+str(repaired),repaired)
            d.openTransaction('Suppress profile/dress-up')
            d.Fillet.Suppressed=True
            if target=='Pad':d.Pad.Suppressed=True
            check(d);d.commitTransaction()
            d.openTransaction('Native parameter update')
            if target=='Pad':d.Base.Height=12
            else:d.Fillet.Radius=.75
            check(d);d.commitTransaction()
            path_before=out/(target+str(repaired)+'-native-before.FCStd');d.saveAs(str(path_before))
            (out/(target+str(repaired)+'-before.json')).write_text(json.dumps(follower_state(d),indent=2))
            if repaired:
                path=out/(target+'.FCStd');d.saveAs(str(path));observer.detach(d);App.closeDocument(d.Name)
                d=App.openDocument(str(path));d.UndoMode=1;observer.attach(d)
                for obj in d.Objects:
                    if obj.isDerivedFrom('Sketcher::SketchObject'):assert obj.solve()==0
                check(d)
            (out/(target+str(repaired)+'-after-open.json')).write_text(json.dumps(follower_state(d),indent=2))
            d.openTransaction('Restore profile/dress-up')
            if target=='Pad':d.Pad.Suppressed=False
            if target=='Fillet':d.Fillet.Suppressed=False
            check(d);d.commitTransaction()
            if repaired:
                delta=d.Body.Shape.cut(control_shape).Volume+control_shape.cut(d.Body.Shape).Volume
                assert delta==0,delta
                assert follower_state(d)==control_follow,(follower_state(d),control_follow)
                state=follower_state(d)
                with observer.history():d.undo();d.recompute()
                check(d)
                with observer.history():d.redo();d.recompute()
                check(d);assert follower_state(d)==state
                if target=='Fillet':
                    assert '_CadSuppressionShape' not in d.Fillet.PropertiesList
                    assert '_CadSuppressionPlacement' not in d.Fillet.PropertiesList
                    final_path=out/'fillet-restored.FCStd';d.saveAs(str(final_path))
                    observer.detach(d);App.closeDocument(d.Name)
                    d=App.openDocument(str(final_path));observer.attach(d)
                    for obj in d.Objects:
                        if obj.isDerivedFrom('Sketcher::SketchObject'):assert obj.solve()==0
                    check(d);assert follower_state(d)==state
                    assert d.Body.Shape.cut(control_shape).Volume+control_shape.cut(d.Body.Shape).Volume==0
                report[target]=dict(native_shape_difference=delta,follower_exact=True,native_history=True)
            else:
                control_shape=d.Body.Shape.copy();control_follow=follower_state(d)
            observer.detach(d);App.closeDocument(d.Name)
    (out/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
finally:
    App.removeDocumentObserver(observer)
    for key in list(App.listDocuments()):App.closeDocument(key)
