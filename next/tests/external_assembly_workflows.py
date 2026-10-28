"""Public Rust host/compiler lifecycle; every reply retained without filtering."""
import copy, hashlib, json, os, sys
from pathlib import Path
R=Path(os.environ['CAD_EXTERNAL_ROOT'])
sys.path.insert(0,str(Path(__file__).resolve().parent))
from preview_workflows import PreviewSession
from native_workflows import objects, near
from assembly_workflows import compile_command
from external_recorded_session import RecordedSession
out=R/sys.argv[1];out.mkdir()
fixtures=R/'fixtures2'
runtime=json.loads((R/'runtime.json').read_text());host=Path(os.environ['CAD_EXTERNAL_HOST']);compiler=Path(os.environ['CAD_EXTERNAL_COMPILER'])
session=RecordedSession(host,runtime,out,'worker1'); transcript=[]
def call(op,**args):
    if 'document' in args: args.setdefault('expected_revision',session.revisions.get(args['document']))
    try:
        result=session.call(op,**args)
        transcript.append(dict(op=op,parameters=args,result=result));return result
    finally:
        (out/'transcript.json').write_text(json.dumps(transcript,indent=2))
def inspect(doc):return call('inspect',document=doc)
def command(doc,op,**args):
    edits=compile_command(compiler,inspect(doc),dict(op=op,**args))
    return call('transaction',document=doc,label=op,edits=edits)['state']
def save(doc,name):return call('save',document=doc,path=str(out/name))
def source(doc,path='component.FCStd',obj='Block'):
    data=call('assembly_sources',document=doc,path=str(fixtures/path))
    return next(o['source'] for o in data['objects'] if o['name']==obj)
def insertion(value,name='Instance'):
    return dict(op='assembly_insert',assembly='Assembly',name=name,external_source=value,
                placement=dict(position=[20,4,3],rotation=[0,0,0,1]))
def tx(doc,edits,**kw):return call('transaction',document=doc,label='external test',edits=edits,**kw)
def equal_history(a,b):
    assert {k:a[k] for k in ('objects','undo_count','redo_count','revision')}=={k:b[k] for k in ('objects','undo_count','redo_count','revision')}
try:
    a=call('new',name='RootA')['document'];b=call('new',name='RootB')['document']
    tx(b,[dict(op='add_object',type='Part::Box',name='ForeignBox')]);foreign=inspect(b)
    call('assembly_sources',document=a,path=str(fixtures/'component.FCStd'),expect_error='assembly_save_required')
    save(a,'RootA.FCStd');save(b,'RootB.FCStd');foreign=inspect(b)
    command(a,'assembly_create',name='Assembly',joints='Joints')
    baseline=inspect(a);v1=source(a);equal_history(baseline,inspect(a));equal_history(foreign,inspect(b))
    call('assembly_sources',document=a,expected_revision=baseline['revision']-1,path=str(fixtures/'component.FCStd'),expect_error='stale_revision')
    command(a,**insertion(v1));equal_history(foreign,inspect(b))
    assert objects(inspect(a))['Instance']['instance_state']['local'] is False
    command(a,**insertion(v1,'Second'))
    nested=source(a,obj='Nested');command(a,**insertion(nested,'AffineNested'))
    # Native world frame, rendering, topology and BRep measurement all use the
    # root's actual affine Link graph, not a source-placement approximation.
    tx(a,[dict(op='set',object='Assembly',property='Placement',value=dict(position=[70,80,90],rotation=[0,0,2**-.5,2**-.5]))])
    mesh=call('mesh',document=a,object='AffineNested');topology=call('topology',document=a,object='AffineNested')
    measure=call('measure',document=a,selections=[dict(object='AffineNested',subelement='Vertex1')])
    (out/'affine.json').write_text(json.dumps(dict(mesh=mesh,topology=topology,measure=measure),indent=2))
    command(a,'assembly_ground',assembly='Assembly',object='Instance',name='Ground',grounded=True)
    command(a,'assembly_joint',assembly='Assembly',name='Joint',create=True,joint_type='Fixed',reference1={'link':'Instance','subelements':['','']},reference2={'link':'Second','subelements':['Face6','Face6']},properties={})
    # Shared source remains readonly and unregistered as a writable root.
    state=inspect(a);source_doc=objects(state)['Instance']['instance_state']['source_document']
    call('transaction',document=source_doc,expected_revision=0,edits=[dict(op='set',object='Block',property='Length',value=999)],expect_error='not_found')
    call('save',document=a,path=str(fixtures/'component.FCStd'),expect_error='source_ownership')
    # Accepted replacement, rejected replacement and Cancel retain native link identities.
    placement=dict(op='assembly_placement',assembly='Assembly',object='Second',placement=dict(position=[30,5,6],rotation=[0,0,0,1]))
    before=inspect(a)
    call('preview',document=a,token='external',label='External placement',edits=[placement])
    call('assembly_sources',document=a,path=str(fixtures/'revision.FCStd'),expect_error='preview_active')
    call('assembly_sources',document=b,path=str(fixtures/'revision.FCStd'),expect_error='preview_active')
    bad=copy.deepcopy(placement);bad['placement']['rotation']=[0,0,0,0]
    call('preview',document=a,token='external',label='External placement',edits=[bad],expect_error='invalid_assembly')
    inspect(a);call('preview_cancel',document=a,token='external')
    assert objects(inspect(a))==objects(before)
    equal_history(foreign,inspect(b))
    # Exact UUID/ID check, explicit update and root document history.
    candidate=insertion(v1,'PreviewInstance');preview_base=inspect(a)
    call('preview',document=a,token='insert',label='External insertion preview',edits=[candidate])
    candidate['placement']['position']=[50,12,4]
    call('preview',document=a,token='insert',label='External insertion preview',edits=[candidate])
    assert objects(inspect(a))['PreviewInstance']['properties']['LinkPlacement']['position']==[50.,12.,4.]
    call('preview_cancel',document=a,token='insert');assert objects(inspect(a))==objects(preview_base)
    v2=source(a,'revision.FCStd');wrong=source(a,'wrong.FCStd')
    relink=dict(op='assembly_relink',assembly='Assembly',object='Instance',external_source=wrong,solve_joints=True)
    tx(a,[relink],expect_error='source_identity');inspect(a)
    relink['external_source']=v2;relink['solve_joints']=False
    baseline=inspect(a);tx(a,[relink],expect_error='assembly_update_mode_unavailable');equal_history(baseline,inspect(a))
    relink['solve_joints']=True;tx(a,[relink]);after=inspect(a)
    near(objects(after)['Instance']['shape']['volume'],18*9*4)
    assert objects(after)['Second']['instance_state']['external_source']==v2
    call('undo',document=a);near(objects(inspect(a))['Instance']['shape']['volume'],12*6*4)
    call('redo',document=a);near(objects(inspect(a))['Instance']['shape']['volume'],18*9*4)
    call('undo',document=a)
    tx(a,[dict(op='add_object',type='Part::Box',name='BranchFeature')]);assert inspect(a)['redo_count']==0
    tx(a,[relink]);near(objects(inspect(a))['Instance']['shape']['volume'],18*9*4)
    # Native link object and joint references remain identical across relink.
    for name in ['Instance','Second','Joint','Ground']:
        assert objects(after)[name]['type']==objects(before)[name]['type']
    assert objects(after)['Joint']['properties']['Reference1']==objects(before)['Joint']['properties']['Reference1']
    command(b,'assembly_create',name='Assembly',joints='Joints');tx(b,[insertion(v2)])
    shared=inspect(b);close_a=save(a,'Final.FCStd');equal_history(shared,inspect(b));call('close',document=a)
    near(objects(inspect(b))['Instance']['shape']['volume'],18*9*4)
    assert call('mesh',document=b,object='Instance')['positions']
    call('close',document=b)
    # Fresh process, not just closing and reopening within one worker.
    session.close();session=RecordedSession(host,runtime,out,'worker2')
    a=call('open',path=str(out/'Final.FCStd'))['document']
    tx(a,[dict(op='add_object',type='Part::Box',name='SaveAsHistoryMarker')]);call('undo',document=a)
    assert inspect(a)['redo_count']==1
    (out/'SaveAs').mkdir();save(a,'SaveAs/Portable.FCStd');assert inspect(a)['redo_count']==1
    call('redo',document=a);assert 'SaveAsHistoryMarker' in objects(inspect(a))
    call('undo',document=a);save(a,'SaveAs/Portable.FCStd');call('close',document=a)
    a=call('open',path=str(out/'SaveAs/Portable.FCStd'))['document']
    near(objects(inspect(a))['Instance']['shape']['volume'],18*9*4)
    command(a,'assembly_placement',assembly='Assembly',object='Second',placement=dict(position=[30,0,0],rotation=[0,0,0,1]))
    save(a,'Reopened.FCStd')
    # Every external source is byte-identical to its generic immutable fixture.
    manifest=json.loads((fixtures/'manifest.json').read_text())
    assert all(hashlib.sha256((fixtures/n).read_bytes()).hexdigest()==sha for n,sha in manifest['files'].items())
    call('close',document=a)
    (out/'result.json').write_text(json.dumps(dict(passed=True,physical_gui=False,full_assembly_parity=False),indent=2))
    print('external native lifecycle passed')
finally:
    session.close()
