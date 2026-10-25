"""Direct stock lost-placement/map controls and a generic legacy fixture."""
import argparse,json
from pathlib import Path
import FreeCAD as A
import Part,PartDesign,Sketcher
p=argparse.ArgumentParser();p.add_argument('--references',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();out=a.output;out.mkdir(parents=True,exist_ok=False)
assert A.Version()[:3]==['1','1','4'] and Part.OCC_VERSION=='7.8.1'
def follow(d):
    f=d.Follower
    return dict(valid=f.isValid(),placement=list(f.Placement.Base)+list(f.Placement.Rotation.Q),geometry=[g.Content for g in f.Geometry],
        external=[g.Content for g in f.ExternalGeo],ids=[f.getGeometryId(i) for i in range(f.GeometryCount)],
        external_links=[(o.Name,sub) for o,sub in f.ExternalGeometry],
        support=[(o.Name,list(s)) for o,s in f.AttachmentSupport])
report={}
try:
    d=A.newDocument('GenericLegacySuppression');b=d.addObject('PartDesign::Body','Body')
    for name,size,pos in [('Base',[10,10,10],[0,0,0]),('A',[2,2,4],[1,1,10]),('B',[3,3,3],[6,6,10])]:
        obj=b.newObject('PartDesign::AdditiveBox',name);obj.Length,obj.Width,obj.Height=size;obj.Placement.Base=A.Vector(*pos);d.recompute()
    original=d.Body.Shape.copy();placement=list(d.A.Placement.Base)+list(d.A.Placement.Rotation.Q)
    d.A.Suppressed=True;d.recompute();legacy=out/'legacy.FCStd';d.saveAs(str(legacy));A.closeDocument(d.Name)
    d=A.openDocument(str(legacy));d.A.Suppressed=False;d.recompute()
    delta=d.Body.Shape.cut(original).Volume+original.cut(d.Body.Shape).Volume
    assert list(d.A.Placement.Base)+list(d.A.Placement.Rotation.Q)!=placement and delta>0
    report['stock_lost_placement']=dict(csg=delta,actual=list(d.A.Placement.Base),expected=placement)
    A.closeDocument(d.Name)
    d=A.openDocument(str(a.references/'fillet-restored.FCStd'));expected=follow(d);shape=d.Body.Shape.copy();A.closeDocument(d.Name)
    d=A.openDocument(str(a.references/'FilletFalse-native-before.FCStd'));d.Fillet.Suppressed=False;d.recompute();d.Follower.solve();d.recompute();actual=follow(d)
    assert actual!=expected,'stock suppressed fresh Open must reproduce the native persistent-reference loss'
    report['stock_lost_map']=dict(expected=expected,actual=actual,body_csg=d.Body.Shape.cut(shape).Volume+shape.cut(d.Body.Shape).Volume)
    A.closeDocument(d.Name)
    (out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
finally:
    for name in list(A.listDocuments()):A.closeDocument(name)
