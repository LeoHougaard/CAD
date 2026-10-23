"""Native snapshots for the Angle dialog selection lifecycle regression.

Only public host/compiler operations are used. Write to a fresh output folder.
The editor method tests consume immutable captures of these native sequences.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

from angle_via_point_workflows import compile_command, e, fixture, objects, same_object, session_restored, sketch
from native_workflows import transaction
from sketch_editor_workflows import SketchSession


def run(host,compiler,runtime,output):
    output.mkdir(parents=True,exist_ok=False)
    config=output/'native-config';config.mkdir()
    os.environ['FREECAD_USER_HOME']=str(config.resolve())
    session=SketchSession(host,runtime)
    states={};sources={};report=[]
    try:
        def begin(name,family='Line',cleanup=False,fixed=False):
            doc,state=fixture(session,name,family,spline_cleanup=cleanup)
            edits=[dict(op='constraint_state',object='Sketch',index=0,name='PriorCurve')]
            if cleanup:edits.append(dict(op='constraint_state',object='Sketch',index=5,name='PriorRadius'))
            if fixed:edits.append(dict(op='constraint',object='Sketch',kind='Block',args=[1]))
            transaction(session,doc,'Name prior native selection',edits)
            path=output/(name+'-baseline.FCStd')
            session.call('save',document=doc,path=str(path));sources[path.name]=hashlib.sha256(path.read_bytes()).hexdigest()
            session.call('close',document=doc);state=session.call('open',path=str(path));doc=state['document']
            return doc,session.call('sketch_begin',document=doc,object='Sketch',token='angle-selection')

        doc,baseline=begin('SelectionLifecycle')
        selection=[e(0),e(1),e(2,1)]
        edits=compile_command(compiler,baseline,selection)
        reply=session.call('preview',document=doc,token='angle-selection-preview',edits=edits)
        index=reply['edits'][0]['index'];seed=reply['state']
        assert index==4 and sketch(seed)['constraints'][index]['type']=='Angle'
        session.call('preview',document=doc,token='angle-selection-preview',
                     edits=[{**edits[0],'expression':'Missing.Length'}],expect_error='invalid_model')
        failed=session.call('inspect',document=doc)
        assert same_object(objects(failed)['Sketch'],objects(seed)['Sketch'])
        assert failed['preview']==seed['preview'] and failed['undo_count']==seed['undo_count']
        replacement=session.call('preview',document=doc,token='angle-selection-preview',
                                 edits=compile_command(compiler,baseline,selection,value='60 deg'))['state']
        session.call('preview_cancel',document=doc,token='wrong-owner',expect_error='preview_owner')
        retained=session.call('inspect',document=doc)
        assert same_object(objects(retained)['Sketch'],objects(replacement)['Sketch'])
        assert retained['preview']==replacement['preview']
        cancelled=session.call('preview_cancel',document=doc,token='angle-selection-preview')
        session_restored(objects(cancelled)['Sketch'],objects(baseline)['Sketch'])
        assert cancelled['preview'] is None and cancelled['undo_count']==0
        later=transaction(session,doc,'Unrelated native Radius reuses cancelled index',[
            dict(op='geometry',object='Sketch',kind='circle',center=[20,20,0],radius=3,construction=True),
            dict(op='constraint',object='Sketch',kind='Radius',args=[3,3.])])
        assert sketch(later)['constraints'][index]['type']=='Radius'
        deferred=session.call('undo',document=doc)
        assert deferred['redo_count']==1 and deferred['undo_count']==0
        inspected=session.call('inspect',document=doc)
        assert inspected['redo_count']==1 and inspected['revision']==deferred['revision']
        deferred_result=transaction(session,doc,'Deferred native Angle Apply',
                                    compile_command(compiler,deferred,selection,value='75 deg'))
        assert deferred_result['redo_count']==0 and sketch(deferred_result)['constraints'][index]['type']=='Angle'
        state=session.call('undo',document=doc)
        session_restored(objects(state)['Sketch'],objects(baseline)['Sketch'])
        state=session.call('redo',document=doc)
        assert same_object(objects(state)['Sketch'],objects(deferred_result)['Sketch'])
        session.call('sketch_cancel',document=doc,token='angle-selection')
        session.call('close',document=doc)
        states.update(baseline=baseline,seed=seed,failed=failed,replacement=replacement,retained=retained,
                      cancelled=cancelled,later=later,deferred=deferred,deferred_result=deferred_result,index=index)
        report.append(dict(case='CancelReuseFailedReplacementAndDeferred',passed=True,index=index,
                           preview_owner=True,action_cursor=True,full_native_restoration=True))

        doc,baseline=begin('AcceptedSelection')
        edits=compile_command(compiler,baseline,selection)
        seed=session.call('preview',document=doc,token='angle-selection-preview',edits=edits)['state']
        failure=session.call('preview',document=doc,token='angle-selection-preview',
                             edits=[{**edits[0],'expression':'Missing.Length'}],expect_error='invalid_model')
        failed=session.call('inspect',document=doc)
        assert same_object(objects(failed)['Sketch'],objects(seed)['Sketch']) and failed['preview']==seed['preview']
        replacement=session.call('preview',document=doc,token='angle-selection-preview',
                                 edits=compile_command(compiler,baseline,selection,value='60 deg'))['state']
        committed=session.call('preview_commit',document=doc,token='angle-selection-preview')
        session.call('sketch_cancel',document=doc,token='angle-selection');session.call('close',document=doc)
        states['accepted']=dict(baseline=baseline,seed=seed,failed=failed,failure=failure,replacement=replacement,committed=committed)
        report.append(dict(case='AcceptedSelection',passed=True,index=4))

        doc,baseline=begin('CleanupSelection','BSpline',cleanup=True)
        reply=session.call('preview',document=doc,token='angle-selection-preview',
                           edits=compile_command(compiler,baseline,selection))
        seed=reply['state'];removed=reply['edits'][0]['removed_point_on_object']
        assert removed==[1] and sketch(seed)['constraints'][4]['name']=='PriorRadius'
        cancelled=session.call('preview_cancel',document=doc,token='angle-selection-preview')
        session_restored(objects(cancelled)['Sketch'],objects(baseline)['Sketch'])
        states['cleanup']=dict(baseline=baseline,seed=seed,cancelled=cancelled)
        session.call('sketch_cancel',document=doc,token='angle-selection');session.call('close',document=doc)
        report.append(dict(case='NativeCleanupIndexRestoration',passed=True,removed=removed,baseline_radius_index=5,preview_radius_index=4))

        doc,baseline=begin('ImmediateReferenceSelection',fixed=True)
        edits=compile_command(compiler,baseline,selection)
        assert edits[0]['driving'] is False
        result=transaction(session,doc,'Immediate native reference angle',edits)
        assert sketch(result)['constraints'][-1]['driving'] is False
        states['reference']=dict(baseline=baseline,result=result)
        session.call('sketch_cancel',document=doc,token='angle-selection');session.call('close',document=doc)
        report.append(dict(case='ImmediateNativeReferenceSelection',passed=True))
    finally:
        session.close()
    for name,expected in sources.items():assert hashlib.sha256((output/name).read_bytes()).hexdigest()==expected,name
    (output/'states.json').write_text(json.dumps(states,indent=2)+'\n',encoding='utf-8')
    (output/'report.json').write_text(json.dumps(dict(passed=True,cases=report,source_sha256=sources),indent=2)+'\n',encoding='utf-8')
    subprocess.run([runtime['executable'],*runtime['args'],str(Path(__file__).resolve()),'--native-oracle',str(output)],check=True)
    print(json.dumps(dict(passed=True,cases=len(report)),indent=2))


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for key in ['host','compiler','runtime','output']:p.add_argument('--'+key,type=Path,required=True)
    args=p.parse_args()
    run(args.host.resolve(),args.compiler.resolve(),json.loads(args.runtime.read_text(encoding='utf-8-sig')),args.output.resolve())

def native_oracle(output):
    import FreeCAD as App
    from angle_via_point_workflows import native_control,native_signature,same
    path=output/'SelectionLifecycle-baseline.FCStd';protected=hashlib.sha256(path.read_bytes()).hexdigest()
    states=json.loads((output/'states.json').read_text())
    doc=App.openDocument(str(path));doc.UndoMode=1;obj=doc.getObject('Sketch')
    before=native_signature(obj)
    doc.openTransaction('Independent native angle')
    native_control(obj,[e(0),e(1),e(2,1)],'60 deg')
    doc.abortTransaction();obj.solve();obj.recompute()
    assert same(native_signature(obj),before)
    assert obj.State==objects(states['cancelled'])['Sketch']['state']==['Touched']
    assert obj.getStatusString()==objects(states['cancelled'])['Sketch']['status']=='Touched'
    App.closeDocument(doc.Name)
    assert hashlib.sha256(path.read_bytes()).hexdigest()==protected
    (output/'native-oracle.json').write_text(json.dumps(dict(passed=True,phase='abort -> sketch.solve -> sketch.recompute',
        native_graph_restored=True,status='Touched',source_sha256=protected),indent=2)+'\n',encoding='utf-8')

if __name__=='__main__':
    if len(sys.argv)==3 and sys.argv[1]=='--native-oracle':native_oracle(Path(sys.argv[2]))
    else:main()
