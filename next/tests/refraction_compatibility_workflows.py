"""Compare refraction with pinned FreeCAD, generating generic native CI inputs."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys

PRESERVED = ['create_negative_large', 'inactive_negative', 'inactive_zero',
             'reference_becomes_negative', 'expression_negative', 'parent_expression_negative']

def edit(op, **values):
    return dict(op=op, object='Sketch', **values)

def cases():
    return [
        ('raw_signed_create', 'positive_base', [edit('delete_constraint',index=4), edit('constraint',kind='SnellsLaw',args=[0,2,1,1,2,-1.5])], True),
        ('prepared_signed_create', 'positive_base', [edit('delete_constraint',index=4), edit('refraction',args=[0,2,1,1,2,1.0],value='-1.5')], True),
        ('missing_signed_create', 'CreateMissing', [edit('refraction',args=[0,2,1,1,2,1.0],value='-1.5')], True),
        ('creation_quantity_value', 'positive_base', [edit('delete_constraint',index=4), edit('refraction',args=[0,2,1,1,2,1.0],value='30 deg')], True),
        ('small_signed_create', 'negative_small_low_incidence', [edit('delete_constraint',index=4), edit('refraction',args=[0,2,1,1,2,1.0],value='-0.5')], True),
        ('signed_datum', 'positive_base', [edit('datum',index=4,value='-1.5')], True),
        ('quantity_datum', 'positive_base', [edit('datum',index=4,value='2 mm')], True),
        ('angle_quantity_datum', 'negative_small_low_incidence', [edit('datum',index=4,value='30 deg')], True),
        ('signed_expression', 'positive_base', [edit('expression',property='Constraints[4]',expression='-3 / 2')], True),
        ('quantity_expression', 'positive_base', [edit('expression',property='Constraints[4]',expression='2 mm')], True),
        ('angle_quantity_expression', 'positive_base', [edit('expression',property='Constraints[4]',expression='30 deg')], True),
        ('cancelled_unit_expression', 'positive_base', [edit('expression',property='Constraints[4]',expression='3 mm / 2 mm')], True),
        ('negative_reference_move', 'positive_base', [edit('constraint_state',index=4,driving=False),edit('move_geometry',index=1,point=2,position=[-5,5,0])], True),
        ('negative_parent_expression', 'parent_positive_base', [dict(op='set',object='Parent',property='Ratio',value=-1.5)], True),
        ('inactive_zero_datum', 'inactive_zero', [edit('datum',index=4,value='0')], True),
        ('inactive_negative_datum', 'inactive_zero', [edit('datum',index=4,value='-1.5')], True),
        ('inactive_zero_expression', 'inactive_zero', [edit('expression',property='Constraints[4]',expression='0')], True),
        ('raw_inactive_zero_create', 'positive_base', [edit('delete_constraint',index=4),edit('constraint',kind='SnellsLaw',args=[0,2,1,1,2,0.0]),edit('constraint_state',index=4,active=False)], True),
        ('inactive_zero_unrelated_edit', 'inactive_zero', [edit('constraint_state',index=4,name='DisabledRatio')], True),
        ('invalid_zero_datum', 'positive_base', [edit('datum',index=4,value='0')], False),
        ('invalid_small_datum', 'positive_base', [edit('datum',index=4,value='0.1')], False),
        ('invalid_zero_expression', 'positive_base', [edit('expression',property='Constraints[4]',expression='0')], False),
        ('invalid_division_expression', 'positive_base', [edit('expression',property='Constraints[4]',expression='1 / 0')], False),
        ('invalid_identifier_expression', 'positive_base', [edit('expression',property='Constraints[4]',expression='MissingRatio')], False),
        ('invalid_parent_expression', 'parent_positive_base', [dict(op='set',object='Parent',property='Ratio',value=0.1)], False),
        ('invalid_missing_create', 'CreateMissing', [edit('refraction',args=[0,2,1,1,2,1.0],value='0')], False),
        ('invalid_compound_create', 'CreateMissing', [edit('refraction',args=[0,2,1,1,2,1.0],value='-1.5'), edit('constraint',kind='SnellsLaw',args=[0,2,1,1,2,-1.5])], False),
    ]

def generate_inputs(inputs):
    """Native-only generic fixtures. No worker, private models or GUI dependency."""
    import FreeCAD as App
    import Part
    import Sketcher
    inputs.mkdir()
    def fixture(name, value=1.5, inactive=False, ray_start=(-5,-5,0)):
        d=App.newDocument(name)
        s=d.addObject('Sketcher::SketchObject','Sketch')
        for start,end in [(ray_start,(0,0,0)),((0,0,0),(5,5,0)),((-10,0,0),(10,0,0))]:
            s.addGeometry(Part.LineSegment(App.Vector(*start),App.Vector(*end)),False)
        for c in [Sketcher.Constraint('Block',0),Sketcher.Constraint('Block',2),
                  Sketcher.Constraint('Coincident',0,2,1,1),Sketcher.Constraint('Distance',1,math.sqrt(50))]:
            s.addConstraint(c)
        index=s.addConstraint(Sketcher.Constraint('SnellsLaw',0,2,1,1,2,float(value)))
        if inactive:s.setActive(index,False)
        d.recompute()
        return d,s
    def save(d):
        d.recompute()
        s=d.Sketch
        assert s.solve()==0 and s.isValid(),d.Name
        assert not any([s.ConflictingConstraints,s.RedundantConstraints,s.MalformedConstraints]),d.Name
        d.saveAs(str(inputs/(d.Name+'.FCStd')))
        App.closeDocument(d.Name)
    for name,value,inactive in [('positive_base',1.5,False),('create_negative_large',-1.5,False),
                                ('inactive_negative',-1.5,True),('inactive_zero',0,True)]:
        d,s=fixture(name,value,inactive);save(d)
    d,s=fixture('negative_small_low_incidence',-.5,ray_start=(-1,-5,0));save(d)
    d,s=fixture('reference_becomes_negative')
    s.setDriving(4,False)
    s.moveGeometry(1,2,App.Vector(-5,5,0),False)
    save(d)
    d,s=fixture('expression_negative')
    s.setExpression('Constraints[4]','-3 / 2');save(d)
    for name,value in [('parent_positive_base',1.5),('parent_expression_negative',-1.5)]:
        d,s=fixture(name)
        parent=d.addObject('App::FeaturePython','Parent')
        parent.addProperty('App::PropertyFloat','Ratio')
        parent.Ratio=1.5
        s.setExpression('Constraints[4]','Parent.Ratio')
        d.recompute()
        parent.Ratio=value
        save(d)
    # The separated-endpoint fixture from refraction_workflows.fixture(prepared=False).
    d=App.newDocument('CreateMissing');s=d.addObject('Sketcher::SketchObject','Sketch')
    for start,end in [((-5,-4,0),(0,1,0)),((2,4,0),(7,9,0)),((-10,0,0),(10,0,0))]:
        s.addGeometry(Part.LineSegment(App.Vector(*start),App.Vector(*end)),False)
    for c in [Sketcher.Constraint('Distance',0,math.sqrt(50)),Sketcher.Constraint('Angle',0,math.pi/4),
              Sketcher.Constraint('DistanceX',0,1,-5.),Sketcher.Constraint('Distance',1,math.sqrt(50)),Sketcher.Constraint('Block',2)]:
        s.addConstraint(c)
    save(d)

def input_hashes(inputs):
    return {p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(inputs.glob('*.FCStd'))}

def verify_inputs(inputs, expected):
    actual=input_hashes(inputs)
    for name,digest in expected.items():
        assert actual.get(name)==digest,('Immutable input mismatch',name)
    required=set(PRESERVED)|{file for _,file,_,_ in cases()}
    assert all(name+'.FCStd' in actual for name in required),'Missing required refraction inputs'
    return actual

def run_native_oracle(runtime, inputs, oracle, generate=False):
    """AppRun gets one script argument; all oracle parameters live in that script."""
    wrapper=oracle.with_name('native-refraction-oracle.py')
    wrapper.write_text(
        'import importlib.util\nfrom pathlib import Path\n'
        +f'spec=importlib.util.spec_from_file_location("refraction_oracle",{str(Path(__file__).resolve())!r})\n'
        +'module=importlib.util.module_from_spec(spec)\nspec.loader.exec_module(module)\n'
        +f'inputs=Path({str(inputs)!r})\n'
        +('module.generate_inputs(inputs)\n' if generate else '')
        +f'module.native_oracle(inputs,Path({str(oracle)!r}))\n',encoding='utf-8')
    command=[runtime['executable'],*runtime.get('args',[]),str(wrapper)]
    oracle.with_name('native-command.json').write_text(json.dumps(command,indent=2)+'\n',encoding='utf-8')
    subprocess.run(command,check=True)

def native_oracle(inputs, output):
    import FreeCAD as App
    import Sketcher
    def state(d):
        s=d.Sketch
        return dict(constraints=[dict(type=c.Type,value=c.Value,driving=c.Driving,active=c.IsActive,name=c.Name) for c in s.Constraints],
                    rays=[dict(start=list(g.StartPoint),end=list(g.EndPoint)) for g in s.Geometry[:2]],
                    expressions=list(s.ExpressionEngine), dof=s.DoF,
                    parent_ratio=d.Parent.Ratio if d.getObject('Parent') else None)
    def valid(d):
        d.recompute()
        s=d.Sketch
        return s.solve()==0 and s.isValid() and not any([s.ConflictingConstraints,s.RedundantConstraints,s.MalformedConstraints])
    def apply(d,e):
        s=d.getObject(e['object']);op=e['op']
        if op=='delete_constraint':s.delConstraint(e['index'])
        elif op=='constraint':s.addConstraint(Sketcher.Constraint(e['kind'],*e['args']))
        elif op=='datum':s.setDatum(e['index'],App.Units.Quantity(e['value']))
        elif op=='expression':s.setExpression(e['property'],e['expression'])
        elif op=='set':setattr(s,e['property'],e['value'])
        elif op=='constraint_state':
            for k,f in [('driving',s.setDriving),('active',s.setActive),('name',s.renameConstraint)]:
                if k in e:f(e['index'],e[k])
        elif op=='move_geometry':s.moveGeometry(e['index'],e['point'],App.Vector(*e['position']),False)
        elif op=='refraction':
            first,p1,second,p2,boundary,_=e['args']
            # Pinned CommandConstraints.cpp:10254, Utils.cpp:366.
            point=s.getPoint(first,p1)
            if not s.isPointOnCurve(second,point.x,point.y):
                s.addConstraint(Sketcher.Constraint('Coincident',first,p1,second,p2))
                if s.solve()!=0:raise ValueError('native Coincident failed')
            point=s.getPoint(first,p1)
            if not s.isPointOnCurve(boundary,point.x,point.y):
                s.addConstraint(Sketcher.Constraint('PointOnObject',first,p1,boundary))
                if s.solve()!=0:raise ValueError('native PointOnObject failed')
            s.addConstraint(Sketcher.Constraint('SnellsLaw',first,p1,second,p2,boundary,App.Units.Quantity(e['value']).Value))
        else:raise AssertionError(op)
    report={}
    for name,file,edits,expected in [('open_'+n,n,[],True) for n in PRESERVED]+cases():
        d=App.openDocument(str(inputs/(file+'.FCStd')));d.UndoMode=1
        assert valid(d),file
        before=state(d)
        d.openTransaction(name)
        error=None
        try:
            for e in edits:apply(d,e)
            accepted=valid(d)
        except Exception as ex:
            accepted=False;error=str(ex)
        assert accepted==expected,(name,accepted,expected,error)
        if accepted:d.commitTransaction()
        else:d.abortTransaction();assert valid(d)
        report[name]={'accepted':accepted,'state':state(d),'before':before,'native_error':error}
        if not accepted:compare(report[name]['state'],before,name+' native rollback')
        App.closeDocument(d.Name)
    output.write_text(json.dumps(report,indent=2)+'\n')
    print('Native oracle:',len(report),'cases')

def compare(actual,expected,path='state'):
    if isinstance(expected,(int,float)) and not isinstance(expected,bool):
        assert math.isclose(actual,expected,rel_tol=1e-7,abs_tol=1e-6),(path,actual,expected)
    elif isinstance(expected,dict):
        for k,v in expected.items():compare(actual[k],v,path+'.'+k)
    elif isinstance(expected,list):
        assert len(actual)==len(expected),(path,actual,expected)
        for i,v in enumerate(expected):compare(actual[i],v,path+f'[{i}]')
    else:assert actual==expected,(path,actual,expected)

def run(a):
    from sketch_editor_workflows import SketchSession
    from native_workflows import objects,transaction
    runtime=json.loads(a.runtime.read_text())
    oracle=a.output/'native-oracle.json'
    generate=a.inputs is None
    if generate:
        assert a.input_manifest is None,'--input-manifest requires explicit --inputs'
        a.inputs=a.output/'inputs'
        assert not a.inputs.exists(),'Refuse to overwrite generated inputs; choose a fresh --output'
    else:
        supplied=json.loads(a.input_manifest.read_text()) if a.input_manifest else {}
        input_digests=verify_inputs(a.inputs,supplied)
    run_native_oracle(runtime,a.inputs,oracle,generate)
    if generate:input_digests=verify_inputs(a.inputs,{})
    else:verify_inputs(a.inputs,input_digests)
    (a.output/'input-manifest.json').write_text(json.dumps(input_digests,indent=2)+'\n',encoding='utf-8')
    expected=json.loads(oracle.read_text())
    class LoggedSession(SketchSession):
        def call(self,op,**kw):
            result=super().call(op,**kw)
            calls.append(dict(op=op,parameters=kw,result=result))
            return result
    def state(snapshot):
        s=objects(snapshot)['Sketch'];native=s['sketch']
        return dict(constraints=[{k:c[k] for k in ['type','value','driving','active','name']} for c in native['constraints']],
                    rays=[{k:g[k] for k in ['start','end']} for g in native['geometry'][:2]],
                    expressions=s['properties']['ExpressionEngine'],dof=native['degrees_of_freedom'],
                    parent_ratio=objects(snapshot).get('Parent',{}).get('properties',{}).get('Ratio'))
    def compile_datum(snapshot,index,value,expression=False):
        request=dict(op='datum',snapshot=objects(snapshot)['Sketch'],index=index,value=value,expression=expression)
        result=subprocess.run([str(a.compiler)],input=json.dumps(request)+'\n',text=True,capture_output=True,check=True)
        response=json.loads(result.stdout);assert response['ok'],response
        return response['edits']
    calls=[];report=[];session=LoggedSession(a.host,runtime)
    try:
        for name,file,edits,accepted in [('open_'+n,n,[],True) for n in PRESERVED]+cases():
            base=session.call('open',path=str(a.inputs/(file+'.FCStd')));doc=base['document']
            before=state(base)
            if edits:
                if accepted:result=transaction(session,doc,name,edits)
                else:
                    code='invalid_refraction' if name=='invalid_missing_create' else 'native_error' if expected[name]['native_error'] else 'invalid_model'
                    session.call('transaction',document=doc,edits=edits,expect_error=code)
                    result=session.call('inspect',document=doc)
                    compare(state(result),before,name+' worker rollback')
            else:result=base
            compare(state(result),expected[name]['state'],name)
            if accepted:
                path=a.output/(name+'.FCStd')
                session.call('save',document=doc,path=str(path))
                session.call('close',document=doc)
                result=session.call('open',path=str(path));doc=result['document']
                compare(state(result),expected[name]['state'],name+' reopened')
            session.call('close',document=doc)
            report.append(dict(case=name,native_agrees=True,accepted=accepted,rollback=not accepted,reopened=accepted))
        # Shared CLI clears a named expression atomically, preserving it on native failure.
        base=session.call('open',path=str(a.inputs/'positive_base.FCStd'));doc=base['document']
        base=transaction(session,doc,'Name ratio',[edit('constraint_state',index=4,name='IndexRatio')])
        base=transaction(session,doc,'Signed named expression',compile_datum(base,4,'-3 / 2',True))
        session.call('transaction',document=doc,edits=compile_datum(base,4,'0'),expect_error='native_error')
        restored=session.call('inspect',document=doc);compare(state(restored),state(base),'named expression rollback')
        changed=transaction(session,doc,'Replace expression with signed datum',compile_datum(restored,4,'-1.25'))
        assert not objects(changed)['Sketch']['properties']['ExpressionEngine']
        assert abs(objects(changed)['Sketch']['sketch']['constraints'][4]['value']+1.25)<1e-8
        session.call('undo',document=doc);restored=session.call('redo',document=doc)
        compare(state(restored),state(changed),'signed datum undo redo')
        session.call('close',document=doc)
        report.append(dict(case='named_expression_atomic_replacement',passed=True))
        # Signed creation and replacement failure inside the existing native sketch session.
        base=session.call('open',path=str(a.inputs/'CreateMissing.FCStd'));doc=base['document']
        initial=state(base)
        session.call('sketch_begin',document=doc,object='Sketch',token='signed-session')
        request=dict(op='constraint',snapshot=objects(base)['Sketch'],kind='SnellsLaw',selection=[dict(index=0,point=2),dict(index=1,point=1),dict(index=2,point=0)],value='-1.5')
        compiled=json.loads(subprocess.run([str(a.compiler)],input=json.dumps(request)+'\n',text=True,capture_output=True,check=True).stdout)
        assert compiled['ok'],compiled
        edits=compiled['edits']
        preview=session.call('preview',document=doc,token='signed-preview',edits=edits)
        assert preview['edits'][0]['index']==7 and preview['edits'][0]['preparation']==[5,6]
        accepted=state(preview['state'])
        bad=[{**edits[0],'value':'0'}]
        session.call('preview',document=doc,token='signed-preview',edits=bad,expect_error='invalid_refraction')
        restored=session.call('inspect',document=doc)
        compare(state(restored),accepted,'replacement preview rollback')
        restored=session.call('preview_cancel',document=doc,token='signed-preview')
        compare(state(restored),initial,'signed preview cancel')
        changed=transaction(session,doc,'Create signed ratio',edits)
        session.call('undo',document=doc);restored=session.call('redo',document=doc)
        compare(state(restored),state(changed),'signed sketch action undo redo')
        cancelled=session.call('sketch_cancel',document=doc,token='signed-session')
        compare(state(cancelled),initial,'whole sketch cancel')
        session.call('close',document=doc)
        report.append(dict(case='signed_preparation_session_preview_history_cancel',passed=True))
    finally:
        session.close()
        verify_inputs(a.inputs,input_digests)
        (a.output/'protocol.json').write_text(json.dumps(calls,indent=2)+'\n')
        (a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print('Native compatibility:',len(report),'cases passed')

if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--oracle',action='store_true')
    for name in ['host','compiler','runtime','inputs','input-manifest','output']:parser.add_argument('--'+name,type=Path)
    a=parser.parse_args()
    for name in (['inputs','output'] if a.oracle else ['host','compiler','runtime','output']):
        if getattr(a,name) is None:parser.error('--'+name+' is required')
    for name in ['host','compiler','runtime','inputs','input_manifest','output']:
        if getattr(a,name) is not None:setattr(a,name,getattr(a,name).resolve())
    if a.oracle:native_oracle(a.inputs,a.output)
    else:a.output.mkdir(parents=True,exist_ok=True);run(a)
