"""Test-only native checkpoint/suppression facts and named recovery faults."""
import builtins,hashlib,io,json,os,sys,zipfile
from pathlib import Path
sys.dont_write_bytecode=True
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'backend'))
import freecad_worker as W
snapshot=W.snapshot;dispatch=W.dispatch;validate=W.validate;restore=W.SketchCheckpoints.restore;close=W.App.closeDocument
remaining=0;restore_failures=0;close_failures=0;request_op=None
trace=Path(os.environ['CAD_SUPPRESSION_TRACE']);trace.parent.mkdir(parents=True,exist_ok=True)

def inventory():
    return {n:dict(uuid=str(d.Uid),undo=d.UndoCount,redo=d.RedoCount,
        undo_names=list(d.UndoNames),redo_names=list(d.RedoNames),file=d.FileName,
        objects={o.Name:o.ID for o in d.Objects}) for n,d in W.App.listDocuments().items()}

def element_map(shape):
    return {kind+str(i):shape.getElementMappedName(kind+str(i)) for kind,count in
        [('Face',len(shape.Faces)),('Edge',len(shape.Edges)),('Vertex',len(shape.Vertexes))] for i in range(1,count+1)}

def shape_fact(shape):
    return dict(element_map=element_map(shape),brep_sha256=hashlib.sha256(shape.exportBrepToString().encode()).hexdigest())

def observed_setattr(obj,field,value):
    # Inspect the actual input to the native property setter. A dynamic native
    # PartShape property adds its own tag to getter aliases; it is not the same
    # property as Feature.Shape. Native reference tests check persisted naming.
    if field==W.Suppression.SHAPE_PROPERTY:
        source=shape_fact(obj.Shape);candidate=shape_fact(value)
        exact=value.isSame(obj.Shape) and candidate==source
    elif field==W.Suppression.PROPERTY:
        source=W.encode(obj.Placement);candidate=W.encode(value);exact=candidate==source
    else:return builtins.setattr(obj,field,value)
    result=builtins.setattr(obj,field,value)
    with trace.open('a',encoding='utf-8') as f:f.write(json.dumps(dict(event='suppression_capture',object=obj.Name,field=field,
        exact_native_input=exact,source=source,candidate=candidate,
        stored=shape_fact(getattr(obj,field)) if field==W.Suppression.SHAPE_PROPERTY else W.encode(getattr(obj,field))))+'\n')
    if not exact:W._native_suppression.remember_error('test oracle: capture input differs from authoritative native feature')
    return result
W.Suppression.setattr=observed_setattr

def observed_snapshot(doc,revision=None):
    state=snapshot(doc,revision)
    state['native_inventory']=inventory()
    state['suppression_managed_documents']=sorted(W._native_suppression.documents)
    for entry in state['objects']:
        obj=doc.getObject(entry['name'])
        if 'sketch' in entry:
            facts=dict(ids=[obj.getGeometryId(i) for i in range(obj.GeometryCount)],
                geometry=[g.Content for g in obj.Geometry],constraints=[c.Content for c in obj.Constraints],
                expressions=list(obj.ExpressionEngine),externals=[g.Content for g in obj.ExternalGeo],element_map=element_map(obj.Shape))
            for prop in ['Geometry','Constraints','ExpressionEngine']:
                with zipfile.ZipFile(io.BytesIO(obj.dumpPropertyContent(prop))) as archive:facts[prop]=archive.read('Persistence.xml').decode('utf-8')
            entry['sketch']['native_xml']=facts
        if obj.isDerivedFrom('PartDesign::Feature'):
            entry['native_suppression_map']=element_map(obj.Shape)
            if W.Suppression.SHAPE_PROPERTY in obj.PropertiesList:
                entry['native_capture_map']=element_map(getattr(obj,W.Suppression.SHAPE_PROPERTY))
    return state

def observed_restore(sketch,payload):
    global restore_failures
    with trace.open('a',encoding='utf-8') as f:f.write(json.dumps(dict(event='checkpoint_restore',op=request_op,history_depth=W._native_suppression.replaying,
        uuid=str(sketch.Document.Uid),object_id=sketch.ID,active_preview=sketch.Document.Name in W._native_previews))+'\n')
    assert W._native_suppression.replaying>0,'checkpoint restore must suspend observer authoring'
    result=restore(sketch,payload)
    if restore_failures:
        restore_failures-=1
        raise RuntimeError('suppression test: fault after native checkpoint restore')
    return result

def observed_validate(*args,**kw):
    global remaining
    result=validate(*args,**kw)
    if remaining:
        remaining-=1
        raise RuntimeError('suppression test: fault after native validation')
    return result

def observed_close(name):
    global close_failures
    if close_failures:
        close_failures-=1
        raise RuntimeError('suppression test: native Close failed once')
    return close(name)

def observed_dispatch(request):
    global remaining,restore_failures,close_failures,request_op
    request_op=request.get('op');remaining=request.pop('suppression_validate_failures',0)
    close_failures=request.pop('suppression_close_failures',0)
    restore_failures=request.pop('suppression_restore_failures',0);before=inventory();error=None
    try:return dispatch(request)
    except Exception as e:error=str(e);raise
    finally:
        with trace.open('a',encoding='utf-8') as f:f.write(json.dumps(dict(event='dispatch',op=request_op,error=error,
            before=before,after=inventory(),history_depth=W._native_suppression.replaying,
            callback_errors=list(W._native_suppression.errors)))+'\n')
        remaining=0;restore_failures=0;close_failures=0

W.snapshot=observed_snapshot;W.validate=observed_validate;W.dispatch=observed_dispatch;W.SketchCheckpoints.restore=observed_restore;W.App.closeDocument=observed_close
W.main()
