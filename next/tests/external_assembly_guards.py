"""Disk races, native restored-source admission and exact foreign isolation."""
import hashlib,json,os,sys
from pathlib import Path
R=Path(os.environ['CAD_EXTERNAL_ROOT']);sys.path.insert(0,str(Path(__file__).resolve().parent))
from preview_workflows import PreviewSession
from native_workflows import objects,near
from external_recorded_session import RecordedSession
out=R/sys.argv[1];out.mkdir();fixtures=R/'fixtures2'
runtime=json.loads((R/'runtime.json').read_text());runtime['script']=str(Path(__file__).resolve().parent/'fixtures/external_observed_worker.py')
os.environ['CAD_EXTERNAL_NATIVE_TRACE']=str(out/'native-trace.ndjson')
s=RecordedSession(Path(os.environ['CAD_EXTERNAL_HOST']),runtime,out,'worker');events=[]
def call(op,**kw):
    if 'document' in kw:kw.setdefault('expected_revision',s.revisions.get(kw['document']))
    result=s.call(op,**kw);events.append(dict(op=op,parameters=kw,result=result))
    (out/'events.json').write_text(json.dumps(events,indent=2));return result
def inspect(d):return call('inspect',document=d)
def tx(d,edits,**kw):return call('transaction',document=d,label='source guard',edits=edits,**kw)
def browse(d,p,obj='Block'):
    data=call('assembly_sources',document=d,path=str(p));return next(o['source'] for o in data['objects'] if o['name']==obj)
def insert(value,name='Instance'):return dict(op='assembly_insert',assembly='Assembly',name=name,external_source=value)
def h(p):return hashlib.sha256(p.read_bytes()).hexdigest()
try:
    a=call('new',name='Root')['document'];call('save',document=a,path=str(out/'Root.FCStd'))
    tx(a,[dict(op='assembly_create',name='Assembly',joints='Joints')])
    b=call('new',name='Foreign')['document'];tx(b,[dict(op='add_object',type='Part::Box',name='Box')]);call('undo',document=b)
    foreign=inspect(b)
    editable=call('open',path=str(fixtures/'component.FCStd'))['document']
    tx(editable,[dict(op='set',object='Block',property='Length',value=14)])
    dirty_source=inspect(editable);base=inspect(a)
    call('assembly_sources',document=a,path=str(fixtures/'component.FCStd'),expect_error='source_ownership')
    assert inspect(editable)==dirty_source;assert inspect(a)==base;assert inspect(b)==foreign
    call('close',document=editable)
    tx(a,[dict(op='add_object',type='Sketcher::SketchObject',name='LeaseSketch')])
    call('sketch_begin',document=a,object='LeaseSketch',token='source-lease')
    call('assembly_sources',document=a,path=str(fixtures/'component.FCStd'),expect_error='sketch_active')
    call('assembly_sources',document=b,path=str(fixtures/'component.FCStd'),expect_error='sketch_active')
    call('sketch_cancel',document=a,token='source-lease')
    # Source browsing closes its scratch docs but retains readonly file ownership.
    browse(a,fixtures/'component.FCStd')
    call('open',path=str(fixtures/'component.FCStd'),expect_error='source_ownership')
    call('save',document=a,path=str(fixtures/'component.FCStd'),expect_error='source_ownership')
    # Guarded source browse validates all new source docs and then closes them.
    for filename,expected in [('hole-invalid.FCStd','invalid_model'),('broken-proxy.FCStd','assembly_proxy_restore')]:
        base=inspect(a);call('assembly_sources',document=a,path=str(fixtures/filename),expect_error=expected)
        assert inspect(a)==base;assert inspect(b)==foreign
    # Stale cached Hole shape is replaced by native regeneration from saved input.
    hole=browse(a,fixtures/'hole-stale.FCStd','Body');tx(a,[insert(hole,'HoleInstance')])
    hole_state=objects(inspect(a))['HoleInstance'];assert hole_state['shape']['valid']
    # Same-path changed file fails before root history. Relink remains usable.
    working=out/'Working.FCStd';working.write_bytes((fixtures/'component.FCStd').read_bytes())
    v1=browse(a,working);tx(a,[insert(v1)]);tx(a,[insert(v1,'Second')])
    loaded_state=inspect(a)
    call('open',path=str(working),expect_error='source_ownership');assert inspect(a)==loaded_state
    tx(a,[dict(op='add_object',type='Part::Box',name='DirtyRootFeature')]);base=inspect(a)
    before_native=json.loads((out/'native-trace.ndjson').read_text().splitlines()[-1])
    working.write_bytes((fixtures/'revision.FCStd').read_bytes())
    call('save',document=a,path=str(out/'Conflict.FCStd'),expect_error='source_changed');assert not (out/'Conflict.FCStd').exists()
    call('assembly_sources',document=a,path=str(working),expect_error='source_changed')
    tx(a,[dict(op='set',object='DirtyRootFeature',property='Length',value=99)],expect_error='source_changed')
    changed=inspect(a)
    for key in ['undo_count','redo_count','revision']:assert changed[key]==base[key]
    assert objects(changed)['DirtyRootFeature']==objects(base)['DirtyRootFeature']
    assert objects(changed)['Instance']['instance_state']['source_status']=='source_changed'
    v2=browse(a,fixtures/'revision.FCStd');relink=dict(op='assembly_relink',assembly='Assembly',object='Instance',external_source=v2,solve_joints=True)
    tx(a,[relink],expected_revision=base['revision']-1,expect_error='stale_revision')
    tx(a,[relink]);recovered=inspect(a);near(objects(recovered)['Instance']['shape']['volume'],648)
    assert objects(recovered)['DirtyRootFeature']==objects(base)['DirtyRootFeature']
    # Undo cannot publish a historical link pointing at an overwritten file.
    call('undo',document=a,expect_error='source_changed');assert objects(inspect(a))==objects(recovered)
    call('save',document=a,path=str(out/'Recovered.FCStd'))
    # Missing loaded source also recovers explicitly without discarding dirty root.
    move=out/'Move.FCStd';move.write_bytes((fixtures/'component.FCStd').read_bytes())
    moved=browse(a,move);tx(a,[insert(moved,'Moved')]);move.unlink()
    call('save',document=a,path=str(out/'Missing.FCStd'),expect_error='source_missing')
    replacement=browse(a,fixtures/'component.FCStd')
    tx(a,[dict(op='assembly_relink',assembly='Assembly',object='Moved',external_source=replacement,solve_joints=True)])
    assert 'DirtyRootFeature' in objects(inspect(a));call('save',document=a,path=str(out/'MissingRecovered.FCStd'))
    assert inspect(b)==foreign
    fresh=out/'FreshAccepted.FCStd';fresh.write_bytes((fixtures/'component.FCStd').read_bytes())
    accepted=browse(a,fresh);tx(a,[insert(accepted,'FreshMissing')])
    call('save',document=a,path=str(out/'MissingFresh.FCStd'));call('close',document=a)
    fresh.unlink()
    error=call('open',path=str(out/'MissingFresh.FCStd'),expect_error='source_missing')
    assert accepted['sha256'] in error['message'] and 'before Open' in error['message']
    assert inspect(b)==foreign
    fresh.write_bytes((fixtures/'component.FCStd').read_bytes())
    a=call('open',path=str(out/'MissingFresh.FCStd'))['document'];assert 'DirtyRootFeature' in objects(inspect(a))
    # Close one root preserves dependencies for the other, final close frees all.
    call('close',document=b);call('close',document=a)
    trace=[v for line in (out/'native-trace.ndjson').read_text().splitlines() if 'request' in (v:=json.loads(line))]
    assert trace[-1]['documents']=={},trace[-1]
    # Every observed Foreign native object, UID and undo/redo count stays exact.
    foreigns=[e['documents']['Foreign'] for e in trace if 'Foreign' in e['documents']]
    settled=next(i for i,e in enumerate(trace) if e['request']['op']=='undo' and e['request'].get('document')=='Foreign')
    controls=[e['documents']['Foreign'] for e in trace[settled:] if 'Foreign' in e['documents']]
    assert all(d==controls[0] for d in controls)
    manifest=json.loads((fixtures/'manifest.json').read_text());assert all(h(fixtures/n)==sha for n,sha in manifest['files'].items())
    (out/'result.json').write_text(json.dumps(dict(passed=True,foreign_native_exact=True,all_dependencies_released=True,hole_volume=hole_state['shape']['volume']),indent=2))
    print('source guard and native isolation checks passed')
finally:s.close()
