"""Passive diagnostics around the unchanged public worker request protocol."""
import json
import os
from pathlib import Path
import sys

sys.path.insert(0, os.environ["OWN_BACKEND"])
import freecad_worker as worker

folder = Path(os.environ["OWN_DIAGNOSTICS"])
folder.mkdir(exist_ok=True)
log = folder/f"{os.getpid()}.jsonl"
dispatch = worker.dispatch


def facts():
    result = dict(active=worker.App.ActiveDocument.Name if worker.App.ActiveDocument else None, documents={})
    for name, doc in worker.App.listDocuments().items():
        entry = dict(undo=doc.UndoCount, redo=doc.RedoCount, file=doc.FileName,
                     pending=doc.HasPendingTransaction)
        for key in ["Uid", "UndoNames", "RedoNames"]:
            try:
                entry[key] = str(getattr(doc, key)) if key == "Uid" else list(getattr(doc, key))
            except AttributeError:
                pass
        entry["objects"] = {obj.Name: getattr(obj, "ID", None) for obj in doc.Objects}
        result["documents"][name] = entry
    return result


def observed(request):
    event = dict(request=request, before=facts())
    try:
        result = dispatch(request)
        event["ok"] = True
        return result
    except Exception as error:
        event.update(ok=False, error=dict(code=getattr(error, "code", "native_error"), message=str(error)))
        raise
    finally:
        event["after"] = facts()
        with log.open("a", encoding="utf-8") as stream:
            stream.write(json.dumps(event)+"\n")


worker.dispatch = observed
worker.main()
