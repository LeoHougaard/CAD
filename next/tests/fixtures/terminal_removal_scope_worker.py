"""Native fixtures and passive call receipts for terminal removal scope tests."""
import os
from pathlib import Path

base = Path(os.environ['R7_BACKEND']).parent / 'tests/fixtures/open_cleanup_dependency_worker.py'
body = base.read_text()
needle = "        if current=='_review_fixture':return native_fixture(req['folder'],req.get('malformed',False))"
extra = '''        if current=='_scope_saved':
            kind=req['kind'];target=w.App.getDocument(req['document']).getObject(req.get('object','Box')) if kind!='ordinary' else None
            doc=w.App.newDocument('ScopeNative');box=doc.addObject('Part::Box','Box');doc.recompute()
            path=Path(req['path']);doc.saveAs(str(path))
            if kind in ('mixed','ordinary'):
                box.addProperty('App::PropertyXLink','NativeReference');box.NativeReference=target
            if kind!='ordinary':
                obj=recovery_link(doc,target)
                if kind=='array':obj.ElementCount=3
            if kind=='mixed':doc.addObject('App::Link','LocalLink').LinkedObject=doc.addObject('Part::Box','LocalBox')
            doc.recompute();doc.saveAs(str(path));close(doc.Name)
            return dict(path=str(path))
        if current=='_scope_trace':return [e for e in events if e.get('event')=='scope_native_call']
        if current=='_scope_lease':
            w.lease(req['action'],req['document'],**req.get('parameters',{}));return facts()
        if current=='_scope_active':w.restore_active_document(req['document']);return facts()
        if current=='_scope_touched_cycle':
            doc=w.App.getDocument(req['document']);obj=doc.Box
            obj.addProperty('App::PropertyXLink','Reverse');obj.Reverse=w.App.getDocument(req['target']).Box
            obj.touch();return facts()
'''
assert body.count(needle) == 1
body = body.replace(needle, extra + needle)
tracing = '''
def scope_trace(kind, original):
    def observed(doc, *args, **kwargs):
        if current=='transaction':
            item=dict(event='scope_native_call',phase=kind,document=doc.Name,
                native_pending=doc.HasPendingTransaction,
                transaction=w.App.getActiveTransaction(),
                active=w.App.activeDocument().Name if w.App.activeDocument() else None)
            if kind=='transaction':item['terminal_recovery']=args[1] if len(args)>1 else kwargs.get('terminal_recovery',False)
            events.append(item)
        return original(doc,*args,**kwargs)
    return observed
for key in ('transaction','apply_edit','recompute_document','validate','publish_state'):
    setattr(w,key,scope_trace(key,getattr(w,key)))
'''
assert body.count('w.main()') == 1
exec(compile(body.replace('w.main()', tracing + '\nw.main()'), str(base), 'exec'), globals())
