"""New shared-host corners; run only after ROOT RESOURCE COMPLETE."""
import json,os,sys,traceback
from pathlib import Path
import argparse
p=argparse.ArgumentParser()
for key in ['host','runtime','output']:p.add_argument('--'+key,type=Path,required=True)
a=p.parse_args();out=a.output;out.mkdir(parents=True,exist_ok=False)
source=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(source/'tests'))
from sketch_editor_workflows import SketchSession
from native_workflows import objects,transaction
runtime=json.loads(a.runtime.read_text())
report={'events':[],'checks':{}}
class Recorded(SketchSession):
    def call(self,op,expect_error=None,**params):
        response=super().call(op,expect_error=expect_error,**params)
        report['events'].append(dict(op=op,expect_error=expect_error,parameters=params,response=response))
        (out/'report.json').write_text(json.dumps(report,indent=2))
        return response
def facts(state):
    return {n:{k:v for k,v in o.items() if k not in ('state','status')} for n,o in objects(state).items()}
s=Recorded(a.host.resolve(),runtime)
try:
    doc=s.call('new',name='ReviewCopyConsumed')['document']
    state=transaction(s,doc,'Consumed circle',[
        dict(op='add_object',type='PartDesign::Body',name='Body'),
        dict(op='set',object='Body',property='AllowCompound',value=False),
        dict(op='add_object',type='Sketcher::SketchObject',name='Sketch',body='Body'),
        dict(op='geometry',object='Sketch',kind='circle',center=[5,5,0],radius=2),
        dict(op='constraint',object='Sketch',kind='Radius',args=[0,2]),
        dict(op='constraint_state',object='Sketch',index=0,name='R'),
        dict(op='expression',object='Sketch',property='Constraints.R',expression='2 mm'),
        dict(op='geometry',object='Sketch',kind='point',position=[2,3,0],construction=True),
        dict(op='add_object',type='PartDesign::Pad',name='Pad',body='Body'),
        dict(op='set',object='Pad',property='Profile',value={'link':'Sketch'}),
        dict(op='set',object='Pad',property='Length',value=5)])
    state=transaction(s,doc,'Follower edge',[
        dict(op='add_object',type='Sketcher::SketchObject',name='Follower'),
        dict(op='external_geometry',object='Follower',source='Sketch',subelement='Edge1')])
    # Retain the live baseline for live recovery checks, and its serialized
    # counterpart for an Open-vs-Open control at the end. Do not normalize away
    # native restore initialization or cached map-version differences.
    baseline_path=out/'baseline.FCStd'
    s.call('save',document=doc,path=str(baseline_path));baseline=state
    # A failed ordinary transaction must restore a registered Move together
    # with native point metadata and external followers.
    s.call('transaction',document=doc,label='Move then invalid index',edits=[
        dict(op='move_geometries',object='Sketch',indices=[0,1],displacement=[3,4,0]),
        dict(op='copy_geometry',object='Sketch',indices=[-3],displacement=[1,0,0])],expect_error='invalid_request')
    state=s.call('inspect',document=doc)
    assert facts(state)==facts(baseline),'failed ordinary Move transaction changed model'
    report['checks']['move_then_invalid_transaction_atomic']=True
    s.call('sketch_begin',document=doc,object='Sketch',token='review-copy')
    state=s.call('inspect',document=doc);base=facts(state)
    state=s.call('preview',document=doc,token='candidate',edits=[
        dict(op='move_geometries',object='Sketch',indices=[0,1],displacement=[3,4,0])])['state']
    accepted=facts(state)
    s.call('preview',document=doc,token='candidate',edits=[
        dict(op='move_geometries',object='Sketch',indices=[0,1],displacement=[30,40,0]),
        dict(op='clone_geometry',object='Sketch',indices=[-3],displacement=[1,0,0])],expect_error='invalid_request')
    state=s.call('inspect',document=doc);assert facts(state)==accepted,'failed Move replacement changed accepted candidate'
    state=s.call('preview_cancel',document=doc,token='candidate');assert facts(state)==base,'Move cancel changed baseline'
    report['checks']['move_failed_replacement_then_cancel_exact']=True
    transaction(s,doc,'Clone disjoint consumed profile',[
        dict(op='clone_geometry',object='Sketch',indices=[0],displacement=[10,0,0])])
    s.call('sketch_finish',document=doc,token='review-copy',expect_error='invalid_model')
    state=s.call('inspect',document=doc)
    assert state['sketch_edit']['token']=='review-copy'
    assert state.get('available') is not False
    assert objects(state)['Sketch']['sketch']['geometry_count']==3
    s.call('undo',document=doc)
    state=s.call('sketch_finish',document=doc,token='review-copy')
    assert state['sketch_edit'] is None
    assert facts(state)==facts(baseline),'repair after invalid Finish differs from baseline'
    report['checks']['consumed_clone_invalid_finish_undo_repair']=True
    # A different failed Finish can still cancel the whole session.
    s.call('sketch_begin',document=doc,object='Sketch',token='review-copy-cancel')
    transaction(s,doc,'Copy disjoint consumed profile',[
        dict(op='copy_geometry',object='Sketch',indices=[0],displacement=[10,0,0])])
    s.call('sketch_finish',document=doc,token='review-copy-cancel',expect_error='invalid_model')
    state=s.call('inspect',document=doc)
    state=s.call('sketch_cancel',document=doc,token='review-copy-cancel')
    assert facts(state)==facts(baseline),'Cancel after invalid Finish differs from baseline'
    report['checks']['consumed_copy_invalid_finish_cancel']=True
    path=out/'recovered.FCStd'
    s.call('save',document=doc,path=str(path));s.call('close',document=doc)
    state=s.call('open',path=str(path));recovered_open=facts(state)
    s.call('close',document=state['document'])
    control=s.call('open',path=str(baseline_path))
    assert recovered_open==facts(control),'fresh worker Open differs from phase-matched baseline Open'
    report['checks']['fresh_open_after_failed_finish_repair']=True
    report['ok']=True
except Exception:
    report['ok']=False;report['error']=traceback.format_exc()
finally:
    s.close()
    report['host_exit_code']=s.process.returncode
    (out/'report.json').write_text(json.dumps(report,indent=2))
print(json.dumps({k:v for k,v in report.items() if k!='events'},indent=2))
raise SystemExit(0 if report.get('ok') else 1)
