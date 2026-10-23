"""Pad/Pocket inactive task values, expressions, downstream history and Redo.

Requires only the pinned runtime and the actual shared UI host/compiler binaries.
All native documents are generated in --output. No GUI or existing model writes.
"""
import argparse
import json
from pathlib import Path
import subprocess
from native_workflows import objects,transaction,set_value,near
from preview_workflows import PreviewSession


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for flag in ['host','compiler','runtime','output']:p.add_argument('--'+flag,type=Path,required=True)
    args=p.parse_args();out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
    runtime=json.loads(args.runtime.read_text(encoding='utf-8-sig'))
    subprocess.run([runtime['executable'],*runtime['args'],str(Path(__file__).with_name('extrusion_termination_workflows.py').resolve()),'--native-fixtures',str(out/'inputs')],check=True)
    fixtures=json.loads((out/'inputs/fixtures.json').read_text())
    def compiled(request,mode='--native-properties'):
        r=subprocess.run([str(args.compiler.resolve()),mode,'-'],input=json.dumps(request),capture_output=True,text=True,check=True)
        return json.loads(r.stdout)
    def task(kind,props,create=False,expressions=None,tip='Result'):
        return dict(type='PartDesign::'+kind,name='Result',body='Body',create=create,
                    properties=props,expressions=expressions or {},tip=tip,tip_visibility=tip!='Finish')
    records=[];s=PreviewSession(args.host.resolve(),runtime)
    try:
        for kind in ['Pad','Pocket']:
            fixture=fixtures[kind]
            front='Target' if kind=='Pad' else 'OtherTarget'
            face=fixture['targets'][front]['upper' if kind=='Pad' else 'lower']
            d=s.call('open',path=fixture['path'])['document']
            baseline=transaction(s,d,'Original dimension',compiled(task(kind,dict(Profile={'link':'Profile'},Length=20),True)))
            s.call('transaction',document=d,label='Computed shape stays protected',
                   edits=[set_value('Result','Shape',None)],expect_error='read_only_property')
            s.call('inspect',document=d)
            s.call('transaction',document=d,label='Sketch data stays protected',
                   edits=[set_value('Profile','Geometry',[])],expect_error='read_only_property')
            s.call('inspect',document=d)
            token=kind+'-retention'
            props=dict(SideType='Two sides',Type='Length',Type2='Length',Length=20,Length2=22,
                       Offset2=3,UseCustomVector=True,Direction=[0.2,0,1],AlongSketchNormal=True)
            expressions={'Length2':'Length + 2 mm'}
            request=task(kind,props,expressions=expressions)
            s.call('preview',document=d,token=token,label='Two sides',edits=compiled(request))
            # This is the same accumulating override map retained by the form.
            props.update(SideType='One side',Type='UpToFace',UpToFace={'link':front,'subelements':[face]},UseCustomVector=False)
            inactive=s.call('preview',document=d,token=token,label='One side target',edits=compiled(request))['state']
            obj=objects(inactive)['Result']
            assert obj['property_metadata']['Length2']['read_only'] is True
            assert obj['property_metadata']['Length2']['task_editable'] is True
            assert obj['properties']['Length2']==22 and obj['properties']['Offset2']==3
            assert obj['expressions']['Length2']=='Length + 2 mm'
            schema=compiled(dict(schema=obj,parameters=request),'--extrusion-schema')
            assert schema['property_metadata']['Length2']['hidden'] is True
            assert schema['property_metadata']['Direction']['hidden'] is True
            assert schema['properties']['Length2']==22 and schema['properties']['Offset2']==3
            # Native stored dormant values and expression survive successful
            # replacement previews, then the task returns to those controls.
            props.update(SideType='Two sides',Type='Length',UseCustomVector=True)
            restored=s.call('preview',document=d,token=token,label='Return to dimensions',edits=compiled(request))['state']
            assert objects(restored)['Result']['properties']['Length2']==22
            canceled=s.call('preview_cancel',document=d,token=token)
            assert objects(canceled)['Result']['properties']['SideType']=='One side'
            assert canceled['undo_count']==baseline['undo_count']
            s.call('preview',document=d,token=token,label='Two sides',edits=compiled(request))
            accepted=s.call('preview_commit',document=d,token=token)
            assert accepted['undo_count']==baseline['undo_count']+1
            undone=s.call('undo',document=d)
            assert objects(undone)['Result']['properties']['SideType']=='One side'
            # Schema and edit construction used by a deferred task perform no
            # document writes; no prototype objects or new native transaction.
            compiled(dict(schema=objects(undone)['Result'],parameters=request),'--extrusion-schema')
            compiled(request)
            unchanged=s.call('inspect',document=d)
            assert unchanged['revision']==undone['revision'] and unchanged['redo_count']==undone['redo_count']==1
            redone=s.call('redo',document=d)
            assert objects(redone)['Result']['properties']['Length2']==22
            saved=out/(kind+'-retained.FCStd');s.call('save',document=d,path=str(saved));s.call('close',document=d)
            reopened=s.call('open',path=str(saved));d=reopened['document']
            assert objects(reopened)['Result']['properties']['Length2']==22
            assert objects(reopened)['Result']['expressions']['Length2']=='Length + 2 mm'
            # Edit the upstream extrusion while a later native feature is Tip.
            finish=dict(type='PartDesign::AdditiveBox',name='Finish',body='Body',create=True,
                        properties=dict(Length=2,Width=2,Height=2,Placement=dict(position=[0,0,15],rotation=[0,0,0,1])),expressions={})
            transaction(s,d,'Downstream feature',compiled(finish))
            before=transaction(s,d,'Hide tip',[set_value('Finish','Visibility',False)])
            request=task(kind,dict(Profile={'link':'Profile'},Type='UpToFace',UpToFace={'link':front,'subelements':[face]}),tip='Finish')
            result=transaction(s,d,'Edit upstream target',compiled(request))
            assert objects(result)['Body']['body_state']['tip']=='Finish'
            assert objects(result)['Finish']['properties']['Visibility'] is False
            # A dependent target cannot disappear in a partially applied batch.
            bad=[set_value('Result','Offset','2 mm'),dict(op='remove_object',object=front)]
            s.call('transaction',document=d,label='Reject target deletion',edits=bad,expect_error='dependent_objects')
            recovered=s.call('inspect',document=d)
            near(objects(recovered)['Result']['properties']['Offset'],objects(result)['Result']['properties']['Offset'])
            assert front in objects(recovered)
            near(objects(s.call('undo',document=d))['Finish']['shape']['volume'],objects(before)['Finish']['shape']['volume'])
            final=s.call('redo',document=d)
            assert objects(final)['Body']['body_state']['tip']=='Finish'
            assert objects(final)['Finish']['properties']['Visibility'] is False
            final_path=out/(kind+'-downstream.FCStd');s.call('save',document=d,path=str(final_path));s.call('close',document=d)
            final=s.call('open',path=str(final_path));d=final['document']
            assert objects(final)['Body']['body_state']['tip']=='Finish'
            assert objects(final)['Finish']['properties']['Visibility'] is False
            records.append(dict(kind=kind,inactive_inputs_retained=True,expressions_retained=True,
                                preview_cancel=True,single_undo_step=True,redo_preserved=True,
                                downstream_tip_retained=True,dependent_target_atomic_recovery=True,
                                saved=str(final_path)))
            s.call('close',document=d)
        (out/'report.json').write_text(json.dumps(records,indent=2))
        print(json.dumps(records,indent=2))
    finally:s.close()

if __name__=='__main__':main()
