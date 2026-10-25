"""Defer shipped joint proxies without changing native creation or fresh reopen."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def native_create(output, backend):
    import FreeCAD as A
    import Part
    sys.path.insert(0, str(backend))
    import assembly as adapter
    assert 'JointObject' not in sys.modules
    d=A.newDocument('LazyAssembly')
    try:
        box=d.addObject('Part::Box','Source');d.recompute()
        outside=d.addObject('App::Link','Outside');outside.setLink(box);d.recompute()
        assert adapter.metadata(outside)=={}
        adapter.link_world_from_shape(outside)
        adapter.apply(d,dict(op='assembly_create',name='Assembly',joints='Joints'))
        for name,x in [('First',0),('Second',40)]:
            adapter.apply(d,dict(op='assembly_insert',assembly='Assembly',name=name,source='Source',
                placement=dict(position=[x,0,0],rotation=[0,0,0,1])))
        d.recompute()
        assert adapter.metadata(d.Assembly)['assembly_state']['joint_types']==list(adapter.JOINT_TYPES)
        assert 'JointObject' not in sys.modules,'Parts, Link frames and empty assembly metadata must not load joint proxies'
        adapter.apply(d,dict(op='assembly_ground',assembly='Assembly',object='First',name='Ground',grounded=True))
        assert 'JointObject' in sys.modules
        native=adapter.joint_module()
        assert list(adapter.JOINT_TYPES)==list(native.JointTypes)
        adapter.apply(d,dict(op='assembly_joint',assembly='Assembly',name='Joint',create=True,joint_type='Fixed',
            reference1=dict(link='First',subelements=['','']),reference2=dict(link='Second',subelements=['','']),properties={}))
        d.recompute();adapter.solve_document(d)
        assert isinstance(d.Joint.Proxy,native.Joint)
        assert isinstance(d.Ground.Proxy,native.GroundedJoint)
        assert (d.First.Placement.Base-d.Second.Placement.Base).Length<1e-8
        assert d.First.Shape.isValid() and abs(d.First.Shape.Volume-box.Shape.Volume)<1e-8
        d.saveAs(str(output/'Assembly.FCStd'))
        (output/'native-create.json').write_text(json.dumps(dict(passed=True,joint_types=list(native.JointTypes),source_volume=box.Shape.Volume),indent=2))
    finally:A.closeDocument(d.Name)


def native_reopen(output, backend):
    import FreeCAD as A
    sys.path.insert(0,str(backend))
    import assembly as adapter
    assert 'JointObject' not in sys.modules
    d=A.openDocument(str(output/'Assembly.FCStd'))
    try:
        assert 'JointObject' in sys.modules,'Native FCStd proxy restoration must load the shipped module'
        native=adapter.joint_module()
        assert isinstance(d.Joint.Proxy,native.Joint)
        assert isinstance(d.Ground.Proxy,native.GroundedJoint)
        assert d.Ground.ObjectToGround==d.First
        assert d.Joint.Reference1==(d.First,['',''])
        assert d.Joint.Reference2==(d.Second,['',''])
        assert adapter.solve_document(d) is not False
        (output/'native-reopen.json').write_text(json.dumps(dict(passed=True,proxy_restored=True),indent=2))
    finally:A.closeDocument(d.Name)


def run(args):
    from preview_workflows import PreviewSession
    output=args.output.resolve();output.mkdir(parents=True,exist_ok=False)
    runtime=json.loads(args.runtime.read_text(encoding='utf-8-sig'))
    backend=Path(runtime['script']).resolve().parent
    assert backend==Path(__file__).resolve().parents[1]/'backend'
    env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',FREECAD_USER_HOME=str(output/'config'))
    for phase in ['create','reopen']:
        command=[runtime['executable'],*runtime['args'],str(Path(__file__).resolve()),'--native-'+phase,str(output),str(backend)]
        with (output/(phase+'.log')).open('x',encoding='utf-8') as log:
            subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT,check=True,
                **({'creationflags':subprocess.BELOW_NORMAL_PRIORITY_CLASS} if os.name=='nt' else {}))
    session=PreviewSession(args.host.resolve(),runtime)
    try:
        types=json.loads((output/'native-create.json').read_text())['joint_types']
        assert session.capabilities['assembly']['joint_types']==types
        state=session.call('open',path=str(output/'Assembly.FCStd'));doc=state['document']
        result=session.call('transaction',document=doc,label='Edit freshly restored native joint',edits=[dict(
            op='assembly_joint',assembly='Assembly',name='Joint',create=False,joint_type='Fixed',
            reference1=dict(link='First',subelements=['','']),reference2=dict(link='Second',subelements=['','']),
            properties=dict(Offset1=dict(position=[0,0,3],rotation=[0,0,0,1])))])
        (output/'host-edit.json').write_text(json.dumps(result,indent=2))
        session.call('undo',document=doc);session.call('redo',document=doc)
        session.call('save',document=doc,path=str(output/'Edited.FCStd'))
        session.call('close',document=doc)
    finally:session.close()
    (output/'report.json').write_text(json.dumps(dict(passed=True,parts_defer_joint_proxies=True,native_types_match=True,
        fresh_native_reopen=True,fresh_worker_reopen_edit_history=True),indent=2))
    print('Native lazy joint loading, capability parity and fresh-process restore/edit passed.')


if __name__=='__main__':
    if len(sys.argv)==4 and sys.argv[1].startswith('--native-'):
        {'--native-create':native_create,'--native-reopen':native_reopen}[sys.argv[1]](Path(sys.argv[2]),Path(sys.argv[3]))
    else:
        p=argparse.ArgumentParser();p.add_argument('--host',required=True,type=Path);p.add_argument('--runtime',required=True,type=Path);p.add_argument('--output',required=True,type=Path);run(p.parse_args())
