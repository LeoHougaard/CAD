"""Combined Link display/selection/measurement gate, run after assembly integration.

Generate independent native controls through the opaque configured launcher; use
the public host to check native topology/mesh frames and witness points. Optional
--editor-tests runs the actual Editor/viewport picking gate from that merged
source's test binary. No assembly candidate module is imported or copied here.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys


def native_fixture(folder):
    import FreeCAD as A
    import Part
    out=Path(folder);out.mkdir()
    d=A.newDocument('MeasurementScene')
    outer=d.addObject('App::Part','Outer')
    outer.Placement=A.Placement(A.Vector(100,20,30),A.Rotation(A.Vector(0,0,1),30))
    inner=d.addObject('App::Part','Inner');outer.addObject(inner)
    inner.Placement=A.Placement(A.Vector(12,-5,7),A.Rotation(A.Vector(1,0,0),20))
    box=d.addObject('Part::Box','Box');inner.addObject(box)
    box.Length=7;box.Width=11;box.Height=13
    box.Placement=A.Placement(A.Vector(3,4,5),A.Rotation(A.Vector(0,1,0),25))
    instance=d.addObject('App::Link','Instance');inner.addObject(instance);instance.setLink(box)
    instance.LinkPlacement=A.Placement(A.Vector(100,60,-9),A.Rotation(A.Vector(0,0,1),75))
    instance.LinkTransform=True
    chained=d.addObject('App::Link','Chained');outer.addObject(chained);chained.setLink(instance)
    chained.LinkPlacement=A.Placement(A.Vector(-75,-90,90),A.Rotation(A.Vector(1,0,1),55))
    false_instance=d.addObject('App::Link','FalseInstance');outer.addObject(false_instance);false_instance.setLink(box)
    false_instance.LinkPlacement=A.Placement(A.Vector(100,-90,15),A.Rotation(A.Vector(1,1,0),40))
    false_instance.LinkTransform=False
    scaled_links=[]
    for name,scale,vector,position in [
        ('UniformScale',2,None,(-100,150,30)),
        ('ReflectedScale',-1,None,(180,200,90)),
        ('VectorScale',1,(1.5,0.7,2),(90,-180,80)),
    ]:
        link=d.addObject('App::Link',name);outer.addObject(link);link.setLink(box)
        link.LinkTransform=True
        link.LinkPlacement=A.Placement(A.Vector(*position),A.Rotation(A.Vector(1,2,3),35))
        if vector is None:link.Scale=scale
        else:link.ScaleVector=A.Vector(*vector)
        scaled_links.append(link)
    d.recompute()
    for link,factor in zip(scaled_links,(8,1,2.1)):
        assert link.Shape.isValid() and not link.Shape.isNull(),link.Name
        assert math.isclose(abs(link.Shape.Volume),box.Shape.Volume*factor,rel_tol=1e-10),(link.Name,link.Shape.Volume)
    d.recompute()
    assert A.Version()[:3]==['1','1','4'];assert Part.OCC_VERSION=='7.8.1'
    records=[]
    for obj in (box,instance,chained,false_instance,*scaled_links):
        parent=obj.getParentGeoFeatureGroup()
        parent_frame=parent.getGlobalPlacement() if parent else A.Placement()
        world_shape=obj.Shape.transformed(parent_frame.toMatrix())
        records.append(dict(object=obj.Name,subelement='Vertex1',point=list(world_shape.Vertexes[0].Point),
            scale=getattr(obj,'Scale',None),scale_vector=list(obj.ScaleVector) if hasattr(obj,'ScaleVector') else None,
            native_volume=world_shape.Volume,
            world_vertices=[list(vertex.Point) for vertex in world_shape.Vertexes],
            world_from_shape=dict(position=list(parent_frame.Base),rotation=list(parent_frame.Rotation.Q)),
            global_placement=dict(position=list(parent_frame.multiply(obj.Placement).Base),rotation=list(parent_frame.multiply(obj.Placement).Rotation.Q))))
    # Independent analytic control includes the source Box Placement because
    # LinkTransform is true. Do not derive the expected answer from getSubObject.
    analytic=outer.Placement.multiply(inner.Placement).multiply(instance.LinkPlacement).multiply(box.Placement).multVec(Part.makeBox(7,11,13).Vertexes[0].Point)
    assert math.dist(list(analytic),records[1]['point'])<1e-8
    d.saveAs(str(out/'scene.FCStd'))
    (out/'controls.json').write_text(json.dumps(dict(version=A.Version(),occt=Part.OCC_VERSION,vertices=records),indent=2))
    A.closeDocument(d.Name)


def transform(p,placement):
    x,y,z,w=placement['rotation'];a,b,c=p
    tx,ty,tz=2*(y*c-z*b),2*(z*a-x*c),2*(x*b-y*a)
    return [a+w*tx+y*tz-z*ty+placement['position'][0],
            b+w*ty+z*tx-x*tz+placement['position'][1],
            c+w*tz+x*ty-y*tx+placement['position'][2]]


def host_proof(host,runtime,folder,check_frames):
    from native_workflows import Session
    control=json.loads((folder/'fixture/controls.json').read_text());captures=[];frames=[]
    session=Session(host,runtime)
    try:
        state=session.call('open',path=str(folder/'fixture/scene.FCStd'));doc=state['document']
        def read(refs):
            before=session.call('inspect',document=doc)
            measured=session.call('measure',document=doc,expected_revision=before['revision'],selections=refs)
            assert session.call('inspect',document=doc)==before
            captures.append(measured);return measured
        for vertex in control['vertices']:
            obj=vertex['object'];ref=dict(object=obj,subelement=vertex['subelement'])
            measured=read([ref])['selections'][0]['center']['value']
            assert math.dist(measured,vertex['point'])<2e-7,(obj,measured,vertex['point'])
            topology=session.call('topology',document=doc,object=obj)
            mesh=session.call('mesh',document=doc,object=obj)
            native_vertex=next(v for v in topology['vertices'] if v['subelement']==vertex['subelement'])
            topology_world=transform(native_vertex['point'],topology['world_from_shape'])
            mesh_world=transform(native_vertex['point'],mesh['world_from_shape'])
            # Check the actual display mesh too, rather than only applying its
            # transform to an independently supplied topology point. These
            # planar box fixtures tessellate to their native corner vertices.
            mesh_points=[transform(point,mesh['world_from_shape']) for point in mesh['positions']]
            assert mesh_points and mesh['triangles'],obj
            expected_vertices=vertex['world_vertices']
            mesh_vertex_error=max(min(math.dist(point,expected) for expected in expected_vertices) for point in mesh_points)
            native_vertex_error=max(min(math.dist(point,actual) for actual in mesh_points) for point in expected_vertices)
            observations=dict(object=obj,measurement=measured,native_control=vertex['point'],
                topology_world=topology_world,mesh_world=mesh_world,
                topology_error_mm=math.dist(topology_world,measured),mesh_error_mm=math.dist(mesh_world,measured),
                actual_mesh_vertex_error_mm=mesh_vertex_error,native_vertex_coverage_error_mm=native_vertex_error,
                global_placement=next(o for o in state['objects'] if o['name']==obj).get('global_placement'))
            frames.append(observations)
        pair=control['vertices'][1:3]
        measured=read([dict(object=v['object'],subelement=v['subelement']) for v in pair])
        expected=math.dist(pair[0]['point'],pair[1]['point'])
        assert math.isclose(measured['distance']['metric']['value'],expected,abs_tol=2e-7)
        assert measured['distance']['witnesses'],'Native vertex-pair distance requires witnesses'
        for witnesses in measured['distance']['witnesses']:
            for actual,expected_point in zip(witnesses,pair):assert math.dist(actual['value'],expected_point['point'])<2e-7
        session.call('close',document=doc)
    finally:session.close()
    (folder/'host-captures.json').write_text(json.dumps(captures,indent=2))
    (folder/'frame-observations.json').write_text(json.dumps(frames,indent=2))
    frame_pass=all(max(f['topology_error_mm'],f['mesh_error_mm'],f['actual_mesh_vertex_error_mm'],f['native_vertex_coverage_error_mm'])<2e-7 and f['global_placement'] is not None for f in frames)
    report=dict(native_measurement_passed=True,read_only=True,native_reads=len(captures),
                frame_gate_requested=check_frames,frame_gate_passed=frame_pass,
                editor_display_selection_gate_passed=False)
    (folder/'host-report.json').write_text(json.dumps(report,indent=2))
    if check_frames:assert frame_pass,'P1 gate: mesh/topology/global frames must match native measurements; inspect frame-observations.json'
    return report


def run(args):
    out=Path(args.output).resolve();out.mkdir(exist_ok=False)
    runtime=json.loads(Path(args.runtime).read_text(encoding='utf-8-sig'))
    flags=subprocess.BELOW_NORMAL_PRIORITY_CLASS if os.name=='nt' else 0
    script=Path(__file__).resolve()
    with (out/'fixture.log').open('x',encoding='utf-8') as log:
        subprocess.run([runtime['executable'],*runtime['args'],str(script),'--native-fixture',str(out/'fixture')],check=True,stdout=log,stderr=subprocess.STDOUT,creationflags=flags)
    report=host_proof(Path(args.host).resolve(),runtime,out,not args.measure_only)
    if args.editor_tests:
        assert not args.measure_only,'The combined Editor gate requires native frame checks too'
        env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',CAD_MEASUREMENT_RUNTIME=str(Path(args.runtime).resolve()),CAD_MEASUREMENT_OUTPUT=str(out/'editor'),CAD_MEASUREMENT_SCENE_FIXTURE=str(out/'fixture/scene.FCStd'),CAD_MEASUREMENT_SCENE_CONTROLS=str(out/'fixture/controls.json'))
        cmd=[str(Path(args.editor_tests).resolve()),'editor::measurements::tests::scene_tests::native_measure_link_scene_selection_and_witness_frames','--exact','--ignored','--nocapture','--test-threads=1']
        with (out/'editor.log').open('x',encoding='utf-8') as log:p=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,creationflags=flags)
        report['editor_display_selection_gate_passed']=p.returncode==0
        (out/'editor-run.json').write_text(json.dumps(dict(command=cmd,exit_code=p.returncode,environment={k:v for k,v in env.items() if k.startswith('CAD_MEASUREMENT_')}),indent=2))
        assert p.returncode==0,'Combined Editor gate failed; inspect editor.log'
    report['promotable_combined_gate']=report['frame_gate_passed'] and report['editor_display_selection_gate_passed']
    report['artifacts']={str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest() for p in out.rglob('*') if p.is_file()}
    (out/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps({k:v for k,v in report.items() if k!='artifacts'}))


if __name__=='__main__':
    if len(sys.argv)==3 and sys.argv[1]=='--native-fixture':native_fixture(sys.argv[2])
    else:
        p=argparse.ArgumentParser();p.add_argument('--runtime',required=True);p.add_argument('--host',required=True);p.add_argument('--output',required=True);p.add_argument('--editor-tests');p.add_argument('--measure-only',action='store_true');run(p.parse_args())
