"""Bounded native terminal-recovery controls through public Rust WorkerSession."""
import json, os, subprocess
from pathlib import Path

out = Path(os.environ['R7_OUTPUT']); out.mkdir()
p = subprocess.Popen([os.environ['R7_TEST_HOST'], '--ignored', '--exact',
    'terminal_recovery_tests::public_host', '--nocapture', '--test-threads=1'],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding='utf8',
    creationflags=getattr(subprocess, 'BELOW_NORMAL_PRIORITY_CLASS', 0))
events=[]; revisions={}; assertions=0
def check(value, message):
    global assertions
    assertions+=1
    assert value, message
def call(op, **kw):
    if kw.get('document') in revisions: kw.setdefault('expected_revision',revisions[kw['document']])
    request=dict(op=op,**kw); p.stdin.write(json.dumps(request)+'\n'); p.stdin.flush()
    while True:
        line=p.stdout.readline()
        if not line: raise RuntimeError('Rust host driver EOF')
        if line.startswith('{'): response=json.loads(line);break
    events.append(dict(request=request,response=response))
    (out/'events.json').write_text(json.dumps(events,indent=2))
    if response['ok']:
        state=response['result'].get('state',response['result'])
        if 'document' in state and 'revision' in state: revisions[state['document']]=state['revision']
    return response
def ok(op,**kw):
    r=call(op,**kw);check(r['ok'],r);return r['result']
def denied(code,op,**kw):
    r=call(op,**kw);check(not r['ok'] and r['error']['code']==code,r);return r
def native(): return ok('_native_facts')
def owner(name):
    a=ok('new',name=name)['document']
    ok('transaction',document=a,edits=[dict(op='add_object',type='Part::Box',name='Box'),
        dict(op='add_object',type='Sketcher::SketchObject',name='Sketch'),
        dict(op='geometry',object='Sketch',kind='line',start=[0,0,0],end=[10,0,0])])
    ok('save',document=a,path=str(out/(name+'.FCStd')))
    path=ok('_linked_file_fixture',document=a,object='Box',path=str(out/(name+'Child.FCStd')),local_link=True)['path']
    c=ok('open',path=path)['document']
    return a,c
def exact(before, after, names, active=True):
    for n in names: check(before['documents'][n]==after['documents'][n],n+' native content/history/registry/lease changed')
    if active:
        check(before['active']==after['active'] and before['native_active']==after['native_active'],'active owner changed')
    check(before['transaction']==after['transaction'],'app-wide transaction changed')
    for n in names:check(before['native_pending'][n]==after['native_pending'][n],'native pending transaction changed')
def close(n): return ok('close',document=n)

try:
    ok('capabilities')
    keeper=ok('new',name='UnrelatedKeeper')['document']
    ok('transaction',document=keeper,edits=[dict(op='add_object',type='Part::Box',name='Box')])
    keeper_path=out/'keeper.FCStd';ok('save',document=keeper,path=str(keeper_path));keeper_bytes=keeper_path.read_bytes()
    ok('transaction',document=keeper,edits=[dict(op='set',object='Box',property='Length',value=23)])
    ok('undo',document=keeper);keeper_public=ok('inspect',document=keeper)
    check(keeper_public['redo_count']==1,'keeper Redo absent')
    # Settled sketch: failed/no-op Close and retry never change terminal owner.
    a,c=owner('SketchOwner')
    ok('sketch_begin',document=a,object='Sketch',token='owned-sketch')
    healthy=native()
    denied('sketch_active','close',document=c)
    denied('sketch_active','transaction',document=c,edits=[dict(op='remove_object',object='Target')])
    exact(healthy,native(),[a,c,keeper])
    ok('_poison_owner',document=a);baseline=native()
    poisoned=ok('inspect',document=a)
    check(poisoned['available'] is False and poisoned['sketch_edit']['token']=='owned-sketch','local poison/owner missing')
    metadata=poisoned['close_dependents'];check(len(metadata)==1 and metadata[0]['document']==c and metadata[0]['revision']==revisions[c] and Path(metadata[0]['path'])==out/'SketchOwnerChild.FCStd','exact dependency metadata')
    denied('document_referenced','close',document=a)
    denied('stale_revision','close',document=c,expected_revision=revisions[c]-1)
    denied('sketch_active','transaction',document=c,edits=[dict(op='set',object='Box',property='Length',value=99)],terminal_recovery=True)
    denied('sketch_active','close',document=keeper,terminal_recovery=True)
    denied('native_error','close',document=c,_independent_fault={'close_fail':[c]})
    denied('native_error','close',document=c,_independent_fault={'close_noop':[c]})
    exact(baseline,native(),[a,c,keeper])
    close(c);after=native();exact(baseline,after,[a,keeper]);check(c not in after['documents'],'dependent survived successful Close')
    still=ok('inspect',document=a);expected=dict(poisoned,close_dependents=[])
    check(still==expected,'whole poisoned sketch owner public state changed')
    check(still['available'] is False and still['sketch_edit']==poisoned['sketch_edit'],'poison/sketch cleared')
    check(still['revision']==poisoned['revision'] and still['close_dependents']==[],'owner revision/metadata invalid')
    denied('document_unavailable','sketch_cancel',document=a,token='owned-sketch')
    close(a)
    # Preview owns an open native transaction. Only explicit Close is safe.
    a,c=owner('PreviewOwner')
    ok('preview',document=a,token='owned-preview',edits=[dict(op='set',object='Box',property='Length',value=22)])
    healthy=native();denied('preview_active','close',document=c);exact(healthy,native(),[a,c,keeper])
    ok('_poison_owner',document=a);baseline=native()
    preview_public=ok('inspect',document=a)
    check(baseline['transaction'] and baseline['native_pending'][a],'preview native transaction not pending')
    denied('recovery_busy','transaction',document=c,edits=[dict(op='remove_object',object='Target')])
    denied('native_error','close',document=c,_independent_fault={'close_fail':[c]})
    exact(baseline,native(),[a,c,keeper])
    close(c);exact(baseline,native(),[a,keeper])
    check(ok('inspect',document=a)==dict(preview_public,close_dependents=[]),'whole poisoned preview owner changed')
    close(a)
    # A native document cycle permits removal only of an isolated reference.
    a,c=owner('CycleOwner')
    ok('_late_adopt',document=a,object='Box',target_document=c,target_object='Box',recompute=not bool(os.environ.get('R7_TERMINAL_TOUCHED')))
    ok('sketch_begin',document=a,object='Sketch',token='cycle-sketch')
    ok('_poison_owner',document=a);baseline=native()
    cycle_public=ok('inspect',document=a)
    if os.environ.get('R7_TERMINAL_TOUCHED'):
        denied('document_referenced','close',document=a)
        denied('document_referenced','close',document=c)
        denied('recovery_busy','transaction',document=c,edits=[dict(op='remove_object',object='Target')])
        exact(baseline,native(),[a,c,keeper])
        check(ok('inspect',document=a)==cycle_public,'touched terminal owner changed')
        (out/'results.json').write_text(json.dumps(dict(assertions=assertions,requests=len(events),touched_cycle_fails_closed=True,
            blocker='Native dependent recompute would change touched terminal owner; both explicit Close directions are referenced',
            no_unpoison=True,native_exact=True,private_fixture_teardown_on_eof=True),indent=2))
        raise SystemExit(0)
    denied('document_referenced','close',document=a)
    denied('document_referenced','close',document=c)
    denied('document_referenced','transaction',document=c,edits=[dict(op='remove_object',object='Box')])
    denied('document_unavailable','transaction',document=c,edits=[dict(op='remove_object',object='LocalLink')])
    denied('document_referenced','transaction',document=c,edits=[dict(op='set',object='LocalLink',property='LinkedObject',value=None)])
    denied('document_referenced','transaction',document=c,edits=[dict(op='expression',object='Box',property='Length',expression=None)])
    link_id=baseline['documents'][c]['objects']['Target']['id']
    denied('stale_reference','transaction',document=c,edits=[dict(op='remove_object',object='Target',native_id=link_id+1)])
    exact(baseline,native(),[a,c,keeper])
    old_revision=revisions[c]
    denied('invalid_model','transaction',document=c,edits=[dict(op='remove_object',object='Target')],_independent_fault={'validate':True})
    failed=native();exact(baseline,failed,[a,keeper])
    # Rollback advances only the edited owner's public revision; native links
    # and identities return intact. Inspect provides the retry revision.
    check(failed['documents'][c]['objects']==baseline['documents'][c]['objects'],'failed removal lost native graph')
    check(failed['documents'][c]['revision']!=old_revision,'failed removal reused revision')
    denied('stale_revision','transaction',document=c,expected_revision=old_revision,edits=[dict(op='remove_object',object='Target')])
    ok('inspect',document=c)
    state=ok('transaction',document=c,edits=[dict(op='remove_object',object='Target',native_id=link_id)])['state']
    check(state['available'] and 'Target' not in [x['name'] for x in state['objects']],'safe removal failed')
    after=native();exact(baseline,after,[a,keeper])
    check(ok('inspect',document=a)==dict(cycle_public,close_dependents=[]),'whole poisoned cyclic owner changed')
    check(after['documents'][a]['objects']['Box']['out']==baseline['documents'][a]['objects']['Box']['out'],'poisoned reverse edge changed')
    for op in ('mesh','topology'):
        display=ok(op,document=c,object='Box');check(display['revision']==state['revision'],'fresh display revision')
    close(a);close(c)
    check(ok('inspect',document=keeper)==keeper_public,'unrelated whole public state changed')
    check(keeper_path.read_bytes()==keeper_bytes,'keeper source bytes changed')
    redo=ok('redo',document=keeper);redo=redo.get('state',redo);check(redo['objects'][0]['properties']['Length']==23,'keeper Redo unavailable')
    close(keeper);check(native()['documents']=={},'live native owners remain')
    result=dict(assertions=assertions,requests=len(events),all_passed=True,public_host='WorkerSession.request',native_child_eof_independently_certified=False)
    (out/'results.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
finally:
    p.stdin.close();check(p.wait(timeout=25)==0,'Rust host driver failed')
