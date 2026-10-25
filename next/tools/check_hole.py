"""Focused native Hole/Editor, serialized Open, units and saved-BRep gates."""
import argparse,hashlib,json,os,re,subprocess,sys,tarfile
from pathlib import Path

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for key in ['runtime','editor_tests','output']:p.add_argument('--'+key.replace('_','-'),type=Path,required=True)
    a=p.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);source=Path(__file__).resolve().parents[1]
    fixture=out/'fixtures';fixture.mkdir()
    with tarfile.open(source/'tests/fixtures/hole_native_fixtures.tar.gz') as t:t.extractall(fixture,filter='data')
    def relocate(v):
        if isinstance(v,dict):return {k:relocate(x) for k,x in v.items()}
        if isinstance(v,list):return [relocate(x) for x in v]
        if isinstance(v,str):
            norm=v.replace('\\','/')
            for owner,dest in [('cad-parity-20261025-hole-fixed',''),('cad-parity-20261025-hole-review-r1/native-fixtures','review-fixtures')]:
                prefix='C:/Users/Leo/Code Projects/CAD/.tools/'+owner+'/'
                if norm.startswith(prefix):return str(fixture/dest/norm[len(prefix):])
        return v
    for f in fixture.rglob('*.json'):f.write_text(json.dumps(relocate(json.loads(f.read_text())),indent=2)+'\n')
    runtime=json.loads(a.runtime.read_text());env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',PYTHONUTF8='1',PYTHONIOENCODING='utf-8')
    def sha(path):
        with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
    files=[f for folder in ['src','backend','tests','tools'] for f in (source/folder).rglob('*') if f.is_file() and '__pycache__' not in f.parts]
    inputs={str(f.relative_to(source)):sha(f) for f in files}
    test_sha=sha(a.editor_tests)
    (out/'executed-inputs.json').write_text(json.dumps(dict(sources=inputs,editor_tests=str(a.editor_tests.resolve()),editor_tests_sha256=test_sha,
        runtime=runtime,runtime_executable_sha256=sha(runtime['executable'])),indent=2)+'\n')
    env.setdefault('FREECAD_USER_HOME',str(out/'config'));Path(env['FREECAD_USER_HOME']).mkdir(parents=True,exist_ok=True)
    receipts=[]
    def run(name,command,extra=None):
        command=list(map(str,command))
        with (out/(name+'.log')).open('x',encoding='utf-8') as log:r=subprocess.run(command,env=dict(env,**(extra or {})),stdout=log,stderr=subprocess.STDOUT,creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS if os.name=='nt' else 0)
        receipts.append(dict(name=name,command=command,exit_code=r.returncode,environment=extra));(out/'commands.json').write_text(json.dumps(receipts,indent=2)+'\n');print(name,r.returncode,flush=True)
        if r.returncode:raise SystemExit((out/(name+'.log')).read_text())
    run('native-cases',[runtime['executable'],*runtime['args'],source/'tests/hole_native_cases.py','--fixtures',fixture,'--output',out/'native-cases'])
    prefix='editor::properties::acceptance_probe::hole_acceptance::'
    groups=[('units','hole_commands::tests::'),('reviewer-probes',prefix+'review_'),('fixed-regressions',prefix+'fix_'),('independent-r2',prefix+'r2_')]
    groups += [('retained-'+short,prefix+'hole_editor_'+name) for short,name in [
        ('recovery','recovery_lifecycle'),('computed','computed_protection_and_error_correction'),('expressions','controller_expression_atomic_recovery'),
        ('depth','depth_controller_round_trip'),('missing','missing_preset_forced_custom_recovery_and_protection'),('persistence','persistence_parent_profile_history_and_failed_open')]]
    groups += [('imperial',prefix+'combined_hole_imperial_wrappers_controller_switchback_and_owned_settings'),('inventory',prefix+'r2_serialized_native_contract_and_failed_open_history')]
    for name,selection in groups:
        folder=out/name;folder.mkdir()
        extra=dict(CAD_EDITOR_ACCEPTANCE_RUNTIME=str(a.runtime.resolve()),CAD_EDITOR_ACCEPTANCE_WORKER=str(source/'backend/freecad_worker.py'),CAD_EDITOR_ACCEPTANCE_OUTPUT=str(folder),
          CAD_HOLE_INPUT=str(fixture/'fixtures/input.FCStd'),CAD_HOLE_MATRIX=str(fixture/'native-controls2'),CAD_HOLE_SAVED=str(folder),CAD_HOLE_PERSISTENCE=str(fixture/'persistence2'),
          CAD_HOLE_MISSING_PRESET=str(fixture/'no-norm1'),CAD_HOLE_REVIEW_FIXTURES=str(fixture/'review-fixtures'),CAD_HOLE_FIXED_FIXTURES=str(fixture/'native-controls3'),CAD_R2_FIXTURES=str(out/'native-cases'))
        if name=='inventory':extra['CAD_EDITOR_ACCEPTANCE_WORKER']=str(source/'tests/fixtures/hole_inventory_worker.py')
        run(name,[a.editor_tests.resolve(),selection,'--include-ignored','--test-threads=1','--nocapture'],extra)
        assert re.search(r'test result: ok\. [1-9]',(out/(name+'.log')).read_text()),name
    run('saved-brep',[runtime['executable'],*runtime['args'],source/'tests/hole_saved_native.py','--fixtures',fixture,'--runs',out,'--output',out/'saved-brep'])
    inventory=[json.loads(s) for s in (out/'inventory/inventory.jsonl').read_text().splitlines()]
    rejected=[e for e in inventory if e['op']=='open' and e['error']]
    assert len(rejected)==18 and all(e['before']==e['after'] for e in rejected)
    assert all(sha(source/f)==h for f,h in inputs.items()) and sha(a.editor_tests)==test_sha
    (out/'passed.json').write_text(json.dumps(dict(hole_units=9,reviewer_probes=3,fix_groups=4,independent_groups=4,retained_groups=6,
        imperial_wrappers=True,settings_owned_denials=True,failed_open_inventory_rejections=18,saved_brep=True,resource_comparison=False,physical_gui=False),indent=2)+'\n')

if __name__=='__main__':main()
