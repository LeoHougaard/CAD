"""Focused public Rust/native proof of the isolated-Link terminal boundary."""
import hashlib,json,os,subprocess
from pathlib import Path

out=Path(os.environ['R7_OUTPUT']);out.mkdir()
process=subprocess.Popen([os.environ['R7_TEST_HOST'],'--ignored','--exact','terminal_recovery_tests::public_host','--nocapture','--test-threads=1'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,encoding='utf8',creationflags=getattr(subprocess,'BELOW_NORMAL_PRIORITY_CLASS',0))
events=[];revisions={};sources={};results=[]
def digest(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def call(op,**kw):
    if kw.get('document') in revisions:kw.setdefault('expected_revision',revisions[kw['document']])
    request=dict(op=op,**kw);process.stdin.write(json.dumps(request)+'\n');process.stdin.flush()
    while True:
        line=process.stdout.readline()
        if not line:raise RuntimeError('public Rust driver EOF')
        if line.startswith('{'):response=json.loads(line);break
    events.append(dict(request=request,response=response));(out/'events.json').write_text(json.dumps(events,indent=2))
    if response['ok'] and isinstance(response['result'],dict):
        state=response['result'].get('state',response['result'])
        if 'document' in state and 'revision' in state:revisions[state['document']]=state['revision']
    return response
def ok(op,**kw):
    response=call(op,**kw);assert response['ok'],response;return response['result']
def facts():return ok('_native_facts')
def trace():return ok('_scope_trace')
def public(names):return {n:ok('inspect',document=n) for n in names}
def deny(label,document,edits=None,code=None,op='transaction',**kw):
    before=facts();states=public(before['documents']);entered=trace()
    if edits is not None:kw['edits']=edits
    response=call(op,document=document,**kw)
    assert not response['ok'] and (code is None or response['error']['code']==code),(label,response)
    after=facts();assert before==after,(label,'native/registry/lease/history/active inventory changed')
    assert public(after['documents'])==states,(label,'whole public state changed')
    assert trace()==entered,(label,'entered transaction/edit/recompute/validate/publication')
    results.append(dict(case=label,response=response,native_exact=True,public_exact=True,no_native_entry=True,
        current_ids={n:{k:o['id'] for k,o in d['objects'].items()} for n,d in before['documents'].items()}))
def save(doc,name):
    path=out/name;ok('save',document=doc,path=str(path));sources[str(path)]=digest(path);return path
def new_owner(name):
    doc=ok('new',name=name)['document']
    ok('transaction',document=doc,edits=[dict(op='add_object',type='Part::Box',name='Box'),dict(op='add_object',type='Sketcher::SketchObject',name='Sketch')])
    save(doc,name+'.FCStd');return doc
def child(owner,name,kind='link',object='Box'):
    path=ok('_scope_saved',document=owner,object=object,kind=kind,path=str(out/(name+'.FCStd')))['path']
    sources[path]=digest(path);return ok('open',path=path)['document']
def poison(doc,token):
    ok('sketch_begin',document=doc,object='Sketch',token=token);ok('_poison_owner',document=doc)
def close(doc):ok('close',document=doc)
def same_owner(before,doc):
    after=facts();assert after['documents'][doc]==before['documents'][doc]
    assert (after['active'],after['native_active'],after['transaction'])==(before['active'],before['native_active'],before['transaction'])
    return after

try:
    ok('capabilities')
    qfolder=out/'quarantine-inputs';ok('_review_fixture',folder=str(qfolder),malformed='source')
    for path in qfolder.glob('*.FCStd'):sources[str(path)]=digest(path)
    keeper=ok('new',name='ScopeKeeper')['document'];ok('transaction',document=keeper,edits=[dict(op='add_object',type='Part::Box',name='Box')])
    save(keeper,'keeper.FCStd');ok('transaction',document=keeper,edits=[dict(op='set',object='Box',property='Length',value=23)]);ok('undo',document=keeper)
    keeper_public=ok('inspect',document=keeper);keeper_native=facts()['documents'][keeper];assert keeper_public['redo_count']==1
    owner=new_owner('ScopeOwner');target=child(owner,'ScopeChild','mixed')
    # Preserve the original unannotated foreign-LinkedObject saved file as a
    # compatibility rejection, separate from the supported recovery fixture.
    legacy=ok('_linked_file_fixture',document=owner,object='Box',path=str(out/'legacy-foreign-link.FCStd'),legacy_external_link=True)['path']
    sources[legacy]=digest(legacy);before=facts();states=public(before['documents'])
    rejected=call('open',path=legacy)
    assert not rejected['ok'] and rejected['error']['code']=='source_identity',rejected
    assert facts()==before and public(before['documents'])==states
    results.append(dict(case='legacy unannotated foreign LinkedObject admission rejection',response=rejected,
        native_exact=True,public_exact=True,compatibility_limit=True))
    ok('transaction',document=target,edits=[dict(op='set',object='Box',property='Width',value=13)])
    local_poison=child(owner,'LocalPoisonChild');array=child(owner,'ScopeArray','array');foreign=child(target,'ForeignFollower',object='Target')
    isolated=child(owner,'SoleForeignChild')
    ok('sketch_begin',document=owner,object='Sketch',token='scope-sketch')
    deny('healthy sketch',target,[dict(op='remove_object',object='Target')],'sketch_active')
    ok('_poison_owner',document=owner)
    proof=ok('_recovery_link_proof',document=isolated)
    assert proof['sole_document_foreign_edge'] and not proof['external_dependents'] and owner in proof['dependency_closure']
    # The retained mixed control below still proves other foreign references
    # survive. This distinct positive proves a single foreign-edge document.
    before=facts();old_revision=revisions[isolated]
    state=ok('transaction',document=isolated,edits=[dict(op='remove_object',object='Target',native_id=proof['native_id'])])['state']
    after=same_owner(before,owner)
    assert state['available'] and state['revision']>old_revision
    assert after['documents'][isolated]['objects']=={'Box':before['documents'][isolated]['objects']['Box']}
    assert ok('mesh',document=isolated,object='Box')['revision']==state['revision']
    assert ok('topology',document=isolated,object='Box')['revision']==state['revision']
    results.append(dict(case='sole foreign-edge saved Link remove positive',proof=proof,revision=state['revision'],owner_exact=True))
    owner_before=facts();owner_public=ok('inspect',document=owner)
    deny('foreign native dependent',target,[dict(op='remove_object',object='Target')],'document_referenced')
    ok('_scope_lease',document=keeper,action='preview_stage',parameters=dict(request=dict(token='healthy-foreign'),label='Foreign'))
    ok('_scope_lease',document=keeper,action='preview_resolve',parameters=dict(accepted=True))
    deny('healthy foreign preview',target,[dict(op='remove_object',object='Target')],'preview_active')
    ok('_scope_lease',document=keeper,action='preview_end')
    ok('_scope_lease',document=owner,action='preview_stage',parameters=dict(request=dict(token='pending-native-callback'),label='Pending'))
    deny('pending Rust preview callback',target,[dict(op='remove_object',object='Target')],'sketch_active')
    ok('_scope_lease',document=owner,action='preview_resolve',parameters=dict(accepted=False))
    assert facts()==owner_before
    close(foreign);same_owner(owner_before,owner)
    inventory=facts()['documents'][target]['objects'];link_id=inventory['Target']['id'];box_id=inventory['Box']['id']
    removed=dict(op='remove_object',object='Target',native_id=link_id)
    custom=dict(op='set',object='Box',property='NativeReference',value=None,native_id=box_id)
    linked=dict(op='set',object='Target',property='LinkedObject',value=None,native_id=link_id)
    for label,edits in [('Part Box XLink set',[custom]),('simple Link LinkedObject set',[linked]),
        ('mixed remove then custom set',[removed,custom]),('mixed set then remove',[linked,removed]),
        ('repeated same revision and IDs',[custom]),('empty-list Link set',[dict(linked,value=[])])]:
        deny(label,target,edits,'document_referenced')
    deny('local unrelated Link',target,[dict(op='remove_object',object='LocalLink')],'document_unavailable')
    deny('unrelated owner Close',keeper,code='sketch_active',op='close')
    deny('stale native Link ID',target,[dict(removed,native_id=link_id+1)],'stale_reference')
    array_id=facts()['documents'][array]['objects']['Target']['id']
    deny('array removal',array,[dict(op='remove_object',object='Target',native_id=array_id)],'document_referenced')
    deny('array LinkedObject set',array,[dict(op='set',object='Target',property='LinkedObject',value=None,native_id=array_id)],'document_referenced')
    ok('_poison_owner',document=local_poison)
    deny('local poison',local_poison,[dict(op='remove_object',object='Target')],'document_unavailable')
    close(local_poison)
    before=facts();start=trace();old_revision=revisions[target]
    state=ok('transaction',document=target,edits=[removed])['state'];after=same_owner(before,owner)
    assert state['revision']>old_revision and state['available'] and 'Target' not in after['documents'][target]['objects']
    assert after['documents'][target]['objects']['Box']==before['documents'][target]['objects']['Box']
    assert after['documents'][target]['objects']['Box']['out'] and abs(after['documents'][target]['objects']['Box']['volume']-1300)<1e-10
    entered=trace()[len(start):];assert any(e['phase']=='transaction' and e['terminal_recovery'] for e in entered)
    # Native openTransaction sets the app transaction first. The document
    # enrolls only when that first edit changes it, before recompute/validation.
    assert any(e['phase']=='apply_edit' and e['transaction'] and e['active']==target for e in entered)
    assert any(e['phase'] in ('recompute_document','validate') and e['native_pending'] for e in entered)
    for op in ('mesh','topology'):assert ok(op,document=target,object='Box')['revision']==state['revision']
    results.append(dict(case='isolated Link remove positive',removed_id=link_id,revision=state['revision'],other_reference_and_unsaved_box_retained=True,native_calls=entered))
    close(array);close(target);close(owner);close(isolated)
    # A pending native preview can only use Close; no reference edit reaches native transaction.
    owner=new_owner('PendingOwner');target=child(owner,'PendingChild')
    ok('preview',document=owner,token='native-preview',edits=[dict(op='set',object='Box',property='Length',value=22)])
    ok('_poison_owner',document=owner)
    deny('pending native transaction',target,[dict(op='remove_object',object='Target')],'recovery_busy')
    pending=facts();assert pending['native_pending'][owner] and pending['transaction']
    close(target);same_owner(pending,owner);close(owner)
    # Ordinary global lease admission must retain broad quarantine detach.
    ordinary=child(keeper,'OrdinaryQuarantine','ordinary');ok('_scope_active',document=keeper)
    failure=call('open',path=str(qfolder/'source.FCStd'),_independent_fault=dict(close_fail_files=['shared.FCStd']))
    assert not failure['ok'] and failure['error']['code']=='assembly_proxy_restore' and 'cleanup_errors' in failure['error']['message']
    native=facts();q=next(n for n,d in native['documents'].items() if Path(d['filename']).name=='shared.FCStd')
    assert native['active']==native['native_active']==keeper
    ok('_late_adopt',document=ordinary,object='Box',property='NativeReference',target_document=q,target_object='SharedBox')
    unavailable=ok('inspect',document=ordinary);assert unavailable['available'] is False and unavailable['objects']==[]
    native=facts();qid=native['documents'][ordinary]['objects']['Box']['id'];start=trace()
    fresh=ok('transaction',document=ordinary,edits=[dict(op='set',object='Box',property='NativeReference',value=None,native_id=qid)])['state']
    after=facts();assert after['documents'][q]==native['documents'][q] and fresh['available']
    assert not after['documents'][ordinary]['objects']['Box']['out']
    entered=trace()[len(start):];assert any(e['phase']=='transaction' and e['terminal_recovery'] is False for e in entered)
    for op in ('mesh','topology'):assert ok(op,document=ordinary,object='Box')['revision']==fresh['revision']
    results.append(dict(case='ordinary quarantine generic XLink clear positive',revision=fresh['revision'],native_calls=entered,original_error=failure['error']))
    ok('inspect',document=q);close(q);close(ordinary)
    assert ok('inspect',document=keeper)==keeper_public and facts()['documents'][keeper]==keeper_native
    redo=ok('redo',document=keeper);assert redo['objects'][0]['properties']['Length']==23;close(keeper)
    assert facts()['documents']=={}
    # Preserve the disclosed unrecoverable touched cyclic graph exactly.
    owner=new_owner('TouchedOwner');target=child(owner,'TouchedChild');poison(owner,'touched-sketch')
    ok('_scope_touched_cycle',document=owner,target=target)
    deny('touched cycle mutation',target,[dict(op='remove_object',object='Target')],'recovery_busy')
    deny('touched cycle owner Close',owner,code='document_referenced',op='close')
    deny('touched cycle dependent Close',target,code='document_referenced',op='close')
    assert all(digest(path)==value for path,value in sources.items())
    report=dict(cases=results,requests=len(events),source_hashes=sources,source_bytes_exact=True,rust_unchanged=True,
        touched_cycle_fail_closed=True,touched_private_fixture_teardown_on_eof=True,native_child_exit_independently_certified=False)
    (out/'results.json').write_text(json.dumps(report,indent=2));print(json.dumps(dict(passed_cases=len(results),requests=len(events),source_bytes_exact=True)))
finally:
    process.stdin.close();code=process.wait(timeout=25);(out/'driver-exit.json').write_text(json.dumps(dict(pid=process.pid,exit_code=code)));assert code==0
