"""Document settings and quantity admission through the real Rust host."""
import argparse,json,subprocess
from pathlib import Path


def main():
    p=argparse.ArgumentParser();p.add_argument('--host',type=Path,required=True);p.add_argument('--runtime',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--faults',action='store_true')
    args=p.parse_args();out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
    runtime=json.loads(args.runtime.read_text());records=[];states={}
    cmd=[str(args.host.resolve()),'--executable',runtime['executable'],'--script',runtime['script']]
    for a in runtime['args']:cmd+=['--arg',a]
    proc=subprocess.Popen(cmd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,encoding='utf-8',creationflags=getattr(subprocess,'BELOW_NORMAL_PRIORITY_CLASS',0))
    def call(op,error=None,**kw):
        request=dict(op=op,id=len(records)+1,**kw);proc.stdin.write(json.dumps(request)+'\n');proc.stdin.flush()
        line=proc.stdout.readline();assert line,'Native host exited';r=json.loads(line);records.append(dict(request=request,response=r))
        (out/'events.json').write_text(json.dumps(records,indent=2))
        if error:
            assert not r['ok'] and r['error']['code']==error,r
            return r
        assert r['ok'],r
        result=r['result'];state=result.get('state',result)
        if 'revision' in state and 'document' in state:states[state['document']]=state
        return result
    def mutate(op,doc,error=None,**kw):
        kw.setdefault('expected_revision',states[doc]['revision']);return call(op,error,document=doc,**kw)
    def inspect(doc):return call('inspect',document=doc)
    try:
        for name in ('UnitsA','UnitsB'):
            call('new',name=name)
            mutate('transaction',name,edits=[dict(op='add_object',type='Part::Box',name='Box'),dict(op='set',object='Box',property='Length',value=25.4)])
        d='UnitsA';base=inspect(d)
        call('document_settings','missing_revision',document=d,unit_system=3)
        for revision in (True,False,0,-1,str(base['revision']),float(base['revision']),[],{}):
            call('document_settings','stale_revision',document=d,unit_system=3,expected_revision=revision)
        for index in (True,-1,10,'3',None):mutate('document_settings',d,'invalid_request',unit_system=index)
        assert inspect(d)==base
        shape=base['objects'][0]['shape']
        for index in range(10):
            before=inspect(d);state=mutate('document_settings',d,unit_system=index)
            assert state['units']['index']==index and state['objects'][0]['shape']==shape
            assert (state['undo_count'],state['redo_count'])==(before['undo_count'],before['redo_count'])
            if before['units']['index']==index:assert state['revision']==before['revision']
            else:assert state['revision']>before['revision']
            assert mutate('document_settings',d,unit_system=index)==state
            q=call('quantities',document=d,expected_revision=state['revision'],items=[dict(value=value,unit='mm') for value in (25.4,123.4567890123456,1e-24,1e30)]+[dict(value=1,unit='rad'),dict(value=0.5,unit='')])
            assert inspect(d)==state
            call('quantities','invalid_quantity',document=d,expected_revision=state['revision'],items=[dict(text='2 mm',unit='')])
            suffix=q['items'][0]['default_unit']
            call('quantities',document=d,expected_revision=state['revision'],items=[dict(text='2',unit='mm',default_unit=suffix)])
        mutate('document_settings',d,unit_system=3)
        mutate('transaction',d,edits=[dict(op='set',object='Box',property='Length',value=50.8)])
        mutate('undo',d);before=inspect(d);assert before['redo_count']==1
        mutate('document_settings',d,unit_system=4);mutate('redo',d);assert states[d]['units']['index']==4
        mutate('undo',d);assert states[d]['units']['index']==4
        # All settings writes, even a same-value write, respect global leases.
        mutate('preview',d,'preview_redo_conflict',token='units-preview',edits=[dict(op='set',object='Box',property='Length',value=30)])
        mutate('redo',d)
        mutate('preview',d,token='units-preview',edits=[dict(op='set',object='Box',property='Length',value=30)])
        active=inspect(d)
        mutate('document_settings',d,'preview_active',unit_system=4)
        mutate('document_settings','UnitsB','preview_active',unit_system=0)
        # Read-only cached schemas and quantity queries do not create scratch docs
        # or interfere with the application-wide active preview transaction.
        for i in range(3):
            schema=call('feature_schema',document=d,expected_revision=active['revision'],type='Part::Box',values={'Length':'1 in'})
            assert schema['property_metadata']['Length']['quantity']['default_unit']=='cm'
            call('quantities',document=d,expected_revision=active['revision'],items=[dict(text='1 ft 2 in',unit='mm')])
        assert inspect(d)==active
        mutate('preview_cancel',d,token='units-preview');assert states[d]['redo_count']==0
        mutate('document_settings',d,unit_system=3)
        # Numeric agents stay in mm; new text inputs carry explicit native context.
        mutate('transaction',d,edits=[dict(op='set',object='Box',property='Length',value={'quantity_input':'2','default_unit':'in'})])
        assert states[d]['objects'][0]['properties']['Length']==50.8
        before=inspect(d)
        mutate('transaction',d,'dimension_mismatch',edits=[dict(op='set',object='Box',property='Length',value={'quantity_input':'2 s','default_unit':'in'})])
        assert inspect(d)['objects'][0]['properties']['Length']==50.8
        mutate('transaction',d,edits=[dict(op='set',object='Box',property='Length',value=2)])
        assert states[d]['objects'][0]['properties']['Length']==2
        mutate('save',d,path=str(out/'numeric-contract.FCStd'))
        # Native expressions retain their exact source and numeric base semantics.
        mutate('transaction',d,edits=[dict(op='expression',object='Box',property='Length',expression='1 in')])
        assert states[d]['objects'][0]['expressions']['Length']=='1 in'
        mutate('save',d,path=str(out/'expression.FCStd'))
        for name in ('UnitsA','UnitsB'):mutate('close',name)
        if args.faults:
            for name in ('FailPrepare','FailBump','FailRestore'):
                call('new',name=name);before=inspect(name)
                mutate('document_settings',name,'native_error',unit_system=3)
                restored=inspect(name)
                assert restored['units']==before['units'] and restored['revision']==before['revision']
                assert (restored['undo_count'],restored['redo_count'])==(before['undo_count'],before['redo_count'])
                if name=='FailRestore':
                    assert not restored['available']
                    mutate('document_settings',name,'document_unavailable',unit_system=0)
                    call('quantities','document_unavailable',document=name,expected_revision=restored['revision'],items=[])
                else:assert restored==before
                mutate('close',name)
    finally:
        proc.stdin.close();assert proc.wait(timeout=20)==0
    (out/'report.json').write_text(json.dumps(dict(passed=True,requests=len(records),scenarios=['10 schemas','same-value revision/history','revision admission','local/global leases','native preferred default','native dimensions','active cache reads','legacy numeric mm','expressions','saved native']),indent=2))

if __name__=='__main__':main()
