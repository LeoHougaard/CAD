"""Build the pinned FreeCAD 1.1.4 Windows x64 Mirror ABI adapter, serially."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime',type=Path,required=True)
    parser.add_argument('--vcvars',type=Path,required=True)
    parser.add_argument('--compiler',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True,help='Private build/log directory')
    args=parser.parse_args()
    source=Path(__file__).resolve().parents[1]/'backend/native'
    build=args.output.resolve(); build.mkdir(parents=True,exist_ok=True)
    result=subprocess.check_output('cmd.exe /d /s /c ""'+str(args.vcvars.resolve())+'" >nul && set"',text=True)
    env=dict(os.environ)
    for line in result.splitlines():
        if '=' in line and not line.startswith('='):
            key,value=line.split('=',1); env[key]=value
    command=[str(args.compiler.resolve()),'/nologo','/LD','/MD','/EHsc','/O2','/std:c++20',
        '/D_ITERATOR_DEBUG_LEVEL=0',str(source/'sketch_mirror.cpp'),'/Fo'+str(build/'sketch_mirror.obj'),
        '/link','/Brepro','/OUT:'+str(source/'sketch_mirror.dll'),'/IMPLIB:'+str(build/'sketch_mirror.lib')]
    with (build/'build.log').open('w') as log:
        result=subprocess.run(command,env=env,cwd=build,stdout=log,stderr=subprocess.STDOUT,
            creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS)
    digest=lambda f:hashlib.sha256(f.read_bytes()).hexdigest()
    receipt=dict(command=command,exit_code=result.returncode,priority='BelowNormal',jobs=1,
        compiler_sha256=digest(args.compiler),source_sha256=digest(source/'sketch_mirror.cpp'))
    if not result.returncode:
        receipt['dll_sha256']=digest(source/'sketch_mirror.dll')
        runtime=Path(json.loads(args.runtime.read_text())['executable']).resolve().parents[1]
        (source/'sketch_mirror.json').write_text(json.dumps(dict(bridge_sha256=receipt['dll_sha256'],
            runtime={rel:digest(runtime/rel) for rel in ['bin/FreeCADApp.dll','lib/Sketcher.pyd']}),indent=2)+'\n',newline='\n')
    (build/'build.json').write_text(json.dumps(receipt,indent=2)+'\n')
    if result.returncode:
        raise SystemExit((build/'build.log').read_text())
    print('Built pinned Mirror adapter:',receipt['dll_sha256'])

if __name__=='__main__':
    main()
