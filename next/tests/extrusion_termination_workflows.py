"""Bounded pinned-native Pad/Pocket tasks through the actual UI property compiler.

Creates its own disposable native documents. No private originals, GUI or mocks.
Use --host, --compiler, --runtime and --output, as with feature_workflows.py.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys


def native_fixture(directory):
    import FreeCAD as A
    import Part
    import PartDesign
    import Sketcher
    directory.mkdir(parents=True, exist_ok=True)
    records = {}
    for key in ['Pad', 'Pocket', 'PadTransformed', 'PocketTransformed']:
        kind = 'Pocket' if key.startswith('Pocket') else 'Pad'
        d = A.newDocument('Extrusion' + key)
        b = d.addObject('PartDesign::Body', 'Body')
        base = b.newObject('PartDesign::AdditiveBox', 'Base')
        base.Length, base.Width, base.Height = 20, 20, 30
        base.Placement.Base.z = -15
        sk = b.newObject('Sketcher::SketchObject', 'Profile')
        points = [(4,4), (8,4), (8,8), (4,8)]
        for a,c in zip(points, points[1:]+points[:1]):
            sk.addGeometry(Part.LineSegment(A.Vector(*a,0), A.Vector(*c,0)), False)
        for name,z in [('Target',20), ('OtherTarget',-25)]:
            tb = d.addObject('PartDesign::Body',name+'Body')
            ts = tb.newObject('Sketcher::SketchObject',name+'Sketch')
            points2 = [(0,0),(20,0),(20,20),(0,20)]
            for a,c in zip(points2,points2[1:]+points2[:1]):
                ts.addGeometry(Part.LineSegment(A.Vector(*a,0),A.Vector(*c,0)),False)
            ts.Placement.Base.z = z
            t = tb.newObject('PartDesign::Pad',name)
            t.Profile, t.Length = ts, 5
            ts.Visibility = False
            surface=d.addObject('Part::Feature',name+'Surface')
            surface.Shape=Part.makePlane(20,20)
            surface.Placement.Base.z=z+5 if kind=='Pad' else z
            datum=tb.newObject('PartDesign::Plane',name+'Datum')
            datum.Placement.Base.z=z+5 if kind=='Pad' else z
            tb.Tip=t
        if key.endswith('Transformed'):
            container=d.addObject('App::Part','Container')
            for name in ['Body','TargetBody','OtherTargetBody','TargetSurface','OtherTargetSurface']:container.addObject(d.getObject(name))
            container.Placement=A.Placement(A.Vector(33,-17,9),A.Rotation(A.Vector(0,1,0),25))
            frame=A.Placement(A.Vector(3,5,0),A.Rotation(A.Vector(0,0,1),30))
            for name in ['Body','TargetBody','OtherTargetBody']:d.getObject(name).Placement=frame
            for name in ['TargetSurface','OtherTargetSurface']:
                obj=d.getObject(name);obj.Placement=frame.multiply(obj.Placement)
        d.recompute()
        targets = {}
        for name in ['Target','OtherTarget']:
            t=d.getObject(name)
            targets[name] = {
                'upper': 'Face'+str(max(enumerate(t.Shape.Faces,1), key=lambda v:v[1].CenterOfMass.z)[0]),
                'lower': 'Face'+str(min(enumerate(t.Shape.Faces,1), key=lambda v:v[1].CenterOfMass.z)[0])}
        path = directory / (key+'.FCStd')
        d.saveAs(str(path.resolve()))
        records[key] = dict(path=str(path.resolve()), targets=targets)
        A.closeDocument(d.Name)
    (directory/'fixtures.json').write_text(json.dumps(records,indent=2))


def native_reference(baseline, request_path, output):
    import FreeCAD as A
    import PartDesign
    d=A.openDocument(str(baseline.resolve()))
    request=json.loads(request_path.read_text())
    try:
        obj=d.getObject(request['name'])
        if request['create']:
            obj=d.getObject(request['body']).newObject(request['type'],request['name'])
        def decode(value):
            if isinstance(value,dict) and 'link' in value:
                source=d.getObject(value['link'])
                return (source,value['subelements']) if 'subelements' in value else source
            if isinstance(value,list) and value and isinstance(value[0],dict):
                return [decode(v) for v in value]
            return value
        for edit in request.get('parent_edits',[]):
            parent=d.getObject(edit['object'])
            if edit['op']=='set':
                value=edit['value']
                if edit['property']=='Placement':
                    value=A.Placement(A.Vector(*value['position']),A.Rotation(*value['rotation']))
                setattr(parent,edit['property'],value)
            elif edit['op']=='geometry':
                import Part
                parent.addGeometry(Part.Circle(A.Vector(*edit['center']),A.Vector(0,0,1),edit['radius']),False)
        # Native task setters permit editor-read-only properties until recompute.
        for prop,value in request['properties'].items():
            setattr(obj,prop,A.Vector(*value) if prop == 'Direction' else decode(value))
        for prop,value in request.get('expressions',{}).items():obj.setExpression(prop,value)
        if not request['create'] and request.get('tip'):
            d.getObject(request['body']).Tip=d.getObject(request['tip'])
            d.getObject(request['tip']).Visibility=request.get('tip_visibility',True)
        d.recompute()
    except Exception as error:
        report=dict(valid=False,message=str(error),exception=type(error).__name__)
    else:
        report=dict(valid=obj.isValid(),message=obj.getStatusString())
        if obj.isValid():
            report.update(volume=obj.Shape.Volume,prism_volume=obj.AddSubShape.Volume)
            d.saveAs(str(output.resolve()))
    output.with_suffix('.json').write_text(json.dumps(report,indent=2))
    A.closeDocument(d.Name)


def native_compare(manifest, output):
    import FreeCAD as A
    import PartDesign
    records=[]
    for entry in json.loads(manifest.read_text()):
        before={p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in [entry['actual'],entry['expected']]}
        a=A.openDocument(entry['actual']); b=A.openDocument(entry['expected'])
        x,y=a.getObject('Result'),b.getObject('Result')
        assert x.isValid() and y.isValid(),entry
        for shape,ref in [(x.Shape,y.Shape),(x.AddSubShape,y.AddSubShape)]:
            assert shape.isValid() and ref.isValid()
            assert math.isclose(shape.Volume,ref.Volume,rel_tol=1e-9,abs_tol=1e-7),entry
            assert shape.cut(ref).Volume+ref.cut(shape).Volume < 1e-6,entry
            assert len(shape.Solids)==len(ref.Solids),entry
            assert len(shape.Faces)==len(ref.Faces),entry
        for obj in ['Body','Container','TargetBody','OtherTargetBody']:
            left,right=a.getObject(obj),b.getObject(obj)
            if left is not None:
                assert left.getGlobalPlacement().isSame(right.getGlobalPlacement(),1e-8),entry
        world=x.Shape.copy();expected_world=y.Shape.copy()
        world.Placement=x.getGlobalPlacement();expected_world.Placement=y.getGlobalPlacement()
        assert world.cut(expected_world).Volume+expected_world.cut(world).Volume<1e-6,entry
        for prop in entry['properties']:
            if prop.startswith(('Profile','UpToFace','UpToShape','ReferenceAxis')):
                left=getattr(x,prop); right=getattr(y,prop)
                def identity(v):
                    if v is None:return None
                    if isinstance(v,list):return [identity(z) for z in v]
                    return [v[0].Name,list(v[1])]
                assert identity(left)==identity(right),(entry,prop,left,right)
            else:
                left,right=getattr(x,prop),getattr(y,prop)
                if hasattr(left,'Value'):assert math.isclose(left.Value,right.Value,rel_tol=1e-9,abs_tol=1e-9),entry
                elif isinstance(left,A.Vector):assert (left-right).Length<1e-9,entry
                elif hasattr(left,'Name'):assert left.Name==right.Name,entry
                else:assert left==right,(entry,prop,left,right)
        assert dict(x.ExpressionEngine)==dict(y.ExpressionEngine),entry
        assert a.getObject('Body').Tip.Name==b.getObject('Body').Tip.Name,entry
        if entry.get('analytical_prism') is not None:
            assert math.isclose(x.AddSubShape.Volume,entry['analytical_prism'],rel_tol=1e-8),entry
        records.append(dict(case=entry['case'],volume=x.Shape.Volume,prism_volume=x.AddSubShape.Volume,
                            native_shape_difference=0,expressions=dict(x.ExpressionEngine)))
        A.closeDocument(a.Name);A.closeDocument(b.Name)
        assert all(hashlib.sha256(Path(p).read_bytes()).hexdigest()==h for p,h in before.items())
    output.write_text(json.dumps(records,indent=2))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host',type=Path);parser.add_argument('--compiler',type=Path)
    parser.add_argument('--runtime',type=Path);parser.add_argument('--output',type=Path)
    parser.add_argument('--native-fixtures',type=Path)
    parser.add_argument('--native-reference',nargs=3,type=Path)
    parser.add_argument('--native-compare',nargs=2,type=Path)
    args=parser.parse_args()
    if args.native_fixtures:return native_fixture(args.native_fixtures)
    if args.native_reference:return native_reference(*args.native_reference)
    if args.native_compare:return native_compare(*args.native_compare)
    from native_workflows import objects,near,transaction,set_value
    from preview_workflows import PreviewSession
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
    runtime=json.loads(args.runtime.read_text(encoding='utf-8-sig'))
    def native(*extra):
        subprocess.run([runtime['executable'],str(Path(__file__).resolve()),*map(str,extra)],check=True)
    def compile(parameters,flag='--native-properties'):
        p=subprocess.run([str(args.compiler.resolve()),flag,'-'],input=json.dumps(parameters),capture_output=True,text=True,check=True)
        return json.loads(p.stdout)
    native('--native-fixtures',out/'inputs')
    fixtures=json.loads((out/'inputs/fixtures.json').read_text())
    comparisons=[];failures=[]
    session=PreviewSession(args.host.resolve(),runtime)
    def task(kind,properties,create=True,expressions=None):
        return dict(type='PartDesign::'+kind,name='Result',body='Body',create=create,
                    tip='Result',tip_visibility=True,properties=properties,expressions=expressions or {})
    try:
        for kind in ['Pad','Pocket']:
            fixture=fixtures[kind];targets=fixture['targets']
            front='Target' if kind=='Pad' else 'OtherTarget'
            back='OtherTarget' if kind=='Pad' else 'Target'
            frontface=targets[front]['upper' if kind=='Pad' else 'lower']
            backface=targets[back]['lower' if kind=='Pad' else 'upper']
            link=lambda name,face:dict(link=name,subelements=[face])
            cases=[('default',{},None),('dimension',dict(Length='2 cm'),320),
                ('reverse',dict(Length=20,Reversed=True),320),
                ('symmetric',dict(SideType='Symmetric',Length=40),640),
                ('two_lengths',dict(SideType='Two sides',Length=20,Length2='22 mm'),672),
                ('first',dict(Type='UpToFirst',Offset=1),None),
                ('last' if kind=='Pad' else 'through_all',dict(Type='UpToLast' if kind=='Pad' else 'ThroughAll'),None),
                ('face',dict(Type='UpToFace',UpToFace=link(front,frontface),Offset=1),None),
                ('whole_surface',dict(Type='UpToShape',UpToShape=[dict(link=front+'Surface')]),None),
                ('datum_plane',dict(Type='UpToFace',UpToFace=dict(link=front+'Datum')),None),
                ('shape_faces',dict(Type='UpToShape',UpToShape=[link(front,frontface)],Offset=1),None),
                ('two_faces',dict(SideType='Two sides',Type='UpToFace',UpToFace=link(front,frontface),Offset=1,
                                 Type2='UpToFace',UpToFace2=link(back,backface),Offset2=-1),None),
                ('two_shapes',dict(SideType='Two sides',Type='UpToShape',UpToShape=[link(front,frontface)],
                                  Type2='UpToShape',UpToShape2=[link(back,backface)]),None),
                ('symmetric_face',dict(SideType='Symmetric',Type='UpToFace',UpToFace=link(front,frontface)),None),
                ('second_first',dict(SideType='Two sides',Length=20,Type2='UpToFirst',Offset2=1),None),
                ('second_last' if kind=='Pad' else 'second_through_all',dict(SideType='Two sides',Length=20,Type2='UpToLast' if kind=='Pad' else 'ThroughAll'),None),
                ('two_tapers',dict(SideType='Two sides',Length=20,Length2=20,TaperAngle=1,TaperAngle2=2),None),
                ('custom',dict(Length=20,UseCustomVector=True,Direction=[0.2,0,1],AlongSketchNormal=True),320),
                ('custom_measured',dict(Length=20,UseCustomVector=True,Direction=[0.2,0,1],AlongSketchNormal=False),320/math.sqrt(1.04)),
                ('reference',dict(Length=20,ReferenceAxis=link('Profile','N_Axis')),320),
                ('expression',dict(SideType='Two sides',Length=20,Length2=20),None)]
            for name,changes,analytical in cases:
                case=kind.lower()+'_'+name
                state=session.call('open',path=fixture['path']);doc=state['document']
                properties=dict(Profile=dict(link='Profile'),**changes)
                request=task(kind,properties,expressions={'Length2':'Length + 2 mm'} if name=='expression' else None)
                request_path=out/(case+'-request.json');request_path.write_text(json.dumps(request,indent=2))
                expected=out/(case+'-expected.FCStd')
                native('--native-reference',fixture['path'],request_path,expected)
                direct=json.loads(expected.with_suffix('.json').read_text())
                assert direct['valid'],(case,direct)
                edits=compile(request)
                if name in ['face','shape_faces','reference']:
                    prop='UpToFace' if name=='face' else 'UpToShape' if name=='shape_faces' else 'ReferenceAxis'
                    selected=[(front,frontface)] if prop!='ReferenceAxis' else [('Profile','N_Axis')]
                    with_selection=dict(request,extrusion_selection=dict(property=prop,selections=selected))
                    assert compile(with_selection)==edits
                preview=session.call('preview',document=doc,token=case,label=case,edits=edits)['state']
                near(objects(preview)['Result']['shape']['volume'],direct['volume'])
                schema=compile(dict(schema=objects(preview)['Result'],parameters=task(kind,{},False)),'--extrusion-schema')
                for field in ['Type2','Length','Length2','TaperAngle','TaperAngle2','UpToFace','UpToFace2','UpToShape','UpToShape2','Offset','Offset2','Direction','ReferenceAxis']:
                    assert schema['property_metadata'][field]['read_only']==objects(preview)['Result']['property_metadata'][field]['read_only'],(case,field)

                # Replace with an incomplete target, recover the last candidate,
                # then replace again with the complete original compiled task.
                if name in ['face','two_faces','shape_faces','two_shapes']:
                    invalid=dict(request);invalid['properties']=dict(properties)
                    prop='UpToShape' if 'shape' in name else 'UpToFace'
                    invalid['properties'][prop]=[link('Target','Face99999')] if 'shape' in name else link('Target','Face99999')
                    error=session.call('preview',document=doc,token=case,label=case,edits=compile(invalid),expect_error='invalid_model')
                    failures.append(dict(case=case,error=error))
                    restored=session.call('inspect',document=doc)
                    near(objects(restored)['Result']['shape']['volume'],direct['volume'])
                    session.call('preview',document=doc,token=case,label=case,edits=edits)
                canceled=session.call('preview_cancel',document=doc,token=case)
                assert 'Result' not in objects(canceled)
                session.call('preview',document=doc,token=case,label=case,edits=edits)
                accepted=session.call('preview_commit',document=doc,token=case)
                assert 'Result' not in objects(session.call('undo',document=doc))
                if name=='default':
                    undone=session.call('inspect',document=doc)
                    compile(request) # Opening/compiling a deferred task is read-only.
                    session.call('preview',document=doc,token=case+'redo',label=case,edits=edits,expect_error='preview_redo_conflict')
                    deferred=session.call('inspect',document=doc)
                    assert deferred['revision']==undone['revision'] and deferred['redo_count']==undone['redo_count']
                redone=session.call('redo',document=doc)
                near(objects(redone)['Result']['shape']['volume'],direct['volume'])
                actual=out/(case+'-actual.FCStd');session.call('save',document=doc,path=str(actual))
                session.call('close',document=doc)
                reopened=session.call('open',path=str(actual));doc=reopened['document']
                # Reopened native task: change a controller and its previously
                # locked target in one compiler request, Cancel then commit.
                edit=task(kind,dict(Type='UpToFace',UpToFace=link(front,frontface)),False)
                if name=='default':
                    schema=compile(dict(schema=objects(reopened)['Result'],parameters=edit),'--extrusion-schema')
                    assert schema['property_metadata']['UpToFace']['read_only'] is False
                    session.call('preview',document=doc,token=case+'edit',label='Edit target',edits=compile(edit))
                    session.call('preview_cancel',document=doc,token=case+'edit')
                    near(objects(session.call('inspect',document=doc))['Result']['shape']['volume'],direct['volume'])
                session.call('close',document=doc)
                comparisons.append(dict(case=case,actual=str(actual),expected=str(expected),properties=list(properties),analytical_prism=analytical))
        for kind in ['Pad','Pocket']:
            fixture=fixtures[kind];front='Target' if kind=='Pad' else 'OtherTarget'
            upper=fixture['targets'][front]['upper'];lower=fixture['targets'][front]['lower']
            link=lambda face:dict(link=front,subelements=[face])
            for name,changes in [
                    ('missing_face',dict(Type='UpToFace',UpToFace=None)),
                    ('edge_target',dict(Type='UpToFace',UpToFace=link('Edge1'))),
                    ('whole_closed_shape',dict(Type='UpToShape',UpToShape=[dict(link=front)])),
                    ('multiface_offset',dict(Type='UpToShape',UpToShape=[dict(link=front,subelements=[upper,lower])],Offset=1)),
                    ('perpendicular_direction',dict(UseCustomVector=True,Direction=[1,0,0])),
                    ('zero_length',dict(Length=0)),
                    ('length_units',dict(Length='20 deg')),
                    ('offset_units',dict(Type='UpToFace',UpToFace=link(upper),Offset='1 deg'))]:
                case=kind.lower()+'_invalid_'+name
                state=session.call('open',path=fixture['path']);doc=state['document']
                request=task(kind,dict(Profile=dict(link='Profile'),**changes))
                path=out/(case+'-request.json');path.write_text(json.dumps(request,indent=2))
                expected=out/(case+'-expected.FCStd');native('--native-reference',fixture['path'],path,expected)
                direct=json.loads(expected.with_suffix('.json').read_text());assert not direct['valid'],(case,direct)
                if name=='missing_face':
                    session.call('preview',document=doc,token=case,label=case,edits=compile(request),expect_error='invalid_model')
                    recovered=session.call('inspect',document=doc)
                    assert recovered['preview'] is None and 'Result' not in objects(recovered)
                error=session.call('transaction',document=doc,label=case,edits=compile(request),expect_error='native_error' if direct.get('exception') else 'invalid_model')
                assert direct['message'] in error['message'],(case,direct,error)
                restored=session.call('inspect',document=doc)
                assert 'Result' not in objects(restored) and restored['preview'] is None
                assert restored['undo_count']==state['undo_count']
                transaction(session,doc,'Repair with native dimension',compile(task(kind,dict(Profile=dict(link='Profile'),Length=20))))
                failures.append(dict(case=case,direct=direct,error=error,atomic_recovery=True))
                session.call('close',document=doc)
        # Committed reopened mode changes, parent parameters, native face
        # remapping and nested Part/Body placements use independent references.
        for key in ['Pad','Pocket','PadTransformed','PocketTransformed']:
            kind='Pocket' if key.startswith('Pocket') else 'Pad'
            fixture=fixtures[key]
            front='Target' if kind=='Pad' else 'OtherTarget'
            face=fixture['targets'][front]['upper' if kind=='Pad' else 'lower']
            state=session.call('open',path=fixture['path']);doc=state['document']
            create=task(kind,dict(Profile=dict(link='Profile'),Length=20))
            transaction(session,doc,'Initial dimension',compile(create))
            baseline=out/(key+'-edit-baseline.FCStd')
            session.call('save',document=doc,path=str(baseline))
            changes=dict(Type='UpToFace',UpToFace=dict(link=front,subelements=[face]),Offset='1 mm')
            edit=task(kind,changes,False)
            for stage,parent_edits in [
                    ('mode',[]),
                    ('target_dimension',[set_value(front,'Length',7)]),
                    ('base_dimension',[set_value('Base','Height',32)]),
                    ('topology',[dict(op='geometry',object=front+'Sketch',kind='circle',center=[15,15,0],radius=2)]),
                    ('return_dimension',[]),
                    ('reference_direction',[]),
                    ('parent_placement',[set_value('Container' if key.endswith('Transformed') else 'Body','Placement',dict(position=[41,-8,12],rotation=[0,math.sin(math.pi/9),0,math.cos(math.pi/9)]))] if key.endswith('Transformed') else [])]:
                case=key.lower()+'_edit_'+stage
                if stage=='return_dimension':
                    edit=task(kind,dict(Type='Length',SideType='Two sides',Type2='Length',Length='2 cm',Length2=21,
                                        UseCustomVector=True,Direction=[0.2,0,1],AlongSketchNormal=True),False,
                              {'Length2':'Length + 1 mm'})
                if stage=='reference_direction':
                    edit=task(kind,dict(Type='Length',SideType='Two sides',Type2='Length',Length=20,Length2=21,
                                        UseCustomVector=False,ReferenceAxis=dict(link='Profile',subelements=['N_Axis'])),False)
                edit['parent_edits']=parent_edits
                path=out/(case+'-request.json');path.write_text(json.dumps(edit,indent=2))
                expected=out/(case+'-expected.FCStd')
                native('--native-reference',baseline,path,expected)
                direct=json.loads(expected.with_suffix('.json').read_text());assert direct['valid'],(case,direct)
                before=session.call('inspect',document=doc)
                result=transaction(session,doc,case,[*parent_edits,*compile(edit)])
                near(objects(result)['Result']['shape']['volume'],direct['volume'])
                if stage=='topology':
                    assert objects(result)[front]['shape']['faces']==7
                    old=objects(before)['Result']['properties']['UpToFace']['subelements']
                    new=objects(result)['Result']['properties']['UpToFace']['subelements']
                    assert old!=new,(old,new)
                    # Subsequent tasks use the remapped native property, never
                    # a guessed face number from the original shape.
                    edit['properties']['UpToFace']=objects(result)['Result']['properties']['UpToFace']
                undone=session.call('undo',document=doc)
                near(objects(undone)['Result']['shape']['volume'],objects(before)['Result']['shape']['volume'])
                result=session.call('redo',document=doc)
                actual=out/(case+'-actual.FCStd');session.call('save',document=doc,path=str(actual))
                session.call('close',document=doc)
                reopened=session.call('open',path=str(actual));doc=reopened['document']
                comparisons.append(dict(case=case,actual=str(actual),expected=str(expected),properties=list(edit['properties'])))
                baseline=actual
            session.call('close',document=doc)
        manifest=out/'comparison-manifest.json';manifest.write_text(json.dumps(comparisons,indent=2))
        native('--native-compare',manifest,out/'native-comparison.json')
        (out/'report.json').write_text(json.dumps(dict(cases=len(comparisons),failures=failures,capabilities=session.capabilities),indent=2))
        print(json.dumps(dict(cases=len(comparisons),native_comparison='passed',failure_recoveries=len(failures))))
    finally:session.close()

if __name__=='__main__':main()
