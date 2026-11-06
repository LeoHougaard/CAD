"""Public Rust/native checkpoint history, references, failure and Carbon Copy gate."""
import argparse,json,platform,shutil,sys
from pathlib import Path
from native_workflows import add,constraint,objects,transaction
from sketch_editor_workflows import SketchSession

def native(state,name='Sketch'):return objects(state)[name]['sketch']['native_checkpoint_state']
def main():
    parser=argparse.ArgumentParser()
    for key in ['host','runtime','output']:parser.add_argument('--'+key,type=Path,required=True)
    args=parser.parse_args();out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
    runtime=json.loads(args.runtime.read_text());runtime['script']=str(Path(__file__).parent/'fixtures/checkpoint_linux_worker.py')
    s=SketchSession(args.host.resolve(),runtime);report={}
    try:
        assert s.capabilities['sketch_checkpoints']['available'] is True
        assert s.capabilities['sketch_tasks']['mirror']['create_symmetry_constraints']['available'] is (sys.platform=='win32')
        doc=s.call('new',name='LinuxCheckpointPublic')['document']
        state=transaction(s,doc,'Native consumed circle',[
            add('PartDesign::Body','Body'),add('Sketcher::SketchObject','Sketch','Body'),
            dict(op='geometry',object='Sketch',kind='circle',center=[5,5,0],radius=2),constraint('Sketch','Radius',0,2.),
            dict(op='constraint_state',object='Sketch',index=0,name='SourceRadius'),
            dict(op='expression',object='Sketch',property='Constraints.SourceRadius',expression='2 mm'),
            dict(op='set',object='Body',property='AllowCompound',value=True),add('PartDesign::Pad','Pad','Body'),
            dict(op='set',object='Pad',property='Profile',value={'link':'Sketch'}),dict(op='set',object='Pad',property='Length',value=5)])
        state=transaction(s,doc,'Persistent follower',[add('Sketcher::SketchObject','Follower'),
            dict(op='external_geometry',object='Follower',source='Sketch',subelement='Edge1')])
        source=native(state);follower=native(state,'Follower')
        s.call('sketch_begin',document=doc,object='Sketch',token='session')
        mirror=dict(op='mirror_geometry',object='Sketch',indices=[0],reference=dict(index=-1,point=0))
        state=transaction(s,doc,'Accepted Mirror',[mirror]);accepted=native(state)
        for ref in [dict(index=-2,point=0),dict(index=-1,point=1),dict(index=-2,point=0)]:
            state=s.call('preview',document=doc,token='later',edits=[dict(mirror,reference=ref)])['state']
            assert native(state)['ids'][:len(accepted['ids'])]==accepted['ids']
        preview=native(state)
        s.call('preview',document=doc,token='later',edits=[dict(op='geometry',object='Sketch',kind='circle',center=[20,20,0],radius=1),
            dict(op='set',object='Sketch',property='Missing',value=1)],expect_error='unsupported_property')
        assert native(s.call('inspect',document=doc))==preview
        state=s.call('preview_cancel',document=doc,token='later');assert native(state)==accepted and native(state,'Follower')==follower
        state=s.call('undo',document=doc);assert native(state)==source
        # An unaccepted task preview/Cancel must preserve the action Redo prefix.
        s.call('preview',document=doc,token='redo-cancel',edits=[mirror])
        state=s.call('preview_cancel',document=doc,token='redo-cancel');assert native(state)==source and state['redo_count']==1
        state=s.call('redo',document=doc);assert native(state)==accepted
        s.call('sketch_finish',document=doc,token='session')
        state=s.call('undo',document=doc);assert native(state)==source
        state=s.call('redo',document=doc);assert native(state)==accepted
        path=out/'checkpoint.FCStd';s.call('save',document=doc,path=str(path));s.call('close',document=doc)
        state=s.call('open',path=str(path));doc=state['document'];assert native(state)==accepted and native(state,'Follower')==follower
        s.call('sketch_begin',document=doc,object='Sketch',token='cancel')
        state=transaction(s,doc,'Accepted ordinary circle',[dict(op='geometry',object='Sketch',kind='circle',center=[15,10,0],radius=1,construction=True)])
        ordinary=native(state);s.call('preview',document=doc,token='later',edits=[mirror]);state=s.call('preview_cancel',document=doc,token='later')
        assert native(state)==ordinary;s.call('undo',document=doc);state=s.call('redo',document=doc);assert native(state)==ordinary
        state=s.call('sketch_cancel',document=doc,token='cancel');assert native(state)==accepted
        s.call('sketch_begin',document=doc,object='Sketch',token='invalid')
        state=transaction(s,doc,'Duplicate consumed circle',[dict(mirror,reference=dict(index=0,point=3))]);bad=native(state)
        s.call('sketch_finish',document=doc,token='invalid',expect_error='invalid_model');assert native(s.call('inspect',document=doc))==bad
        s.call('undo',document=doc);state=s.call('redo',document=doc);assert native(state)==bad
        state=s.call('sketch_cancel',document=doc,token='invalid');assert native(state)==accepted
        s.call('close',document=doc);report['history']=dict(source=source,accepted=accepted,follower=follower,exact=True)
        doc=s.call('new',name='LinuxCarbonCopy')['document']
        transaction(s,doc,'Source',[add('PartDesign::Body','Body'),add('Sketcher::SketchObject','Source','Body'),
            dict(op='geometry',object='Source',kind='circle',center=[5,5,0],radius=2),constraint('Source','Radius',0,2.),
            dict(op='constraint_state',object='Source',index=0,name='SourceRadius'),add('Sketcher::SketchObject','Copy','Body')])
        s.call('sketch_begin',document=doc,object='Copy',token='carbon')
        state=transaction(s,doc,'Carbon Copy',[dict(op='carbon_copy',object='Copy',source='Source',construction=False)]);carbon=native(state,'Copy')
        s.call('preview',document=doc,token='later',edits=[dict(mirror,object='Copy')]);state=s.call('preview_cancel',document=doc,token='later');assert native(state,'Copy')==carbon
        s.call('undo',document=doc);state=s.call('redo',document=doc);assert native(state,'Copy')==carbon
        s.call('sketch_finish',document=doc,token='carbon')
        state=transaction(s,doc,'Source propagates',[dict(op='datum',object='Source',index=0,value='3 mm')]);assert objects(state)['Copy']['sketch']['geometry'][0]['radius']==3
        path=out/'carbon.FCStd';s.call('save',document=doc,path=str(path));s.call('close',document=doc)
        state=s.call('open',path=str(path));doc=state['document'];state=transaction(s,doc,'Reopened source drives copy',[dict(op='datum',object='Source',index=0,value='4 mm')]);assert objects(state)['Copy']['sketch']['geometry'][0]['radius']==4
        s.call('close',document=doc);report['carbon']=dict(accepted=carbon,live_dependency_after_reopen=True)
        for fault in ['base64','native_archive']:
            doc=s.call('new',name='LinuxFailure_'+fault)['document']
            transaction(s,doc,'Sketch',[add('Sketcher::SketchObject','Sketch'),dict(op='geometry',object='Sketch',kind='circle',center=[5,5,0],radius=2)])
            s.call('sketch_begin',document=doc,object='Sketch',token='failure')
            transaction(s,doc,'Fault',[mirror],checkpoint_fault=fault)
            s.call('preview',document=doc,token='broken',edits=[mirror],expect_error='native_error');state=s.call('inspect',document=doc);assert state['available'] is False
            s.call('save',document=doc,path=str(out/('blocked-'+fault+'.FCStd')),expect_error='document_unavailable');s.call('close',document=doc)
            report[fault]=dict(document_poisoned=True,save_blocked=True)
    finally:s.close()
    # Separate workers exercise public fail-closed pin guards before Begin.
    for fault in ['runtime_hash','adapter_hash','architecture']:
        backend=out/('pins-'+fault)
        shutil.copytree(Path(runtime['script']).parents[2]/'backend',backend)
        arch={'arm64':'aarch64','amd64':'x86_64'}.get(platform.machine().lower(),platform.machine().lower())
        manifest=backend/('native/checkpoint_restore.json' if sys.platform=='win32' else 'native/checkpoint_restore-linux-'+arch+'.json')
        pins=json.loads(manifest.read_text())
        if fault=='runtime_hash':pins['runtime'][next(iter(pins['runtime']))]='0'*64
        elif fault=='adapter_hash':pins['adapter_sha256']='0'*64
        else:pins['platform' if sys.platform=='win32' else 'architecture']='unsupported'
        manifest.write_text(json.dumps(pins))
        s=SketchSession(args.host.resolve(),dict(runtime,script=str(backend/'freecad_worker.py')))
        try:
            status=s.capabilities['sketch_checkpoints'];assert not status['available'] and status['reason']
            doc=s.call('new',name='Pins_'+fault)['document']
            before=transaction(s,doc,'Native sketch',[add('Sketcher::SketchObject','Sketch')])
            s.call('sketch_begin',document=doc,object='Sketch',token='blocked',expect_error='native_checkpoint_unavailable')
            assert s.call('inspect',document=doc)==before
            s.call('close',document=doc);report[fault]=dict(capability=status,pre_mutation_rejection=True)
        finally:s.close()
    (out/'results.json').write_text(json.dumps(report,indent=2)+'\n');print('Public checkpoint history/reference/Carbon Copy/failure gate passed.')
if __name__=='__main__':main()
