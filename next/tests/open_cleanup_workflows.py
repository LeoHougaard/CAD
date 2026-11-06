"""Public Rust-host regression. Run with the test-only open_cleanup_worker.py."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

output = Path(os.environ['R7_OUTPUT'])
output.mkdir(exist_ok=False)
runtime = json.loads(Path(os.environ['R7_RUNTIME']).read_text())
wrapper = Path(__file__).resolve().parent/'fixtures/open_cleanup_worker.py'
command = [os.environ['R7_HOST'], '--executable', runtime['executable'], '--script', str(wrapper)]
for arg in runtime['args']: command += ['--arg', arg]
process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding='utf8',
    creationflags=getattr(subprocess, 'BELOW_NORMAL_PRIORITY_CLASS', 0))
events, revisions, results = [], {}, []

def call(op, error=None, **values):
    if op in ('transaction','undo','redo','save','close','document_settings') and values.get('document') in revisions:
        values.setdefault('expected_revision', revisions[values['document']])
    request = dict(op=op, **values)
    process.stdin.write(json.dumps(request)+'\n'); process.stdin.flush()
    reply = json.loads(process.stdout.readline())
    events.append(dict(request=request, response=reply))
    (output/'events.json').write_text(json.dumps(events, indent=2))
    if error:
        assert not reply['ok'] and reply['error']['code'] == error, reply
        return reply['error']
    assert reply['ok'], reply
    result = reply['result']
    state = result.get('state', result)
    if 'document' in state and 'revision' in state: revisions[state['document']] = state['revision']
    return result

def receipt():
    return json.loads(next(Path(os.environ['R7_BINDINGS']).glob('*.json')).read_text())

def facts():
    return receipt()['events'][-1]['after']

def keeper():
    key = call('new', name='Keeper')['document']
    call('transaction', document=key, edits=[dict(op='add_object',type='Part::Box',name='Box')])
    call('transaction', document=key, edits=[dict(op='set',object='Box',property='Length',value=23)])
    call('undo', document=key)
    return key

def check_keeper(key, state, native):
    assert call('inspect',document=key) == state
    assert facts()['documents'][key] == native
    assert facts()['active'] == key
    assert facts()['native_active'] == key

try:
    capabilities = call('capabilities')
    assert capabilities['freecad_version'][:3] == ['1','1','4']
    source = call('new',name='Source')['document']
    call('transaction',document=source,edits=[dict(op='add_object',type='Part::Box',name='Source')])
    path = output/'source.FCStd'
    call('save',document=source,path=str(path)); call('close',document=source)
    saved_hash = hashlib.sha256(path.read_bytes()).hexdigest()
    key = keeper()
    state = call('inspect',document=key); native = facts()['documents'][key]
    original = call('open',path=str(path),_r7_fault=dict(validate=1,close=1,validation_code='invalid_model'),error='invalid_model')
    assert original['message'].startswith('r7 injected validation failure'), original
    assert 'r7 injected native Close failure' in original['message'] and 'source' in original['message']
    check_keeper(key,state,native)
    recovery = call('inspect',document='source')
    assert recovery['available'] is False and recovery['objects'] == [] and recovery['recovery']['action'] == 'close', recovery
    rev = recovery['revision']
    assert facts()['documents']['source']['revision'] == rev
    assert 'source' in facts()['observers']
    assert facts()['documents']['source']['lease']['available'] is False
    repeated=call('open',path=str(path),_r7_fault=dict(close=1),error='invalid_model')
    assert repeated['message'].startswith('r7 injected validation failure')
    recovery=call('inspect',document='source')
    assert len(recovery['recovery']['cleanup_errors']) == 2
    for op,params in [('transaction',dict(edits=[dict(op='set',object='Source',property='Length',value=99)])),
                      ('undo',{}),('redo',{}),('save',dict(path=str(output/'must-not-exist.FCStd'))),
                      ('preview',dict(token='forbidden',edits=[])),('document_settings',dict(unit_system=0)),
                      ('mesh',dict(object='Source')),('topology',dict(object='Source')),
                      ('export_exchange',dict(path=str(output/'must-not-exist.step'))),
                      ('feature_schema',dict(type='Part::Box')),('quantities',dict(items=[]))]:
        call(op,document='source',expected_revision=rev,error='document_unavailable',**params)
    assert not (output/'must-not-exist.FCStd').exists() and not (output/'must-not-exist.step').exists()
    call('close',document='source',expected_revision=rev-1,error='stale_revision')
    call('close',document='source',_r7_fault=dict(close=1),error='native_error')
    assert call('inspect',document='source') == recovery
    assert 'source' in facts()['observers']
    check_keeper(key,state,native)
    call('redo',document=key)
    assert call('inspect',document=key)['objects'][0]['properties']['Length'] == 23
    call('undo',document=key)
    # Native Redo survives recovery; the accepted Undo changes its public revision.
    state=call('inspect',document=key); native=facts()['documents'][key]
    call('close',document='source')
    assert 'source' not in facts()['documents'] and 'source' not in facts()['observers']
    call('inspect',document='source',error='not_found')
    check_keeper(key,state,native)
    results.append('original error, keeper, poison guards, stale Close, failed Close retry, history, observer detach')

    # Ordinary failed cleanup leaves no recovery registry or observers.
    call('open',path=str(path),_r7_fault=dict(validate=1),error='native_error')
    assert set(facts()['documents']) == {key}
    assert facts()['observers'] == [key]
    check_keeper(key,state,native)
    results.append('ordinary failed Open cleanup')

    # All newly created native dependency owners are visited despite two failures.
    error=call('open',path=str(path),_r7_fault=dict(validate=1,dependencies=['A_Dep','Z_Dep'],existing_dependency=key,close_names=['A_Dep','source']),error='native_error')
    assert 'A_Dep' in error['message'] and 'source' in error['message']
    assert set(facts()['documents']) == {key,'A_Dep','source'}
    for survivor in ('A_Dep','source'):
        value=call('inspect',document=survivor)
        assert value['available'] is False and value['objects'] == []
    assert 'Z_Dep' not in facts()['observers']
    check_keeper(key,state,native)
    call('open',path=str(path),_r7_fault=dict(close_names=['source']),error='native_error')
    assert set(facts()['documents']) == {key,'source'}
    check_keeper(key,state,native)
    opened=call('open',path=str(path)); assert opened['available'] is True
    call('close',document=opened['document'])
    call('document_settings',document=key,unit_system=state['units']['index'])
    check_keeper(key,state,native)
    results.append('multiple native dependencies and partial cleanup')

    # A different path and a reused native name cannot join a failed-load batch.
    other=output/'other.FCStd'; other.write_bytes(path.read_bytes())
    call('open',path=str(path),_r7_fault=dict(validate=1,close=1),error='native_error')
    call('open',path=str(other),_r7_fault=dict(validate=1,close=1),error='native_error')
    other_recovery=call('inspect',document='other')
    opened=call('open',path=str(path)); assert opened['available'] is True
    assert call('inspect',document='other') == other_recovery
    call('close',document=opened['document']); call('close',document='other')
    call('document_settings',document=key,unit_system=state['units']['index'])
    check_keeper(key,state,native)
    results.append('exact-path batch isolation')

    call('open',path=str(path),_r7_fault=dict(validate=1,noop_close=True),error='native_error')
    assert call('inspect',document='source')['available'] is False
    assert 'source' in facts()['observers']
    call('close',document='source')
    check_keeper(key,state,native)
    call('open',path=str(path),_r7_fault=dict(invalid_proxy=True,close=1),error='assembly_proxy_restore')
    value=call('inspect',document='source')
    assert value['available'] is False and value['objects'] == []
    opened=call('open',path=str(path)); assert opened['available'] is True
    call('close',document=opened['document'])
    call('document_settings',document=key,unit_system=state['units']['index'])
    check_keeper(key,state,native)
    results.append('native no-op Close and actual proxy-validation quarantine')

    # Repeated failure and explicit cleanup never reserve the path permanently.
    for index in range(2):
        call('open',path=str(path),_r7_fault=dict(validate=1,close=1),error='native_error')
        call('inspect',document='source'); call('close',document='source')
    accepted=call('open',path=str(path)); assert accepted['available'] is True
    call('close',document=accepted['document'])
    call('transaction',document=key,edits=[dict(op='set',object='Box',property='Width',value=17)])
    call('close',document=key)
    results.append('repeated failure, same-path reopen and later keeper write')

    # No previous owner must restore native ActiveDocument to None.
    assert facts()['active'] is None and facts()['documents'] == {}
    call('open',path=str(path),_r7_fault=dict(validate=1,close=1),error='native_error')
    assert facts()['active'] is None
    assert facts()['native_active'] is None
    call('inspect',document='source'); call('close',document='source')
    call('open',path=str(path),_r7_fault=dict(validate=1),error='native_error')
    assert facts()['active'] is None and facts()['documents'] == {}
    results.append('previous owner absent')
    call('new',name='FailedNew',_r7_fault=dict(validate=1,close=1),error='native_error')
    assert facts()['active'] is None and facts()['native_active'] is None
    assert call('inspect',document='FailedNew')['available'] is False
    call('new',name='FailedNew',error='name_conflict')
    call('close',document='FailedNew')
    assert call('new',name='FailedNew')['available'] is True
    call('close',document='FailedNew')
    results.append('failed New quarantine and same-name reuse')
    assert hashlib.sha256(path.read_bytes()).hexdigest() == saved_hash
    assert facts()['observers'] == [] and facts()['errors'] == [] and facts()['pending'] == [] and facts()['replaying'] == 0
    trace = receipt()['events']
    for i,event in enumerate(trace):
        if event['event']=='closed' and event['observer']:
            following=trace[i+1]
            assert following == dict(event='detach',name=event['name']), following
    (output/'results.json').write_text(json.dumps(dict(passed=results,source_exact=True,requests=len(events),capabilities=capabilities),indent=2))
    print(json.dumps(dict(passed=results,requests=len(events))))
finally:
    process.stdin.close()
    assert process.wait(timeout=20) == 0
