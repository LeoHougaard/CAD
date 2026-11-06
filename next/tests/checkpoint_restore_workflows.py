"""Focused public native checkpoint lifetime, availability and recovery gate."""
import argparse, base64, gc, json, os, shutil, sys
from pathlib import Path

from native_workflows import add, objects, transaction
from sketch_editor_workflows import SketchSession

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['host','runtime','output']:parser.add_argument('--'+name,required=True,type=Path)
    args=parser.parse_args();out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
    runtime=json.loads(args.runtime.read_text());events=[]
    s=SketchSession(args.host.resolve(),runtime)
    try:
        assert s.capabilities['sketch_checkpoints']['available'] is True
        doc=s.call('new',name='CheckpointRestoreGate')['document']
        transaction(s,doc,'Native circle',[add('Sketcher::SketchObject','Sketch'),
            dict(op='geometry',object='Sketch',kind='circle',center=[5,5,0],radius=2)])
        s.call('sketch_begin',document=doc,object='Sketch',token='session')
        mirror=dict(op='mirror_geometry',object='Sketch',indices=[0],reference=dict(index=-1,point=0))
        accepted=transaction(s,doc,'Accepted native Mirror',[mirror]);before=objects(accepted)['Sketch']['sketch']
        for i in range(30):
            s.call('preview',document=doc,token='later',edits=[dict(mirror,reference=dict(index=-2,point=0))])
            state=s.call('preview_cancel',document=doc,token='later');assert objects(state)['Sketch']['sketch']==before
            assert state['undo_count']==1 and state['redo_count']==0
        s.call('undo',document=doc);state=s.call('redo',document=doc);assert objects(state)['Sketch']['sketch']==before
        s.call('sketch_finish',document=doc,token='session');path=out/'restored.FCStd';s.call('save',document=doc,path=str(path))
        s.call('close',document=doc);state=s.call('open',path=str(path));assert objects(state)['Sketch']['sketch']==before
        s.call('close',document=state['document'])
        events.append(dict(repeated_same_history_preview_cancel=30,history_counts_constant=True,undo_redo_reopen=True))
    finally:s.close()
    # A private missing-adapter runtime must reject Begin without changing its
    # existing native document, revision, ownership or history.
    backend=out/'missing-adapter';original=Path(runtime['script']).parent
    shutil.copytree(original,backend,ignore=shutil.ignore_patterns('checkpoint_restore.dll','checkpoint_restore-linux-*.so'))
    s=SketchSession(args.host.resolve(),dict(runtime,script=str(backend/'freecad_worker.py')))
    try:
        status=s.capabilities['sketch_checkpoints'];assert not status['available'] and status['reason']
        doc=s.call('new',name='UnavailableCheckpoint')['document']
        before=transaction(s,doc,'Native sketch',[add('Sketcher::SketchObject','Sketch')])
        error=s.call('sketch_begin',document=doc,object='Sketch',token='session',expect_error='native_checkpoint_unavailable')
        assert s.call('inspect',document=doc)==before
        s.call('close',document=doc);events.append(dict(unavailable=status,error=error,pre_mutation_rejection=True))
    finally:s.close()
    (out/'results.json').write_text(json.dumps(events,indent=2)+'\n');print('Checkpoint public regression passed.')
if __name__=='__main__':main()
