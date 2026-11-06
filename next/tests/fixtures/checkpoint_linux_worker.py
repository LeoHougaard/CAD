"""Test-only exact native inspection and corrupt-checkpoint fault injection."""
import io,json,os,sys,zipfile
from pathlib import Path
sys.dont_write_bytecode=True
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'backend'))
import freecad_worker as W
original_snapshot=W.snapshot;original_dispatch=W.dispatch;capture=W.SketchCheckpoints.capture
def snapshot(doc,revision=None):
    result=original_snapshot(doc,revision)
    for entry in result['objects']:
        if 'sketch' not in entry:continue
        s=doc.getObject(entry['name'])
        info=dict(ids=[s.getGeometryId(i) for i in range(s.GeometryCount)],geometry=[g.Content for g in s.Geometry],
            constraints=[c.Content for c in s.Constraints],expressions=list(s.ExpressionEngine),dof=s.DoF,
            element_map={kind+str(i):s.Shape.getElementMappedName(kind+str(i))
                for kind,n in [('Edge',len(s.Shape.Edges)),('Vertex',len(s.Shape.Vertexes))] for i in range(1,n+1)})
        for prop in ['Geometry','Constraints','ExpressionEngine','ExternalGeometry','ExternalGeo']:
            with zipfile.ZipFile(io.BytesIO(s.dumpPropertyContent(prop))) as z:info[prop]=z.read('Persistence.xml').decode()
        entry['sketch']['native_checkpoint_state']=info
    return result
def dispatch(request):
    fault=request.pop('checkpoint_fault',None)
    if fault=='native_archive':W.SketchCheckpoints.capture=lambda s:'ZGFtYWdlZCBhcmNoaXZlIGJ5dGVz'
    if fault=='base64':W.SketchCheckpoints.capture=lambda s:'@@@'
    try:return original_dispatch(request)
    finally:W.SketchCheckpoints.capture=capture
W.snapshot=snapshot;W.dispatch=dispatch;W.main()
