"""Angle through two native curves and an exposed point via shared Rust commands.

Fresh outputs only. The direct comparator transcribes pinned 1.1.4 command
preparation and calls native measurement/constructors/solver, without the worker.
"""
import argparse
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys

from native_workflows import add, constraint, near, objects, rectangle, set_value, solid, transaction, link
from sketch_editor_workflows import SketchSession
from common_dimension_workflows import same
from via_point_workflows import curve


def e(index, point=0): return dict(index=index, point=point)
def c(kind,*args): return constraint('Sketch',kind,*args)
def geo(kind,**fields): return dict(op='geometry',object='Sketch',kind=kind,**fields)
def digest(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def sketch(state): return objects(state)['Sketch']['sketch']


def same_object(a,b):
    return same(a,b)


def restored_object(actual,before,case):
    if case == 'BothAxesReference':
        # Direct pinned native abort AND Undo initialize exactly this format
        # version for the one unconstrained standalone-point fixture. No other
        # metadata paths or transitions are exempted. native_compare verifies
        # the whole version sequence independently below.
        assert before['properties']['_ElementMapVersion'] == ''
        assert actual['properties']['_ElementMapVersion'] == '15.70200.5'
        expected = {**before,'properties':{**before['properties'],'_ElementMapVersion':'15.70200.5'}}
    else:
        expected = before
    assert same_object(actual,expected),case


def session_restored(actual,before):
    # The consumed-session native oracle checks this exact status after
    # abort + sketch.solve/recompute. All graph/property/shape paths stay checked.
    assert before['state']==['Up-to-date'] and before['status']=='Valid'
    expected={**before,'state':['Touched'],'status':'Touched'}
    assert same_object(actual,expected)


def reopened_object(actual,before,case):
    if case=='BothAxesReference':
        # This construction-point-only sketch has no shape to initialize the
        # transient native element-map version during restore. The native
        # comparator verifies the loaded value at the same reopen boundary.
        assert before['properties']['_ElementMapVersion']=='15.70200.5'
        assert actual['properties']['_ElementMapVersion']==''
        before={**before,'properties':{**before['properties'],'_ElementMapVersion':''}}
    assert same_object(actual,before)


def oracle_rejections(actual,before,oracle):
    for path,value in [(('properties','Label'),'arbitrary metadata'),
                       (('properties','_ElementMapVersion'),'unknown version'),
                       (('status',),'Invalid'),(('state',),['Invalid'])]:
        changed=copy.deepcopy(actual)
        target=changed
        for key in path[:-1]: target=target[key]
        target[path[-1]]=value
        try: oracle(changed,before)
        except AssertionError: pass
        else: raise AssertionError(('metadata oracle accepted arbitrary change',path))


def compile_command(compiler,state,selection,kind='Angle',value=None,expression=None,index=None,driving=None):
    request = dict(op='constraint',snapshot=objects(state)['Sketch'],kind=kind,selection=selection)
    if index is not None:
        request = dict(op='datum',snapshot=objects(state)['Sketch'],index=index,value=value,expression=expression is True)
    else:
        if value is not None: request['value'] = value
        if expression is not None: request['expression'] = expression
        if driving is not None: request['driving'] = driving
    result = subprocess.run([str(compiler)],input=json.dumps(request)+'\n',capture_output=True,text=True,encoding='utf-8',check=True)
    response = json.loads(result.stdout)
    assert response['ok'],response
    return response['edits']


def fixture(session,name,family='Line',missing=False,spline_cleanup=False):
    doc = session.call('new',name='Angle'+name)['document']
    line = geo('line',start=[-5,1,0],end=[5,3,0]) if missing else geo('line',start=[0,0,0],end=[5,5,0])
    edits = [add('Sketcher::SketchObject','Sketch'),curve(family),line,geo('point',position=[0,0,0],construction=True),c('Block',0)]
    if spline_cleanup:
        edits += [c('PointOnObject',2,1,0),c('DistanceX',2,1,0.)]
    else: edits += [c('Coincident',2,1,-1,1)]
    edits += [c('Distance',1,math.sqrt(104) if missing else math.sqrt(50))]
    edits += [c('DistanceX',1,1,-5.)] if missing else [c('Coincident',1,1,-1,1)]
    if spline_cleanup:
        edits += [geo('circle',center=[20,20,0],radius=5,construction=True),c('Radius',3,5.),c('DistanceX',3,3,20.),c('DistanceY',3,3,20.)]
    state = transaction(session,doc,'Native curves and exposed point',edits)
    if spline_cleanup:
        count = sketch(state)['constraint_count']
        state = transaction(session,doc,'Native expression after removable point-on-spline',[
            dict(op='expression',object='Sketch',property=f'Constraints[{count-1}]',expression=f'Constraints[{count-2}]')])
    return doc,state


def angle(state,index):
    native = sketch(state)['constraints'][index]
    assert native['type'] == 'Angle' and native['third'] != -2000,native
    witness = native['angle_via_point']
    assert 'display_error' not in witness and math.isfinite(witness['start_angle'])
    assert len(witness['center']) == 3
    assert not sketch(state)['conflicting_constraints'] and not sketch(state)['redundant_constraints']
    return native


def lifecycle(session,compiler,out,name,doc,state,selection,value=None,expression=None,driving=None):
    baseline = out/(name+'-baseline.FCStd')
    session.call('save',document=doc,path=str(baseline))
    protected = digest(baseline)
    # Native Sketch restore normalizes AttacherEngine on a newly created sketch.
    # Start both the worker and direct-native control from the same saved input.
    session.call('close',document=doc)
    state = session.call('open',path=str(baseline))
    doc = state['document']
    before = objects(state)['Sketch']
    cursor = (state['undo_count'],state['redo_count'])
    edits = compile_command(compiler,state,selection,value=value,expression=expression,driving=driving)
    result = session.call('preview',document=doc,token='native-angle',edits=edits)
    info = result['edits'][0]
    index = info['index']
    accepted = angle(result['state'],index)
    after = objects(result['state'])['Sketch']
    state = session.call('preview_cancel',document=doc,token='native-angle')
    restored_object(objects(state)['Sketch'],before,name)
    if name=='BothAxesReference':
        oracle_rejections(objects(state)['Sketch'],before,lambda actual,expected:restored_object(actual,expected,name))
    cancelled_version = objects(state)['Sketch']['properties']['_ElementMapVersion']
    assert (state['undo_count'],state['redo_count']) == cursor and state['preview'] is None
    result = session.call('transaction',document=doc,label='Native angle through point',edits=edits)
    state = result['state']
    if not same_object(objects(state)['Sketch'],after):
        (out/(name+'-replay-difference.json')).write_text(json.dumps(dict(preview=after,transaction=objects(state)['Sketch']),indent=2),encoding='utf-8')
    assert result['edits'][0]['index'] == index and same_object(objects(state)['Sketch'],after)
    state = session.call('undo',document=doc)
    restored_object(objects(state)['Sketch'],before,name)
    undone_version = objects(state)['Sketch']['properties']['_ElementMapVersion']
    state = session.call('redo',document=doc)
    assert same_object(objects(state)['Sketch'],after)
    target = out/(name+'-result.FCStd')
    session.call('save',document=doc,path=str(target))
    session.call('close',document=doc)
    state = session.call('open',path=str(target))
    if name!='BothAxesReference' and not same_object(objects(state)['Sketch'],after):
        (out/(name+'-reopen-difference.json')).write_bytes(json.dumps(dict(before=after,after=objects(state)['Sketch']),indent=2).encode())
    reopened_object(objects(state)['Sketch'],after,name)
    if name=='BothAxesReference':
        oracle_rejections(objects(state)['Sketch'],after,lambda actual,expected:reopened_object(actual,expected,name))
    reopened_version=objects(state)['Sketch']['properties']['_ElementMapVersion']
    angle(state,index)
    session.call('close',document=state['document'])
    assert digest(baseline) == protected
    return dict(case=name,passed=True,baseline=str(baseline),source_sha256=protected,result=str(target),
                selection=selection,value=value,expression=expression,requested_driving=driving,index=index,
                versions=dict(before=before['properties']['_ElementMapVersion'],active=after['properties']['_ElementMapVersion'],cancel=cancelled_version,undo=undone_version,reopen=reopened_version),
                native_identity=[accepted[k] for k in ['first','second','third','third_pos']],
                driving=accepted['driving'],preparation=info['preparation'],removed=info['removed_point_on_object'],
                preview_cancel=True,document_undo_redo=True,reopen=True)


def families(session,compiler,out):
    report = []
    for family in ['Line','Circle','Arc','Ellipse','EllipseArc','HyperbolaArc','ParabolaArc','BSpline']:
        doc,state = fixture(session,family,family,spline_cleanup=family=='BSpline')
        selection = [e(2,1),e(0),e(1)] if family in ['Line','Ellipse','BSpline'] else [e(0),e(2,1),e(1)]
        report.append(lifecycle(session,compiler,out,family,doc,state,selection))
    for name,missing,selection in [('PreparedNegative',False,[e(1),e(0),e(2,1)]),('MissingPreparation',True,[e(0),e(1),e(2,1)])]:
        doc,state = fixture(session,name,missing=missing)
        report.append(lifecycle(session,compiler,out,name,doc,state,selection))
    return report


def two_splines(session,compiler,out):
    doc=session.call('new',name='TwoNativeSplines')['document']
    state=transaction(session,doc,'Two spline point-on-object relations',[
        add('Sketcher::SketchObject','Sketch'),curve('BSpline'),
        geo('bspline',poles=[[-1,-5,0],[1/3,-2,0],[1/3,2,0],[-1,5,0]]),
        geo('point',position=[0,0,0],construction=True),c('Block',0),c('Block',1),
        c('PointOnObject',2,1,0),c('PointOnObject',2,1,1)])
    row=lifecycle(session,compiler,out,'TwoSplines',doc,state,[e(0),e(1),e(2,1)])
    assert row['removed']==[2,3] and not row['preparation'] and not row['driving']
    return [row]


def axes_and_fixed(session,compiler,out):
    report = []
    for axis in [-1,-2]:
        name = 'NativeXaxis' if axis == -1 else 'NativeYaxis'
        doc = session.call('new',name=name)['document']
        state = transaction(session,doc,'Line attached to native origin',[
            add('Sketcher::SketchObject','Sketch'),geo('line',start=[0,0,0],end=[5,5,0]),
            c('Coincident',0,1,-1,1),c('Distance',0,math.sqrt(50))])
        assert [a['index'] for a in sketch(state)['axes']] == [-1,-2]
        assert all(not a['handles'] and not a['editable'] for a in sketch(state)['axes'])
        report.append(lifecycle(session,compiler,out,name,doc,state,[e(0),e(-1,1),e(axis)]))
    doc = session.call('new',name='BothNativeAxes')['document']
    state = transaction(session,doc,'Point prepared onto both native axes',[
        add('Sketcher::SketchObject','Sketch'),geo('point',position=[2,3,0],construction=True)])
    report.append(lifecycle(session,compiler,out,'BothAxesReference',doc,state,[e(-1),e(0,1),e(-2)]))
    assert report[-1]['driving'] is False
    doc,state = fixture(session,'BothBlocked')
    state = transaction(session,doc,'Both curves natively blocked',[c('Block',1)])
    report.append(lifecycle(session,compiler,out,'BothBlockedReference',doc,state,[e(0),e(1),e(2,1)]))
    assert report[-1]['driving'] is False
    return report


def values(session,compiler,out):
    report = []
    for name,value,expression,driving in [
            ('Signed','-45 deg',None,None),('Reflex','270 deg',None,None),
            ('MultiTurn','450 deg',None,None),('Zero','0 deg',None,None),
            ('NativeDimensionless','0.5',None,None),('NativeLengthQuantity','1 mm',None,None),
            ('Expression',None,'30 deg + 15 deg',None),('RequestedReference',None,None,False)]:
        doc,state = fixture(session,name)
        report.append(lifecycle(session,compiler,out,name,doc,state,[e(0),e(1),e(2,1)],value,expression,driving))
    return report


def edit_and_failures(session,compiler,out):
    doc,state = fixture(session,'DatumEditing')
    selection = [e(0),e(1),e(2,1)]
    edits = compile_command(compiler,state,selection)
    reply = session.call('transaction',document=doc,edits=edits)
    state,index = reply['state'],reply['edits'][0]['index']
    state = transaction(session,doc,'Name native angle',[
        dict(op='constraint_state',object='Sketch',index=index,name='AngleAtPoint',virtual=True)])
    baseline = out/'Editing-baseline.FCStd'
    session.call('save',document=doc,path=str(baseline));protected=digest(baseline)
    steps = []
    for value,expr in [('30 deg + 15 deg',True),('-45 deg',False),('270 deg',False),('450 deg',False),('0 deg',False),('0.5',False),('1 mm',False)]:
        before = objects(state)['Sketch']
        edits = compile_command(compiler,state,[],value=value,expression=expr,index=index)
        state = transaction(session,doc,'Edit native via-point angle',edits)
        assert angle(state,index)['name'] == 'AngleAtPoint'
        if not expr: assert not objects(state)['Sketch']['properties']['ExpressionEngine']
        accepted = objects(state)['Sketch']
        state = session.call('undo',document=doc);assert same_object(objects(state)['Sketch'],before)
        state = session.call('redo',document=doc);assert same_object(objects(state)['Sketch'],accepted)
        path=out/f'Editing-step{len(steps)}.FCStd';session.call('save',document=doc,path=str(path))
        steps.append(dict(value=value,expression=expr,path=str(path)))
    before = objects(state)['Sketch'];cursor=(state['undo_count'],state['redo_count'])
    for edit,code in [
            (dict(op='datum',object='Sketch',index=index,value='bad unit!'),'native_error'),
            (dict(op='expression',object='Sketch',property=f'Constraints[{index}]',expression='Missing.Length'),'invalid_model'),
            (dict(op='datum',object='Sketch',index=9999,value='45 deg'),'invalid_request')]:
        error=session.call('transaction',document=doc,edits=[edit],expect_error=code)
        state=session.call('inspect',document=doc)
        assert same_object(objects(state)['Sketch'],before) and (state['undo_count'],state['redo_count'])==cursor,error
        assert state['preview'] is None and state['sketch_edit'] is None
    for flags in [dict(active=False),dict(active=True),dict(driving=False),dict(driving=True)]:
        state=transaction(session,doc,'Native angle state',[
            dict(op='constraint_state',object='Sketch',index=index,**flags)])
        for key,value in flags.items(): assert angle(state,index)[key]==value
    target=out/'Editing-result.FCStd';session.call('save',document=doc,path=str(target));session.call('close',document=doc)
    state=session.call('open',path=str(target));assert angle(state,index)['name']=='AngleAtPoint'
    session.call('close',document=state['document']);assert digest(baseline)==protected

    # Failure after preparation must retain the last accepted native preview.
    doc,state=fixture(session,'AtomicPreparation',missing=True)
    before=objects(state)['Sketch'];cursor=(state['undo_count'],state['redo_count'])
    edits=compile_command(compiler,state,selection)
    reply=session.call('preview',document=doc,token='owned-angle',edits=edits)
    accepted=objects(reply['state'])['Sketch'];owner=reply['state']['preview']
    session.call('preview_commit',document=doc,token='wrong-owner',expect_error='preview_owner')
    owned=session.call('inspect',document=doc)
    assert owned['preview']==owner and same_object(objects(owned)['Sketch'],accepted)
    bad=[{**edits[0],'expression':'Missing.Length'}]
    session.call('preview',document=doc,token='owned-angle',edits=bad,expect_error='invalid_model')
    state=session.call('inspect',document=doc)
    assert same_object(objects(state)['Sketch'],accepted) and state['preview']==owner
    state=session.call('preview_cancel',document=doc,token='owned-angle')
    assert same_object(objects(state)['Sketch'],before) and (state['undo_count'],state['redo_count'])==cursor
    session.call('transaction',document=doc,edits=bad,expect_error='invalid_model')
    state=session.call('inspect',document=doc)
    assert same_object(objects(state)['Sketch'],before) and (state['undo_count'],state['redo_count'])==cursor
    state=transaction(session,doc,'Recover after invalid expression',edits);angle(state,sketch(state)['constraint_count']-1)
    session.call('close',document=doc)
    return [dict(case='ExistingDatumExpressionState',passed=True,input=str(baseline),source_sha256=protected,
                 steps=steps,index=index,result=str(target),native_edit_comparison=True),
            dict(case='AtomicFailureAfterPreparation',passed=True,accepted_preview_owner=True,history=True,recovery=True)]


def consumed_session(session,compiler,out):
    doc=session.call('new',name='AngleConsumed')['document']
    state=transaction(session,doc,'Consumed profile and construction helpers',[
        add('PartDesign::Body','Body'),add('Sketcher::SketchObject','Sketch',body='Body'),
        *rectangle('Sketch',40,30),
        geo('line',start=[-10,0,0],end=[10,0,0],construction=True),
        geo('line',start=[0,0,0],end=[5,5,0],construction=True),
        geo('point',position=[0,0,0],construction=True),c('Block',4),c('Coincident',5,1,-1,1),
        c('Distance',5,math.sqrt(50)),c('Coincident',6,1,-1,1),
        add('PartDesign::Pad','Pad',body='Body'),set_value('Pad','Profile',link('Sketch')),set_value('Pad','Length',5)])
    selection=[e(4),e(6,1),e(5)]
    baseline=objects(state)['Sketch'];cursor=state['undo_count'];solid(state,'Pad',6000)
    protected_path=out/'Consumed-baseline.FCStd';session.call('save',document=doc,path=str(protected_path));protected=digest(protected_path)
    session.call('close',document=doc);state=session.call('open',path=str(protected_path));doc=state['document']
    baseline=objects(state)['Sketch'];cursor=state['undo_count']
    for discard in [True,False]:
        state=session.call('sketch_begin',document=doc,object='Sketch',token='angle-session')
        session_baseline=objects(state)['Sketch']
        edits=compile_command(compiler,state,selection,value='60 deg')
        reply=session.call('preview',document=doc,token='angle-preview',edits=edits)
        index=reply['edits'][0]['index'];angle(reply['state'],index)
        state=session.call('preview_cancel',document=doc,token='angle-preview')
        session_restored(objects(state)['Sketch'],session_baseline)
        oracle_rejections(objects(state)['Sketch'],session_baseline,session_restored)
        assert state['undo_count']==0
        state=session.call('preview',document=doc,token='angle-preview',edits=edits)['state']
        state=session.call('preview_commit',document=doc,token='angle-preview');angle(state,index)
        assert state['undo_count']==1 and state['redo_count']==0
        state=session.call('undo',document=doc);session_restored(objects(state)['Sketch'],session_baseline)
        assert state['redo_count']==1
        # Merely opening/canceling the UI's deferred dialog performs no native
        # request. Verify that inspect retains the action cursor and ownership.
        untouched=session.call('inspect',document=doc)
        assert untouched['undo_count']==0 and untouched['redo_count']==1 and untouched['sketch_edit']['token']=='angle-session'
        state=session.call('redo',document=doc);angle(state,index)
        state=transaction(session,doc,'Edit accepted session angle',compile_command(compiler,state,[],value='75 deg',index=index))
        assert state['undo_count']==2
        broken=transaction(session,doc,'Open consumed profile',[dict(op='delete_geometry',object='Sketch',index=0)])
        session.call('sketch_finish',document=doc,token='angle-session',expect_error='invalid_model')
        restored=session.call('inspect',document=doc)
        assert restored['sketch_edit']['token']=='angle-session' and restored['undo_count']==broken['undo_count']
        state=session.call('undo',document=doc);angle(state,index)
        if discard:
            state=session.call('sketch_cancel',document=doc,token='angle-session')
            assert same_object(objects(state)['Sketch'],baseline) and state['undo_count']==cursor
        else:
            state=session.call('sketch_finish',document=doc,token='angle-session')
            assert state['sketch_edit'] is None and state['undo_count']==cursor+1
        solid(state,'Pad',6000)
    accepted=objects(state)['Sketch']
    state=session.call('undo',document=doc);assert same_object(objects(state)['Sketch'],baseline)
    # Canceling an untouched editing session preserves native document Redo.
    session.call('sketch_begin',document=doc,object='Sketch',token='angle-redo')
    state=session.call('sketch_cancel',document=doc,token='angle-redo');assert state['redo_count']==1
    state=session.call('redo',document=doc);assert same_object(objects(state)['Sketch'],accepted)
    path=out/'Consumed-result.FCStd';session.call('save',document=doc,path=str(path));session.call('close',document=doc)
    state=session.call('open',path=str(path));angle(state,index);solid(state,'Pad',6000);session.call('close',document=state['document'])
    assert digest(protected_path)==protected
    return [dict(case='ConsumedSessionHistory',passed=True,input=str(protected_path),source_sha256=protected,result=str(path),
                 native_consumed_comparison=True,selection=selection,index=index,
                 action_undo_redo=True,preview_cancel_commit=True,finish_failure_ownership=True,session_cancel_finish=True,document_undo_redo=True,reopen=True)]


def parents_and_points(session,compiler,out):
    rows=[]
    doc=session.call('new',name='AngleTransformedParent')['document']
    placement=dict(position=[20,30,10],rotation=[0,0,math.sin(math.pi/8),math.cos(math.pi/8)])
    state=transaction(session,doc,'Transformed authoritative source objects',[
        add('Sketcher::SketchObject','Master'),
        dict(op='geometry',object='Master',kind='circle',center=[0,5,0],radius=5),
        constraint('Master','DistanceX',0,3,0.),constraint('Master','DistanceY',0,3,5.),constraint('Master','Radius',0,5.),
        add('Sketcher::SketchObject','SourcePoint'),dict(op='geometry',object='SourcePoint',kind='point',position=[0,0,0],construction=False),
        add('Sketcher::SketchObject','Sketch'),geo('line',start=[0,0,0],end=[5,5,0]),c('Distance',0,math.sqrt(50)),
        *[set_value(name,'Placement',placement) for name in ['Master','SourcePoint','Sketch']]])
    state=transaction(session,doc,'Project native edge and unambiguous standalone vertex',[
        dict(op='external_geometry',object='Sketch',source='Master',subelement='Edge1'),
        dict(op='external_geometry',object='Sketch',source='SourcePoint',subelement='Vertex1'),c('Coincident',0,1,-4,1)])
    rows.append(lifecycle(session,compiler,out,'TransformedExternalPoint',doc,state,[e(-3),e(-4,1),e(0)]))
    row=rows[-1];state=session.call('open',path=row['result']);doc=state['document'];index=row['index']
    before=objects(state);external_before=sketch(state)['externals']
    state=transaction(session,doc,'Update authoritative transformed parents',[
        dict(op='datum',object='Master',index=2,value='6 mm'),
        dict(op='move_geometry',object='SourcePoint',index=0,point=1,position=[0,-1,0])])
    angle(state,index);near(sketch(state)['geometry'][0]['start'][1],-1.)
    near(angle(state,index)['value'],math.pi/4)
    after=objects(state)
    assert [x['geometry_index'] for x in sketch(state)['externals']]==[-3,-4]
    for old,new in zip(external_before,sketch(state)['externals']):
        for key in old:
            if key!='geometry': assert old[key]==new[key],(key,old,new)
    state=session.call('undo',document=doc);assert same(objects(state),before)
    state=session.call('redo',document=doc);assert same(objects(state),after)
    path=out/'TransformedExternalPoint-updated.FCStd';session.call('save',document=doc,path=str(path));session.call('close',document=doc)
    state=session.call('open',path=str(path));assert same(objects(state),after);angle(state,index)
    session.call('close',document=state['document'])
    rows.append(dict(case='TransformedParentUpdates',passed=True,input=row['result'],result=str(path),native_parent_comparison=True,index=index,
                     persistent_external_ids=[-3,-4],undo_redo_reopen=True))
    for name,point in [('Endpoint',e(1,1)),('CircleCenter',e(3,3))]:
        doc,state=fixture(session,name)
        if name=='CircleCenter':
            state=transaction(session,doc,'Expose native circle center',[
                geo('circle',center=[0,0,0],radius=2,construction=True),c('Block',3)])
        rows.append(lifecycle(session,compiler,out,name,doc,state,[e(0),point,e(1)]))
    return rows


def native_rejections(session,compiler,out):
    doc=session.call('new',name='AngleFailedNativePreparation')['document']
    state=transaction(session,doc,'Fixed point lies off fixed circle',[
        add('Sketcher::SketchObject','Sketch'),geo('circle',center=[0,6,0],radius=5),
        geo('line',start=[-5,1,0],end=[5,3,0]),geo('point',position=[0,0,0],construction=True),
        c('DistanceX',0,3,0.),c('DistanceY',0,3,6.),c('Radius',0,5.),
        c('Coincident',2,1,-1,1),c('Distance',1,math.sqrt(104)),c('DistanceX',1,1,-5.)])
    baseline=objects(state)['Sketch'];cursor=(state['undo_count'],state['redo_count'])
    edits=compile_command(compiler,state,[e(0),e(1),e(2,1)])
    session.call('preview',document=doc,token='failed-angle',edits=edits,expect_error='invalid_via_point')
    state=session.call('inspect',document=doc)
    assert same_object(objects(state)['Sketch'],baseline) and state['preview'] is None and (state['undo_count'],state['redo_count'])==cursor
    state=transaction(session,doc,'Unfix native point for recovery',[dict(op='delete_constraint',object='Sketch',index=3)])
    state=transaction(session,doc,'Recover failed preparation',compile_command(compiler,state,[e(0),e(1),e(2,1)]));angle(state,sketch(state)['constraint_count']-1)
    session.call('close',document=doc)
    doc,state=fixture(session,'WeightGuard','BSpline',spline_cleanup=True)
    state=transaction(session,doc,'Expose native weight circles',[dict(op='expose_internal',object='Sketch',index=0)])
    weight=sketch(state)['spline_controls'][0]['control_index'];before=objects(state)['Sketch']
    request=dict(op='constraint',snapshot=before,kind='Angle',selection=[e(weight),e(1),e(2,1)])
    result=subprocess.run([str(compiler)],input=json.dumps(request)+'\n',capture_output=True,text=True,check=True)
    assert not json.loads(result.stdout)['ok']
    session.call('transaction',document=doc,edits=[dict(op='via_point',object='Sketch',kind='AngleViaPoint',args=[weight,1,2,1,0.])],expect_error='invalid_via_point')
    state=session.call('inspect',document=doc);assert same_object(objects(state)['Sketch'],before)
    session.call('close',document=doc)
    return [dict(case='NativePreparationAndWeightRejections',passed=True,failed_preparation_atomic=True,recovery=True,native_weight_guard=True)]


def native_control(obj,selection,value=None,expression=None,driving=None):
    """Pinned CmdSketcherConstrainAngle core, independent of worker helpers."""
    import FreeCAD as App
    import Part
    import Sketcher
    curves = [item['index'] for item in selection if item['point'] == 0]
    point = next(item for item in selection if item['point'] != 0)
    first,second = curves
    fixed = lambda i: i < 0 or any(c.Type=='Block' and c.First==i and c.FirstPos==0 for c in obj.Constraints)
    reference = (fixed(first) and fixed(second)) or driving is False
    geometry = lambda i: obj.Geometry[i] if i >= 0 else obj.ExternalGeo[-i-1]
    for i in [first,second,first]:
        p = obj.getPoint(point['index'],point['point'])
        if not obj.isPointOnCurve(i,p.x,p.y) and not isinstance(geometry(i),Part.BSplineCurve):
            obj.addConstraint(Sketcher.Constraint('PointOnObject',point['index'],point['point'],i))
            obj.solve()
    p = obj.getPoint(point['index'],point['point'])
    measured = obj.calculateAngleViaPoint(first,second,p.x,p.y)
    if measured < -Part.Precision.angular():
        first,second,measured = second,first,-measured
    obj.addConstraint(Sketcher.Constraint('AngleViaPoint',first,second,point['index'],point['point'],float(format(measured,'.8g'))))
    removed = [i for i,c in enumerate(obj.Constraints) if c.Type=='PointOnObject' and c.First==point['index']
               and c.Second in curves and isinstance(geometry(c.Second),Part.BSplineCurve)]
    for i in reversed(removed): obj.delConstraint(i)
    index = obj.ConstraintCount-1
    if reference: obj.setDriving(index,False)
    if value is not None: obj.setDatum(index,App.Units.Quantity(value))
    if expression is not None: obj.setExpression(f'Constraints[{index}]',expression)
    assert obj.solve() == 0,(obj.ConflictingConstraints,obj.RedundantConstraints)
    # Match the ordinary edit boundary: via-point solver/cleanup first, then
    # document recompute, with no additional solve after serialization timing.
    obj.Document.recompute()
    return index


def native_signature(obj):
    import Part
    point = lambda p: [p.x,p.y,p.z]
    geometry = []
    for i,g in enumerate(obj.Geometry):
        handles = [point(obj.getPoint(i,p)) for p in ([1] if isinstance(g,Part.Point) else [3] if isinstance(g,(Part.Circle,Part.Ellipse)) else [1,2])]
        entry = dict(type=g.TypeId,construction=obj.getConstruction(i),id=obj.getGeometryId(i),handles=handles)
        if isinstance(g,Part.BSplineCurve):
            entry.update(poles=[point(p) for p in g.getPoles()],weights=g.getWeights(),knots=g.getKnots(),
                         multiplicities=g.getMultiplicities(),degree=g.Degree,periodic=g.isPeriodic())
        for key in ['Radius','MajorRadius','MinorRadius','Focal','AngleXU','FirstParameter','LastParameter']:
            if hasattr(g,key): entry[key] = getattr(g,key)
        geometry.append(entry)
    return dict(geometry=geometry,constraints=[{key:getattr(c,key) for key in [
        'Type','First','FirstPos','Second','SecondPos','Third','ThirdPos','Value','Driving','IsActive','Name','InVirtualSpace']}
        for c in obj.Constraints],expressions=list(obj.ExpressionEngine))


def native_compare(output,report_path):
    import FreeCAD as App
    report = json.loads((output/'report.json').read_text())
    comparisons = []
    for row in report:
        if row.get('native_consumed_comparison'):
            doc=App.openDocument(row['input']);doc.UndoMode=1;obj=doc.getObject('Sketch')
            before=native_signature(obj)
            assert obj.State==['Up-to-date'] and obj.getStatusString()=='Valid'
            doc.openTransaction('Pinned native sketch action')
            native_control(obj,row['selection'],'60 deg')
            doc.abortTransaction();obj.solve();obj.recompute()
            assert obj.State==['Touched'] and obj.getStatusString()=='Touched'
            assert same(native_signature(obj),before)
            doc.recompute();assert obj.State==['Up-to-date'] and obj.getStatusString()=='Valid'
            native_control(obj,row['selection'],'75 deg');expected=native_signature(obj)
            saved=App.openDocument(row['result']);assert same(native_signature(saved.getObject('Sketch')),expected)
            near(saved.getObject('Pad').Shape.Volume,6000.)
            App.closeDocument(saved.Name);App.closeDocument(doc.Name)
            comparisons.append(dict(case=row['case'],passed=True,matched_session_status=True,native_result=True))
            continue
        if row.get('native_parent_comparison'):
            import FreeCAD as App
            doc=App.openDocument(row['input']);obj=doc.getObject('Sketch')
            doc.getObject('Master').setDatum(2,App.Units.Quantity('6 mm'))
            doc.getObject('SourcePoint').moveGeometry(0,1,App.Vector(0,-1,0),False)
            doc.recompute();expected=native_signature(obj)
            saved=App.openDocument(row['result']);assert same(native_signature(saved.getObject('Sketch')),expected)
            assert [(o.Name,list(names)) for o,names in obj.ExternalGeometry]==[(o.Name,list(names)) for o,names in saved.getObject('Sketch').ExternalGeometry]
            App.closeDocument(saved.Name);App.closeDocument(doc.Name)
            comparisons.append(dict(case=row['case'],passed=True,native_parent_update=True,external_source_links=True))
            continue
        if row.get('native_edit_comparison'):
            import FreeCAD as App
            assert digest(row['input'])==row['source_sha256']
            doc=App.openDocument(row['input']);obj=doc.getObject('Sketch');index=row['index']
            for step in row['steps']:
                if step['expression']:
                    obj.setExpression(f'Constraints[{index}]',step['value'])
                else:
                    obj.setExpression(f'Constraints[{index}]',None)
                    obj.setDatum(index,App.Units.Quantity(step['value']))
                doc.recompute();assert obj.solve()==0
                expected=native_signature(obj)
                saved=App.openDocument(step['path']);actual=native_signature(saved.getObject('Sketch'))
                assert same(actual,expected),(row['case'],step['value'])
                App.closeDocument(saved.Name)
            App.closeDocument(doc.Name)
            assert digest(row['input'])==row['source_sha256']
            comparisons.append(dict(case=row['case'],passed=True,steps=len(row['steps']),native_units_values_expressions=True))
            continue
        if not row.get('baseline'): continue
        assert digest(row['baseline']) == row['source_sha256']
        doc = App.openDocument(row['baseline'])
        obj = doc.getObject('Sketch')
        doc.UndoMode = 1
        version = lambda: getattr(obj,'_ElementMapVersion')
        versions = dict(before=version())
        baseline_signature = native_signature(obj)
        def create():
            return native_control(obj,row['selection'],row.get('value'),row.get('expression'),row.get('requested_driving'))
        doc.openTransaction('Pinned native preview')
        create()
        versions['active'] = version()
        doc.abortTransaction();doc.recompute()
        versions['cancel'] = version()
        assert same(native_signature(obj),baseline_signature),row['case']+' native abort'
        doc.openTransaction('Pinned native action')
        index = create()
        doc.commitTransaction()
        expected = native_signature(obj)
        doc.undo();doc.recompute()
        versions['undo'] = version()
        assert same(native_signature(obj),baseline_signature),row['case']+' native Undo'
        App.closeDocument(doc.Name)
        doc = App.openDocument(row['result'])
        obj = doc.getObject('Sketch')
        versions['reopen']=getattr(obj,'_ElementMapVersion')
        assert versions == row['versions'],(row['case'],versions,row['versions'])
        actual = native_signature(obj)
        assert same(actual,expected),row['case']
        assert index == row['index'] and obj.Constraints[index].Type == 'Angle'
        c = obj.Constraints[index]
        p = obj.getPoint(c.Third,c.ThirdPos)
        start = obj.calculateAngleViaPoint(-1,c.First,p.x,p.y)
        assert math.isfinite(start)
        App.closeDocument(doc.Name)
        assert digest(row['baseline']) == row['source_sha256']
        comparisons.append(dict(case=row['case'],passed=True,native_identity=True,geometry_branch=True,
                                constraints_expressions=True,matched_metadata_oracle=versions,source_hash_preserved=True))
    with report_path.open('x',encoding='utf-8') as stream: stream.write(json.dumps(comparisons,indent=2)+'\n')
    print('Native comparisons',len(comparisons),flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ['host','compiler','runtime','output']: parser.add_argument('--'+key,type=Path,required=True)
    parser.add_argument('--group',choices=['all','creation','splines','values','editing','consumed','parents','rejections'],default='all')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True,exist_ok=False)
    config = output/'native-config';config.mkdir();os.environ['FREECAD_USER_HOME'] = str(config)
    runtime = json.loads(args.runtime.read_text(encoding='utf-8-sig'))
    session = SketchSession(args.host.resolve(),runtime)
    report = []
    try:
        groups = dict(creation=[families,axes_and_fixed],splines=[two_splines],values=[values],editing=[edit_and_failures],consumed=[consumed_session],
                      parents=[parents_and_points],rejections=[native_rejections])
        for key,functions in groups.items():
            if args.group not in ['all',key]: continue
            for function in functions:
                report += function(session,args.compiler.resolve(),output)
                print(function.__name__,'passed',flush=True)
    finally:
        session.close()
        (output/'report.json').write_bytes((json.dumps(report,indent=2)+'\n').encode())
    subprocess.run([runtime['executable'],*runtime['args'],str(Path(__file__).resolve()),
                    '--native-compare',str(output),str(output/'native-comparison.json')],check=True)
    print(json.dumps(dict(passed=True,cases=len(report)),indent=2))


if __name__ == '__main__':
    if len(sys.argv)==4 and sys.argv[1]=='--native-compare': native_compare(Path(sys.argv[2]),Path(sys.argv[3]))
    else: main()
