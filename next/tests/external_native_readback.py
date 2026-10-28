"""Independent direct-native saved-file and affine controls. No worker imports."""
import hashlib,json,math,os,sys
from pathlib import Path
import FreeCAD as App
import Part,PartDesign,Sketcher,Assembly,JointObject,UtilsAssembly
R=Path(os.environ['CAD_EXTERNAL_ROOT']); out=R/sys.argv[1];out.mkdir()
native=R/sys.argv[2]; guards=R/sys.argv[3]; editor=R/sys.argv[4]
fixtures=R/'fixtures2';manifest=json.loads((fixtures/'manifest.json').read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def close_all():
    for name in list(App.listDocuments()):App.closeDocument(name)
def near(a,b,tol=2e-7):assert abs(a-b)<tol,(a,b)
def world(obj,sub=''):
    parent=obj.getParentGeoFeatureGroup();initial=parent.getGlobalPlacement().toMatrix() if parent else App.Matrix()
    return obj.getSubObject(sub,0,initial)
def compare(a,b):
    delta=a.cut(b).Volume+b.cut(a).Volume
    near(delta,0);near(a.Volume,b.Volume);near(a.Area,b.Area)
    return dict(symmetric_difference=delta,volume=a.Volume,area=a.Area)
results={}
for label,path in [('native-final',native/'Final.FCStd'),('native-reopened',native/'Reopened.FCStd'),('editor',editor/'ExternalEditorSaved.FCStd')]:
    doc=App.openDocument(str(path)); entries=[]
    for link in doc.Objects:
        if link.TypeId!='App::Link' or 'CadExternalSource' not in link.PropertiesList:continue
        value=json.loads(link.CadExternalSource);source=link.LinkedObject
        assert source is not None
        assert str(source.Document.Uid)==value['document_uid'] and source.ID==value['object_id'] and source.Name==value['object'] and source.TypeId==value['type']
        assert sha(Path(value['path']))==value['sha256']
        entries.append(dict(link=link.Name,id=link.ID,source_uid=str(source.Document.Uid),source_id=source.ID,
                            source_file=source.Document.FileName,shape_valid=link.Shape.isValid(),volume=link.Shape.Volume))
        assert link.Shape.isValid()
    if label.startswith('native'):
        assert isinstance(doc.Joint.Proxy,JointObject.Joint) and isinstance(doc.Ground.Proxy,JointObject.GroundedJoint)
        assert doc.Joint.Reference1[0] is doc.Instance and doc.Joint.Reference2[0] is doc.Second
        assert doc.Ground.ObjectToGround is doc.Instance
        control=App.newDocument('IndependentControl');control.saveAs(str(out/(label+'-control.FCStd')))
        assembly=control.addObject('Assembly::AssemblyObject','Assembly');assembly.Type='Assembly'
        group=assembly.newObject('Assembly::JointGroup','Joints')
        assembly.Placement=App.Placement(App.Vector(70,80,90),App.Rotation(App.Vector(0,0,1),90))
        for name in ['Instance','Second','AffineNested']:
            link=assembly.newObject('App::Link',name);link.setLink(doc.getObject(name).LinkedObject)
            link.LinkPlacement=App.Placement(App.Vector(20,4,3),App.Rotation())
        ground=group.newObject('App::FeaturePython','Ground');JointObject.GroundedJoint(ground,control.Instance)
        joint=group.newObject('App::FeaturePython','Joint');JointObject.Joint(joint,JointObject.JointTypes.index('Fixed'))
        joint.Proxy.setJointConnectors(joint,[(control.Instance,['','']),(control.Second,['Face6','Face6'])])
        control.recompute();assert assembly.solve()==0;control.recompute()
        matched={name:compare(world(doc.getObject(name)),world(control.getObject(name))) for name in ['Instance','Second','AffineNested']}
        affine=json.loads((native/'affine.json').read_text());native_vertex=world(control.AffineNested,'Vertex1').Point
        measured=affine['measure']['selections'][0]['center']['value']
        for a,b in zip(measured,native_vertex):near(a,b)
        frame=affine['mesh']['world_from_shape'];plc=App.Placement(App.Vector(*frame['position']),App.Rotation(*frame['rotation']))
        vertex=next(v for v in affine['topology']['vertices'] if v['subelement']=='Vertex1')
        rendered=plc.multVec(App.Vector(*vertex['point']))
        for a,b in zip(rendered,native_vertex):near(a,b)
        world_mesh=[plc.multVec(App.Vector(*p)) for p in affine['mesh']['positions']]
        bound=world(control.AffineNested).BoundBox
        for axis,low,high in [('x',bound.XMin,bound.XMax),('y',bound.YMin,bound.YMax),('z',bound.ZMin,bound.ZMax)]:
            near(min(getattr(p,axis) for p in world_mesh),low);near(max(getattr(p,axis) for p in world_mesh),high)
        results[label]=dict(entries=entries,matched=matched,affine_vertex=list(native_vertex),render_topology_measure_equal=True)
    else:
        assert len(entries)==1;near(entries[0]['volume'],648);results[label]=dict(entries=entries)
    close_all()
# Independent native Hole control restores the authored custom inputs through
# native setters, touches the source and recomputes. No production helper used.
doc=App.openDocument(str(fixtures/'hole-stale.FCStd'));hole=doc.Hole
hole.HoleCutCustomValues=True;hole.HoleCutDiameter=9;hole.HoleCutDepth=2;hole.Depth=8
hole.touch();doc.recompute();assert hole.Shape.isValid()
observed=json.loads((guards/'result.json').read_text())['hole_volume'];near(hole.Shape.Volume,observed)
results['hole']=dict(volume=hole.Shape.Volume,custom_diameter=hole.HoleCutDiameter.Value,custom_depth=hole.HoleCutDepth.Value,depth=hole.Depth.Value)
close_all()
assert all(sha(fixtures/n)==v for n,v in manifest['files'].items())
assert {e['link']:e['id'] for e in results['native-final']['entries']}=={e['link']:e['id'] for e in results['native-reopened']['entries']}
results['all_source_bytes_unchanged']=True
(out/'results.json').write_text(json.dumps(results,indent=2));print('independent native graph, saved geometry and affine controls passed')
