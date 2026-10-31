"""Native reference ownership and failed-Open recovery through the public Rust host."""
import hashlib,json,os,subprocess
from pathlib import Path

O=Path(os.environ['R7_OUTPUT']);O.mkdir()
rt=json.loads(Path(os.environ['R7_RUNTIME']).read_text());cmd=[os.environ['R7_HOST'],'--executable',rt['executable'],'--script',rt['script']]
for value in rt['args']:cmd+=['--arg',value]
p=subprocess.Popen(cmd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,encoding='utf8',creationflags=getattr(subprocess,'BELOW_NORMAL_PRIORITY_CLASS',0))
events=[];revisions={};results=[]
def call(op,error=None,**kw):
    if kw.get('document') in revisions:kw.setdefault('expected_revision',revisions[kw['document']])
    request=dict(op=op,**kw);p.stdin.write(json.dumps(request)+'\n');p.stdin.flush();reply=json.loads(p.stdout.readline())
    events.append(dict(request=request,response=reply));(O/'events.json').write_text(json.dumps(events,indent=2))
    if error:
        assert not reply['ok'] and reply['error']['code']==error,reply
        return reply['error']
    assert reply['ok'],reply
    value=reply['result'];state=value.get('state',value)
    if 'document' in state and 'revision' in state:revisions[state['document']]=state['revision']
    return value
def facts():return call('_native_facts')
def saved_state(key):
    state=call('inspect',document=key)
    displays={}
    for obj in state['objects']:
        if obj.get('shape') and not obj['shape']['null']:
            displays[obj['name']]=[call(op,document=key,object=obj['name']) for op in ('mesh','topology')]
    return dict(state=state,display=displays,native=facts()['documents'][key])
display_phases=[]
def exact(key,before):
    # Compare every native member BEFORE an intentional new mesh call.
    # Stock OCCT tessellation changes serialized face Checked flags.
    native=facts()['documents'][key]
    assert native==before['native']
    after=saved_state(key)
    assert after['state']==before['state'] and after['display']==before['display']
    display_phases.append(dict(document=key,native_before_display=native,native_after_display=after['native'],
        native_guard_boundary_exact=True,public_state_mesh_topology_exact=True))
    (O/'display-phase-ledger.json').write_text(json.dumps(display_phases,indent=2))
    return after
def quarantine(path):
    error=call('open',path=str(path),_independent_fault=dict(close_fail_files=['shared.FCStd']),error='assembly_proxy_restore')
    state=facts();keys=[key for key,batch in state['failed_loads'].items() if Path(batch['path'])==path]
    assert len(keys)==1,state
    key=keys[0];value=call('inspect',document=key)
    assert value['available'] is False
    return key,error

try:
    caps=call('capabilities')
    sets=[list(map(Path,call('_review_fixture',folder=str(O/('fixtures'+str(i))),malformed='root')['files'])) for i in range(2)]
    nested_path=Path(call('_nested_fixture',path=str(sets[0][1]),folder=str(O/'fixtures0'))['path'])
    property_path=Path(call('_property_fixture',folder=str(O/'property-fixture'))['path'])
    follower_path=Path(call('_follower_fixture',path=str(property_path),folder=str(O/'property-fixture'))['path'])
    outer_path=Path(call('_follower_fixture',path=str(follower_path),object='Box',filename='outer-follower.FCStd',folder=str(O/'property-fixture'))['path'])
    poison_path=Path(call('_follower_fixture',path=str(property_path),filename='poison-follower.FCStd',folder=str(O/'property-fixture'))['path'])
    hashes={str(q):hashlib.sha256(q.read_bytes()).hexdigest() for group in sets for q in group}
    hashes[str(property_path)]=hashlib.sha256(property_path.read_bytes()).hexdigest()
    hashes[str(nested_path)]=hashlib.sha256(nested_path.read_bytes()).hexdigest()
    hashes[str(follower_path)]=hashlib.sha256(follower_path.read_bytes()).hexdigest()
    hashes[str(outer_path)]=hashlib.sha256(outer_path.read_bytes()).hexdigest()
    hashes[str(poison_path)]=hashlib.sha256(poison_path.read_bytes()).hexdigest()
    shared,other,source=sets[0]
    keeper=call('new',name='Keeper')['document']
    call('transaction',document=keeper,edits=[dict(op='add_object',type='Part::Box',name='Box')])
    call('save',document=keeper,path=str(O/'keeper.FCStd'))
    call('transaction',document=keeper,edits=[dict(op='set',object='Box',property='Length',value=23)])
    call('undo',document=keeper);before=saved_state(keeper)
    q,original=quarantine(source);qnative=facts()['documents'][q]
    denied=call('open',path=str(other),error='document_unavailable')
    assert 'assembly_proxy_restore' in denied['message'] and original['message'].split('\n')[0] in denied['message']
    before=exact(keeper,before);assert facts()['documents'][q]==qnative
    assert set(facts()['documents'])=={keeper,q}
    call('open',path=str(nested_path),error='document_unavailable')
    before=exact(keeper,before);assert facts()['documents'][q]==qnative
    assert set(facts()['documents'])=={keeper,q}
    results.append('decisive real proxy fault: second root denied before publication; whole keeper/mesh/topology/revision/native graph exact')

    properties=call('open',path=str(property_path))['document']
    for i,value in enumerate([dict(link='B'),dict(link='B'),dict(link='B',subelements=['']),[dict(link='B')],dict(link='B'),dict(link='B',subelements=['']),[dict(link='B',subelements=[''])]]):
        call('transaction',document=properties,edits=[dict(op='set',object='A',property='Ref'+str(i),value=value)])
    supported=saved_state(properties)
    follower=call('open',path=str(outer_path))['document'];follower_before=saved_state(follower)
    poisoned=call('open',path=str(poison_path))['document']
    call('transaction',document=properties,edits=[dict(op='expression',object='A',property='Length',expression=q+'#SharedBox.Length')],error='document_unavailable')
    supported=exact(properties,supported)
    # A test-only late adoption models accepted references left by prior software/native callbacks.
    call('_late_adopt',document=properties,object='B',target_document=q,target_object='SharedBox')
    call('_late_expression',document=properties,object='B',property='Length',expression=q+'#SharedBox.Length')
    call('_poison_owner',document=poisoned)
    poison_before=facts()['documents']
    call('transaction',document=properties,edits=[dict(op='set',object='B',property='NativeReference',value=None)],error='document_unavailable')
    assert facts()['documents']==poison_before
    call('inspect',document=poisoned);call('close',document=poisoned)
    graph=facts();assert graph['documents'][properties]['revision']==supported['state']['revision']
    for prop in ['Ref0','Ref1','Ref2','Ref3','Ref4','Ref5','Ref6']:
        value=[dict(link='B')] if prop in ('Ref3','Ref6') else dict(link='B')
        call('transaction',document=properties,edits=[dict(op='set',object='A',property=prop,value=value)],error='document_unavailable')
        assert facts()['documents']==graph['documents']
    recovery=call('inspect',document=properties)
    assert recovery['available'] is False and recovery['objects']==[] and recovery['revision']==supported['state']['revision']
    for op,kw in [('mesh',dict(object='B')),('topology',dict(object='B')),('save',dict(path=str(O/'forbidden.FCStd'))),('transaction',dict(edits=[dict(op='set',object='A',property='Width',value=29)]))]:
        call(op,document=properties,error='document_unavailable',**kw);assert facts()['documents']==graph['documents']
    assert not (O/'forbidden.FCStd').exists()
    call('close',document=q,error='document_referenced');assert facts()['documents']==graph['documents']
    retry=call('open',path=str(source),error='assembly_proxy_restore')
    assert 'document_referenced' in retry['message'] and original['message'].split('\n')[0] in retry['message']
    assert facts()['documents']==graph['documents']
    # A failed partial detach must expose the new Rust revision without certifying geometry.
    call('_late_adopt',document=properties,object='A',property='SecondReference',target_document=q,target_object='SharedBox')
    partial_before=facts()['documents']
    call('transaction',document=properties,edits=[dict(op='set',object='B',property='NativeReference',value=None)],error='document_unavailable')
    refresh=call('inspect',document=properties)
    assert refresh['available'] is False and refresh['objects']==[] and refresh['revision']>recovery['revision']
    partial_after=facts()['documents']
    before_objects=partial_before[properties]['objects'];after_objects=partial_after[properties]['objects']
    assert before_objects.keys()==after_objects.keys()
    for object_name,old in before_objects.items():
        new=after_objects[object_name]
        # Native abort touches the edited property owner. Its new revision is
        # recovery-only above; geometry must not be accepted at the old token.
        if (old['state'],old['status'])!=(new['state'],new['status']):
            assert object_name=='B' and old['state']==['Up-to-date'] and old['status']=='Valid'
            assert new['state']==['Touched'] and new['status']=='Touched'
            assert {k:v for k,v in old.items() if k not in ('state','status','content')}=={k:v for k,v in new.items() if k not in ('state','status','content')}
            assert [name for name,_ in old['content']]==[name for name,_ in new['content']]
            assert [(name,value) for name,value in old['content'] if name!='Persistence.xml']==[(name,value) for name,value in new['content'] if name!='Persistence.xml']
            # Both full original XML payloads are checked byte-for-byte against
            # fresh stock native set/abort after this public host exits below.
            (O/'abort-raw-input.json').write_text(json.dumps(dict(shared_path=partial_before[q]['filename'],
                properties_path=partial_before[properties]['filename'],
                before=dict(owner=partial_before[properties],object=old),
                after=dict(owner=partial_after[properties],object=new)),indent=2))
        else:
            assert old==new
    assert partial_before[properties]['undo']==partial_after[properties]['undo']
    assert partial_before[properties]['redo']==partial_after[properties]['redo']
    assert partial_after[properties]['lease']['available'] is True
    assert partial_before[q]==partial_after[q]
    assert partial_after[follower]['revision']>partial_before[follower]['revision']
    call('transaction',document=properties,edits=[dict(op='set',object='B',property='NativeReference',value=None),dict(op='set',object='A',property='SecondReference',value=None),dict(op='expression',object='B',property='Length',expression=None)])
    fresh_follower=saved_state(follower)
    assert fresh_follower['state']['revision']>follower_before['state']['revision']
    assert fresh_follower['state']['available'] is True and fresh_follower['state']['objects'][0]['properties']['Length']==7
    assert abs(fresh_follower['state']['objects'][0]['shape']['volume']-700)<1e-10
    assert fresh_follower['native']['uuid']==follower_before['native']['uuid']
    assert fresh_follower['native']['objects']['Box']['id']==follower_before['native']['objects']['Box']['id']
    call('save',document=follower,expected_revision=follower_before['state']['revision'],path=str(O/'stale-follower.FCStd'),error='stale_revision')
    assert not (O/'stale-follower.FCStd').exists()
    assert facts()['documents'][q]==qnative
    call('_late_adopt',document=properties,object='B',target_document=q,target_object='SharedBox')
    unavailable=call('inspect',document=properties)
    assert unavailable['available'] is False and unavailable['objects']==[]
    call('inspect',document=follower);call('close',document=follower)
    call('close',document=properties)
    assert facts()['documents'][q]==qnative
    retry=call('open',path=str(source),error='assembly_proxy_restore')
    assert q not in facts()['documents'];before=exact(keeper,before)
    after_close=facts()
    assert q not in after_close['failed_loads']
    # A later explicit Open may try the file again; the intentionally broken
    # proxy still fails, and clean teardown must not recreate a quarantine.
    call('open',path=str(source),error='assembly_proxy_restore')
    after_retry=facts()
    assert after_retry['documents']==after_close['documents'] and after_retry['failed_loads']==after_close['failed_loads']
    before=exact(keeper,before)
    # A new error after native Close is different from the historical failure:
    # do not reopen on this action, retain the diagnostics, and allow a later
    # explicit retry to open and cleanly reject the intentionally bad file.
    q,original=quarantine(source)
    error=call('open',path=str(source),_independent_fault=dict(close_after_files=['shared.FCStd']),error='assembly_proxy_restore')
    assert original['message'].split('\n')[0] in error['message'] and 'independent post-Close fault' in error['message']
    after_close=facts()
    assert q not in after_close['documents'] and q not in after_close['failed_loads']
    call('open',path=str(source),error='assembly_proxy_restore')
    after_retry=facts()
    assert after_retry['documents']==after_close['documents'] and after_retry['failed_loads']==after_close['failed_loads']
    before=exact(keeper,before)
    results.append('Link/hidden/Sub/List/XLink public setters, early expression gate, late adoption recovery-only inspect, partial detach rollback/revision refresh, explicit/retry Close blockers and public detach')

    q1,_=quarantine(source);q2,_=quarantine(sets[1][2]);second=facts()['documents'][q2]
    call('open',path=str(source),error='assembly_proxy_restore')
    assert q1 not in facts()['documents'] and facts()['documents'][q2]==second
    call('inspect',document=q2);call('close',document=q2)
    results.append('two exact path batches and source bytes')

    target=call('open',path=str(shared))['document'];target_state=saved_state(target)
    # The original annotated readonly source cannot adopt an editable owner.
    # Keep that saved path and its exact rejection separate from generic native
    # reference recovery through the approved local Link/hidden XLink fixture.
    native_before=facts();call('open',path=str(other),error='source_ownership')
    assert facts()==native_before
    assert hashes=={q:hashlib.sha256(Path(q).read_bytes()).hexdigest() for q in hashes}
    legal_path=call('_linked_file_fixture',document=target,object='SharedBox',link_name='SharedLink',
        box_length=5,path=str(O/'generic-dependent.FCStd'))['path']
    hashes[legal_path]=hashlib.sha256(Path(legal_path).read_bytes()).hexdigest()
    root=call('open',path=legal_path)['document'];root_state=saved_state(root)
    proof=call('_recovery_link_proof',document=root,link_name='SharedLink')
    assert proof['sole_document_foreign_edge'] and not proof['external_dependents']
    assert proof['hidden_foreign_edge']['document']==target and target in proof['dependency_closure']
    assert proof['native_id']==root_state['native']['objects']['SharedLink']['id']
    (O/'generic-native-open-proof.json').write_text(json.dumps(dict(saved_file=legal_path,
        file_sha256=hashes[legal_path],proof=proof,expected_revision=revisions[root],
        native_inventory=root_state['native'],external_dual_ownership_original_denied=True),indent=2))
    call('close',document=target,error='document_referenced');target_state=exact(target,target_state);root_state=exact(root,root_state)
    call('close',document=target,expected_revision=revisions[target]-1,error='stale_revision')
    call('_late_adopt',document=target,object='SharedBox',target_document=root,target_object='Box',type='App::PropertyXLinkSub')
    cycle=facts()['documents']
    call('close',document=root,error='document_referenced');call('close',document=target,error='document_referenced')
    assert facts()['documents']==cycle
    call('transaction',document=target,edits=[dict(op='set',object='SharedBox',property='NativeReference',value=None)])
    call('transaction',document=root,edits=[dict(op='remove_object',object='SharedLink',native_id=proof['native_id'])])
    call('close',document=target);call('close',document=root)
    replacement=call('open',path=str(shared))
    assert replacement['revision']>target_state['state']['revision']
    call('close',document=replacement['document'],expected_revision=target_state['state']['revision'],error='stale_revision')
    call('close',document=replacement['document']);before=exact(keeper,before)
    call('redo',document=keeper);assert call('inspect',document=keeper)['objects'][0]['properties']['Length']==23
    call('close',document=keeper)
    results.append('accepted dependency Close guards, native cycle, safe native property detach, name reuse and keeper Redo')
    assert hashes=={q:hashlib.sha256(Path(q).read_bytes()).hexdigest() for q in hashes}
    borrowed=call('_cleanup_borrowed_fixture')
    assert all(value['revision'] is None for value in borrowed['before']['documents'].values())
    assert facts()['documents']=={} and facts()['failed_loads']=={}
    (O/'results.json').write_text(json.dumps(dict(passed=results,requests=len(events),capabilities=caps,source_bytes_exact=True,borrowed_native_fixture_cleanup=borrowed),indent=2));print(json.dumps(dict(passed=results,requests=len(events))))
finally:p.stdin.close();assert p.wait(timeout=20)==0

# Serial native proof after the public host has exited. No recoverable live
# state or graceful native-child EOF is inferred from the observed host exit.
command=[rt['executable'],*rt['args'],str(Path(__file__).with_name('open_cleanup_abort_native.py')),
    str(O/'abort-raw-input.json'),str(Path(os.environ['R7_BINDINGS'])/'native-members'),str(O/'abort-native-control')]
with (O/'abort-native-control.log').open('x',encoding='utf-8') as log:
    control=subprocess.Popen(command,stdout=log,stderr=subprocess.STDOUT,
        creationflags=getattr(subprocess,'BELOW_NORMAL_PRIORITY_CLASS',0))
    code=control.wait()
(O/'abort-native-command.json').write_text(json.dumps(dict(command=command,pid=control.pid,exit_code=code,
    public_host_pid=p.pid,public_host_exit_code=p.returncode,
    native_child_graceful_exit_behind_host_certified=False),indent=2))
assert code==0,(O/'abort-native-control.log').read_text()
report=json.loads((O/'results.json').read_text());report['raw_abort_native_control']=True
(O/'results.json').write_text(json.dumps(report,indent=2))
