"""Matched native measurements, independent controls, read/gesture invariants.

Run with --host --runtime --output. The runtime executable/args are opaque.
The control process uses native Shapes and explicit native placement composition,
without importing the production measurement module or requesting display meshes.
"""
import argparse
import json
import math
import os
from pathlib import Path
import subprocess
import sys


def ref(obj, subelement="", **extra):
    return dict(object=obj, subelement=subelement, **extra)


def controls(output):
    import FreeCAD as A
    import Part
    import Sketcher
    import PartDesign
    d=A.newDocument('MeasurementFixture')
    outer=d.addObject('App::Part','Outer')
    outer.Placement=A.Placement(A.Vector(100,20,30),A.Rotation(A.Vector(0,0,1),30))
    inner=d.addObject('App::Part','Inner');outer.addObject(inner)
    inner.Placement=A.Placement(A.Vector(12,-5,7),A.Rotation(A.Vector(1,0,0),20))
    body=d.addObject('PartDesign::Body','Body');inner.addObject(body)
    body.Placement=A.Placement(A.Vector(2,3,4),A.Rotation(A.Vector(0,1,0),15))
    box=body.newObject('PartDesign::AdditiveBox','Box');box.Length=25.4;box.Width=17;box.Height=11
    box.setExpression('Height','Width * 11 / 17')
    link=d.addObject('App::Link','Instance');inner.addObject(link);link.setLink(body)
    link.LinkPlacement=A.Placement(A.Vector(80,40,-10),A.Rotation(A.Vector(0,0,1),70))
    transformed_link=d.addObject('App::Link','TransformedInstance');inner.addObject(transformed_link);transformed_link.setLink(body)
    transformed_link.LinkTransform=True
    transformed_link.LinkPlacement=A.Placement(A.Vector(-80,40,15),A.Rotation(A.Vector(1,1,0),40))
    chained_link=d.addObject('App::Link','ChainedInstance');outer.addObject(chained_link);chained_link.setLink(transformed_link)
    chained_link.LinkPlacement=A.Placement(A.Vector(0,90,15),A.Rotation(A.Vector(0,0,1),130))
    cylinder=d.addObject('Part::Cylinder','Cylinder');cylinder.Radius=4;cylinder.Height=18
    cylinder.Placement.Base=A.Vector(-30,20,0)
    sphere=d.addObject('Part::Sphere','Sphere');sphere.Radius=6;sphere.Placement.Base=A.Vector(15,-20,10)
    sketch=d.addObject('Sketcher::SketchObject','Sketch');inner.addObject(sketch)
    sketch.Placement=A.Placement(A.Vector(1,2,3),A.Rotation(A.Vector(0,1,0),35))
    sketch.addGeometry(Part.LineSegment(A.Vector(0,0,0),A.Vector(12,0,0)),False)
    sketch.addConstraint(Sketcher.Constraint('Horizontal',0))
    sketch.addGeometry(Part.Circle(A.Vector(4,5,0),A.Vector(0,0,1),3),True)
    leaf=d.addObject('Part::Box','Leaf');leaf.Placement.Base=A.Vector(250,0,0)
    d.recompute()
    assert A.Version()[:3]==['1','1','4'];assert Part.OCC_VERSION=='7.8.1'
    # Independent controls start with native Shape, which already contains its
    # own Placement. Apply its parent's native global placement once.
    def world(obj):
        parent=obj.getParentGeoFeatureGroup()
        return obj.Shape.transformed(parent.getGlobalPlacement().toMatrix()) if parent else obj.Shape.copy()
    box_shape=world(box);body_shape=world(body);link_shape=world(link)
    cyl_shape=world(cylinder);sphere_shape=world(sphere)
    circle_edge=next(e for e in cyl_shape.Edges if not e.Degenerated and isinstance(e.Curve,Part.Circle))
    circle_name='Edge'+str(next(i for i,e in enumerate(cyl_shape.Edges,1) if e.isSame(circle_edge)))
    curved_face=next((i,f) for i,f in enumerate(cyl_shape.Faces,1) if isinstance(f.Surface,Part.Cylinder))
    plane_face=next((i,f) for i,f in enumerate(cyl_shape.Faces,1) if isinstance(f.Surface,Part.Plane))
    bs=Part.BSplineCurve();bs.interpolate([A.Vector(0,0,0),A.Vector(5,4,2),A.Vector(11,0,3)])
    spline=d.addObject('Part::Feature','Spline');spline.Shape=bs.toShape()
    d.recompute()
    cases=[]
    def add(name, references, shapes):
        case=dict(name=name,selections=references,expected=[])
        for s in shapes:
            kind=s.ShapeType
            m={}
            if kind!='Vertex':m['length']=s.Length
            if s.Faces:m['area']=s.Area
            if s.Solids:m['volume']=s.Volume
            g=s.Curve if kind=='Edge' and not s.Degenerated else s.Surface if kind=='Face' else None
            if isinstance(g,(Part.Circle,Part.Cylinder,Part.Sphere)):
                m.update(radius=g.Radius,diameter=2*g.Radius)
            expected=dict(shape_type=kind,metrics=m,center=list(s.Point if kind=='Vertex' else s.CenterOfGravity))
            if g is not None:
                expected['geometry_type']=g.TypeId
                if hasattr(g,'Center'):expected['geometry_center']=list(g.Center)
                if hasattr(g,'Axis'):expected['axis']=list(g.Axis)
            if kind=='Edge' and not s.Degenerated and isinstance(s.Curve,Part.Line):expected['direction']=list(s.Curve.Direction)
            if kind=='Face' and isinstance(s.Surface,Part.Plane):expected['direction']=list(s.normalAt(0,0))
            case['expected'].append(expected)
        if len(shapes)==2:
            dist,pairs,_=shapes[0].distToShape(shapes[1])
            case['distance']=dist;case['witnesses']=[[list(a),list(b)] for a,b in pairs]
            directions=[e.get('direction') for e in case['expected']]
            if all(directions):
                angle=A.Vector(*directions[0]).getAngle(A.Vector(*directions[1]))
                kinds=[s.ShapeType for s in shapes]
                if kinds[0]!=kinds[1]:angle=abs(math.pi/2-angle)
                case['angle']=math.degrees(angle)
        cases.append(case)
    add('whole transformed feature',[ref('Box')],[box_shape])
    add('whole transformed body',[ref('Body')],[body_shape])
    add('whole nested Part',[ref('Inner')],[inner.Shape.transformed(outer.getGlobalPlacement().toMatrix())])
    add('whole Link',[ref('Instance')],[link_shape])
    add('LinkTransform true',[ref('TransformedInstance','Face1')],[world(transformed_link).Faces[0]])
    add('Link to Link',[ref('ChainedInstance','Edge1')],[world(chained_link).Edges[0]])
    add('nested path face',[ref('Outer','Inner.Body.Box.Face1')],[box_shape.Faces[0]])
    add('nested Link face',[ref('Outer','Inner.Instance.Face1')],[link_shape.Faces[0]])
    add('vertex',[ref('Box','Vertex1')],[box_shape.Vertexes[0]])
    add('straight edge',[ref('Box','Edge1')],[box_shape.Edges[0]])
    add('planar face',[ref('Box','Face1')],[box_shape.Faces[0]])
    add('circle',[ref('Cylinder',circle_name)],[circle_edge])
    add('cylinder',[ref('Cylinder','Face'+str(curved_face[0]))],[curved_face[1]])
    add('sphere',[ref('Sphere','Face1')],[sphere_shape.Faces[0]])
    add('spline',[ref('Spline','Edge1')],[spline.Shape])
    add('objects distance',[ref('Box'),ref('Cylinder')],[box_shape,cyl_shape])
    add('face edge',[ref('Box','Face1'),ref('Cylinder','Edge1')],[box_shape.Faces[0],cyl_shape.Edges[0]])
    add('face vertex',[ref('Box','Face1'),ref('Sphere','Vertex1')],[box_shape.Faces[0],sphere_shape.Vertexes[0]])
    add('edge vertex',[ref('Box','Edge1'),ref('Sphere','Vertex1')],[box_shape.Edges[0],sphere_shape.Vertexes[0]])
    add('line directions',[ref('Box','Edge1'),ref('Box','Edge2')],[box_shape.Edges[0],box_shape.Edges[1]])
    add('plane normals',[ref('Box','Face1'),ref('Cylinder','Face'+str(plane_face[0]))],[box_shape.Faces[0],plane_face[1]])
    add('line to plane',[ref('Box','Edge1'),ref('Cylinder','Face'+str(plane_face[0]))],[box_shape.Edges[0],plane_face[1]])
    add('curved angle unavailable',[ref('Cylinder','Face'+str(curved_face[0])),ref('Box','Face1')],[curved_face[1],box_shape.Faces[0]])
    add('touching zero distance',[ref('Box','Face1'),ref('Box','Edge1')],[box_shape.Faces[0],box_shape.Edges[0]])
    add('Link witness distance',[ref('Instance','Face1'),ref('Body','Vertex1')],[link_shape.Faces[0],body_shape.Vertexes[0]])
    add('transformed sketch edge',[ref('Sketch',sketch_geometry=dict(index=0,point=0))],[sketch.Geometry[0].toShape().transformed(sketch.getGlobalPlacement().toMatrix())])
    add('construction circle',[ref('Sketch',sketch_geometry=dict(index=1,point=0))],[sketch.Geometry[1].toShape().transformed(sketch.getGlobalPlacement().toMatrix())])
    add('sketch native point',[ref('Sketch',sketch_geometry=dict(index=0,point=2))],[Part.Vertex(sketch.getPoint(0,2)).transformed(sketch.getGlobalPlacement().toMatrix())])
    add('sketch origin',[ref('Sketch',sketch_geometry=dict(index=-1,point=1))],[Part.Vertex(sketch.getPoint(-1,1)).transformed(sketch.getGlobalPlacement().toMatrix())])
    output=Path(output);output.mkdir(exist_ok=True)
    d.saveAs(str(output/'standard.FCStd'));d.UnitSystem=3;d.saveAs(str(output/'imperial.FCStd'))
    # The quantities use the document native unit schema. No preference change.
    quantity_control={}
    for key,unit in [('length','mm'),('area','mm^2'),('volume','mm^3'),('radius','mm'),('diameter','mm')]:
        v=cases[0]['expected'][0]['metrics'].get(key,25.4)
        quantity_control[key]=A.Units.schemaTranslate(A.Units.Quantity(v,A.Units.Unit(unit)),3)[0]
    (output/'controls.json').write_text(json.dumps(dict(version=A.Version(),occt=Part.OCC_VERSION,cases=cases,imperial_display=quantity_control),indent=2))
    A.closeDocument(d.Name)


def run(args):
    from native_workflows import Session
    output=Path(args.output);output.mkdir(exist_ok=False)
    runtime=json.loads(Path(args.runtime).read_text(encoding='utf-8-sig'))
    with (output/'control.log').open('x',encoding='utf-8') as log:
        c=subprocess.run([runtime['executable'],*runtime['args'],str(Path(__file__).resolve()),'--control',str(output/'controls')],stdout=log,stderr=subprocess.STDOUT)
    assert c.returncode==0, (output/'control.log').read_text()
    control=json.loads((output/'controls/controls.json').read_text())
    session=Session(Path(args.host),runtime);captures=[];comparisons=0
    def near(a,b):
        nonlocal comparisons
        comparisons+=1
        if isinstance(a,list):
            assert len(a)==len(b)
            for x,y in zip(a,b):near(x,y)
        else:assert math.isclose(a,b,rel_tol=2e-9,abs_tol=2e-7),(a,b)
    def measure(selections,expected_error=None,**extra):
        before=session.call('inspect',document=doc)
        result=session.call('measure',document=doc,expected_revision=before['revision'],selections=selections,expect_error=expected_error,**extra)
        after=session.call('inspect',document=doc)
        assert before==after,'Read changed revision, history, geometry or native owner'
        captures.append(dict(selections=selections,result=result,before=before['revision'],after=after['revision']))
        return result
    try:
        state=session.call('open',path=str(output/'controls/standard.FCStd'));doc=state['document']
        for case in control['cases']:
            result=measure(case['selections'])
            for actual,expected in zip(result['selections'],case['expected']):
                assert actual['shape_type']==expected['shape_type'],case['name']
                assert {m['key'] for m in actual['metrics']}==set(expected['metrics']),case['name']
                for m in actual['metrics']:near(m['value'],expected['metrics'][m['key']])
                near(actual['center']['value'],expected['center'])
                for key in ('geometry_center','axis','direction'):
                    if key in expected:near(actual[key]['value'] if key=='geometry_center' else actual[key],expected[key])
                if 'geometry_type' in expected:assert actual['geometry_type']==expected['geometry_type']
            if 'distance' in case:
                near(result['distance']['metric']['value'],case['distance'])
                # Native solutions can have several equivalent closest pairs.
                # Compare the independent native witness set, not display pixels.
                for pair in result['distance']['witnesses']:
                    values=[x['value'] for x in pair]
                    assert any(all(math.dist(a,b)<2e-7 for a,b in zip(values,w)) for w in case['witnesses']),(case['name'],values)
                    near(math.dist(*values),case['distance'])
                if 'angle' in case:near(result['angle']['value'],case['angle'])
                else:assert 'angle' not in result and result['angle_unavailable']
        for refs,code in [([], 'invalid_request'),([ref('Box')]*3,'invalid_request'),([ref('Box')]*2,'invalid_request'),([ref('Gone')],'not_found'),([ref('Outer','Inner.Gone.Face1')],'not_found'),([ref('Instance','Gone.Edge1')],'not_found'),([ref('Box','Edge999999')],'not_found'),([ref('Sketch',sketch_geometry=dict(index=9999,point=0))],'unsupported_reference'),([ref('Sketch',sketch_geometry=dict(index=0,point=3))],'unsupported_reference'),([ref('Sketch',sketch_geometry=dict(index=-1,point=0))],'unsupported_reference')]:
            measure(refs,code)
        before=session.call('inspect',document=doc)
        session.call('measure',document=doc,expected_revision=before['revision']-1,selections=[ref('Box')],expect_error='stale_revision')
        session.call('measure',document=doc,selections=[ref('Box')],expect_error='missing_revision')
        assert session.call('inspect',document=doc)==before
        # Deleted references fail without borrowing a new same-numbered edge.
        session.call('transaction',document=doc,label='Delete leaf',edits=[dict(op='remove_object',object='Leaf')])
        measure([ref('Leaf')],'not_found');session.call('undo',document=doc)
        # Redo stack and preview lease survive successful and failed read calls.
        redo=session.call('inspect',document=doc)['redo_count'];assert redo>0
        measure([ref('Box')]);assert session.call('inspect',document=doc)['redo_count']==redo
        session.call('redo',document=doc)
        state=session.call('preview',document=doc,expected_revision=session.revisions[doc],token='measurement-preview',label='Native read candidate',edits=[dict(op='set',object='Box',property='Length',value=35)])['state']
        m=measure([ref('Box')]);near(next(x['value'] for x in m['selections'][0]['metrics'] if x['key']=='volume'),35*17*11)
        assert m['preview']['token']=='measurement-preview';measure([ref('Gone')],'not_found')
        session.call('transaction',document=doc,label='Owner guard still active',edits=[dict(op='set',object='Box',property='Length',value=2)],expect_error='preview_active')
        session.call('preview_cancel',document=doc,expected_revision=session.revisions[doc],token='measurement-preview')
        session.call('sketch_begin',document=doc,expected_revision=session.revisions[doc],object='Sketch',token='measurement-sketch')
        measure([ref('Sketch',sketch_geometry=dict(index=0,point=0))])
        measure([ref('Box')],'stale_context')
        session.call('transaction',document=doc,sketch_token='measurement-sketch',label='Sketch action',edits=[dict(op='move_geometry',object='Sketch',index=0,point=2,position=[20,0,0])])
        m=measure([ref('Sketch',sketch_geometry=dict(index=0,point=0))]);near(m['selections'][0]['metrics'][0]['value'],20)
        measure([ref('Sketch',sketch_geometry=dict(index=999,point=1))],'unsupported_reference')
        session.call('sketch_cancel',document=doc,expected_revision=session.revisions[doc],token='measurement-sketch')
        # Native save/reopen and continued edits use the same stable names.
        reopen_refs=[ref('Outer','Inner.Instance.Face1'),ref('Box','Vertex1')]
        before_reopen=measure(reopen_refs)
        session.call('save',document=doc,path=str(output/'saved.FCStd'));session.call('close',document=doc)
        doc=session.call('open',path=str(output/'saved.FCStd'))['document'];after_reopen=measure(reopen_refs)
        for key in ('selections','distance','angle','angle_unavailable','unit_system'):assert before_reopen.get(key)==after_reopen.get(key),key
        session.call('close',document=doc)
        doc=session.call('open',path=str(output/'controls/imperial.FCStd'))['document']
        m=measure([ref('Box')]);assert 'Imperial decimal' in m['unit_system']
        for metric in m['selections'][0]['metrics']:assert metric['display']==control['imperial_display'][metric['key']],metric
        session.call('save',document=doc,path=str(output/'imperial-reopened.FCStd'));session.call('close',document=doc)
        doc=session.call('open',path=str(output/'imperial-reopened.FCStd'))['document'];reopened=measure([ref('Box')]);assert reopened['unit_system']==m['unit_system'];assert reopened['selections']==m['selections']
        session.call('close',document=doc)
    finally:
        session.close()
        (output/'captures.json').write_text(json.dumps(captures,indent=2))
    report=dict(passed=True,controls=len(control['cases']),scalar_comparisons=comparisons,reads=len(captures),versions=dict(freecad=control['version'],occt=control['occt']),limits='Native/headless only; no real GUI or full parity claim')
    (output/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report))


if __name__=='__main__':
    if len(sys.argv)==3 and sys.argv[1]=='--control':controls(sys.argv[2])
    else:
        p=argparse.ArgumentParser();p.add_argument('--host',required=True);p.add_argument('--runtime',required=True);p.add_argument('--output',required=True);run(p.parse_args())
