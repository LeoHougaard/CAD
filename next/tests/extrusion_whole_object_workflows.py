"""Whole-object extrusion selection and native-reference decoding regression.

Uses the public host/compiler and pinned FreeCAD; creates synthetic documents.
Run with --host, --compiler, --runtime, --output. Optional --fixtures reuses a
native_fixture fixtures.json read-only. No private evidence path is required.
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
sys.dont_write_bytecode=True
from extrusion_termination_workflows import native_reference,native_compare

def dump(path,value):path.write_text(json.dumps(value,indent=2))
def identity(value):
    if value is None:return None
    if isinstance(value,list):return [identity(v) for v in value]
    if isinstance(value,tuple):return [value[0].Name,list(value[1])]
    return value.Name

def prepare_native(fixtures,out):
    import FreeCAD as A
    import PartDesign
    records=json.loads(fixtures.read_text())
    for kind in ['Pad','Pocket']:
        d=A.openDocument(records[kind]['path']);r=d.getObject('Body').newObject('PartDesign::'+kind,'Result')
        r.Profile=d.getObject('Profile');r.Length=20
        for ptype,name in [('Link','ProbeLink'),('LinkSub','ProbeSub'),('LinkList','ProbeLinks'),('LinkSubList','ProbeSubList')]:
            r.addProperty('App::Property'+ptype,name,'Oracle regression')
        d.getObject('Body').Tip=r;d.recompute();assert r.isValid()
        d.saveAs(str(out/(kind+'-baseline.FCStd')));A.closeDocument(d.Name)

def check_native(manifest_path):
    import FreeCAD as A
    import PartDesign
    manifest=json.loads(manifest_path.read_text());out=manifest_path.parent
    host_pairs=[];oracle_pairs=[];errors=[];boundaries=[];model_cases=[]
    for entry in manifest:
        kind=entry['kind'];front=entry['front'];back=entry['back'];face=entry['face'];otherface=entry['otherface']
        baseline=Path(entry['baseline']);raw=Path(entry['request']);oracle=out/(kind+'-oracle.FCStd')
        native_reference(baseline,raw,oracle)
        report=json.loads(oracle.with_suffix('.json').read_text())
        # Independent exact control: native assignment, no oracle decoder.
        d=A.openDocument(str(baseline));r=d.getObject('Result')
        r.Type='UpToShape';r.UpToShape=[(d.getObject(front+'Surface'),[''])];r.Offset=0
        d.getObject('Body').Tip=r;d.recompute()
        assert r.isValid() and math.isclose(r.AddSubShape.Volume,400,abs_tol=1e-7)
        total=12160 if kind=='Pad' else 11760
        assert math.isclose(r.Shape.Volume,total,abs_tol=1e-7)
        exact=out/(kind+'-exact.FCStd');d.saveAs(str(exact));A.closeDocument(d.Name)
        properties=['Type','UpToShape','Offset']
        host_pairs.append(dict(case=kind+'-host-exact',actual=entry['actual'],expected=str(exact),properties=properties,analytical_prism=400))
        if report['valid']:
            oracle_pairs.append(dict(case=kind+'-oracle-exact',actual=str(oracle),expected=str(exact),properties=properties,analytical_prism=400))
        if not report['valid'] or not math.isclose(report.get('prism_volume',-1),400,abs_tol=1e-7) or not math.isclose(report.get('volume',-1),total,abs_tol=1e-7):
            errors.append(dict(case=kind+'-oracle-model',actual=report,expected_volume=total,expected_prism=400))
        model_cases.append(dict(kind=kind,oracle=report,expected_volume=total,expected_prism=400))

        # Native dynamic properties exercise the actual assignment boundary,
        # without requiring these references to be supported extrusion targets.
        cases=['ordinary-links','omitted','empty-tuple','explicit-whole','mixed','empty-list']
        for case in cases:
            if case=='ordinary-links':
                props=dict(ProbeLink=dict(link=front+'Surface'),ProbeSub=dict(link=front,subelements=[]),
                           ProbeLinks=[dict(link=front+'Surface'),dict(link=back+'Surface')])
            else:
                values={'omitted':[dict(link=front+'Surface')],
                        'empty-tuple':[dict(link=front+'Surface',subelements=[])],
                        'explicit-whole':[dict(link=front+'Surface',subelements=[''])],
                        'mixed':[dict(link=front+'Surface',subelements=[]),dict(link=back+'Surface'),dict(link=front,subelements=[face,otherface])],
                        'empty-list':[]}
                props=dict(ProbeSubList=values[case])
            request=dict(type='PartDesign::'+kind,name='Result',body='Body',create=False,tip='Result',tip_visibility=True,properties=props,expressions={})
            path=out/(kind+'-boundary-'+case+'-request.json');dump(path,request)
            result_path=out/(kind+'-boundary-'+case+'-oracle.FCStd');native_reference(baseline,path,result_path)
            result_report=json.loads(result_path.with_suffix('.json').read_text())
            assert result_report['valid'],(case,result_report)
            # Expected assignments are explicit native objects/tuples. In
            # particular, ordinary LinkSub deliberately retains its empty list.
            d=A.openDocument(str(baseline));r=d.getObject('Result')
            f=d.getObject(front);fs=d.getObject(front+'Surface');bs=d.getObject(back+'Surface')
            if case=='ordinary-links':r.ProbeLink=fs;r.ProbeSub=(f,[]);r.ProbeLinks=[fs,bs]
            else:
                r.ProbeSubList={'omitted':[fs],'empty-tuple':[(fs,[''])],
                                'explicit-whole':[(fs,[''])],'mixed':[(fs,['']),bs,(f,[face,otherface])],
                                'empty-list':[]}[case]
            d.recompute();assert r.isValid()
            expected={p:identity(getattr(r,p)) for p in props}
            a=A.openDocument(str(result_path));actual={p:identity(getattr(a.getObject('Result'),p)) for p in props}
            assert a.getObject('Result').Shape.cut(r.Shape).Volume+r.Shape.cut(a.getObject('Result').Shape).Volume<1e-6
            matched=actual==expected
            if not matched:errors.append(dict(case=kind+'-'+case,actual=actual,expected=expected))
            boundaries.append(dict(kind=kind,case=case,actual=actual,expected=expected,matched=matched))
            A.closeDocument(a.Name);A.closeDocument(d.Name)

        # Mixed whole-object/face targets must retain identities even when
        # native modeling rejects their particular geometry.
        mixed_path=Path(entry['mixed_request']);mixed_oracle=out/(kind+'-mixed-model-oracle.FCStd')
        native_reference(baseline,mixed_path,mixed_oracle)
        mixed_report=json.loads(mixed_oracle.with_suffix('.json').read_text())
        d=A.openDocument(str(baseline));r=d.getObject('Result');r.Type='UpToShape';r.Offset=0
        r.UpToShape=[(d.getObject(front+'Surface'),['']),(d.getObject(front),[face]),(d.getObject(back+'Surface'),[''])]
        d.getObject('Body').Tip=r;d.recompute()
        exact_report=dict(valid=r.isValid(),message=r.getStatusString(),targets=identity(r.UpToShape))
        assert len(r.UpToShape)==3 and exact_report['targets']==[[front+'Surface',['']],[front,[face]],[back+'Surface',['']]]
        if not r.isValid():
            assert not entry['mixed_host']['ok'],entry['mixed_host']
            assert entry['mixed_host']['error']['code']=='invalid_model'
            assert exact_report['message'] in entry['mixed_host']['error']['message']
            if mixed_report['valid'] or mixed_report['message']!=exact_report['message']:
                errors.append(dict(case=kind+'-mixed-model',oracle=mixed_report,exact=exact_report))
        else:
            assert entry['mixed_host']['ok'] and mixed_report['valid']
            exact_mixed=out/(kind+'-mixed-model-exact.FCStd');d.saveAs(str(exact_mixed))
            host_pairs.append(dict(case=kind+'-mixed-host-exact',actual=entry['mixed_actual'],expected=str(exact_mixed),properties=properties))
            oracle_pairs.append(dict(case=kind+'-mixed-oracle-exact',actual=str(mixed_oracle),expected=str(exact_mixed),properties=properties))
        A.closeDocument(d.Name)
        model_cases.append(dict(kind=kind,mixed_oracle=mixed_report,mixed_exact=exact_report,host=entry['mixed_host']))
    dump(out/'boundaries.json',boundaries);dump(out/'native-model-cases.json',model_cases)
    dump(out/'host-pairs.json',host_pairs);native_compare(out/'host-pairs.json',out/'host-native-comparison.json')
    dump(out/'oracle-pairs.json',oracle_pairs)
    try:native_compare(out/'oracle-pairs.json',out/'oracle-native-comparison.json')
    except AssertionError as error:
        (out/'oracle-native-compare-failure.txt').write_text(str(error))
        errors.append(dict(case='unchanged-native-compare',message=str(error)))
    dump(out/'result.json',dict(passed=not errors,model_cases=model_cases,boundary_count=len(boundaries),errors=errors))
    assert not errors,errors

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['host','compiler','runtime','output','fixtures']:parser.add_argument('--'+name,type=Path)
    parser.add_argument('--native-prepare',nargs=2,type=Path);parser.add_argument('--native-check',type=Path)
    args=parser.parse_args()
    if args.native_prepare:return prepare_native(*args.native_prepare)
    if args.native_check:return check_native(args.native_check)
    from native_workflows import objects
    from preview_workflows import PreviewSession
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
    os.environ['PYTHONDONTWRITEBYTECODE']='1'
    runtime=json.loads(args.runtime.read_text(encoding='utf-8-sig'));commands=[]
    def native(*extra):
        cmd=[runtime['executable'],*runtime['args'],str(Path(__file__).resolve()),*map(str,extra)]
        commands.append(cmd);dump(out/'native-commands.json',commands)
        with open(out/('native-'+str(len(commands))+'.log'),'w') as log:
            result=subprocess.run(cmd,stdout=log,stderr=log)
        assert result.returncode==0,(cmd,result.returncode)
    fixtures=args.fixtures.resolve() if args.fixtures else out/'inputs/fixtures.json'
    if not args.fixtures:
        cmd=[runtime['executable'],*runtime['args'],str(Path(__file__).with_name('extrusion_termination_workflows.py')),'--native-fixtures',str(out/'inputs')]
        commands.append(cmd);dump(out/'native-commands.json',commands);subprocess.run(cmd,check=True)
    immutable={str(fixtures):hashlib.sha256(fixtures.read_bytes()).hexdigest()}
    fixtures_data=json.loads(fixtures.read_text())
    for f in fixtures_data.values():immutable[f['path']]=hashlib.sha256(Path(f['path']).read_bytes()).hexdigest()
    native('--native-prepare',fixtures,out)
    session=PreviewSession(args.host.resolve(),runtime);manifest=[]
    def compile(request):
        r=subprocess.run([str(args.compiler.resolve()),'--native-properties','-'],input=json.dumps(request),text=True,capture_output=True,check=True)
        return json.loads(r.stdout)
    try:
        for kind in ['Pad','Pocket']:
            f=fixtures_data[kind];front='Target' if kind=='Pad' else 'OtherTarget';back='OtherTarget' if kind=='Pad' else 'Target'
            face=f['targets'][front]['upper' if kind=='Pad' else 'lower'];otherface=f['targets'][front]['lower' if kind=='Pad' else 'upper']
            baseline=out/(kind+'-baseline.FCStd');state=session.call('open',path=str(baseline));doc=state['document']
            request=dict(type='PartDesign::'+kind,name='Result',body='Body',create=False,tip='Result',tip_visibility=True,properties=dict(Type='UpToShape',Offset=0),expressions={},
                         extrusion_selection=dict(property='UpToShape',selections=[[front+'Surface','']]))
            edits=compile(request);target=next(e['value'] for e in edits if e.get('property')=='UpToShape')
            assert target==[dict(link=front+'Surface',subelements=[])]
            preview=session.call('preview',document=doc,token=kind,label='Selected whole-object target',edits=edits)
            committed=session.call('preview_commit',document=doc,token=kind)
            total=12160 if kind=='Pad' else 11760
            assert math.isclose(objects(committed)['Result']['shape']['volume'],total,abs_tol=1e-7)
            actual=out/(kind+'-actual.FCStd');session.call('save',document=doc,path=str(actual));session.call('close',document=doc)
            selected=copy.deepcopy(request);selected.pop('extrusion_selection');selected['properties']['UpToShape']=target
            path=out/(kind+'-selected-request.json');dump(path,selected)
            # The mixed/multiple case starts from the same original baseline.
            state=session.call('open',path=str(baseline));doc=state['document']
            mixed=copy.deepcopy(request);mixed['extrusion_selection']['selections']=[[front+'Surface',''],[front,face],[back+'Surface','']]
            mixed_edits=compile(mixed);mixed_target=next(e['value'] for e in mixed_edits if e.get('property')=='UpToShape')
            assert mixed_target==[dict(link=front+'Surface',subelements=[]),dict(link=front,subelements=[face]),dict(link=back+'Surface',subelements=[])]
            # Preserve the complete protocol response for either native result.
            # Both outcomes are checked against an independent native control.
            wire=dict(op='preview',document=doc,token=kind,label='Mixed native targets',edits=mixed_edits,expected_revision=session.revisions[doc])
            session.process.stdin.write(json.dumps(wire)+'\n');session.process.stdin.flush()
            mixed_host=json.loads(session.process.stdout.readline())
            mixed_actual=None
            if mixed_host['ok']:
                result=mixed_host['result'];session.revisions[doc]=result['revision']
                stored=objects(result['state'])['Result']['properties']['UpToShape']
                assert stored==[dict(link=front+'Surface',subelements=['']),dict(link=front,subelements=[face]),dict(link=back+'Surface',subelements=[''])]
                session.call('preview_commit',document=doc,token=kind)
                mixed_actual=str(out/(kind+'-mixed-actual.FCStd'));session.call('save',document=doc,path=mixed_actual)
            else:
                assert mixed_host['error']['code']=='invalid_model'
                recovered=session.call('inspect',document=doc)
                assert recovered['preview'] is None and objects(recovered)['Result']['properties']==objects(state)['Result']['properties']
            session.call('close',document=doc)
            mixed.pop('extrusion_selection');mixed['properties']['UpToShape']=mixed_target
            mixed_path=out/(kind+'-mixed-request.json');dump(mixed_path,mixed)
            manifest.append(dict(kind=kind,front=front,back=back,face=face,otherface=otherface,baseline=str(baseline),request=str(path),actual=str(actual),preview=preview,
                                 compiled_edits=edits,mixed_request=str(mixed_path),mixed_host=mixed_host,mixed_actual=mixed_actual,mixed_compiled_edits=mixed_edits))
        dump(out/'manifest.json',manifest)
    finally:session.close()
    try:native('--native-check',out/'manifest.json')
    finally:
        assert all(hashlib.sha256(Path(p).read_bytes()).hexdigest()==h for p,h in immutable.items())
        dump(out/'fixture-hashes.json',immutable)
    print(json.dumps(dict(passed=True,selected_whole_object_models=2,native_property_boundaries=12)))

if __name__=='__main__':main()
