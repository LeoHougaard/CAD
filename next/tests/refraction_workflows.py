"""Native refraction workflow acceptance using the editor's shared Rust compiler.

Every output document is disposable. No GUI automation or production writes.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess
from native_workflows import add, constraint, objects, transaction, near, rectangle, set_value, solid
from sketch_editor_workflows import SketchSession
from common_dimension_workflows import same


def geo(kind, **kw):
    return dict(op='geometry', object='Sketch', kind=kind, **kw)


def c(kind, *args):
    return constraint('Sketch', kind, *args)


def e(index, point=0):
    return dict(index=index, point=point)


SELECTION = [e(0, 2), e(1, 1), e(2)]


def compile_request(compiler, **request):
    result = subprocess.run([str(compiler)], input=json.dumps(request)+'\n', capture_output=True, text=True, encoding='utf-8', check=True)
    response = json.loads(result.stdout)
    assert response['ok'], response
    return response['edits']


def create(compiler, state, value='1.5', selection=SELECTION):
    return compile_request(compiler, op='constraint', snapshot=objects(state)['Sketch'], kind='SnellsLaw', selection=selection, value=value)


def datum(compiler, state, index, value, expression=False):
    return compile_request(compiler, op='datum', snapshot=objects(state)['Sketch'], index=index, value=value, expression=expression)


def ratio(state, index, expected, boundary_horizontal=True):
    sketch = objects(state)['Sketch']['sketch']
    snell = sketch['constraints'][index]
    assert snell['type'] == 'SnellsLaw'
    near(snell['value'], expected)
    assert not sketch['conflicting_constraints'] and not sketch['redundant_constraints']
    if boundary_horizontal and snell['driving']:
        sine = []
        for line in sketch['geometry'][:2]:
            x, y = [line['end'][i]-line['start'][i] for i in range(2)]
            sine.append(abs(x)/math.hypot(x, y))
        near(sine[0]/sine[1], abs(expected))


def fixture(session, name, prepared=True, boundary=None):
    state = session.call('new', name=name)
    doc = state['document']
    if prepared:
        base = [geo('line', start=[-5,-5,0], end=[0,0,0]), geo('line', start=[0,0,0], end=[5,5,0]),
                boundary or geo('line', start=[-10,0,0], end=[10,0,0]),
                c('Block',0),c('Block',2),c('Coincident',0,2,1,1),c('Distance',1,math.sqrt(50))]
    else:
        base = [geo('line', start=[-5,-4,0], end=[0,1,0]),geo('line',start=[2,4,0],end=[7,9,0]),
                geo('line',start=[-10,0,0],end=[10,0,0]),c('Distance',0,math.sqrt(50)),c('Angle',0,math.pi/4),
                c('DistanceX',0,1,-5.),c('Distance',1,math.sqrt(50)),c('Block',2)]
    state = transaction(session, doc, 'Native refraction fixture',[add('Sketcher::SketchObject','Sketch')]+base)
    return doc,state


def rejected(session, doc, edits, code, baseline):
    error = session.call('transaction',document=doc,edits=edits,expect_error=code)
    state = session.call('inspect', document=doc)
    assert same(objects(state)['Sketch']['sketch'],objects(baseline)['Sketch']['sketch']), (error,state)
    return state


def workflow(session, compiler, out):
    report=[]
    for prepared in [True,False]:
        name='Prepared' if prepared else 'PrepareMissing'
        doc,base=fixture(session,'Refraction'+name,prepared)
        before=objects(base)['Sketch']['sketch']
        edits=create(compiler,base)
        index=before['constraint_count']+(0 if prepared else 2)
        undo=base['undo_count']
        candidate=session.call('preview', document=doc,token='create',edits=edits)
        assert len(candidate['edits'][0]['preparation'])==(0 if prepared else 2)
        ratio(candidate['state'],index,1.5)
        candidate=session.call('preview', document=doc,token='create',edits=create(compiler,base,'1.25'))
        ratio(candidate['state'],index,1.25)
        state=session.call('preview_cancel',document=doc,token='create')
        assert same(objects(state)['Sketch']['sketch'],before) and state['undo_count']==undo
        state=transaction(session,doc,'Create refraction',edits)
        ratio(state,index,1.5)
        assert state['undo_count']==undo+1
        state=session.call('undo',document=doc);assert same(objects(state)['Sketch']['sketch'],before)
        state=session.call('redo',document=doc);ratio(state,index,1.5)
        state=transaction(session,doc,'Existing ratio unitless datum',datum(compiler,state,index,'1.25'))
        ratio(state,index,1.25)
        state=transaction(session,doc,'Native ratio expression',datum(compiler,state,index,'3 / 2',True))
        ratio(state,index,1.5)
        state=session.call('undo',document=doc);ratio(state,index,1.25)
        state=session.call('redo',document=doc);ratio(state,index,1.5)
        state=transaction(session,doc,'Name refraction ratio',[dict(op='constraint_state',object='Sketch',index=index,name='IndexRatio')])
        state=transaction(session,doc,'Replace named expression with datum',datum(compiler,state,index,'1.2'))
        assert not objects(state)['Sketch']['properties']['ExpressionEngine']
        ratio(state,index,1.2)
        # Every failed action must restore geometry, constraints and expressions.
        # Pinned native setDatum accepts signed values and Quantity inputs.
        for value,expected in [('2 mm',2),('180 deg',math.pi),('-1',-1)]:
            state=transaction(session,doc,'Native signed/quantity datum',datum(compiler,state,index,value))
            ratio(state,index,expected)
        for value,code in [('0','native_error'),('0.1','native_error')]:
            state=rejected(session,doc,datum(compiler,state,index,value),code,state)
        for value,expected in [('2 mm',2),('30 deg',30),('-2',-2),('3 mm / 2 mm',1.5)]:
            state=transaction(session,doc,'Native signed/quantity expression',datum(compiler,state,index,value,True))
            ratio(state,index,expected)
        for value in ['0','0.1','1 / 0','MissingRatio']:
            # FreeCAD emits its own error for parse/evaluation failures.
            if value in ['1 / 0','MissingRatio']:
                session.call('transaction',document=doc,edits=datum(compiler,state,index,value,True),expect_error='invalid_model')
                new=session.call('inspect',document=doc)
                assert same(objects(new)['Sketch']['sketch'],objects(state)['Sketch']['sketch'])
                state=new
            else:
                state=rejected(session,doc,datum(compiler,state,index,value,True),'invalid_model',state)
        # Native reference/driving policy is tested, not inferred from getDatum.
        state=transaction(session,doc,'Native reference ratio',[dict(op='constraint_state',object='Sketch',index=index,driving=False)])
        assert objects(state)['Sketch']['sketch']['constraints'][index]['driving'] is False
        state=transaction(session,doc,'Return native driving ratio',[dict(op='constraint_state',object='Sketch',index=index,driving=True)])
        state=transaction(session,doc,'Recover after rejected ratios',datum(compiler,state,index,'1.5'))
        ratio(state,index,1.5)
        path=out/(name+'.FCStd')
        session.call('save',document=doc,path=str(path));session.call('close',document=doc)
        state=session.call('open',path=str(path));doc=state['document'];ratio(state,index,1.5)
        state=transaction(session,doc,'Edit after reopen',datum(compiler,state,index,'1.25'));ratio(state,index,1.25)
        # Change incidence through the native datum path, preserving ratio.
        if not prepared:
            state=transaction(session,doc,'Change incidence angle',[dict(op='datum',object='Sketch',index=1,value='35 deg')]);ratio(state,index,1.25)
        session.call('close',document=doc)
        report.append(dict(case=name,passed=True,prepared_relations=0 if prepared else 2,preview_replace_cancel=True,document_undo_redo=True,datum_expression=True,invalid_atomic_recovery=True,native_reference_policy=True,reopen=True,path=str(path)))
    return report



def boundary_workflows(session, compiler, out):
    boundaries = [
        ('Circle',geo('circle',center=[0,10,0],radius=10)),
        ('Arc',geo('arc',start=[-10,10,0],mid=[0,0,0],end=[10,10,0])),
        ('Ellipse',geo('ellipse',major=[10,5,0],minor=[0,10,0],center=[0,5,0])),
        ('EllipseArc',geo('ellipse_arc',major=[10,5,0],minor=[0,10,0],center=[0,5,0],start_parameter=math.pi,last_parameter=2*math.pi,end_parameter=2*math.pi)),
        ('ParabolaArc',geo('parabola_arc',center=[0,0,0],focal=2,angle=math.pi/2,start_parameter=-2,end_parameter=2)),
        ('HyperbolaArc',geo('hyperbola_arc',major=[0,0,0],minor=[3,2,0],center=[0,2,0],start_parameter=-1,end_parameter=1)),
        ('BSpline',geo('bspline',poles=[[-10,1,0],[-5,-1/3,0],[5,-1/3,0],[10,1,0]])),
    ]
    report=[]
    for name,boundary in boundaries:
        boundary.pop('last_parameter',None)
        doc,state=fixture(session,'Boundary'+name,boundary=boundary)
        base=state
        edits=create(compiler,state)
        if name == 'BSpline':
            session.call('preview',document=doc,token='curve',edits=edits,expect_error='invalid_refraction')
            state=session.call('inspect',document=doc)
            assert state['preview'] is None and same(objects(state)['Sketch']['sketch'],objects(base)['Sketch']['sketch'])
            session.call('close',document=doc)
            report.append(dict(case='BoundaryBSplineNativeRejection',passed=True,native_fixture_also_rejected=True,atomic_rollback=True))
            continue
        result=session.call('preview',document=doc,token='curve',edits=edits)
        index=result['edits'][0]['index']
        ratio(result['state'],index,1.5,False)
        state=session.call('preview_cancel',document=doc,token='curve')
        assert same(objects(state)['Sketch']['sketch'],objects(base)['Sketch']['sketch'])
        state=transaction(session,doc,'Native curved refraction',edits)
        ratio(state,index,1.5,False)
        state=transaction(session,doc,'Curved boundary ratio edit',datum(compiler,state,index,'1.25'))
        ratio(state,index,1.25,False)
        session.call('undo',document=doc);state=session.call('redo',document=doc);ratio(state,index,1.25,False)
        path=out/('Boundary'+name+'.FCStd');session.call('save',document=doc,path=str(path));session.call('close',document=doc)
        state=session.call('open',path=str(path));ratio(state,index,1.25,False);session.call('close',document=state['document'])
        report.append(dict(case='Boundary'+name,passed=True,path=str(path),native_index=index,preview_cancel=True,datum_undo_redo_reopen=True))
    return report


def external_workflow(session, compiler, out):
    doc=session.call('new',name='RefractionExternal')['document']
    state=transaction(session,doc,'Native parent and rays',[
        add('Sketcher::SketchObject','Master'),dict(op='geometry',object='Master',kind='line',start=[0,0,0],end=[10,0,0]),
        constraint('Master','Coincident',0,1,-1,1),constraint('Master','Distance',0,10.),constraint('Master','Angle',0,0.),
        add('Sketcher::SketchObject','Sketch'),geo('line',start=[-5,-5,0],end=[0,0,0]),geo('line',start=[0,0,0],end=[5,5,0]),
        c('Block',0),c('Coincident',0,2,1,1),c('Distance',1,math.sqrt(50))])
    state=transaction(session,doc,'Project recomputed parent',[dict(op='external_geometry',object='Sketch',source='Master',subelement='Edge1')])
    state=transaction(session,doc,'External boundary refraction',create(compiler,state,selection=[e(0,2),e(1,1),e(-3)]))
    index=3
    state=transaction(session,doc,'Rotate parent boundary',[dict(op='datum',object='Master',index=2,value='15 deg')])
    ratio(state,index,1.5,False)
    def check(state):
        sketch=objects(state)['Sketch']['sketch'];line=sketch['externals'][0]['geometry']
        tangent=[line['end'][i]-line['start'][i] for i in range(2)];sines=[]
        for ray in sketch['geometry'][:2]:
            direction=[ray['end'][i]-ray['start'][i] for i in range(2)]
            sines.append(abs(sum(a*b for a,b in zip(tangent,direction)))/(math.hypot(*tangent)*math.hypot(*direction)))
        near(sines[0]/sines[1],1.5)
    check(state)
    session.call('undo',document=doc);state=session.call('redo',document=doc);check(state)
    path=out/'ExternalParent.FCStd';session.call('save',document=doc,path=str(path));session.call('close',document=doc)
    state=session.call('open',path=str(path));check(state);session.call('close',document=state['document'])
    return [dict(case='ExternalParent',passed=True,parent_rotation=True,undo_redo_reopen=True,path=str(path))]


def consumed_workflow(session, compiler, out):
    doc=session.call('new',name='ConsumedRefraction')['document']
    rays=[geo('line',start=[-5,-5,0],end=[0,0,0],construction=True),geo('line',start=[0,0,0],end=[5,5,0],construction=True),
          geo('line',start=[-10,0,0],end=[10,0,0],construction=True),c('Block',4),c('Block',6),c('Coincident',4,2,5,1),c('Distance',5,math.sqrt(50))]
    base=transaction(session,doc,'Consumed sketch with construction rays',[add('PartDesign::Body','Body'),add('Sketcher::SketchObject','Sketch','Body')]+rectangle('Sketch')+rays+[
        add('PartDesign::Pad','Pad','Body'),set_value('Pad','Profile',dict(link='Sketch')),set_value('Pad','Length',5)])
    solid(base,'Pad',6000)
    before=objects(base)['Sketch']['sketch'];index=before['constraint_count'];undo=base['undo_count']
    for token in ['cancel-refraction','finish-refraction']:
        state=session.call('sketch_begin',document=doc,object='Sketch',token=token)
        edits=create(compiler,state,selection=[e(4,2),e(5,1),e(6)])
        state=session.call('preview',document=doc,token='new-ratio',edits=edits)['state']
        near(objects(state)['Sketch']['sketch']['constraints'][index]['value'],1.5);solid(state,'Pad',6000)
        state=session.call('preview_cancel',document=doc,token='new-ratio');assert same(objects(state)['Sketch']['sketch'],before)
        state=transaction(session,doc,'Refraction action',edits);assert state['undo_count']==1
        state=session.call('undo',document=doc);assert same(objects(state)['Sketch']['sketch'],before)
        state=session.call('redo',document=doc);near(objects(state)['Sketch']['sketch']['constraints'][index]['value'],1.5)
        baseline=state
        state=rejected(session,doc,datum(compiler,state,index,'0'),'native_error',baseline)
        state=session.call('preview',document=doc,token='ratio-edit',edits=datum(compiler,state,index,'1.25'))['state']
        state=session.call('preview_cancel',document=doc,token='ratio-edit');near(objects(state)['Sketch']['sketch']['constraints'][index]['value'],1.5)
        state=transaction(session,doc,'Ratio expression action',datum(compiler,state,index,'5 / 4',True))
        near(objects(state)['Sketch']['sketch']['constraints'][index]['value'],1.25)
        if token.startswith('cancel'):
            state=session.call('sketch_cancel',document=doc,token=token);assert same(objects(state)['Sketch']['sketch'],before) and state['undo_count']==undo
        else:
            state=session.call('sketch_finish',document=doc,token=token);assert state['undo_count']==undo+1 and state['sketch_edit'] is None
        solid(state,'Pad',6000)
    state=session.call('undo',document=doc);assert same(objects(state)['Sketch']['sketch'],before)
    state=session.call('redo',document=doc);near(objects(state)['Sketch']['sketch']['constraints'][index]['value'],1.25)
    path=out/'Consumed.FCStd';session.call('save',document=doc,path=str(path));session.call('close',document=doc)
    state=session.call('open',path=str(path));doc=state['document'];solid(state,'Pad',6000)
    state=session.call('sketch_begin',document=doc,object='Sketch',token='reopen-edit')
    state=transaction(session,doc,'Edit reopened consumed ratio',datum(compiler,state,index,'1.5'))
    state=session.call('sketch_finish',document=doc,token='reopen-edit');near(objects(state)['Sketch']['sketch']['constraints'][index]['value'],1.5);solid(state,'Pad',6000)
    session.call('close',document=doc)
    return [dict(case='ConsumedSketchSession',passed=True,construction_geometry=True,action_undo_redo=True,preview_cancel=True,invalid_atomic_recovery=True,session_cancel=True,finish_one_document_undo=True,reopen_edit=True,path=str(path))]


def failure_and_dependency_workflows(session, compiler, out):
    doc,state=fixture(session,'RefractionFailureRecovery',False)
    baseline=state;edits=create(compiler,state)
    # Invalid create must also remove all automatically added relationships.
    bad=create(compiler,state,'0')
    session.call('preview',document=doc,token='failed-first',edits=bad,expect_error='invalid_refraction')
    state=session.call('inspect',document=doc)
    assert state['preview'] is None and same(objects(state)['Sketch']['sketch'],objects(baseline)['Sketch']['sketch'])
    accepted=session.call('preview',document=doc,token='replace',edits=edits)['state']
    session.call('preview',document=doc,token='replace',edits=create(compiler,baseline,'0.1'),expect_error='invalid_refraction')
    state=session.call('inspect',document=doc)
    assert state['preview']['token']=='replace' and same(objects(state)['Sketch']['sketch'],objects(accepted)['Sketch']['sketch'])
    state=session.call('preview_cancel',document=doc,token='replace')
    assert same(objects(state)['Sketch']['sketch'],objects(baseline)['Sketch']['sketch'])
    # A conflict after valid preparation must roll back the complete compound edit.
    impossible=edits+[c('SnellsLaw',0,2,1,1,2,1.5)]
    state=rejected(session,doc,impossible,'invalid_model',state)
    state=transaction(session,doc,'Recover after atomic create failure',edits)
    session.call('close',document=doc)
    doc,state=fixture(session,'RefractionDependentExpression',True)
    state=transaction(session,doc,'Create dependent ratio',create(compiler,state))
    state=transaction(session,doc,'Bind to native ray length',datum(compiler,state,4,'2 * Constraints[3] / (5 mm) - 1',True))
    ratio(state,4,2*math.sqrt(50)/5-1)
    state=transaction(session,doc,'Update expression parent length',[dict(op='datum',object='Sketch',index=3,value='6 mm')])
    ratio(state,4,1.4)
    # Recompute must reject a newly zero expression even when a different datum changed.
    state=rejected(session,doc,[dict(op='datum',object='Sketch',index=3,value='2.5 mm')],'invalid_model',state)
    state=transaction(session,doc,'Recover parent-dependent expression',[dict(op='datum',object='Sketch',index=3,value='7 mm')])
    ratio(state,4,1.8)
    path=out/'DependentExpression.FCStd';session.call('save',document=doc,path=str(path));session.call('close',document=doc)
    state=session.call('open',path=str(path));ratio(state,4,1.8);session.call('close',document=state['document'])
    return [dict(case='FailureAndDependentExpression',passed=True,first_preview_failure=True,replacement_preserves_last=True,compound_atomic_failure=True,parent_expression_update_and_rejection=True,recovery_reopen=True,path=str(path))]

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--host',type=Path,required=True);parser.add_argument('--compiler',type=Path,required=True);parser.add_argument('--runtime',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    a=parser.parse_args();a.output.mkdir(parents=True,exist_ok=True)
    session=SketchSession(a.host,json.loads(a.runtime.read_text()))
    try:
        report=workflow(session,a.compiler,a.output)
        report+=boundary_workflows(session,a.compiler,a.output)
        report+=external_workflow(session,a.compiler,a.output)
        report+=consumed_workflow(session,a.compiler,a.output)
        report+=failure_and_dependency_workflows(session,a.compiler,a.output)
        (a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps(report,indent=2))
    finally:session.close()


if __name__=='__main__':main()
