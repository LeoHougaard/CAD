"""Real pinned-native feature/property workflows through the Rust worker host."""
import argparse
import json
import math
import subprocess
from pathlib import Path
from native_workflows import Session, add, set_value as put, link, transaction, objects, solid, near
COMPILER = None
EDIT_VERIFICATIONS = []


def compiled(flag, parameters):
    result = subprocess.run([str(COMPILER),flag,'-'],input=json.dumps(parameters),text=True,
                            capture_output=True,check=True)
    return json.loads(result.stdout)


def placement(x=0,y=0,z=0,rotation=None):
    return dict(position=[x,y,z],rotation=rotation or [0,0,0,1])


def rect(obj,x,y,w,h):
    p=[[x,y,0],[x+w,y,0],[x+w,y+h,0],[x,y+h,0]]
    return [dict(op='geometry',object=obj,kind='line',start=p[i],end=p[(i+1)%4]) for i in range(4)]


def circle(obj,r):
    return dict(op='geometry',object=obj,kind='circle',center=[0,0,0],radius=r)


def same_geometry(actual, expected):
    if isinstance(expected, dict):
        assert actual.keys()==expected.keys()
        for key,value in expected.items():same_geometry(actual[key],value)
    elif isinstance(expected, list):
        assert len(actual)==len(expected)
        for left,right in zip(actual,expected):same_geometry(left,right)
    elif isinstance(expected, (int,float)) and not isinstance(expected,bool):near(actual,expected)
    else:assert actual==expected


def feature(obj,type_id,**props):
    return compiled('--native-properties',dict(type='PartDesign::'+type_id,name=obj,body='Body',
                                               create=True,properties=props,expressions={}))


def begin(s,name,base=None):
    d=s.call('new',name=name)['document']
    edits=[add('PartDesign::Body','Body')]
    if base:
        kind,props=base
        edits+=feature('Base',kind,**props)
    transaction(s,d,'Start',edits)
    return d


def finish(s,d,obj,out,expected=None):
    state=s.call('inspect',document=d)
    assert objects(state)['Body']['body_state']['tip']==obj
    assert objects(state)['Body']['body_state']['display_object']==obj
    shape=objects(state)[obj]['shape']
    assert shape['valid'] and shape['solids']>=1,shape
    if expected is not None: near(shape['volume'],expected)
    path=str((out/(d+'.FCStd')).resolve())
    s.call('save',document=d,path=path)
    s.call('close',document=d)
    state=s.call('open',path=path)
    near(objects(state)[obj]['shape']['volume'],shape['volume'])
    doc=state['document']
    native=objects(state)[obj]
    kind=native['type'].removeprefix('PartDesign::')
    parameters={'Revolution':('Angle',270),'Groove':('Angle',180),'Chamfer':('Size',1.5),
                'Thickness':('Value',1.5),'Draft':('Angle',6),'LinearPattern':('Occurrences',2),
                'PolarPattern':('Occurrences',3),'Scaled':('Factor',1.6),'Hole':('Diameter',5)}
    primitive_parameters={'Box':('Length',15),'Cylinder':('Radius',11),'Sphere':('Radius',6),
                'Cone':('Radius1',3),'Ellipsoid':('Radius1',3),'Torus':('Radius2',3),
                'Prism':('Circumradius',3),'Wedge':('X2max',9),'Helix':('Pitch',4)}
    if kind in parameters:
        prop,value=parameters[kind];edited_object=obj;edits=[put(obj,prop,value)]
    elif kind in ['AdditiveLoft','SubtractiveLoft']:
        edited_object='Section';prop=None;value=None
        edits=[dict(op='move_geometry',object='Section',index=0,point=0,position=[5,0,0],relative=False)]
    elif kind in ['AdditivePipe','SubtractivePipe']:
        edited_object='Path';prop=None;value=None
        edits=[dict(op='move_geometry',object='Path',index=0,point=2,position=[0,25,0],relative=False)]
    elif kind=='MultiTransform':
        edited_object='Linear';prop='Occurrences';value=3;edits=[put(edited_object,prop,value)]
    elif kind=='Mirrored':
        edited_object='Protrusion';prop='Height';value=5;edits=[put(edited_object,prop,value)]
    else:
        simple=kind.removeprefix('Additive').removeprefix('Subtractive')
        prop,value=primitive_parameters[simple];edited_object=obj;edits=[put(obj,prop,value)]
    if prop:
        edits=compiled('--native-properties',dict(type=objects(state)[edited_object]['type'],
            name=edited_object,body='Body',tip=obj,tip_visibility=True,create=False,properties={prop:value},expressions={}))
    changed=transaction(s,doc,'Edit reopened native parameters',edits)
    assert objects(changed)['Body']['body_state']['tip']==obj
    assert objects(changed)['Body']['body_state']['display_object']==obj
    changed_shape=objects(changed)[obj]['shape']
    assert changed_shape['valid']
    assert not math.isclose(changed_shape['volume'],shape['volume'],rel_tol=1e-7), (kind,shape,changed_shape)
    if prop:near(objects(changed)[edited_object]['properties'][prop],value)
    near(objects(s.call('undo',document=doc))[obj]['shape']['volume'],shape['volume'])
    redone=s.call('redo',document=doc)
    assert objects(redone)['Body']['body_state']['tip']==obj
    assert objects(redone)['Body']['body_state']['display_object']==obj
    near(objects(redone)[obj]['shape']['volume'],changed_shape['volume'])
    edited_geometry=objects(redone)[edited_object].get('sketch')
    edited_path=str((out/(d+'-edited.FCStd')).resolve())
    s.call('save',document=doc,path=edited_path)
    s.call('close',document=doc)
    restored=s.call('open',path=edited_path)
    assert objects(restored)['Body']['body_state']['tip']==obj
    assert objects(restored)['Body']['body_state']['display_object']==obj
    near(objects(restored)[obj]['shape']['volume'],changed_shape['volume'])
    if prop:near(objects(restored)[edited_object]['properties'][prop],value)
    else:same_geometry(objects(restored)[edited_object]['sketch']['geometry'],edited_geometry['geometry'])
    s.call('close',document=restored['document'])
    EDIT_VERIFICATIONS.append({'document':d,'type':native['type'],'edited_object':edited_object,
                               'property':prop or 'native sketch geometry','baseline_volume':shape['volume'],
                               'edited_volume':changed_shape['volume'],'undo_redo':True,'second_reopen':True})
    return shape['volume']


def workflows(s,out):
    results={}
    d=begin(s,'Revolution')
    transaction(s,d,'Revolve',[add('Sketcher::SketchObject','Profile','Body'),
        put('Profile','Placement',placement(rotation=[math.sqrt(.5),0,0,math.sqrt(.5)])),
        *rect('Profile',2,0,2,10),*feature('Result','Revolution',Profile=link('Profile'),ReferenceAxis=link('Profile','V_Axis'))])
    results['revolution']=finish(s,d,'Result',out,120*math.pi)
    d=begin(s,'Groove',('AdditiveCylinder',dict(Radius=5,Height=10)))
    transaction(s,d,'Groove',[add('Sketcher::SketchObject','Profile','Body'),
        put('Profile','Placement',placement(rotation=[math.sqrt(.5),0,0,math.sqrt(.5)])),
        *rect('Profile',3,2,3,6),*feature('Result','Groove',Profile=link('Profile'),ReferenceAxis=link('Profile','V_Axis'))])
    results['groove']=finish(s,d,'Result',out,(250-96)*math.pi)
    d=begin(s,'Loft')
    transaction(s,d,'Loft',[add('Sketcher::SketchObject','Profile','Body'),circle('Profile',2),
        add('Sketcher::SketchObject','Section','Body'),put('Section','Placement',placement(z=20)),circle('Section',4),
        *feature('Result','AdditiveLoft',Profile=link('Profile'),Sections=[link('Section')])])
    results['loft']=finish(s,d,'Result',out,math.pi*20*28/3)
    d=begin(s,'Pipe')
    transaction(s,d,'Sweep',[add('Sketcher::SketchObject','Profile','Body'),circle('Profile',2),
        add('Sketcher::SketchObject','Path','Body'),put('Path','Placement',placement(rotation=[math.sqrt(.5),0,0,math.sqrt(.5)])),
        dict(op='geometry',object='Path',kind='line',start=[0,0,0],end=[0,20,0]),
        *feature('Result','AdditivePipe',Profile=link('Profile'),Spine=link('Path','Edge1'))])
    results['pipe']=finish(s,d,'Result',out,80*math.pi)
    for kind in ['Chamfer','Thickness','Draft']:
        d=begin(s,kind,('AdditiveBox',dict(Length=20,Width=20,Height=10)))
        top=s.call('topology',document=d,object='Base')
        topface=max(top['faces'],key=lambda f:f['center'][2])['subelement']
        side=min(top['faces'],key=lambda f:f['center'][0])['subelement']
        props=dict(Base=link('Base','Edge1'),Size=1) if kind=='Chamfer' else dict(Base=link('Base',topface),Value=1) if kind=='Thickness' else dict(Base=link('Base',side),NeutralPlane=link('Base',topface),Angle=5)
        transaction(s,d,kind,feature('Result',kind,**props))
        results[kind.lower()]=finish(s,d,'Result',out,1084 if kind=='Thickness' else None)
    for kind in ['LinearPattern','PolarPattern','Mirrored','Scaled']:
        d=begin(s,kind,('AdditiveBox',dict(Length=20,Width=20,Height=2,Placement=placement(-10,-10))))
        transaction(s,d,'Protrusion',feature('Protrusion','AdditiveBox',Length=2,Width=2,Height=4,Placement=placement(2,2,2)))
        props=dict(Originals=[link('Protrusion')])
        if kind=='LinearPattern': props.update(Direction=link('X_Axis',''),Length=4,Occurrences=3)
        elif kind=='PolarPattern': props.update(Axis=link('Z_Axis',''),Occurrences=4)
        elif kind=='Mirrored': props.update(MirrorPlane=link('YZ_Plane',''))
        else: props.update(Factor=1.5,Occurrences=2)
        transaction(s,d,kind,feature('Result',kind,**props))
        results[kind.lower()]=finish(s,d,'Result',out,848 if kind=='LinearPattern' else 864 if kind=='PolarPattern' else 832 if kind=='Mirrored' else None)
    d=begin(s,'Hole',('AdditiveBox',dict(Length=20,Width=20,Height=10)))
    transaction(s,d,'Hole',[add('Sketcher::SketchObject','Profile','Body'),put('Profile','Placement',placement(10,10,10)),circle('Profile',2),*feature('Result','Hole',Profile=link('Profile'),Diameter=4,DepthType='ThroughAll')])
    results['hole']=finish(s,d,'Result',out,4000-40*math.pi)
    d=begin(s,'MultiTransform',('AdditiveBox',dict(Length=20,Width=20,Height=2,Placement=placement(-10,-10))))
    transaction(s,d,'Protrusion',feature('Protrusion','AdditiveBox',Length=2,Width=2,Height=4,Placement=placement(2,2,2)))
    transaction(s,d,'Multi transform',[
        *feature('Result','MultiTransform',Originals=[link('Protrusion')]),
        *feature('Linear','LinearPattern',Direction=link('X_Axis',''),Length=4,Occurrences=2),
        *compiled('--native-properties',dict(type='PartDesign::MultiTransform',name='Result',body='Body',
            tip='Result',tip_visibility=True,create=False,properties={'Transformations':[link('Linear')]},expressions={}))])
    results['multitransform']=finish(s,d,'Result',out,832)
    d=begin(s,'DatumAndBinders',('AdditiveBox',dict(Length=20,Width=20,Height=10)))
    state=transaction(s,d,'Native reference geometry',[
        *feature('Datum','Plane',AttachmentSupport=[link('Base','Face6')],MapMode='FlatFace'),
        *feature('Line','Line'),*feature('Point','Point'),*feature('CS','CoordinateSystem'),
        *feature('Binder','ShapeBinder',Support=[link('Base','Face6')]),
        *feature('SubBinder','SubShapeBinder',Support=[link('Base','Face6')])])
    near(objects(state)['Datum']['properties']['Placement']['position'][2],10)
    assert objects(state)['SubBinder']['properties']['Support']==[link('Base','Face6')]
    state=transaction(s,d,'Hide native Body',[put('Body','Visibility',False)])
    assert not objects(state)['Datum']['reference_display']['visible']
    transaction(s,d,'Show native Body',[put('Body','Visibility',True)])
    state=transaction(s,d,'Hide native Origin',[put('Origin','Visibility',False)])
    assert not objects(state)['X_Axis']['reference_display']['visible']
    transaction(s,d,'Show native Origin',[put('Origin','Visibility',True)])
    transaction(s,d,'Clear native attachment',[put('Datum','MapMode','Deactivated'),put('Datum','AttachmentSupport',None)])
    assert objects(s.call('inspect',document=d))['Datum']['properties']['AttachmentSupport']==[]
    results['references']=finish(s,d,'Base',out,4000)
    return results


def primitive_workflows(s,out):
    results={}
    for kind in ['Box','Cylinder','Sphere','Cone','Ellipsoid','Torus','Prism','Wedge']:
        d=begin(s,'Additive'+kind)
        state=transaction(s,d,'Add native solid',feature('Result','Additive'+kind))
        volume=solid(state,'Result')['volume']
        assert volume>0
        analytical={'Box':1000,'Cylinder':1000*math.pi,'Sphere':500*math.pi/3,
                    'Cone':280*math.pi/3,'Torus':80*math.pi**2,
                    'Prism':60*math.sqrt(3),'Wedge':1960/3}
        if kind in analytical:near(volume,analytical[kind])
        finish(s,d,'Result',out,volume)
        d=begin(s,'Subtractive'+kind,('AdditiveBox',dict(Length=100,Width=100,Height=100,Placement=placement(-50,-50,-50))))
        state=transaction(s,d,'Subtract native solid',feature('Result','Subtractive'+kind))
        near(solid(state,'Result')['volume'],1_000_000-volume)
        finish(s,d,'Result',out,1_000_000-volume)
        results[kind]={'additive_volume':volume,'subtractive_volume':1_000_000-volume}
    for operation in ['Additive','Subtractive']:
        d=begin(s,operation+'Helix',('AdditiveBox',dict(Length=100,Width=100,Height=100,Placement=placement(-50,-50,-50))) if operation=='Subtractive' else None)
        state=transaction(s,d,'Native helix',[add('Sketcher::SketchObject','Profile','Body'),
            put('Profile','Placement',placement(rotation=[math.sqrt(.5),0,0,math.sqrt(.5)])),
            *rect('Profile',5,0,1,1),*feature('Result',operation+'Helix',Profile=link('Profile'),
                ReferenceAxis=link('Profile','V_Axis'),Pitch=3,Height=9)])
        volume=solid(state,'Result')['volume']
        if operation=='Additive':helix_volume=volume
        else:near(volume,1_000_000-helix_volume)
        finish(s,d,'Result',out,volume)
        results[operation+'Helix']=volume
    return results


def metadata_and_atomicity(s,out):
    schemas={t:s.call('feature_schema',type=t) for t in s.capabilities['object_types']}
    d=begin(s,'AtomicFeature',('AdditiveBox',dict(Length=20,Width=20,Height=10)))
    seed_state=s.call('inspect',document=d)
    catalog=json.loads(subprocess.check_output([str(COMPILER),'--feature-catalog'],text=True))
    for descriptor in catalog:
        schema=schemas[descriptor['type_id']]
        assert all(p in schema['property_types'] for p in descriptor['preferred_properties']),descriptor
        seed=compiled('--feature-seed',dict(type=descriptor['type_id'],state=seed_state,body='Body',
                    name='Candidate',selected_object=None,selections=[]))
        assert all(p in schema['property_types'] for p in seed),(descriptor,seed)
    assert len(schemas)>=59
    pad=schemas['PartDesign::Pad']
    assert 'Length' in pad['properties'] and pad['property_types']['Length']=='App::PropertyLength'
    assert pad['property_metadata']['Shape']['read_only']
    assert pad['property_metadata']['_Body']['read_only']
    assert pad['property_metadata']['Length']['documentation']
    assert pad['properties']['Type'] in pad['property_metadata']['Type']['enumerations']
    s.call('feature_schema',type='Unsupported::Feature',expect_error='unsupported_object_type')
    state=transaction(s,d,'Expression',[dict(op='expression',object='Base',property='Height',expression='5 mm + 5 mm')])
    assert objects(state)['Base']['expressions']['Height']=='5 mm + 5 mm'
    rev=state['revision']; count=state['undo_count']
    token='native-feature-preview'
    preview=s.call('preview',document=d,expected_revision=rev,token=token,edits=feature('Result','Chamfer',Base=link('Base','Edge1'),Size=1))['state']
    schema=s.call('feature_schema',type='PartDesign::Thickness')
    assert schema['type']=='PartDesign::Thickness'
    unchanged=s.call('inspect',document=d)
    assert unchanged['revision']==preview['revision'] and unchanged['undo_count']==preview['undo_count']
    s.call('preview',document=d,expected_revision=unchanged['revision'],token=token,
           edits=feature('Result','Chamfer',Base=link('Base','Edge1'),Size=100),expect_error='invalid_model')
    unchanged=s.call('inspect',document=d)
    assert objects(unchanged)['Body']['body_state']['tip']=='Result'
    assert objects(unchanged)['Body']['body_state']['display_object']=='Result'
    assert objects(unchanged)['Result']['properties']['Visibility']
    state=s.call('preview_cancel',document=d,expected_revision=unchanged['revision'],token=token)
    assert 'Result' not in objects(state) and state['undo_count']==count
    assert objects(state)['Body']['body_state']['tip']=='Base'
    assert objects(state)['Base']['properties']['Visibility']
    assert objects(state)['Body']['body_state']['display_object']=='Base'
    near(solid(state,'Base')['volume'],4000)
    for prop,value in [('Shape',None),('_Body',None),('_ElementMapVersion','corrupted')]:
        s.call('transaction',document=d,label='Denied computed write',edits=[put('Base',prop,value)],expect_error='read_only_property')
        state=s.call('inspect',document=d)
        assert state['available'];near(solid(state,'Base')['volume'],4000)
    s.call('transaction',document=d,label='Invalid feature',edits=feature('Result','Chamfer',Base=link('Base','Edge1'),Size=100),expect_error='invalid_model')
    state=s.call('inspect',document=d)
    assert state['available'] and 'Result' not in objects(state)
    assert objects(state)['Body']['body_state']['tip']=='Base'
    assert objects(state)['Base']['properties']['Visibility']
    assert objects(state)['Body']['body_state']['display_object']=='Base'
    near(solid(state,'Base')['volume'],4000)
    state=transaction(s,d,'Chamfer',feature('Result','Chamfer',Base=link('Base','Edge1'),Size=1))
    baseline_views={obj['name']:obj['properties'].get('Visibility') for obj in state['objects']}
    s.call('transaction',document=d,label='Failed delete batch',
           edits=[dict(op='remove_object',object='Result'),put('Base','UnknownProperty',1)],
           expect_error='unsupported_property')
    state=s.call('inspect',document=d)
    assert objects(state)['Body']['body_state']['tip']=='Result'
    assert objects(state)['Body']['body_state']['display_object']=='Result'
    assert {obj['name']:obj['properties'].get('Visibility') for obj in state['objects']}==baseline_views
    s.call('transaction',document=d,label='Protect dependency',edits=[dict(op='remove_object',object='Base')],expect_error='dependent_objects')
    s.call('inspect',document=d)
    state=transaction(s,d,'Delete native leaf',[dict(op='remove_object',object='Result')])
    assert 'Result' not in objects(state)
    assert objects(state)['Body']['body_state']['tip']=='Base'
    assert objects(state)['Body']['body_state']['display_object']=='Base'
    state=s.call('undo',document=d);assert 'Result' in objects(state)
    assert objects(state)['Body']['body_state']['display_object']=='Result'
    state=s.call('redo',document=d);assert 'Result' not in objects(state)
    assert objects(state)['Body']['body_state']['display_object']=='Base'
    topo=s.call('topology',document=d,object='Base')
    assert [v['subelement'] for v in topo['vertices']]==['Vertex'+str(i) for i in range(1,9)]
    assert all(len(v['point'])==3 and all(math.isfinite(x) for x in v['point']) for v in topo['vertices'])
    transaction(s,d,'Clear expression',[dict(op='expression',object='Base',property='Height',expression=None)])
    assert not objects(s.call('inspect',document=d))['Base']['expressions']
    finish(s,d,'Base',out,4000)
    return {'schema_count':len(schemas),'rust_catalog_count':len(catalog),'native_vertices':len(topo['vertices'])}


def conditional_hole_workflow(s,out):
    state=s.call('open',path=str((out/'Hole.FCStd').resolve()))
    doc=state['document']
    baseline=solid(state,'Result')['volume']
    properties=dict(ThreadType='ISOMetricProfile',Threaded=True,ThreadSize='M1.8x0.35',ThreadClass='6H',
                    DepthType='Dimension',ThreadDepthType='Dimension',HoleCutType='Counterbore',
                    HoleCutCustomValues=True,DrillPoint='Angled',Depth=8,ThreadDepth=8,
                    HoleCutDiameter=4,HoleCutDepth=1.5,DrillPointAngle=118)
    edits=compiled('--native-properties',dict(type='PartDesign::Hole',name='Result',body='Body',tip='Result',tip_visibility=True,create=False,
                                             properties=properties,expressions={}))
    changed=transaction(s,doc,'Threaded counterbore controls',edits)
    volume=solid(changed,'Result')['volume']
    assert not math.isclose(volume,baseline,rel_tol=1e-8)
    for key,value in properties.items():
        actual=objects(changed)['Result']['properties'][key]
        if isinstance(value,(int,float)) and not isinstance(value,bool):near(actual,value)
        else:assert actual==value,(key,actual,value)
    near(solid(s.call('undo',document=doc),'Result')['volume'],baseline)
    near(solid(s.call('redo',document=doc),'Result')['volume'],volume)
    path=str((out/'Hole-threaded-counterbore.FCStd').resolve())
    s.call('save',document=doc,path=path)
    s.call('close',document=doc)
    restored=s.call('open',path=path)
    near(solid(restored,'Result')['volume'],volume)
    for key,value in properties.items():
        assert objects(restored)['Result']['properties'][key]==value,(key,value)
    s.call('close',document=restored['document'])
    return {'volume':volume,'compiled_fields':len(properties),'undo_redo_second_reopen':True}


def container_visibility(s,out):
    doc=s.call('new',name='ContainerVisibility')['document']
    state=transaction(s,doc,'Native container contents',[
        add('App::Part','Assembly'),add('PartDesign::Body','Body'),*feature('Solid','AdditiveBox'),
        add('Part::Box','LooseSolid'),add('Sketcher::SketchObject','LooseSketch'),
        dict(op='geometry',object='LooseSketch',kind='line',start=[0,0,0],end=[10,0,0]),
        put('Assembly','Group',[link('Body'),link('LooseSolid'),link('LooseSketch')])])
    assert objects(state)['Body']['body_state']['display_object']=='Solid'
    state=transaction(s,doc,'Hide native App Part',[put('Assembly','Visibility',False)])
    for key in ['Body','Solid','LooseSolid','LooseSketch','X_Axis']:
        assert not objects(state)[key]['effective_visibility'],key
    assert not objects(state)['X_Axis']['reference_display']['visible']
    assert objects(state)['Body']['body_state']['display_object'] is None
    path=str((out/'ContainerVisibility.FCStd').resolve())
    s.call('save',document=doc,path=path)
    s.call('close',document=doc)
    state=s.call('open',path=path);doc=state['document']
    assert not objects(state)['LooseSketch']['effective_visibility']
    assert objects(state)['Body']['body_state']['display_object'] is None
    state=transaction(s,doc,'Show native App Part',[put('Assembly','Visibility',True)])
    for key in ['Body','Solid','LooseSolid','LooseSketch']:
        assert objects(state)[key]['effective_visibility'],key
    assert objects(state)['Body']['body_state']['display_object']=='Solid'
    s.call('close',document=doc)
    return {'native_container_visibility':True,'reopened_visibility':True}


def main():
    global COMPILER
    p=argparse.ArgumentParser()
    p.add_argument('--host',required=True,type=Path)
    p.add_argument('--compiler',required=True,type=Path)
    p.add_argument('--runtime',required=True,type=Path)
    p.add_argument('--output',required=True,type=Path)
    a=p.parse_args()
    a.output.mkdir(parents=True,exist_ok=True)
    COMPILER=a.compiler
    s=Session(a.host,json.loads(a.runtime.read_text()))
    try:
        result=workflows(s,a.output)
        result['primitives']=primitive_workflows(s,a.output)
        result['metadata_atomicity']=metadata_and_atomicity(s,a.output)
        result['conditional_hole']=conditional_hole_workflow(s,a.output)
        result['container_visibility']=container_visibility(s,a.output)
        result['reopened_parameter_edits']=EDIT_VERIFICATIONS
        (a.output/'feature-report.json').write_text(json.dumps(result,indent=2))
        print(json.dumps(result,indent=2))
    finally:s.close()

if __name__=='__main__':main()






