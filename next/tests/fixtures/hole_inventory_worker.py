"""Read-only test telemetry around the unchanged guarded worker dispatch."""
import json,os,sys
from pathlib import Path
O=Path(__file__).resolve().parent
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'backend'))
import freecad_worker as W
original=W.dispatch
def inventory():
 return dict(active=W.App.ActiveDocument.Name if W.App.ActiveDocument else None,documents={n:dict(path=d.FileName,undo=d.UndoCount,redo=d.RedoCount,objects=[o.Name for o in d.Objects]) for n,d in W.App.listDocuments().items()})
def observed(request):
 before=inventory();error=None
 try:return original(request)
 except Exception as e:
  error=dict(type=type(e).__name__,code=getattr(e,'code',None),message=str(e));raise
 finally:
  path=Path(os.environ['CAD_EDITOR_ACCEPTANCE_OUTPUT'])/'inventory.jsonl'
  with path.open('a') as f:f.write(json.dumps(dict(op=request.get('op'),path=request.get('path'),before=before,after=inventory(),error=error))+'\n')
W.dispatch=observed
W.main()
