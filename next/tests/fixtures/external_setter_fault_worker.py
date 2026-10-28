"""Test-only forwarding native setter faults and unmodified binding observation."""
import hashlib,json,os,sys
from pathlib import Path
R=Path(__file__).resolve().parent;SOURCE=Path(__file__).resolve().parents[2]/'backend'
sys.path.insert(0,str(SOURCE))
import freecad_worker as W
trace=Path(os.environ['CAD_FIX_TRACE']);control=Path(os.environ['CAD_FIX_CONTROL'])
original=W.dispatch;native_rebase=W.AssemblySources.rebase_after_saveas
attempts={}
def state():
    return dict(active=None if W.App.ActiveDocument is None else W.App.ActiveDocument.Name,
        documents={n:dict(file=d.FileName,uid=str(d.Uid),undo=d.UndoCount,redo=d.RedoCount,
            objects=[dict(name=o.Name,id=o.ID,type=o.TypeId,content_sha256=hashlib.sha256(o.dumpContent()).hexdigest(),
                target=None if o.TypeId!='App::Link' or o.LinkedObject is None else dict(name=o.LinkedObject.Name,id=o.LinkedObject.ID,uid=str(o.LinkedObject.Document.Uid),file=o.LinkedObject.Document.FileName)) for o in d.Objects]) for n,d in W.App.listDocuments().items()})
def emit(value):
    with trace.open('a',encoding='utf-8') as f:f.write(json.dumps(value)+'\n')
class Link:
    def __init__(self,obj):object.__setattr__(self,'native',obj)
    def __getattr__(self,key):return getattr(self.native,key)
    def __setattr__(self,key,value):setattr(self.native,key,value)
    def setLink(self,target):
        spec=json.loads(control.read_text()) if control.exists() else {'mode':'off'}
        key=(spec.get('generation'),self.native.Name)
        should_fail=target is not None and self.native.Name in spec.get('links',[]) and (
            spec.get('mode')=='persistent' or spec.get('mode')=='once' and attempts.get(key,0)<1)
        if should_fail:
            attempts[key]=attempts.get(key,0)+1
            emit(dict(event='restoring_setLink_exception',mode=spec['mode'],link=self.native.Name,
                native_id=self.native.ID,live_target_is_none=self.native.LinkedObject is None,
                requested_target_id=target.ID,requested_target_uid=str(target.Document.Uid)))
            raise RuntimeError('injected '+spec['mode']+' restoring setLink(target): '+self.native.Name)
        return self.native.setLink(target)
class Doc:
    def __init__(self,doc):self.native=doc
    def __getattr__(self,key):return getattr(self.native,key)
    @property
    def Objects(self):return [Link(o) if o.TypeId=='App::Link' and W.AssemblySources.stored(o) is not None else o for o in self.native.Objects]
def rebase(doc,after_clear=lambda:None):return native_rebase(Doc(doc),after_clear)
W.AssemblySources.rebase_after_saveas=rebase
emit(dict(event='binding',pid=os.getpid(),freecad=list(W.App.Version()),occt=W.Part.OCC_VERSION,
    modules={n:dict(path=m.__file__,sha256=hashlib.sha256(Path(m.__file__).read_bytes()).hexdigest()) for n,m in sys.modules.copy().items() if getattr(m,'__file__',None) and Path(m.__file__).is_relative_to(SOURCE)}))
def dispatch(request):
    before=state()
    try:return original(request)
    finally:emit(dict(request=request,before=before,after=state()))
W.dispatch=dispatch;W.main()
emit(dict(event='native_eof',final=state()))
