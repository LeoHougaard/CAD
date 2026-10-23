"""Native Tangent/Perpendicular through an exposed point, via the shared Rust builder."""
import argparse
import json
import math
from pathlib import Path
import subprocess
from native_workflows import add, constraint, objects, transaction, near, rectangle, set_value, solid
from sketch_editor_workflows import SketchSession
from common_dimension_workflows import same


def geo(kind, **kw):return dict(op='geometry',object='Sketch',kind=kind,**kw)
def c(kind,*args):return constraint('Sketch',kind,*args)
def e(index,point=0):return dict(index=index,point=point)
SELECTION=[e(0),e(1),e(2,1)]


def compile_command(compiler,state,kind,selection=SELECTION):
    request=dict(op='constraint',snapshot=objects(state)['Sketch'],kind=kind,selection=selection)
    result=subprocess.run([str(compiler)],input=json.dumps(request)+'\n',capture_output=True,text=True,encoding='utf-8',check=True)
    response=json.loads(result.stdout);assert response['ok'],response
    return response['edits']


def curve(name):
    return {
        'Line':geo('line',start=[-10,0,0],end=[10,0,0]),
        'Circle':geo('circle',center=[0,5,0],radius=5),
        'Arc':geo('arc',start=[-5,5,0],mid=[0,0,0],end=[5,5,0]),
        'Ellipse':geo('ellipse',major=[10,5,0],minor=[0,10,0],center=[0,5,0]),
        'EllipseArc':geo('ellipse_arc',major=[10,5,0],minor=[0,10,0],center=[0,5,0],start_parameter=math.pi,end_parameter=2*math.pi),
        'HyperbolaArc':geo('hyperbola_arc',major=[0,0,0],minor=[3,2,0],center=[0,2,0],start_parameter=-1,end_parameter=1),
        'ParabolaArc':geo('parabola_arc',center=[0,0,0],focal=2,angle=math.pi/2,start_parameter=-2,end_parameter=2),
        'BSpline':geo('bspline',poles=[[-10,1,0],[-5,-1/3,0],[5,-1/3,0],[10,1,0]])}[name]


def fixture(session,name,kind,family='Circle',prepared=True):
    doc=session.call('new',name=name)['document']
    tangent=kind=='Tangent'
    start,end=([-5,0,0],[5,0,0]) if tangent else ([0,-5,0],[0,5,0])
    if not prepared:
        start,end=([-5,1,0],[5,3,0]) if tangent else ([1,-5,0],[3,5,0])
    edits=[add('Sketcher::SketchObject','Sketch'),curve(family),geo('line',start=start,end=end),geo('point',position=[0,0,0],construction=True),c('Block',0)]
    if family=='BSpline':
        edits += [c('PointOnObject',2,1,0),c('DistanceX',2,1,0.)]
    else:edits += [c('Coincident',2,1,-1,1)]
    edits += [c('Distance',1,math.dist(start,end)),c('DistanceX' if tangent else 'DistanceY',1,1,-5.)]
    if prepared:edits += [c('PointOnObject',2,1,1)]
    state=transaction(session,doc,'Native via-point fixture',edits)
    return doc,state


def identity(state,index,kind,args=(0,1,2,1)):
    sketch=objects(state)['Sketch']['sketch'];native=sketch['constraints'][index]
    assert native['type']==kind,(kind,native)
    actual=(native['first'],native['second'],native['third'],native['third_pos'])
    assert actual==args,(actual,args)
    assert not sketch['conflicting_constraints'] and not sketch['redundant_constraints']
    return sketch


def workflow(session,compiler,out,kind,family,prepared):
    name=kind+family+('Prepared' if prepared else 'Missing')
    doc,state=fixture(session,'Via'+name,kind,family,prepared)
    baseline=state;before=objects(state)['Sketch']['sketch'];undo=state['undo_count']
    baseline_path=out/(name+'-baseline.FCStd');session.call('save',document=doc,path=str(baseline_path))
    edits=compile_command(compiler,state,kind)
    candidate=session.call('preview',document=doc,token='via-point',edits=edits)
    info=candidate['edits'][0];index=info['index']
    identity(candidate['state'],index,kind)
    assert len(info['removed_point_on_object'])==(1 if family=='BSpline' else 0),info
    assert len(info['preparation'])==(0 if prepared else 1),info
    state=session.call('preview',document=doc,token='via-point',edits=edits)['state']
    identity(state,index,kind)
    state=session.call('preview_cancel',document=doc,token='via-point')
    assert same(objects(state)['Sketch']['sketch'],before) and state['undo_count']==undo
    state=transaction(session,doc,'Via-point creation',edits);identity(state,index,kind)
    after=objects(state)['Sketch']['sketch']
    state=session.call('undo',document=doc);assert same(objects(state)['Sketch']['sketch'],before)
    state=session.call('redo',document=doc);assert same(objects(state)['Sketch']['sketch'],after)
    path=out/(name+'.FCStd');session.call('save',document=doc,path=str(path));session.call('close',document=doc)
    state=session.call('open',path=str(path));identity(state,index,kind);session.call('close',document=state['document'])
    return dict(case=name,passed=True,baseline=str(baseline_path),result=str(path),kind=kind,args=[0,1,2,1],native_index=index,preparation=info['preparation'],removed=info['removed_point_on_object'],preview_cancel=True,undo_redo_reopen=True)



def cleanup_workflow(session,compiler,out,kind):
    name=kind+'SplineExpressionCleanup'
    doc,state=fixture(session,'Via'+name,kind,'BSpline',True)
    state=transaction(session,doc,'Native dimensions after removable spline POO',[
        geo('circle',center=[20,20,0],radius=5,construction=True),c('DistanceX',3,3,20.),c('DistanceY',3,3,20.),c('Radius',3,5.),
        dict(op='constraint_state',object='Sketch',index=5,name='KeepLinePoint',virtual=True),
        dict(op='expression',object='Sketch',property='Constraints[3]',expression='Constraints[8] * 2'),
        geo('point',position=[3,0,0],construction=True),c('PointOnObject',4,1,0),c('DistanceX',4,1,3.),
        dict(op='constraint_state',object='Sketch',index=9,name='KeepOtherPoint')])
    baseline_path=out/(name+'-baseline.FCStd');session.call('save',document=doc,path=str(baseline_path))
    before=objects(state)['Sketch'];edits=compile_command(compiler,state,kind)
    result=session.call('preview',document=doc,token='cleanup',edits=edits);state=result['state'];info=result['edits'][0]
    assert info['removed_point_on_object']==[1] and info['index']==10,info
    sketch=identity(state,10,kind)
    assert sketch['constraints'][8]['name']=='KeepOtherPoint' and sketch['constraints'][8]['first']==4 and sketch['constraints'][8]['second']==0
    assert sketch['constraints'][4]['name']=='KeepLinePoint' and sketch['constraints'][4]['virtual']
    expression=objects(state)['Sketch']['properties']['ExpressionEngine']
    assert expression==[['Constraints[2]','Constraints[7] * 2']],expression
    state=session.call('preview_cancel',document=doc,token='cleanup')
    assert same(objects(state)['Sketch']['sketch'],before['sketch'])
    assert objects(state)['Sketch']['properties']['ExpressionEngine']==before['properties']['ExpressionEngine']
    state=transaction(session,doc,'Accept native POO substitution',edits)
    state=session.call('undo',document=doc);assert same(objects(state)['Sketch']['sketch'],before['sketch'])
    state=session.call('redo',document=doc);identity(state,10,kind)
    path=out/(name+'.FCStd');session.call('save',document=doc,path=str(path));session.call('close',document=doc)
    state=session.call('open',path=str(path));identity(state,10,kind);assert objects(state)['Sketch']['properties']['ExpressionEngine']==expression
    session.call('close',document=state['document'])
    return dict(case=name,passed=True,baseline=str(baseline_path),result=str(path),kind=kind,args=[0,1,2,1],native_index=10,removed=[1],native_expression_index_remapping=True,unrelated_constraint_state_preserved=True,preview_cancel_undo_redo_reopen=True)


def double_spline_workflow(session,compiler,out,kind):
    name=kind+'TwoSplines'
    doc=session.call('new',name='Via'+name)['document']
    poles=[[-5,0.2,0],[-2,-0.2/3,0],[2,-0.2/3,0],[5,0.2,0]] if kind=='Tangent' else [[-1,-5,0],[1/3,-2,0],[1/3,2,0],[-1,5,0]]
    state=transaction(session,doc,'Two native spline curves and point',[add('Sketcher::SketchObject','Sketch'),curve('BSpline'),geo('bspline',poles=poles),geo('point',position=[0,0,0],construction=True),
        c('Block',0),c('PointOnObject',2,1,0),c('PointOnObject',2,1,1),c('DistanceX',2,1,0.)])
    baseline_path=out/(name+'-baseline.FCStd');session.call('save',document=doc,path=str(baseline_path))
    before=objects(state)['Sketch']['sketch'];edits=compile_command(compiler,state,kind)
    result=session.call('preview',document=doc,token='two-splines',edits=edits);info=result['edits'][0]
    assert info['removed_point_on_object']==[1,2] and info['preparation']==[],info
    identity(result['state'],2,kind)
    state=session.call('preview_cancel',document=doc,token='two-splines');assert same(objects(state)['Sketch']['sketch'],before)
    state=transaction(session,doc,'Native two-spline via point',edits);identity(state,2,kind)
    session.call('undo',document=doc);state=session.call('redo',document=doc);identity(state,2,kind)
    path=out/(name+'.FCStd');session.call('save',document=doc,path=str(path));session.call('close',document=doc)
    state=session.call('open',path=str(path));identity(state,2,kind);session.call('close',document=state['document'])
    return dict(case=name,passed=True,baseline=str(baseline_path),result=str(path),kind=kind,args=[0,1,2,1],native_index=2,removed=[1,2],preview_cancel_undo_redo_reopen=True)


def external_workflow(session,compiler,out,kind):
    name=kind+'ExternalParent'
    doc=session.call('new',name='Via'+name)['document']
    tangent=kind=='Tangent';start,end=([-5,0,0],[5,0,0]) if tangent else ([0,-5,0],[0,5,0])
    state=transaction(session,doc,'Native external source and editable line',[
        add('Sketcher::SketchObject','Master'),dict(op='geometry',object='Master',kind='circle',center=[0,5,0],radius=5),
        constraint('Master','DistanceX',0,3,0.),constraint('Master','DistanceY',0,3,5.),constraint('Master','Radius',0,5.),
        add('Sketcher::SketchObject','Sketch'),geo('line',start=start,end=end),geo('point',position=[0,0,0],construction=True),
        c('Distance',0,10.),c('DistanceX' if tangent else 'DistanceY',0,1,-5.),c('DistanceX',1,1,0.)])
    state=transaction(session,doc,'Project source and preserve point-on relations',[
        dict(op='external_geometry',object='Sketch',source='Master',subelement='Edge1'),c('PointOnObject',1,1,-3),c('PointOnObject',1,1,0)])
    selection=[e(-3),e(0),e(1,1)];args=(-3,0,1,1)
    baseline_path=out/(name+'-baseline.FCStd');session.call('save',document=doc,path=str(baseline_path))
    edits=compile_command(compiler,state,kind,selection);state=transaction(session,doc,'External curve via-point',edits);index=5;identity(state,index,kind,args)
    path=out/(name+'.FCStd');session.call('save',document=doc,path=str(path))
    state=transaction(session,doc,'Native source radius update',[dict(op='datum',object='Master',index=2,value='6 mm')]);identity(state,index,kind,args)
    near(objects(state)['Sketch']['sketch']['geometry'][1]['position'][1],-1)
    session.call('undo',document=doc);state=session.call('redo',document=doc);near(objects(state)['Sketch']['sketch']['geometry'][1]['position'][1],-1)
    updated=out/(name+'-updated.FCStd');session.call('save',document=doc,path=str(updated));session.call('close',document=doc)
    state=session.call('open',path=str(updated));identity(state,index,kind,args);near(objects(state)['Sketch']['sketch']['geometry'][1]['position'][1],-1);session.call('close',document=state['document'])
    return dict(case=name,passed=True,baseline=str(baseline_path),result=str(path),kind=kind,args=list(args),native_index=index,parent_update_undo_redo_reopen=True)


def failures(session,compiler,out,kind):
    doc=session.call('new',name='Via'+kind+'FailedPreparation')['document']
    tangent=kind=='Tangent'
    start,end=([-5,1,0],[5,3,0]) if tangent else ([1,-5,0],[3,5,0])
    state=transaction(session,doc,'Fixed selected point off fixed circle',[
        add('Sketcher::SketchObject','Sketch'),geo('circle',center=[0,6,0],radius=5),geo('line',start=start,end=end),geo('point',position=[0,0,0],construction=True),
        c('DistanceX',0,3,0.),c('DistanceY',0,3,6.),c('Radius',0,5.),c('Coincident',2,1,-1,1),c('Distance',1,math.dist(start,end)),c('DistanceX' if tangent else 'DistanceY',1,1,-5.)])
    before=objects(state)['Sketch']['sketch'];edits=compile_command(compiler,state,kind)
    session.call('preview',document=doc,token='failed-prep',edits=edits,expect_error='invalid_via_point')
    state=session.call('inspect',document=doc);assert state['preview'] is None and same(objects(state)['Sketch']['sketch'],before)
    state=transaction(session,doc,'Unfix point for recovery',[dict(op='delete_constraint',object='Sketch',index=3)])
    state=transaction(session,doc,'Recover via-point creation',compile_command(compiler,state,kind))
    session.call('close',document=doc)
    doc,state=fixture(session,'Via'+kind+'FixedCurves',kind)
    state=transaction(session,doc,'Block both selected curves',[c('Block',1)])
    before=objects(state)['Sketch']['sketch']
    edit=dict(op='via_point',object='Sketch',kind=kind+'ViaPoint',args=[0,1,2,1])
    session.call('transaction',document=doc,edits=[edit],expect_error='invalid_via_point')
    state=session.call('inspect',document=doc);assert same(objects(state)['Sketch']['sketch'],before)
    session.call('transaction',document=doc,edits=[dict(edit,kind='AngleViaPoint')],expect_error='invalid_request')
    state=session.call('inspect',document=doc);assert same(objects(state)['Sketch']['sketch'],before)
    session.call('close',document=doc)
    return dict(case=kind+'FailedPreparationAndFixedCurves',passed=True,failed_first_preview_atomic=True,recovery=True,both_fixed_rejected=True,malformed_angle_via_point_rejected=True)


def consumed(session,compiler,out,kind):
    name=kind+'ConsumedSketchSession';doc=session.call('new',name='Via'+name)['document'];tangent=kind=='Tangent'
    start,end=([10,15,0],[20,15,0]) if tangent else ([15,10,0],[15,20,0])
    state=transaction(session,doc,'Consumed profile and construction curves',[add('PartDesign::Body','Body'),add('Sketcher::SketchObject','Sketch','Body')]+rectangle('Sketch')+[
        geo('circle',center=[15,20,0],radius=5,construction=True),geo('line',start=start,end=end,construction=True),geo('point',position=[15,15,0],construction=True),
        c('Block',4),c('DistanceX',6,1,15.),c('DistanceY',6,1,15.),c('Distance',5,10.),c('DistanceX' if tangent else 'DistanceY',5,1,10.),c('PointOnObject',6,1,5),
        add('PartDesign::Pad','Pad','Body'),set_value('Pad','Profile',dict(link='Sketch')),set_value('Pad','Length',5)])
    solid(state,'Pad',6000);before=objects(state)['Sketch']['sketch'];undo=state['undo_count'];index=before['constraint_count'];selection=[e(4),e(5),e(6,1)];args=(4,5,6,1)
    baseline_path=out/(name+'-baseline.FCStd');session.call('save',document=doc,path=str(baseline_path))
    for token in ['cancel-via','finish-via']:
        state=session.call('sketch_begin',document=doc,object='Sketch',token=token);edits=compile_command(compiler,state,kind,selection)
        state=session.call('preview',document=doc,token='gesture',edits=edits)['state'];identity(state,index,kind,args);solid(state,'Pad',6000)
        state=session.call('preview_cancel',document=doc,token='gesture');assert same(objects(state)['Sketch']['sketch'],before)
        state=transaction(session,doc,'Via-point sketch action',edits)
        state=session.call('undo',document=doc);assert same(objects(state)['Sketch']['sketch'],before)
        state=session.call('redo',document=doc);identity(state,index,kind,args)
        if token.startswith('cancel'):
            state=session.call('sketch_cancel',document=doc,token=token);assert same(objects(state)['Sketch']['sketch'],before) and state['undo_count']==undo
        else:
            state=session.call('sketch_finish',document=doc,token=token);assert state['undo_count']==undo+1
        solid(state,'Pad',6000)
    state=session.call('undo',document=doc);assert same(objects(state)['Sketch']['sketch'],before)
    state=session.call('redo',document=doc);identity(state,index,kind,args)
    path=out/(name+'.FCStd');session.call('save',document=doc,path=str(path));session.call('close',document=doc)
    state=session.call('open',path=str(path));identity(state,index,kind,args);solid(state,'Pad',6000);session.call('close',document=state['document'])
    return dict(case=name,passed=True,baseline=str(baseline_path),result=str(path),kind=kind,args=list(args),native_index=index,preview_cancel=True,action_undo_redo=True,session_cancel_finish=True,one_document_undo=True,reopen=True)


def exposed_points(session,compiler,out,kind):
    rows=[]
    for source in ['Origin','ArcEndpoint','CircleCenter','ExternalEndpoint']:
        name=kind+source
        if source=='Origin' or source=='ExternalEndpoint':
            doc,state=fixture(session,'Via'+name,kind)
            point=(-1,1)
            if source=='ExternalEndpoint':
                transaction(session,doc,'Native external point source',[add('Sketcher::SketchObject','Master'),dict(op='geometry',object='Master',kind='line',start=[0,0,0],end=[2,2,0])])
                state=transaction(session,doc,'Project native point',[dict(op='external_geometry',object='Sketch',source='Master',subelement='Edge1')]);point=(-3,1)
        else:
            doc=session.call('new',name='Via'+name)['document'];tangent=kind=='Tangent'
            if source=='ArcEndpoint':
                start,end=([-5,0,0],[-5,10,0]) if tangent else ([-10,5,0],[0,5,0]);point=(0,1)
                extra=[];first_curve=curve('Arc');datum_kind='DistanceY' if tangent else 'DistanceX';datum_value=0. if tangent else -10.
            else:
                start,end=([-5,0,0],[5,0,0]) if tangent else ([0,-5,0],[0,5,0]);point=(2,3)
                extra=[geo('circle',center=[0,0,0],radius=1,construction=True),c('Block',2)];first_curve=curve('Circle');datum_kind='DistanceX' if tangent else 'DistanceY';datum_value=-5.
            state=transaction(session,doc,'Native exposed curve handle fixture',[add('Sketcher::SketchObject','Sketch'),first_curve,geo('line',start=start,end=end)]+extra+[
                c('Block',0),c('Distance',1,10.),c(datum_kind,1,1,datum_value),c('PointOnObject',point[0],point[1],1)])
        selection=[e(0),e(1),e(*point)];args=(0,1,*point)
        baseline_path=out/(name+'-baseline.FCStd');session.call('save',document=doc,path=str(baseline_path))
        result=session.call('preview',document=doc,token='point-kind',edits=compile_command(compiler,state,kind,selection));index=result['edits'][0]['index'];identity(result['state'],index,kind,args)
        state=session.call('preview_commit',document=doc,token='point-kind');identity(state,index,kind,args)
        path=out/(name+'.FCStd');session.call('save',document=doc,path=str(path));session.call('close',document=doc)
        state=session.call('open',path=str(path));identity(state,index,kind,args);session.call('close',document=state['document'])
        rows.append(dict(case=name,passed=True,baseline=str(baseline_path),result=str(path),kind=kind,args=list(args),native_index=index,preview_commit_reopen=True))
    return rows


def weight_rejection(session,compiler,out,kind):
    doc,state=fixture(session,'Via'+kind+'WeightRejection',kind,'BSpline')
    state=transaction(session,doc,'Expose native spline controls',[dict(op='expose_internal',object='Sketch',index=0)])
    control=objects(state)['Sketch']['sketch']['spline_controls'][0]['control_index']
    before=objects(state)['Sketch']['sketch']
    edit=dict(op='via_point',object='Sketch',kind=kind+'ViaPoint',args=[control,1,2,1])
    session.call('transaction',document=doc,edits=[edit],expect_error='invalid_via_point')
    state=session.call('inspect',document=doc);assert same(objects(state)['Sketch']['sketch'],before)
    request=dict(op='constraint',snapshot=objects(state)['Sketch'],kind=kind,selection=[e(control),e(1),e(2,1)])
    reply=subprocess.run([str(compiler)],input=json.dumps(request)+'\n',capture_output=True,text=True,encoding='utf-8',check=True)
    assert json.loads(reply.stdout)['ok'] is False
    session.call('close',document=doc)
    return dict(case=kind+'WeightCircleRejected',passed=True,compiler_and_worker=True,atomic=True)


def compound_failures_and_missing(session,compiler,out,kind):
    doc=session.call('new',name='Via'+kind+'TwoPreparations')['document'];tangent=kind=='Tangent'
    start,end=([-5,2,0],[5,3,0]) if tangent else ([2,-5,0],[3,5,0])
    state=transaction(session,doc,'Point initially on neither curve',[add('Sketcher::SketchObject','Sketch'),curve('Circle'),geo('line',start=start,end=end),geo('point',position=[1,1,0],construction=True),
        c('Block',0),c('DistanceX',2,1,1.),c('Distance',1,math.dist(start,end)),c('DistanceX' if tangent else 'DistanceY',1,1,-5.)])
    name=kind+'TwoMissingRelations';baseline_path=out/(name+'-baseline.FCStd');session.call('save',document=doc,path=str(baseline_path));before=objects(state)['Sketch']['sketch']
    edits=compile_command(compiler,state,kind);reply=session.call('preview',document=doc,token='accepted',edits=edits);info=reply['edits'][0];accepted=reply['state']
    assert len(info['preparation'])==2,info
    # A failed replacement after complete native preparation must restore the
    # last accepted candidate and ownership, not just the original baseline.
    session.call('preview',document=doc,token='accepted',edits=edits+[c(kind+'ViaPoint',0,1,2,1)],expect_error='invalid_model')
    state=session.call('inspect',document=doc);assert state['preview']['token']=='accepted' and same(objects(state)['Sketch']['sketch'],objects(accepted)['Sketch']['sketch'])
    state=session.call('preview_cancel',document=doc,token='accepted');assert same(objects(state)['Sketch']['sketch'],before)
    session.call('transaction',document=doc,edits=edits+[c(kind+'ViaPoint',0,1,2,1)],expect_error='invalid_model')
    state=session.call('inspect',document=doc);assert same(objects(state)['Sketch']['sketch'],before)
    state=transaction(session,doc,'Recover complete compound creation',edits)
    path=out/(name+'.FCStd');session.call('save',document=doc,path=str(path));session.call('close',document=doc)
    return dict(case=name,passed=True,baseline=str(baseline_path),result=str(path),kind=kind,args=[0,1,2,1],native_index=info['index'],two_preparations=True,failed_replacement_preserves_accepted=True,compound_failure_atomic=True,recovery=True)


def external_fixed_rejection(session,compiler,out,kind):
    doc,state=fixture(session,'Via'+kind+'ExternalFixedRejection',kind)
    transaction(session,doc,'External fixed circle source',[add('Sketcher::SketchObject','Master'),dict(op='geometry',object='Master',kind='circle',center=[0,5,0],radius=5)])
    state=transaction(session,doc,'Project and block other curve',[dict(op='external_geometry',object='Sketch',source='Master',subelement='Edge1'),c('Block',1)])
    before=objects(state)['Sketch']['sketch'];selection=[e(-3),e(1),e(2,1)]
    edit=dict(op='via_point',object='Sketch',kind=kind+'ViaPoint',args=[-3,1,2,1])
    session.call('transaction',document=doc,edits=[edit],expect_error='invalid_via_point')
    state=session.call('inspect',document=doc);assert same(objects(state)['Sketch']['sketch'],before)
    request=dict(op='constraint',snapshot=objects(state)['Sketch'],kind=kind,selection=selection)
    response=subprocess.run([str(compiler)],input=json.dumps(request)+'\n',capture_output=True,text=True,encoding='utf-8',check=True)
    assert json.loads(response.stdout)['ok'] is False
    session.call('close',document=doc)
    return dict(case=kind+'ExternalFixedRejected',passed=True,compiler_and_worker=True,atomic=True)

def main():
    p=argparse.ArgumentParser();p.add_argument('--host',type=Path,required=True);p.add_argument('--compiler',type=Path,required=True);p.add_argument('--runtime',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True);session=SketchSession(a.host,json.loads(a.runtime.read_text()));report=[]
    try:
        for kind in ['Tangent','Perpendicular']:
            for family in ['Line','Circle','Arc','Ellipse','EllipseArc','HyperbolaArc','ParabolaArc','BSpline']:
                for prepared in ([True,False] if family=='Circle' else [True]):
                    row=workflow(session,a.compiler,a.output,kind,family,prepared);report.append(row);print(row['case'],'passed',flush=True)
            for extra in [cleanup_workflow,double_spline_workflow,external_workflow,failures,consumed,weight_rejection,compound_failures_and_missing,external_fixed_rejection]:
                row=extra(session,a.compiler,a.output,kind);report.append(row);print(row['case'],'passed',flush=True)
            report+=exposed_points(session,a.compiler,a.output,kind)
        (a.output/'report.json').write_bytes((json.dumps(report,indent=2)+'\n').encode())
    finally:session.close()

if __name__=='__main__':main()
