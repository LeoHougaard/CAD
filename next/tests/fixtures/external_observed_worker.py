"""Evidence-only wrapper around exact candidate dispatch; never changes requests."""
import hashlib, json, os, sys
from pathlib import Path
R=Path(__file__).resolve().parent
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'backend'))
import freecad_worker as W
original=W.dispatch
out=Path(os.environ['CAD_EXTERNAL_NATIVE_TRACE'])
def dispatch(request):
    try:
        return original(request)
    finally:
        state={n:dict(file=d.FileName,uid=str(d.Uid),undo=d.UndoCount,redo=d.RedoCount,
                      objects=[dict(name=o.Name,id=o.ID,content_sha256=hashlib.sha256(o.dumpContent()).hexdigest()) for o in d.Objects])
               for n,d in W.App.listDocuments().items()}
        with out.open('a',encoding='utf-8') as f:
            f.write(json.dumps(dict(request=request,active=None if W.App.ActiveDocument is None else W.App.ActiveDocument.Name,documents=state))+'\n')
with out.open('a',encoding='utf-8') as f:
    f.write(json.dumps(dict(event='binding',modules={n:dict(path=m.__file__,sha256=hashlib.sha256(Path(m.__file__).read_bytes()).hexdigest()) for n,m in sys.modules.copy().items() if getattr(m,'__file__',None) and Path(m.__file__).is_relative_to(Path(__file__).resolve().parents[2]/'backend')}))+'\n')
W.dispatch=dispatch
W.main()
