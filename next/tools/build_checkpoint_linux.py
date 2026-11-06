"""Build one tiny Linux adapter inside the configured pinned native runtime.

Invoke with that runtime's bundled Python, not the system Python. No native
core or dependencies are built. ELF declarations and loaded libraries are read
from the actual runtime. Run aarch64 and x86_64 acceptance independently.
"""
import argparse,ctypes,hashlib,json,os,platform,shutil,subprocess,sys
from pathlib import Path
NEXT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(NEXT/'backend'))
import checkpoint_linux as L

DECLARATIONS={
    'CHECKPOINT_APPLICATION':('app','App::Application::_pcSingleton'),
    'CHECKPOINT_DOCUMENT':('app','App::Application::getDocument(char const*) const'),
    'CHECKPOINT_OBJECT':('app','App::Document::getObject(char const*) const'),
    'CHECKPOINT_RESTORE':('base','Base::Persistence::restoreFromStream(std::basic_istream<char, std::char_traits<char> >&)'),
}
def exports(path):
    raw=[line.split()[-1] for line in subprocess.check_output(['nm','-D','--defined-only',str(path)],text=True).splitlines() if line.strip()]
    decoded=subprocess.check_output(['c++filt'],input='\n'.join(raw)+'\n',text=True).splitlines()
    return {name:symbol for symbol,name in zip(raw,decoded)}

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--compiler',default=os.environ.get('CXX','c++'))
    args=parser.parse_args();arch=L.architecture()
    if sys.platform!='linux' or arch not in ('aarch64','x86_64'):raise RuntimeError('Unsupported adapter platform')
    import FreeCAD
    if FreeCAD.Version()[:3]!=['1','1','4']:raise RuntimeError('FreeCAD 1.1.4 is required')
    extension=Path(FreeCAD.__file__).resolve();runtime=extension.parents[2]
    marker=json.loads((runtime.parent/'.runtime.json').read_text())
    lock=json.loads((NEXT/'runtime.json').read_text());key='linux-'+arch
    if marker!={'platform':key,'sha256':lock['assets'][key]['sha256']}:raise RuntimeError('Pinned runtime extraction marker mismatch')
    folder=NEXT/'backend/native';out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
    app=(runtime/'usr/lib/libFreeCADApp.so').resolve();base=(runtime/'usr/lib/libFreeCADBase.so').resolve()
    tables={'app':exports(app),'base':exports(base)}
    symbols={macro:tables[role].get(declaration) for macro,(role,declaration) in DECLARATIONS.items()}
    if not all(symbols.values()):raise RuntimeError('Required exact native ELF declarations unavailable')
    (out/'checkpoint_symbols.h').write_text(''.join('#define '+key+' '+json.dumps(value)+'\n' for key,value in symbols.items()))
    compiler=shutil.which(args.compiler)
    if not compiler:raise RuntimeError('A C++ compiler is required to build the Linux checkpoint adapter')
    artifact=folder/('checkpoint_restore-linux-'+arch+'.so')
    command=[compiler,'-std=c++17','-O2','-shared','-fPIC','-fvisibility=hidden','-I'+str(out),
        str(folder/'checkpoint_restore_linux.cpp'),'-Wl,-z,defs','-Wl,--build-id=sha1','-ldl','-o',str(artifact)]
    with (out/'build.log').open('x') as log:r=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT)
    receipt=dict(command=command,exit_code=r.returncode,architecture=arch,compiler_version=subprocess.check_output([compiler,'--version'],text=True).splitlines()[0],
        compiler_sha256=L.digest(Path(compiler)),source_sha256=L.digest(folder/'checkpoint_restore_linux.cpp'),symbols=symbols,
        declarations=DECLARATIONS,pixel_build_jobs=os.environ.get('PIXEL_BUILD_JOBS'))
    (out/'build.json').write_text(json.dumps(receipt,indent=2)+'\n')
    if r.returncode:raise RuntimeError((out/'build.log').read_text())
    # Resolve/check the libraries already loaded by this exact bundled runtime.
    mode=os.RTLD_NOW|os.RTLD_NOLOAD
    app_handle=ctypes.PyDLL(str(app),mode=mode);base_handle=ctypes.PyDLL(str(base),mode=mode)
    std=ctypes.PyDLL('libstdc++.so.6',mode=mode);python=ctypes.PyDLL(None)
    libraries={
        'app':dict(path=str(app.relative_to(runtime)),symbol=symbols['CHECKPOINT_DOCUMENT']),
        'base':dict(path=str(base.relative_to(runtime)),symbol=symbols['CHECKPOINT_RESTORE']),
        'stdlib':dict(path=str(L.symbol_path(std,'_Znwm').relative_to(runtime)),symbol='_Znwm'),
        'python':dict(path=str(L.symbol_path(python,'Py_GetVersion').relative_to(runtime)),symbol='Py_GetVersion',process=True)}
    paths={extension,Path(sys.executable).resolve(),*[runtime/v['path'] for v in libraries.values()]}
    for path in paths:L.verify_elf(path,arch)
    L.verify_elf(artifact,arch)
    manifest=dict(architecture=arch,python=list(sys.version_info[:3]),freecad_version=list(FreeCAD.Version()),
        archive_sha256=marker['sha256'],freecad_extension=str(extension.relative_to(runtime)),
        python_executable=str(Path(sys.executable).resolve().relative_to(runtime)),libraries=libraries,
        runtime={str(path.relative_to(runtime)):L.digest(path) for path in sorted(paths)},
        adapter=artifact.name,adapter_sha256=L.digest(artifact),symbols=symbols)
    target=folder/('checkpoint_restore-linux-'+arch+'.json');target.write_text(json.dumps(manifest,indent=2)+'\n')
    # Loading and resolving the built adapter is a mandatory build success check.
    L.load()
    print('Built/loaded native checkpoint adapter '+arch,file=sys.stderr)
if __name__=='__main__':
    try:main()
    except Exception as error:raise SystemExit('checkpoint adapter build: '+str(error))
