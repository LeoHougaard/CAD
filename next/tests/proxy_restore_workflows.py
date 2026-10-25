"""Focused native Assembly proxy admission, atomic Open and manual recovery."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from native_workflows import Session, add, objects, transaction

HERE=Path(__file__).resolve().parent
SUPPORT=HERE/'fixtures/proxy_restore_support.py'
ORIGINAL=HERE/'fixtures/proxy_restore_original_fault_worker.py'


def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class Recorded(Session):
    def __init__(self,host,runtime,folder):
        self.folder=folder;self.events=[]
        super().__init__(host,runtime)

    def raw(self,op,**parameters):
        doc=parameters.get('document')
        if op in {'transaction','undo','redo','save','close','export_exchange'} and doc in self.revisions:
            parameters.setdefault('expected_revision',self.revisions[doc])
        request=dict(op=op,**parameters)
        self.process.stdin.write(json.dumps(request)+'\n');self.process.stdin.flush()
        line=self.process.stdout.readline();assert line,'host exited'
        response=json.loads(line)
        self.events.append(dict(request=request,response=response))
        (self.folder/'events.json').write_text(json.dumps(self.events,indent=2))
        if response['ok']:
            result=response['result']
            if 'document' in result and 'revision' in result:self.revisions[result['document']]=result['revision']
            if op=='close':self.revisions.pop(doc,None)
        return response

    def call(self,op,expect_error=None,**parameters):
        response=self.raw(op,**parameters)
        if expect_error:
            assert not response['ok'] and response['error']['code']==expect_error,response
            return response['error']
        assert response['ok'],response
        return response['result']


def native(runtime,arguments,log):
    command=[runtime['executable'],*runtime['args'],str(SUPPORT),*map(str,arguments)]
    with log.open('x',encoding='utf-8') as out:
        p=subprocess.run(command,stdout=out,stderr=subprocess.STDOUT,timeout=45,
            **({'creationflags':subprocess.BELOW_NORMAL_PRIORITY_CLASS} if os.name=='nt' else {}))
    assert p.returncode==0,(command,log)


def session(args,folder,mode='healthy',original=False):
    folder.mkdir()
    runtime=dict(args.runtime_data,script=str(ORIGINAL if original else SUPPORT))
    os.environ.update(PROXY_TEST_BACKEND=str(args.backend),PROXY_TEST_MODE=mode,
        PROXY_TEST_DIAGNOSTIC=str(folder/'native.jsonl'),PROXY_TEST_TRIGGER=str(folder/'trigger.json'),
        R2_BACKEND=str(args.backend))
    return Recorded(args.host,runtime,folder)


def keeper(s):
    doc=s.call('new',name='Keeper')['document']
    transaction(s,doc,'Keeper solid',[add('Part::Box','Box')])
    path=s.folder/'Keeper.FCStd';s.call('save',document=doc,path=str(path))
    dirty=transaction(s,doc,'Unsaved keeper edit',[dict(op='set',object='Box',property='Length',value=23)])
    return doc,dirty,path,sha(path)


def reject_open(args,fixtures,label,mode='healthy',filename='healthy.FCStd',original=False):
    s=session(args,args.output/label,mode,original)
    try:
        doc,before,path,checksum=keeper(s)
        native_before=None if original else json.loads((s.folder/'native.jsonl').read_text().splitlines()[-1])
        source=fixtures/filename;source_hash=sha(source)
        error=s.call('open',path=str(source),expect_error='assembly_proxy_restore')
        assert 'Close and reopen' in error['message']
        after=s.call('inspect',document=doc)
        assert after==before and sha(path)==checksum and sha(source)==source_hash
        if not original:
            native_after=json.loads((s.folder/'native.jsonl').read_text().splitlines()[-1])
            assert native_after['documents']==native_before['documents']
            assert native_after['active']==native_before['active']=='Keeper'
        s.call('inspect',document=source.stem,expect_error='not_found')
        # Failed publication registers no native document or revision owner.
        reused=s.call('new',name=source.stem)
        s.call('close',document=reused['document'])
        s.call('save',document=doc,path=str(s.folder/'stale.FCStd'),
            expected_revision=before['revision']-1,expect_error='stale_revision')
        assert not (s.folder/'stale.FCStd').exists()
        s.call('close',document=doc)
    finally:s.close()


def original_regression(args,fixtures,baseline):
    s=session(args,args.output/('original-baseline' if baseline else 'original-fixed'),original=True)
    source=fixtures/'healthy.FCStd';checksum=sha(source)
    try:
        response=s.raw('open',path=str(source))
        if not response['ok']:
            assert not baseline and response['error']['code']=='assembly_proxy_restore',response
            assert sha(source)==checksum
            s.call('inspect',document=source.stem,expect_error='not_found')
            replacement=s.call('new',name=source.stem)
            s.call('close',document=replacement['document'])
            return
        assert baseline,response
        doc=response['result']['document']
        saved=s.folder/'HostSaved.FCStd'
        s.call('save',document=doc,path=str(saved));s.call('close',document=doc)
    finally:s.close()
    result=s.folder/'healthy-reopen.json'
    native(args.runtime_data,['--check-native',saved,result],s.folder/'healthy-reopen.log')
    lost=json.loads(result.read_text())
    assert lost['joint_proxy_none'] and lost['ground_proxy_none']
    assert sha(source)==checksum
    assert not response['ok'],'Failed JointObject restore must not publish a successful editable document'


def normal_parts_and_first_failure(args,fixtures):
    s=session(args,args.output/'parts-and-first-failure',original=True)
    try:
        state=s.call('open',path=str(fixtures/'part.FCStd'));doc=state['document']
        assert s.capabilities['assembly']['joint_types']==json.loads((fixtures/'native-contract.json').read_text())['joint_types']
        # Every request here runs with the exact original import blocker active.
        s.call('mesh',document=doc,object='Outside');s.call('topology',document=doc,object='Outside')
        s.call('inspect',document=doc)
        before=transaction(s,doc,'Insert without proxy import',[dict(op='assembly_insert',assembly='EmptyAssembly',
            name='First',source='Source',placement=dict(position=[0,0,0],rotation=[0,0,0,1]))])
        s.call('transaction',document=doc,label='Rejected first grounding',edits=[dict(op='assembly_ground',
            assembly='EmptyAssembly',object='First',name='Ground',grounded=True)],expect_error='native_error')
        after=s.call('inspect',document=doc)
        assert after['objects']==before['objects'] and after['undo_count']==before['undo_count']
        assert after['preview'] is None and after['revision']>before['revision']
        s.call('save',document=doc,path=str(s.folder/'stale.FCStd'),expected_revision=before['revision'],expect_error='stale_revision')
        transaction(s,doc,'Part edit after import failure',[dict(op='set',object='Source',property='Length',value=17)])
        s.call('save',document=doc,path=str(s.folder/'PartSaved.FCStd'))
        s.call('export_exchange',document=doc,objects=['Outside'],path=str(s.folder/'Part.step'))
        s.call('close',document=doc)
    finally:s.close()


def healthy_history(args,fixtures):
    s=session(args,args.output/'healthy-history')
    try:
        state=s.call('open',path=str(fixtures/'healthy.FCStd'));doc=state['document']
        original=objects(state)['Joint']['expressions']
        edited=transaction(s,doc,'Edit restored native joint',[dict(op='assembly_joint',assembly='Assembly',name='Joint',
            create=False,joint_type='Fixed',reference1=dict(link='First',subelements=['','']),
            reference2=dict(link='Second',subelements=['','']),properties={},expressions={'Offset1.Base.z':'Source.Length / 2'})])
        accepted=objects(edited)['Joint']['expressions'];assert accepted!=original
        assert objects(s.call('undo',document=doc))['Joint']['expressions']==original
        assert objects(s.call('redo',document=doc))['Joint']['expressions']==accepted
        s.call('save',document=doc,path=str(s.folder/'Edited.FCStd'))
        s.call('close',document=doc)
    finally:s.close()
    native(args.runtime_data,['--check-native',s.folder/'Edited.FCStd',s.folder/'fresh-native.json'],s.folder/'fresh-native.log')
    state=json.loads((s.folder/'fresh-native.json').read_text())
    assert state['joint_is_native'] and state['ground_is_native'] and abs(state['offset1_z']-5)<1e-8
    assert state['reference1']==['First',['','']] and state['reference2']==['Second',['','']]


def live_admission_and_recovery(args,fixtures):
    s=session(args,args.output/'live-admission')
    try:
        keep,keep_state,keep_path,keep_hash=keeper(s)
        state=s.call('open',path=str(fixtures/'healthy.FCStd'));doc=state['document'];revision=state['revision']
        existing=s.folder/'Existing.FCStd';s.call('save',document=doc,path=str(existing));revision=s.revisions[doc]
        existing_hash=sha(existing)
        sentinel=s.folder/'Existing.step';sentinel.write_bytes(b'Preserve existing export bytes\n');sentinel_hash=sha(sentinel)
        (s.folder/'trigger.json').write_text(json.dumps(dict(document=doc,object='Ground')))
        s.call('save',document=doc,path=str(existing),expected_revision=revision-1,expect_error='stale_revision')
        for op,parameters in [('inspect',{}),('mesh',dict(object='First')),('topology',dict(object='First')),
            ('save',dict(path=str(existing))),('save',dict(path=str(s.folder/'Denied.FCStd'))),
            ('transaction',dict(label='Denied before native transaction',edits=[dict(op='set',object='Source',property='Length',value=99)])),
            ('undo',{}),('redo',{}),
            ('export_exchange',dict(objects=['First'],path=str(sentinel),overwrite=True))]:
            s.call(op,document=doc,expect_error='assembly_proxy_restore',**parameters)
        assert s.revisions[doc]==revision and sha(existing)==existing_hash and sha(sentinel)==sentinel_hash
        assert not (s.folder/'Denied.FCStd').exists() and not list(s.folder.glob('.cad-exchange-*'))
        assert s.call('inspect',document=keep)==keep_state and sha(keep_path)==keep_hash
        s.call('close',document=doc,expected_revision=revision-1,expect_error='stale_revision')
        s.call('close',document=doc)
        # Close/reopen is explicit recovery. No request assigns or guesses a proxy.
        reopened=s.call('open',path=str(existing));assert objects(reopened)['Joint']['valid']
        s.call('close',document=reopened['document']);s.call('close',document=keep)
    finally:s.close()


def open_raises_cleanup(args,fixtures):
    s=session(args,args.output/'open-exception','open_raises')
    try:
        doc,before,path,checksum=keeper(s)
        native_before=json.loads((s.folder/'native.jsonl').read_text().splitlines()[-1])
        s.call('open',path=str(fixtures/'healthy.FCStd'),expect_error='native_error')
        assert s.call('inspect',document=doc)==before and sha(path)==checksum
        native_after=json.loads((s.folder/'native.jsonl').read_text().splitlines()[-1])
        assert native_after['documents']==native_before['documents'] and native_after['active']=='Keeper'
        s.call('close',document=doc)
    finally:s.close()


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--host',required=True,type=Path);p.add_argument('--runtime',required=True,type=Path)
    p.add_argument('--output',required=True,type=Path);p.add_argument('--baseline',action='store_true')
    p.add_argument('--fixtures',type=Path)
    args=p.parse_args();args.output=args.output.resolve();args.output.mkdir(parents=True,exist_ok=False)
    if args.baseline and args.fixtures is None:p.error('--baseline requires fixtures generated with the fixed candidate')
    args.runtime_data=json.loads(args.runtime.read_text(encoding='utf-8-sig'))
    args.backend=Path(args.runtime_data['script']).resolve().parent
    os.environ['PYTHONDONTWRITEBYTECODE']='1'
    fixtures=args.fixtures.resolve() if args.fixtures else args.output/'fixtures'
    if args.fixtures is None:
        fixtures.mkdir();native(args.runtime_data,['--fixtures',args.backend,fixtures],args.output/'fixtures.log')
    original_regression(args,fixtures,args.baseline)
    if args.baseline:raise AssertionError('baseline must fail original regression')
    normal_parts_and_first_failure(args,fixtures)
    healthy_history(args,fixtures)
    for label,file in [('missing-joint','missing_joint.FCStd'),('missing-ground','missing_ground.FCStd'),('missing-both','missing_both.FCStd')]:
        reject_open(args,fixtures,label,filename=file)
    reject_open(args,fixtures,'partial-joint','partial_joint')
    reject_open(args,fixtures,'partial-ground','partial_ground')
    reject_open(args,fixtures,'original-blocker-preserves-keeper',original=True)
    live_admission_and_recovery(args,fixtures)
    open_raises_cleanup(args,fixtures)
    (args.output/'report.json').write_text(json.dumps(dict(passed=True,original_fault_rejected=True,
        missing_joint_ground_and_both_rejected=True,partial_restore_rejected=True,atomic_open=True,
        existing_part_preserved=True,parts_stay_deferred=True,first_authoring_failure_preserved=True,
        native_history_and_fresh_reopen=True,read_save_export_admission=True,manual_close_reopen=True),indent=2))
    print('Focused native proxy restoration, admission, atomic cleanup and recovery passed.')


if __name__=='__main__':main()
