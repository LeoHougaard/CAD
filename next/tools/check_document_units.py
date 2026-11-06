"""Focused native document-unit, Editor, CLI, restoration and preference gates."""
import argparse, hashlib, json, os, subprocess, sys
from pathlib import Path

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for key in ('runtime','host','compiler','editor_tests','output'):
        p.add_argument('--'+key.replace('_','-'),type=Path,required=True)
    a=p.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
    source=Path(__file__).resolve().parents[1];runtime=json.loads(a.runtime.read_text())
    runtime['script']=str(source/'tests/fixtures/document_units_process_observer.py')
    observed=out/'observed-runtime.json';observed.write_text(json.dumps(runtime,indent=2)+'\n')
    env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',PYTHONUTF8='1',PYTHONIOENCODING='utf-8',
             CAD_UNITS_RUNTIME=str(observed),CAD_UNITS_OUTPUT=str(out/'editor'),
             CAD_UNITS_R1_FIXTURE=str(source/'tests/fixtures/units_quantity.FCStd'),
             CAD_UNITS_PREFS_REPORT=str(out/'preferences.json'))
    env.setdefault('FREECAD_USER_HOME',str(out/'config'));Path(env['FREECAD_USER_HOME']).mkdir(parents=True,exist_ok=True)
    receipts=[]
    files=[a.host,a.compiler,a.editor_tests,*source.glob('src/**/*.rs'),*source.glob('backend/**/*.py'),
           *source.glob('backend/native/*'),*source.glob('tests/*units*.py'),*source.glob('tests/fixtures/*units*'),Path(__file__)]
    def sha(f):
        with f.open('rb') as s:return hashlib.file_digest(s,'sha256').hexdigest()
    hashes={str(f.resolve()):sha(f) for f in files if f.is_file()}
    (out/'executed-inputs.json').write_text(json.dumps(hashes,indent=2)+'\n')
    def run(name,command,extra=None):
        command=list(map(str,command))
        with (out/(name+'.log')).open('x',encoding='utf-8') as log:
            r=subprocess.run(command,env=dict(env,**(extra or {})),stdout=log,stderr=subprocess.STDOUT,
                             creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS if os.name=='nt' else 0)
        receipts.append(dict(name=name,command=command,exit_code=r.returncode,environment=extra))
        (out/'commands.json').write_text(json.dumps(receipts,indent=2)+'\n');print(name,r.returncode,flush=True)
        if r.returncode:raise SystemExit((out/(name+'.log')).read_text())
    run('public',[sys.executable,'-B',source/'tests/document_units_workflows.py','--host',a.host.resolve(),
        '--runtime',observed,'--output',out/'public','--faults'])
    for name,pattern in [('six-editor','native_units_'),('fixed-findings','fixed_r1_'),('r2-acceptance','review_r2_')]:
        run(name,[a.editor_tests.resolve(),pattern,'--ignored','--test-threads=1','--nocapture'])
    populated=dict(runtime,script=str(source/'tests/fixtures/document_units_populated_observer.py'))
    populated_path=out/'populated-runtime.json';populated_path.write_text(json.dumps(populated,indent=2)+'\n')
    run('populated-settings',[a.editor_tests.resolve(),'review_r1_settings_failure_restores_populated_model_and_redo',
        '--ignored','--test-threads=1','--nocapture'],dict(CAD_UNITS_RUNTIME=str(populated_path),CAD_UNITS_PREFS_REPORT=str(out/'preferences-populated.json')))
    events=json.loads((out/'public/events.json').read_text())
    state=next(e['response']['result'] for e in events if e['request']['op']=='document_settings' and e['response']['ok'])
    request=dict(state=state,unit_system=3)
    command=[str(a.compiler.resolve()),'--document-units','-']
    r=subprocess.run(command,input=json.dumps(request),env=env,encoding='utf-8',capture_output=True,
                     creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS if os.name=='nt' else 0)
    expected=dict(op='document_settings',document=state['document'],expected_revision=state['revision'],unit_system=3)
    (out/'cli.json').write_text(json.dumps(dict(command=command,request=request,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr),indent=2)+'\n')
    assert r.returncode==0 and json.loads(r.stdout)==expected
    run('native-readback',[runtime['executable'],*runtime['args'],str(source/'tests/units_saved_native.py'),
        '--input',out/'editor','--output',out/'native-readback.json'])
    prefs=[json.loads(f.read_text()) for f in out.glob('preferences*.json')]
    assert prefs and all(x['unchanged'] for x in prefs)
    assert all(sha(Path(f))==h for f,h in hashes.items())
    (out/'passed.json').write_text(json.dumps(dict(native_public=True,actual_editor_cases=6,fixed_findings=3,
        additional_r2_cases=3,populated_settings_fault_cases=3,cli=True,native_readbacks=3,
        preference_reports=len(prefs),preferences_unchanged=True,source_unchanged=True,resource_comparison=False,physical_gui=False),indent=2)+'\n')

if __name__=='__main__':main()
