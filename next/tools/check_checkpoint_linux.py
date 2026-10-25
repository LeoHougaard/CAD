"""CI/native gate for the configured pinned Linux checkpoint runtime.

Run after setup_runtime.py and a matching Rust host build. This deliberately
requires an explicit expected architecture so ARM evidence cannot stand in for x64.
"""
import argparse,json,os,platform,subprocess,sys
from pathlib import Path
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['runtime','host','output']:parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--expected-arch',choices=['aarch64','x86_64'],required=True)
    args=parser.parse_args();arch={'arm64':'aarch64','amd64':'x86_64'}.get(platform.machine().lower(),platform.machine().lower())
    if sys.platform!='linux' or arch!=args.expected_arch:raise SystemExit('Checkpoint gate architecture mismatch')
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
    native=Path(__file__).resolve().parents[1];runtime=json.loads(args.runtime.read_text())
    commands=[('native',[runtime['executable'],*runtime['args'],str(native/'tests/checkpoint_linux_native.py'),'--output',str(out/'native')]),
        ('public',[sys.executable,str(native/'tests/checkpoint_linux_workflows.py'),'--runtime',str(args.runtime.resolve()),'--host',str(args.host.resolve()),'--output',str(out/'public')]),
        ('availability',[sys.executable,str(native/'tests/checkpoint_restore_workflows.py'),'--runtime',str(args.runtime.resolve()),'--host',str(args.host.resolve()),'--output',str(out/'availability')])]
    for name,command in commands:
        with (out/(name+'.log')).open('x') as log:r=subprocess.run(command,env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1'),stdout=log,stderr=subprocess.STDOUT)
        (out/(name+'-receipt.json')).write_text(json.dumps(dict(command=command,exit_code=r.returncode,architecture=arch),indent=2)+'\n')
        print(name,r.returncode,flush=True)
        if r.returncode:raise SystemExit((out/(name+'.log')).read_text())
    (out/'passed.json').write_text(json.dumps(dict(architecture=arch,native=True,public=True,availability=True,physical_ui=False),indent=2)+'\n')
if __name__=='__main__':main()
