"""Pinned native projected-point serialization, selection, updates and history.

Uses actual Rust host/compiler and native FCStd files. All outputs belong in a
fresh disposable directory. No private .tools fixtures or helpers are imported.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess
from native_workflows import add, constraint, objects, rectangle, transaction, near, set_value
from sketch_editor_workflows import SketchSession


def point(obj,position):return dict(op='geometry',object=obj,kind='point',position=position)
def c(obj,kind,*args):return constraint(obj,kind,*args)
def placement(x,y,z=0,quarter_turn=False):return dict(position=[x,y,z],rotation=[0,0,math.sqrt(.5),math.sqrt(.5)] if quarter_turn else [0,0,0,1])

def compile(compiler,state,kind,selection):
    request=dict(op='constraint',snapshot=objects(state)['Sketch'],kind=kind,selection=[dict(index=i,point=p) for i,p in selection])
    result=subprocess.run([str(compiler)],input=json.dumps(request)+'\n',text=True,encoding='utf-8',capture_output=True,check=True)
    reply=json.loads(result.stdout);assert reply['ok'],reply
    return reply['edits']


def external(state,source,subelement='Vertex1'):
    entries=[entry for entry in objects(state)['Sketch']['sketch']['externals'] if entry['source']==dict(link=source,subelements=[subelement])]
    assert len(entries)==1,entries
    item=entries[0]
    assert item['geometry_index']==-3-item['index'] and item['geometry']['index']==item['geometry_index']
    assert item['persistent_reference'] and 'metadata_error' not in item
    assert item['projection'] is True and item['intersection'] is False
    assert item['missing'] is False and item['frozen'] is False
    return item


def position(value,expected):
    for x,y in zip(value,expected):near(x,y)


def sketch_state(state):
    obj=objects(state)['Sketch']
    return dict(sketch=obj['sketch'],expressions=obj['properties']['ExpressionEngine'])


def protected_save(session,doc,path):
    session.call('save',document=doc,path=str(path))
    return hashlib.sha256(path.read_bytes()).hexdigest()


def standalone(session,compiler,out):
    doc=session.call('new',name='ProjectedStandalonePoint')['document']
    transaction(session,doc,'Native standalone vertex and target point',[
        add('Sketcher::SketchObject','Master'),point('Master',[1,2,0]),c('Master','DistanceX',0,1,1.),c('Master','DistanceY',0,1,2.),
        add('Sketcher::SketchObject','Sketch'),point('Sketch',[0,0,0])])
    state=transaction(session,doc,'Project standalone native Vertex1',[dict(op='external_geometry',object='Sketch',source='Master',subelement='Vertex1')])
    entry=external(state,'Master');assert entry['geometry']['kind']=='point' and entry['source_index']==0
    assert entry['geometry']['handles'][0]['point']==1 and entry['geometry']['editable'] is False
    ref=entry['persistent_reference'];position(entry['geometry']['position'],[1,2,0])
    protected=out/'standalone-source.FCStd';digest=protected_save(session,doc,protected)
    state=transaction(session,doc,'Select real signed projected point',compile(compiler,state,'Coincident',[(0,1),(entry['geometry_index'],1)]))
    assert objects(state)['Sketch']['sketch']['constraints'][0]['second']==-3
    state=transaction(session,doc,'Transformed source update',[set_value('Master','Placement',placement(5,6,7,True))])
    position(external(state,'Master')['geometry']['position'],[3,7,0]);position(objects(state)['Sketch']['sketch']['geometry'][0]['position'],[3,7,0])
    state=transaction(session,doc,'Transformed target projection',[set_value('Sketch','Placement',placement(1,-1,2,True))])
    position(external(state,'Master')['geometry']['position'],[8,-2,0]);position(objects(state)['Sketch']['sketch']['geometry'][0]['position'],[8,-2,0])
    state=session.call('undo',document=doc);position(external(state,'Master')['geometry']['position'],[3,7,0])
    state=session.call('redo',document=doc);position(external(state,'Master')['geometry']['position'],[8,-2,0])
    state=transaction(session,doc,'Native source dimension update',[dict(op='datum',object='Master',index=0,value='4 mm')])
    position(external(state,'Master')['geometry']['position'],[11,-2,0]);position(objects(state)['Sketch']['sketch']['geometry'][0]['position'],[11,-2,0])
    state=session.call('undo',document=doc);position(external(state,'Master')['geometry']['position'],[8,-2,0])
    state=session.call('redo',document=doc);position(external(state,'Master')['geometry']['position'],[11,-2,0])
    assert external(state,'Master')['persistent_reference']==ref
    state=transaction(session,doc,'Native defining-point state',[dict(op='external_state',object='Sketch',index=0,defining=True,intersection=False)])
    assert external(state,'Master')['defining'] is True
    state=session.call('undo',document=doc);assert external(state,'Master')['defining'] is False
    state=session.call('redo',document=doc);assert external(state,'Master')['defining'] is True
    before=sketch_state(state)
    session.call('transaction',document=doc,edits=[dict(op='external_geometry',object='Sketch',source='Absent',subelement='Vertex1')],expect_error='invalid_geometry')
    state=session.call('inspect',document=doc);assert sketch_state(state)==before
    session.call('transaction',document=doc,edits=[c('Sketch','DistanceX',0,1,100.)],expect_error='invalid_model')
    state=session.call('inspect',document=doc);assert sketch_state(state)==before
    # Native Sketcher retains a missing-source cache and its constraints.
    state=transaction(session,doc,'Remove source vertex natively',[dict(op='delete_geometry',object='Master',index=0)])
    missing=objects(state)['Sketch']['sketch']['externals'][0]
    assert missing['missing'] is True and missing['persistent_reference']==ref and missing['source'] is None
    assert objects(state)['Sketch']['sketch']['constraints']==before['sketch']['constraints']
    state=session.call('undo',document=doc);assert sketch_state(state)==before
    state=session.call('redo',document=doc);assert objects(state)['Sketch']['sketch']['externals'][0]['missing'] is True
    state=session.call('undo',document=doc);assert sketch_state(state)==before
    # Reads must preserve the native cache, identities, metadata and revision.
    read=session.call('inspect',document=doc);assert read['revision']==state['revision'] and sketch_state(read)==before
    state=transaction(session,doc,'Delete projected source group',[dict(op='delete_external',object='Sketch',index=0)])
    assert not objects(state)['Sketch']['sketch']['externals'] and objects(state)['Sketch']['sketch']['constraint_count']==0
    state=session.call('undo',document=doc);assert external(state,'Master')['persistent_reference']==ref and objects(state)['Sketch']['sketch']['constraint_count']==1
    state=session.call('redo',document=doc);assert not objects(state)['Sketch']['sketch']['externals']
    state=session.call('undo',document=doc)
    result=out/'standalone-result.FCStd';session.call('save',document=doc,path=str(result));session.call('close',document=doc)
    state=session.call('open',path=str(result));doc=state['document'];position(external(state,'Master')['geometry']['position'],[11,-2,0]);assert external(state,'Master')['defining'] is True
    session.call('close',document=doc);assert hashlib.sha256(protected.read_bytes()).hexdigest()==digest
    return dict(case='StandalonePoint',passed=True,protected=str(protected),sha256=digest,result=str(result),selection_constraint=True,transformed_source_target=True,native_parent_dimension=True,defining_history=True,failed_edit_atomic=True,missing_source_history=True,delete_remaps_constraints=True,reopen=True)


def mixed(session,compiler,out):
    doc=session.call('new',name='MixedProjectedPointCache')['document']
    transaction(session,doc,'Mixed native projection sources',[
        add('Part::Box','Box'),set_value('Box','Length',10),set_value('Box','Width',8),set_value('Box','Height',2),
        add('Sketcher::SketchObject','PointA'),point('PointA',[2,3,0]),c('PointA','DistanceX',0,1,2.),c('PointA','DistanceY',0,1,3.),
        add('Sketcher::SketchObject','PointB'),point('PointB',[2,3,0]),c('PointB','DistanceX',0,1,2.),c('PointB','DistanceY',0,1,3.),
        add('Sketcher::SketchObject','Sketch'),point('Sketch',[0,0,0])])
    state=transaction(session,doc,'Face curves and identical-position point sources',[
        dict(op='external_geometry',object='Sketch',source='Box',subelement='Face6'),dict(op='external_geometry',object='Sketch',source='PointB',subelement='Vertex1'),
        dict(op='external_geometry',object='Sketch',source='Box',subelement='Edge1'),dict(op='external_geometry',object='Sketch',source='PointA',subelement='Vertex1')])
    a,b=external(state,'PointA'),external(state,'PointB')
    assert a['persistent_reference']!=b['persistent_reference'] and a['source_index']==3 and b['source_index']==1
    assert a['index']>a['source_index'] and b['index']>b['source_index']
    position(a['geometry']['position'],b['geometry']['position'])
    assert len([x for x in objects(state)['Sketch']['sketch']['externals'] if x['source']==dict(link='Box',subelements=['Face6'])])==4
    protected=out/'mixed-source.FCStd';digest=protected_save(session,doc,protected)
    state=transaction(session,doc,'Constrain to authoritative source A',compile(compiler,state,'Coincident',[(0,1),(a['geometry_index'],1)]))
    state=transaction(session,doc,'Target one flattened source among many cache entries',[dict(op='external_state',object='Sketch',index=a['index'],defining=True,intersection=False)])
    assert external(state,'PointA')['defining'] is True and external(state,'PointB')['defining'] is False
    state=transaction(session,doc,'Update only source A',[set_value('PointA','Placement',placement(10,0))])
    position(external(state,'PointA')['geometry']['position'],[12,3,0]);position(external(state,'PointB')['geometry']['position'],[2,3,0]);position(objects(state)['Sketch']['sketch']['geometry'][0]['position'],[12,3,0])
    state=transaction(session,doc,'Delete multi-curve face group',[dict(op='delete_external',object='Sketch',index=0)])
    a=external(state,'PointA');assert objects(state)['Sketch']['sketch']['constraints'][0]['second']==a['geometry_index']
    state=session.call('undo',document=doc);a=external(state,'PointA');assert objects(state)['Sketch']['sketch']['constraints'][0]['second']==a['geometry_index']
    state=session.call('redo',document=doc);a=external(state,'PointA');assert objects(state)['Sketch']['sketch']['constraints'][0]['second']==a['geometry_index']
    state=transaction(session,doc,'Delete different identical-position point source',[dict(op='delete_external',object='Sketch',index=external(state,'PointB')['index'])])
    a=external(state,'PointA');assert objects(state)['Sketch']['sketch']['constraints'][0]['second']==a['geometry_index']
    position(objects(state)['Sketch']['sketch']['geometry'][0]['position'],[12,3,0])
    result=out/'mixed-result.FCStd';session.call('save',document=doc,path=str(result));session.call('close',document=doc)
    state=session.call('open',path=str(result));external(state,'PointA');session.call('close',document=state['document']);assert hashlib.sha256(protected.read_bytes()).hexdigest()==digest
    return dict(case='MixedCache',passed=True,protected=str(protected),sha256=digest,result=str(result),mixed_face_edge_vertex=True,identical_coordinates_distinct_sources=True,flattened_source_mapping=True,source_update=True,group_delete_constraint_renumbering=True,undo_redo_reopen=True)


def via_point(session,compiler,out,kind):
    doc=session.call('new',name='ProjectedPoint'+kind)['document'];tangent=kind=='Tangent'
    transaction(session,doc,'Native external point and editable curves',[
        add('Sketcher::SketchObject','Master'),point('Master',[0,0,0]),c('Master','DistanceX',0,1,0.),c('Master','DistanceY',0,1,0.),
        add('Sketcher::SketchObject','Sketch'),dict(op='geometry',object='Sketch',kind='circle',center=[0,5,0],radius=5),
        dict(op='geometry',object='Sketch',kind='line',start=[-5,0,0] if tangent else [0,-5,0],end=[5,0,0] if tangent else [0,5,0]),
        c('Sketch','Radius',0,5.),c('Sketch','DistanceX',0,3,0.),c('Sketch','Distance',1,10.),c('Sketch','DistanceX' if tangent else 'DistanceY',1,1,-5.)])
    state=transaction(session,doc,'Project and constrain to native standalone point',[dict(op='external_geometry',object='Sketch',source='Master',subelement='Vertex1'),c('Sketch','PointOnObject',-3,1,0),c('Sketch','PointOnObject',-3,1,1)])
    protected=out/(kind+'-source.FCStd');digest=protected_save(session,doc,protected);before=sketch_state(state)
    edits=compile(compiler,state,kind,[(-3,1),(0,0),(1,0)])
    session.call('sketch_begin',document=doc,object='Sketch',token='point-via')
    state=session.call('preview',document=doc,token='via',edits=edits)['state'];assert objects(state)['Sketch']['sketch']['constraints'][6]['third']==-3
    state=session.call('preview_cancel',document=doc,token='via');assert sketch_state(state)==before
    state=transaction(session,doc,'Native via-point action',edits)
    state=session.call('undo',document=doc);assert sketch_state(state)==before
    state=session.call('redo',document=doc);native=objects(state)['Sketch']['sketch']['constraints'][6];assert native['type']==kind and native['third']==-3 and native['third_pos']==1
    state=session.call('sketch_finish',document=doc,token='point-via')
    state=transaction(session,doc,'Move transformed native point parent',[set_value('Master','Placement',placement(1,1,5,True))]);position(external(state,'Master')['geometry']['position'],[1,1,0])
    state=session.call('undo',document=doc);position(external(state,'Master')['geometry']['position'],[0,0,0])
    state=session.call('redo',document=doc);position(external(state,'Master')['geometry']['position'],[1,1,0])
    result=out/(kind+'-result.FCStd');session.call('save',document=doc,path=str(result));session.call('close',document=doc)
    state=session.call('open',path=str(result));native=objects(state)['Sketch']['sketch']['constraints'][6];assert native['type']==kind and native['third']==-3;external(state,'Master');session.call('close',document=state['document'])
    assert hashlib.sha256(protected.read_bytes()).hexdigest()==digest
    return dict(case='ProjectedPoint'+kind,passed=True,protected=str(protected),sha256=digest,result=str(result),shared_compiler_selection=True,native_third=-3,preview_cancel=True,action_undo_redo=True,finish=True,parent_update_history=True,reopen=True)


def independent_copy(session,out,defining):
    case='IndependentDefiningPoint' if defining else 'IndependentProjectedPoint'
    doc=session.call('new',name=case)['document']
    state=transaction(session,doc,'Native profile containing an external standalone point',[
        add('Sketcher::SketchObject','Master'),point('Master',[15,20,0]),c('Master','DistanceX',0,1,15.),c('Master','DistanceY',0,1,20.),
        add('Sketcher::SketchObject','Sketch'),*rectangle('Sketch',20,10),add('PartDesign::Body','TargetBody')])
    state=transaction(session,doc,'Project native vertex after source recompute',[
        dict(op='external_geometry',object='Sketch',source='Master',subelement='Vertex1',defining=defining),
        c('Sketch','DistanceX',-3,1,15.),dict(op='constraint_state',object='Sketch',index=11,driving=False),
        dict(op='expression',object='Sketch',property='Constraints[10]',expression='Constraints[9] * 0.5')])
    assert external(state,'Master')['defining'] is defining
    before=sketch_state(state);protected=out/(case+'-source.FCStd');digest=protected_save(session,doc,protected)
    edit=dict(op='reference_copy',source='Sketch',name='CopySource',body='TargetBody',mode='independent')
    state=transaction(session,doc,'Existing independent profile copy with external point',[edit])
    assert sketch_state(state)==before
    copy=objects(state)['CopySource']['sketch']
    assert copy['geometry_count']==4+int(defining) and copy['constraint_count']==11 and not copy['externals']
    if defining:
        assert copy['geometry'][4]['kind']=='point' and not copy['geometry'][4]['construction']
        position(copy['geometry'][4]['position'],[15,20,0])
    assert objects(state)['CopySource']['shape']==objects(state)['Sketch']['shape']
    state=session.call('undo',document=doc);assert 'CopySource' not in objects(state) and sketch_state(state)==before
    state=session.call('redo',document=doc);assert objects(state)['CopySource']['sketch']==copy and sketch_state(state)==before
    state=transaction(session,doc,'Source stays dependent; copy is independent',[set_value('Master','Placement',placement(3,4))])
    position(external(state,'Master')['geometry']['position'],[18,24,0]);assert objects(state)['CopySource']['sketch']==copy
    result=out/(case+'-result.FCStd');session.call('save',document=doc,path=str(result));session.call('close',document=doc)
    state=session.call('open',path=str(result));assert objects(state)['CopySource']['sketch']==copy and external(state,'Master')['defining'] is defining
    session.call('close',document=state['document']);assert hashlib.sha256(protected.read_bytes()).hexdigest()==digest
    return dict(case=case,passed=True,protected=str(protected),sha256=digest,result=str(result),source_preserved=True,defining_materialized=defining,external_constraints_removed=True,copy_history=True,copy_independent=True,reopen=True)


def main():
    p=argparse.ArgumentParser();p.add_argument('--host',type=Path,required=True);p.add_argument('--compiler',type=Path,required=True);p.add_argument('--runtime',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False);s=SketchSession(a.host,json.loads(a.runtime.read_text()));report=[]
    try:
        for fn in [standalone,mixed]:report.append(fn(s,a.compiler,a.output));print(report[-1]['case'],'passed',flush=True)
        for kind in ['Tangent','Perpendicular']:report.append(via_point(s,a.compiler,a.output,kind));print(report[-1]['case'],'passed',flush=True)
        for defining in [False,True]:report.append(independent_copy(s,a.output,defining));print(report[-1]['case'],'passed',flush=True)
        (a.output/'report.json').write_bytes((json.dumps(report,indent=2)+'\n').encode())
    finally:s.close()

if __name__=='__main__':main()
