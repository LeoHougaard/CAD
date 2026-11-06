"""Shared CLI builder and guarded persistent host, with real native state."""
import json
import os
from pathlib import Path
import subprocess
import sys

import argparse
p=argparse.ArgumentParser()
for key in ['host','compiler','runtime','output']:p.add_argument('--'+key,type=Path,required=True)
a=p.parse_args();out=a.output;out.mkdir(parents=True,exist_ok=False)
source=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(source/'tests'))
from sketch_editor_workflows import SketchSession
from native_workflows import objects,transaction

runtime=json.loads(a.runtime.read_text())
session=SketchSession(a.host.resolve(),runtime)
report={'compiled':[],'rejected':[]}
try:
    doc=session.call('new',name='CopyCli')['document']
    state=transaction(session,doc,'Source',[
        dict(op='add_object',type='Sketcher::SketchObject',name='Sketch'),
        dict(op='geometry',object='Sketch',kind='circle',center=[5,5,0],radius=2),
        dict(op='constraint',object='Sketch',kind='Radius',args=[0,2]),
        dict(op='constraint_state',object='Sketch',index=0,name='SourceRadius'),
        dict(op='expression',object='Sketch',property='Constraints.SourceRadius',expression='2 mm')])
    session.call('sketch_begin',document=doc,object='Sketch',token='copy-cli')
    for kind in ('copy','clone','move','rectangular_array'):
        baseline=session.call('inspect',document=doc); snapshot=objects(baseline)['Sketch']
        params=dict(displacement=['1 in','-2 cm',0],all_geometry=False)
        if kind=='rectangular_array':
            params.update(columns=3,rows=2,clone=True,constrain_displacement=False,perpendicular_scale=0.5)
        request=dict(op='edit',snapshot=snapshot,kind=kind,selection=[dict(index=0,point=0)],parameters=params)
        compiled=subprocess.run([str(a.compiler.resolve())],input=json.dumps(request)+'\n',
            text=True,capture_output=True,check=True,creationflags=getattr(subprocess,"BELOW_NORMAL_PRIORITY_CLASS",0))
        response=json.loads(compiled.stdout); assert response['ok'],response
        edits=response['edits']; report['compiled'].append(dict(request=request,response=response))
        error=session.call('transaction',document=doc,edits=edits,expected_revision=baseline['revision']-1,
            expect_error='stale_revision')
        assert objects(session.call('inspect',document=doc))['Sketch']==snapshot
        report['rejected'].append(dict(kind=kind,error=error,preserved=True))
        state=transaction(session,doc,kind,edits)
        count=objects(state)['Sketch']['sketch']['geometry_count']
        assert count==({'move':1,'rectangular_array':6}.get(kind,2))
        if kind in ('copy','clone','rectangular_array'):
            generated=objects(state)['Sketch']['sketch']['geometry'][1]
            assert abs(generated['center'][0]-30.4)<1e-10
            assert abs(generated['center'][1]+15)<1e-10
        if kind=='clone':
            state=transaction(session,doc,'Native source expression update',[
                dict(op='expression',object='Sketch',property='Constraints.SourceRadius',expression='3 mm')])
            assert all(g['radius']==3 for g in objects(state)['Sketch']['sketch']['geometry'])
            session.call('undo',document=doc)
        session.call('undo',document=doc)
        after=objects(session.call('inspect',document=doc))['Sketch']
        (out/(kind+'-undo.json')).write_text(json.dumps(dict(before=snapshot,after=after),indent=2))
        # Native session Undo intentionally leaves the object Touched until
        # whole-document Finish; compare all model facts, retaining raw status.
        assert {k:v for k,v in after.items() if k not in ('state','status')}=={
            k:v for k,v in snapshot.items() if k not in ('state','status')}
        session.call('redo',document=doc)
        session.call('undo',document=doc)
    session.call('sketch_cancel',document=doc,token='copy-cli')
    report['final']=session.call('inspect',document=doc)
finally:
    session.close()
(out/'report.json').write_text(json.dumps(report,indent=2))
print(json.dumps({'compiled_native_operations':4,'stale_rejections_preserved':4,'live_clone_expression':True}))
