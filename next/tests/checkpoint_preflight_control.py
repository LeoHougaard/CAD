"""Windows native regression control. No build, global setting, or GUI input."""
from pathlib import Path
import argparse,ctypes,hashlib,json,os,runpy,subprocess,sys

TEST='editor::sketch::lifecycle_tests::sketch_creation_missing_checkpoint_preserves_redo_before_native_creation'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()

def worker_mode(script):
    # The configured worker is the genuine FreeCAD Python process, not a shim.
    k=ctypes.WinDLL('kernel32',use_last_error=True)
    k.GetCurrentProcess.argtypes=[];k.GetCurrentProcess.restype=ctypes.c_void_p
    k.GetPriorityClass.argtypes=[ctypes.c_void_p];k.GetPriorityClass.restype=ctypes.c_uint32
    priority=k.GetPriorityClass(k.GetCurrentProcess());assert priority==16384
    root=Path(os.environ['CAD_PREFLIGHT_CONTROL_ROOT']).resolve()
    script=Path(script).resolve();backend=script.parents[2]/'backend'
    sys.path.insert(0,str(backend));sys.argv=[str(script)]
    import freecad_worker as worker
    profiles={p:os.environ[p] for p in ['HOME','USERPROFILE','FREECAD_USER_HOME','APPDATA','LOCALAPPDATA','TMP','TEMP']}
    assert all(Path(v).resolve().is_relative_to(root) for v in profiles.values())
    actual={p:worker.App.ConfigGet(p) for p in ['UserAppData','UserParameter']}
    assert all(v and Path(v).resolve().is_relative_to(root) for v in actual.values())
    trace=root/('native-'+str(os.getpid())+'.jsonl')
    def emit(data):
        with trace.open('a',encoding='utf-8') as f:f.write(json.dumps(data,allow_nan=False)+'\n')
    emit(dict(kind='binding',pid=os.getpid(),priority=priority,profiles=profiles,native_config=actual,
              version=worker.App.Version(),worker_sha256=sha(backend/'freecad_worker.py')))
    original_main=worker.main
    def observed_main():
        original_snapshot=worker.snapshot
        def snapshot(doc,revision=None):
            result=original_snapshot(doc,revision)
            for item in result['objects']:
                item['observed_native_id']=doc.getObject(item['name']).ID
            return result
        worker.snapshot=snapshot
        dispatch=worker.dispatch
        def observed(request):
            copied=json.loads(json.dumps(request));result=None
            try:result=dispatch(request);return result
            finally:emit(dict(kind='dispatch',request=copied,result=result))
        worker.dispatch=observed
        try:original_main()
        finally:emit(dict(kind='main-finally'))
    worker.main=observed_main
    runpy.run_path(str(script),run_name='__main__')

def host_mode():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime',type=Path,required=True)
    parser.add_argument('--tests',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if os.name!='nt':parser.error('This control removes the Windows checkpoint DLL and requires Windows')
    code=Path(__file__).resolve();project=code.parents[1]
    # Installed location is next/tests/checkpoint_preflight_control.py.
    assert (project/'backend/freecad_worker.py').is_file()
    out=args.output.resolve()
    if out.is_relative_to(project):parser.error('Output must be outside the next source directory')
    if out.exists():parser.error('Output must be a NEW directory; old attempts are never overwritten')
    assert args.tests.is_file() and args.runtime.is_file()
    out.mkdir(parents=True)
    backend=project/'backend';copied={}
    for original in backend.rglob('*'):
        if not original.is_file():continue
        name=original.relative_to(backend).as_posix()
        if name=='native/checkpoint_restore.dll':continue
        target=out/'missing/next/backend'/name
        target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(original.read_bytes());copied[name]=sha(target)
    assert (backend/'native/checkpoint_restore.dll').is_file()
    fixture=out/'missing/next/tests/fixtures/lifecycle_fault_worker.py'
    fixture.parent.mkdir(parents=True);fixture.write_bytes((project/'tests/fixtures/lifecycle_fault_worker.py').read_bytes())
    runtime=json.loads(args.runtime.read_bytes())
    runtime.update(args=['-B','-u',str(code),'--worker'],script=str(out/'missing/next/backend/freecad_worker.py'),test_worker=str(fixture))
    rt=out/'runtime.json';rt.write_text(json.dumps(runtime,indent=2)+'\n')
    env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',PYTHONNOUSERSITE='1',PYTHONUTF8='1',PYTHONIOENCODING='utf-8')
    env.pop('PYTHONPATH',None);env.pop('PYTHONHOME',None)
    for key,sub in [('HOME','home'),('USERPROFILE','home'),('FREECAD_USER_HOME','config'),('APPDATA','appdata'),('LOCALAPPDATA','localappdata'),('TMP','tmp'),('TEMP','tmp')]:
        p=out/sub;p.mkdir(exist_ok=True);env[key]=str(p)
    output=out/'output';output.mkdir()
    env.update(CAD_LIFECYCLE_RUNTIME=str(rt),CAD_LIFECYCLE_OUTPUT=str(output),CAD_PREFLIGHT_CONTROL_ROOT=str(out))
    exe=args.tests.resolve();flags=subprocess.BELOW_NORMAL_PRIORITY_CLASS|subprocess.CREATE_NO_WINDOW
    listing=subprocess.run([str(exe),TEST,'--exact','--list'],capture_output=True,text=True,env=env,creationflags=flags)
    (out/'registration.log').write_text(listing.stdout+listing.stderr)
    assert listing.returncode==0 and listing.stdout.count(TEST+': test')==1
    pins={str(p.resolve()):sha(p) for p in [code,args.runtime,exe,fixture,Path(runtime['executable'])]}
    command=[str(exe),TEST,'--exact','--ignored','--test-threads=1','--nocapture']
    (out/'request.json').write_text(json.dumps(dict(command=command,pins=pins,copied_backend=copied,omitted=['native/checkpoint_restore.dll'],priority='BelowNormal',profiles={k:env[k] for k in ['HOME','USERPROFILE','FREECAD_USER_HOME','APPDATA','LOCALAPPDATA','TMP','TEMP']}),indent=2)+'\n')
    with (out/'raw.log').open('xb') as log:
        child=subprocess.Popen(command,cwd=project,env=env,stdout=log,stderr=subprocess.STDOUT,creationflags=flags)
        exit_code=child.wait(timeout=90)
    assert all(sha(p)==digest for p,digest in pins.items())
    assert all(sha(out/'missing/next/backend'/name)==digest for name,digest in copied.items())
    verdict=dict(test_exit=exit_code,pins_unchanged=True,graceful_native_eof_certified=False)
    traces=list(out.glob('native-*.jsonl'));assert len(traces)==1
    rows=[json.loads(s) for s in traces[0].read_text().splitlines()]
    calls=[x for x in rows if x['kind']=='dispatch']
    caps=next(x['result']['sketch_checkpoints'] for x in calls if x['request']['op']=='capabilities')
    verdict.update(native_capability=caps,transactions=sum(x['request']['op']=='transaction' for x in calls),sketch_begin=sum(x['request']['op']=='sketch_begin' for x in calls))
    verdict['control_pass']=exit_code==0 and caps['available'] is False and 'checkpoint_restore.dll' in caps['reason'] and verdict['transactions']==1 and verdict['sketch_begin']==0
    (out/'result.json').write_text(json.dumps(verdict,indent=2)+'\n')
    print(json.dumps(verdict));return 0 if verdict['control_pass'] else 1

if __name__=='__main__':
    if len(sys.argv)==3 and sys.argv[1]=='--worker':worker_mode(sys.argv[2])
    else:sys.exit(host_mode())
