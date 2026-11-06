"""Build only the pinned checkpoint adapter; no FreeCAD/dependency rebuild."""
import argparse, hashlib, json, os, subprocess
from pathlib import Path
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['runtime','vcvars','compiler','output']:parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args();source=Path(__file__).resolve().parents[1]/'backend/native'
    build=args.output.resolve();build.mkdir(parents=True,exist_ok=False)
    environment=subprocess.check_output('cmd.exe /d /s /c ""'+str(args.vcvars.resolve())+'" >nul && set"',text=True)
    env=dict(os.environ)
    for line in environment.splitlines():
        if '=' in line and not line.startswith('='):
            key,value=line.split('=',1);env[key]=value
    command=[str(args.compiler.resolve()),'/nologo','/LD','/MD','/EHsc','/O2','/std:c++20',
        '/D_ITERATOR_DEBUG_LEVEL=0',str(source/'checkpoint_restore.cpp'),'/Fo'+str(build/'checkpoint_restore.obj'),
        '/link','/Brepro','/OUT:'+str(source/'checkpoint_restore.dll'),'/IMPLIB:'+str(build/'checkpoint_restore.lib')]
    with (build/'build.log').open('x') as log:
        result=subprocess.run(command,env=env,cwd=build,stdout=log,stderr=subprocess.STDOUT,
            creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS)
    digest=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
    receipt=dict(command=command,exit_code=result.returncode,priority='BelowNormal',jobs=1,
        compiler_sha256=digest(args.compiler),source_sha256=digest(source/'checkpoint_restore.cpp'))
    if not result.returncode:
        runtime=Path(json.loads(args.runtime.read_text())['executable']).resolve().parents[1]
        receipt['dll_sha256']=digest(source/'checkpoint_restore.dll')
        (source/'checkpoint_restore.json').write_text(json.dumps(dict(platform='win32',python=[3,11,14],
            adapter_sha256=receipt['dll_sha256'],runtime={rel:digest(runtime/rel) for rel in
                ['bin/FreeCADBase.dll','bin/FreeCADApp.dll','bin/python311.dll']}),indent=2)+'\n',newline='\n')
    (build/'build.json').write_text(json.dumps(receipt,indent=2)+'\n')
    if result.returncode:raise SystemExit((build/'build.log').read_text())
    print('Built checkpoint adapter',receipt['dll_sha256'])
if __name__=='__main__':main()
