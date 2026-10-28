"""Named native faults and passive receipts for failed-load recovery tests only."""
import hashlib
import json
import os
from pathlib import Path
import sys

sys.dont_write_bytecode = True
backend = Path(os.environ.get('R7_BACKEND', str(Path(__file__).resolve().parents[2] / 'backend'))).resolve()
sys.path.insert(0, str(backend))
import freecad_worker as w

dispatch, validate, close = w.dispatch, w.validate, w.App.closeDocument
detach = w._native_suppression.detach
fault = {}
events = []
planned_opens = {}
output = Path(os.environ['R7_BINDINGS'])
output.mkdir(parents=True, exist_ok=True)

def facts():
    return dict(active=w.App.ActiveDocument.Name if w.App.ActiveDocument else None,
        native_active=w.App.activeDocument().Name if w.App.activeDocument() else None,
        documents={n:dict(uid=str(d.Uid), objects={o.Name:dict(id=o.ID, type=o.TypeId) for o in d.Objects},
            undo=d.UndoCount, redo=d.RedoCount, undo_names=list(d.UndoNames), redo_names=list(d.RedoNames),
            filename=d.FileName, revision=w.revision_request('lookup', n), lease=w.lease('metadata', n))
            for n,d in w.App.listDocuments().items()},
        observers=sorted(w._native_suppression.documents), pending=list(map(list,w._native_suppression.pending)),
        errors=list(w._native_suppression.errors), replaying=w._native_suppression.replaying)

def injected_validate(doc):
    result = validate(doc)
    if fault.pop('invalid_proxy', False):
        obj = doc.addObject('App::FeaturePython', 'BrokenGroundedJoint')
        obj.addProperty('App::PropertyLinkGlobal', 'ObjectToGround', 'Ground')
        return validate(doc)
    for name in fault.pop('dependencies', []):
        dependency = w.App.newDocument(name)
        if fault.get('unlinked_dependencies'):
            dependency.addObject('Part::Box', 'Sibling')
        else:
            dependency.addObject('App::Link', 'Follower').LinkedObject = doc.Objects[0]
        dependency.recompute()
    if fault.get('existing_dependency'):
        doc.addObject('App::Link', 'PreExistingDependency').LinkedObject = w.App.getDocument(fault['existing_dependency']).Objects[0]
    if fault.get('validate'):
        fault['validate'] -= 1
        if fault.get('validation_code'):
            w.fail(fault['validation_code'], 'r7 injected validation failure')
        raise RuntimeError('r7 injected validation failure')
    return result

def injected_close(name):
    events.append(dict(event='close', name=name, observer=name in w._native_suppression.documents))
    if fault.get('noop_close'):
        return None
    if fault.get('close') or name in fault.get('close_names', []):
        fault['close'] = max(0, fault.get('close', 0)-1)
        raise RuntimeError('r7 injected native Close failure: ' + name)
    result = close(name)
    events.append(dict(event='closed', name=name, observer=name in w._native_suppression.documents))
    return result

def observed_detach(name):
    assert name not in w.App.listDocuments(), 'observer detached before successful native Close'
    events.append(dict(event='detach', name=name))
    return detach(name)

def bound(request):
    global fault
    fault = request.pop('_r7_fault', {})
    plan_path = os.environ.get('R7_EDITOR_FAULT_PLAN')
    if plan_path and request.get('op') == 'open' and Path(plan_path).is_file():
        plan = json.loads(Path(plan_path).read_text())
        if request.get('path') == plan['path']:
            index = planned_opens.get(plan['path'], 0)
            planned_opens[plan['path']] = index + 1
            fault = plan['attempts'][index] if index < len(plan['attempts']) else {}
            if os.environ.get('R7_EDITOR_DEPENDENCIES') and fault.get('validate'):
                fault = dict(fault, dependencies=['A_EditorDependency','Z_EditorDependency'], unlinked_dependencies=True)
    injected_fault = json.loads(json.dumps(fault))
    before = facts()
    error = None
    try:
        return dispatch(request)
    except Exception as exc:
        error = str(exc)
        raise
    finally:
        events.append(dict(event='request', request=request, faults=injected_fault, before=before, after=facts(), error=error))
        modules = {}
        for name, module in list(sys.modules.items()):
            p = getattr(module, '__file__', None)
            if p and (Path(p).resolve().is_relative_to(backend) or name in ('FreeCAD','Part','Sketcher')):
                p = Path(p).resolve()
                modules[name] = dict(path=str(p), sha256=hashlib.sha256(p.read_bytes()).hexdigest())
        (output/(str(os.getpid())+'.json')).write_text(json.dumps(dict(pid=os.getpid(), ppid=os.getppid(),
            executable=sys.executable, argv=sys.argv, events=events, modules=modules), indent=2))
        fault = {}

w.validate, w.App.closeDocument, w.dispatch = injected_validate, injected_close, bound
w._native_suppression.detach = observed_detach
w.main()
