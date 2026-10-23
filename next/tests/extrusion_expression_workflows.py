"""Pending Pad/Pocket controller expressions through the UI's actual schema/compiler.

Generate disposable inputs, reproduce incomplete previews, supply the target via
its now-reachable field, and compare saved results with direct native setters.
No expression evaluation in the test client or Rust, and no GUI is driven.
"""
import argparse
import copy
import json
from pathlib import Path
import subprocess
import sys
sys.dont_write_bytecode=True


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for key in ['host','compiler','runtime','output']:p.add_argument('--'+key,type=Path,required=True)
    args=p.parse_args();out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
    runtime=json.loads(args.runtime.read_text(encoding='utf-8-sig'))
    helper=Path(__file__).with_name('extrusion_termination_workflows.py').resolve()
    def native(*arguments):subprocess.run([runtime['executable'],*runtime['args'],str(helper),*map(str,arguments)],check=True)
    native('--native-fixtures',out/'inputs')
    fixtures=json.loads((out/'inputs/fixtures.json').read_text())
    def compile(request,flag='--native-properties'):
        q=subprocess.run([str(args.compiler.resolve()),flag,'-'],input=json.dumps(request),capture_output=True,text=True,check=True)
        return json.loads(q.stdout)
    from native_workflows import objects,transaction,near
    from preview_workflows import PreviewSession
    s=PreviewSession(args.host.resolve(),runtime);comparisons=[];checks=[]
    def task(kind,properties=None,expressions=None,create=False):
        return dict(type='PartDesign::'+kind,name='Result',body='Body',create=create,
                    properties=properties or {},expressions=expressions or {},tip='Result',tip_visibility=True)
    def schema(obj,request):return compile(dict(schema=obj,parameters=request),'--extrusion-schema')
    def reachable(obj,request,fields,label=None):
        view=schema(obj,request)
        if label:
            (out/(label+'-schema-input.json')).write_text(json.dumps(dict(schema=obj,parameters=request),indent=2))
            (out/(label+'-schema-output.json')).write_text(json.dumps(view,indent=2))
        for field in fields:
            metadata=view['property_metadata'][field]
            assert view['property_types'][field],field
            assert metadata['hidden'] is False and metadata['read_only'] is False and metadata['task_editable'] is True,(field,metadata,request)
            # These are the exact row filter, writable and expression-widget
            # conditions consumed by editor/properties.rs. Rust unit tests call
            # the shared property_commands::writable predicate too.
            assert not request['expressions'].get(field) and not obj.get('expressions',{}).get(field),(field,'target itself expression-bound')
        return {field:view['property_metadata'][field] for field in fields}
    def compare_case(case,doc,baseline,request):
        path=out/(case+'-request.json');path.write_text(json.dumps(request,indent=2))
        expected=out/(case+'-expected.FCStd');native('--native-reference',baseline,path,expected)
        direct=json.loads(expected.with_suffix('.json').read_text());assert direct['valid'],(case,direct)
        actual=out/(case+'-actual.FCStd');s.call('save',document=doc,path=str(actual))
        current=s.call('inspect',document=doc);near(objects(current)['Result']['shape']['volume'],direct['volume'])
        comparisons.append(dict(case=case,actual=str(actual),expected=str(expected),properties=list(request['properties'])))
        return actual
    try:
        for kind in ['Pad','Pocket']:
            f=fixtures[kind];front='Target' if kind=='Pad' else 'OtherTarget';back='OtherTarget' if kind=='Pad' else 'Target'
            face=f['targets'][front]['upper' if kind=='Pad' else 'lower'];face2=f['targets'][back]['lower' if kind=='Pad' else 'upper']
            link=lambda obj,sub:dict(link=obj,subelements=[sub])
            cases=[
                ('original_type',{}, {'Type':'3'}, {}, {'UpToFace':link(front,face)},['UpToFace','Offset']),
                ('shape_self_relative',{}, {'.Type':'5'}, {'UpToShape':[link(front,'Face99999')]}, {'UpToShape':[dict(link=front+'Surface')]},['UpToShape']),
                ('second_type',{}, {'Type2':'3'}, {'SideType':'Two sides'}, {'UpToFace2':link(back,face2)},['UpToFace2','Offset2']),
                ('side_and_second',{}, {'SideType':'Base.Length > 10 mm ? 1 : 0', '.Type2':'3'}, {}, {'UpToFace2':link(back,face2)},['Type2','UpToFace2']),
                ('custom_bool',{}, {'UseCustomVector':'1','.Direction.x':'0.2'}, {'Direction':[1,0,0]}, {'Direction':[0.2,0,1]},['Direction','AlongSketchNormal']),
                ('reference_bool',{'UseCustomVector':True,'Direction':[0.2,0,1]}, {'.UseCustomVector':'0'}, {'ReferenceAxis':link('Profile','H_Axis')}, {'ReferenceAxis':link('Profile','N_Axis')},['ReferenceAxis','AlongSketchNormal']),
            ]
            for name,initial_props,expressions,properties,correction,fields in cases:
                case=kind+'-'+name
                d=s.call('open',path=f['path'])['document']
                base=transaction(s,d,'Initial dimension',compile(task(kind,dict(Profile=dict(link='Profile'),**initial_props),create=True)))
                baseline=out/(case+'-baseline.FCStd');s.call('save',document=d,path=str(baseline))
                request=task(kind,copy.deepcopy(properties),copy.deepcopy(expressions))
                before=reachable(objects(base)['Result'],request,fields)
                error=s.call('preview',document=d,token=case,label=case,edits=compile(request),expect_error='invalid_model')
                recovered=s.call('inspect',document=d)
                assert recovered['preview'] is None
                assert objects(recovered)['Result']['properties']['Type']=='Length'
                after=reachable(objects(recovered)['Result'],request,fields,case)
                request['properties'].update(correction)
                candidate=s.call('preview',document=d,token=case,label=case,edits=compile(request))['state']
                assert objects(candidate)['Result']['valid']
                # The controller expressions remain in the request unchanged.
                assert request['expressions']==expressions
                canceled=s.call('preview_cancel',document=d,token=case)
                assert canceled['undo_count']==base['undo_count']
                s.call('preview',document=d,token=case,label=case,edits=compile(request))
                committed=s.call('preview_commit',document=d,token=case)
                assert committed['undo_count']==base['undo_count']+1
                s.call('undo',document=d);s.call('redo',document=d)
                actual=compare_case(case,d,baseline,request)
                s.call('close',document=d)
                state=s.call('open',path=str(actual));d=state['document']
                obj=objects(state)['Result']
                for field in fields:
                    # Reopened native expressions, with no pending overrides,
                    # must retain the same conservative reachability.
                    view=schema(obj,task(kind))
                    assert view['property_metadata'][field]['hidden'] is False,(case,field)
                if name=='original_type':
                    for protected in [task(kind,{'Shape':None}),task(kind,{}, {'.Shape.Placement.Base.x':'1 mm'})]:
                        s.call('transaction',document=d,label='ReadOnly guard unchanged',edits=compile(protected),expect_error='read_only_property')
                        s.call('inspect',document=d)
                    clear=task(kind,{'Type':'Length'},{'Type':None})
                    assert schema(obj,clear)['property_metadata']['UpToFace']['hidden'] is True
                    transaction(s,d,'Clear controller expression',compile(clear))
                    assert not objects(s.call('inspect',document=d))['Result']['expressions'].get('Type')
                s.call('close',document=d)
                checks.append(dict(case=case,pending_request=task(kind,properties,expressions),error=error,before=before,after=after,correction=correction,preview_cancel=True,undo_redo=True,reopened=True))

            # Dormant mode, target and dimensional expressions survive failed
            # activation, correction, deactivation, native parent changes and
            # exact native expression-path clears, without being evaluated in Rust.
            case=kind+'-dormant'
            d=s.call('open',path=f['path'])['document']
            transaction(s,d,'Initial dimension',compile(task(kind,{'Profile':dict(link='Profile')},create=True)))
            baseline=out/(case+'-baseline.FCStd');s.call('save',document=d,path=str(baseline))
            first=task(kind,{'UpToShape2':[dict(link=back+'Surface')]},
                       {'.SideType':'0','.Type2':'3','.Length2':'Length + 4 mm','Offset2':'2 mm'})
            transaction(s,d,'Dormant second side',compile(first));baseline=compare_case(case+'-stored',d,baseline,first)
            activate=task(kind,{}, {'SideType':'1'})
            error=s.call('preview',document=d,token=case,label=case,edits=compile(activate),expect_error='invalid_model')
            recovered=s.call('inspect',document=d)
            reachable(objects(recovered)['Result'],activate,['Type2','UpToFace2','Length2','UpToShape2'])
            activate['properties']['UpToFace2']=link(back,face2)
            s.call('preview',document=d,token=case,label=case,edits=compile(activate))
            accepted=s.call('preview_commit',document=d,token=case)
            assert objects(accepted)['Result']['expressions'].get('.Length2')=='Length + 4 mm'
            baseline=compare_case(case+'-activated',d,baseline,activate)
            deactivate=task(kind,{}, {'SideType':'0'})
            transaction(s,d,'Deactivate second side',compile(deactivate));baseline=compare_case(case+'-deactivated',d,baseline,deactivate)
            cleared=task(kind,{'SideType':'Two sides','Type2':'Length'},{'.SideType':None,'SideType':None,'.Type2':None,'Type2':None})
            view=schema(objects(s.call('inspect',document=d))['Result'],cleared)
            assert view['property_metadata']['UpToFace2']['hidden'] is True
            transaction(s,d,'Clear exact controller paths',compile(cleared));baseline=compare_case(case+'-cleared',d,baseline,cleared)
            checks.append(dict(case=case,error=error,dormant_values_expressions_targets_retained=True,exact_path_clears=True))
            s.call('close',document=d)

            case=kind+'-parent-expression'
            d=s.call('open',path=f['path'])['document']
            transaction(s,d,'Initial dimension',compile(task(kind,{'Profile':dict(link='Profile')},create=True)))
            baseline=out/(case+'-baseline.FCStd');s.call('save',document=d,path=str(baseline))
            request=task(kind,{}, {'Type':'Base.Length > 30 mm ? 5 : 3'})
            s.call('preview',document=d,token=case,label=case,edits=compile(request),expect_error='invalid_model')
            recovered=s.call('inspect',document=d);reachable(objects(recovered)['Result'],request,['UpToFace','UpToShape'])
            request['properties']['UpToFace']=link(front,face)
            s.call('preview',document=d,token=case,label=case,edits=compile(request));s.call('preview_commit',document=d,token=case)
            baseline=compare_case(case+'-face',d,baseline,request)
            request=task(kind,{'UpToShape':[dict(link=front+'Surface')]})
            request['parent_edits']=[dict(op='set',object='Base',property='Length',value=60)]
            # The retained native expression changes Type from UpToFace to
            # UpToShape after this parent edit. No numeric setter translation.
            reachable(objects(s.call('inspect',document=d))['Result'],request,['UpToShape'])
            transaction(s,d,'Native parent-controlled mode',[*request['parent_edits'],*compile(request)])
            assert objects(s.call('inspect',document=d))['Result']['properties']['Type']=='UpToShape'
            compare_case(case+'-shape',d,baseline,request)
            s.call('close',document=d)

        manifest=out/'comparisons.json';manifest.write_text(json.dumps(comparisons,indent=2))
        native('--native-compare',manifest,out/'native-comparison.json')
        (out/'report.json').write_text(json.dumps(dict(cases=len(comparisons),checks=checks,capabilities=s.capabilities),indent=2))
        print(json.dumps(dict(native_comparisons=len(comparisons),failed_preview_corrections=len([x for x in checks if 'correction' in x]),dormant_sequences=2)))
    finally:s.close()

if __name__=='__main__':main()
