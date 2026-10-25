"""Native Linux checkpoint ABI, exact state and shared-byte ownership controls."""
import argparse,base64,gc,hashlib,io,json,random,sys,weakref,zipfile
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'backend'))
import FreeCAD as A, Part, Sketcher
import checkpoint_native as N, sketch_checkpoints as C

def state(s):
    s.recompute()  # Refresh native Shape/ElementMap before inspecting them.
    result=dict(ids=[s.getGeometryId(i) for i in range(s.GeometryCount)],geometry=[g.Content for g in s.Geometry],
        constraints=[c.Content for c in s.Constraints],expressions=list(s.ExpressionEngine),dof=s.DoF,
        element_map={kind+str(i):s.Shape.getElementMappedName(kind+str(i))
            for kind,n in [('Edge',len(s.Shape.Edges)),('Vertex',len(s.Shape.Vertexes))] for i in range(1,n+1)})
    for prop in ['Geometry','Constraints','ExpressionEngine','ExternalGeometry','ExternalGeo']:
        with zipfile.ZipFile(io.BytesIO(s.dumpPropertyContent(prop))) as z:result[prop]=z.read('Persistence.xml').decode()
    return result

def make(name):
    d=A.newDocument(name);d.UndoMode=1;s=d.addObject('Sketcher::SketchObject','Sketch')
    s.addGeometry(Part.Circle(A.Vector(5,5,0),A.Vector(0,0,1),2));i=s.addConstraint(Sketcher.Constraint('Radius',0,2.))
    s.renameConstraint(i,'SourceRadius');s.setExpression('Constraints.SourceRadius','2 mm')
    s.addProperty('App::PropertyString','CheckpointPayload');s.CheckpointPayload=random.Random(11).randbytes(262144).hex()
    d.recompute()
    f=d.addObject('Sketcher::SketchObject','Follower');f.addExternal('Sketch','Edge1');d.recompute();s.solve()
    return d,s,f

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
    assert N.capabilities()['available'],N.capabilities()
    d,s,f=make('LinuxCheckpoint');source=state(s);external=state(f)
    d.openTransaction('A');s.addSymmetric([0],-1);s.solve();accepted=state(s)
    raw=bytes(s.dumpContent());encoded=base64.b64encode(raw).decode();(out/'shared-checkpoint.bin').write_bytes(raw)
    assert len(raw)>128*1024
    property_hash=hashlib.sha256(s.CheckpointPayload.encode()).hexdigest()
    d.abortTransaction();d.openTransaction('Session');counts=[]
    initial_history=(d.UndoCount,d.RedoCount)
    def rss_bytes():
        return int(next(line.split()[1] for line in Path('/proc/self/status').read_text().splitlines() if line.startswith('VmRSS:')))*1024
    for _ in range(8):N.restore_bytes(s,raw);s.solve()
    history=(d.UndoCount,d.RedoCount)
    assert initial_history==(0,0) and history==(1,0)  # One changed pending native Session.
    gc.collect();rss=[rss_bytes()]
    for _ in range(40):
        before=sys.getrefcount(raw);N.restore_bytes(s,raw);s.solve();after=sys.getrefcount(raw)
        assert before==after;assert state(s)==accepted;assert state(f)==external
        assert (d.UndoCount,d.RedoCount)==history and len(A.listDocuments())==1
        assert hashlib.sha256(s.CheckpointPayload.encode()).hexdigest()==property_hash
        counts.append([before,after])
        if len(counts)%8==0:gc.collect();rss.append(rss_bytes())
    assert max(rss[1:])-rss[0]<=8*1024*1024,('same-payload RSS retention',rss)
    for axis in [-2,-1,-2]:
        s.addSymmetric([0],axis);s.solve();d.abortTransaction();d.openTransaction('Session');C.restore(s,encoded);s.solve()
        assert state(s)==accepted and state(f)==external
    d.abortTransaction();s.solve();assert state(s)==source
    bad=bytes(bytearray(b'invalid native checkpoint'))
    failures=[]
    for _ in range(8):
        before=sys.getrefcount(bad)
        try:N.restore_bytes(s,bad)
        except OSError as e:message=str(e)
        else:raise AssertionError('invalid archive accepted')
        gc.collect();assert sys.getrefcount(bad)==before
        failures.append(dict(message=message,reference_balanced=True))
    class Buffer(bytearray):pass
    mutable=Buffer(raw);observed=weakref.ref(mutable)
    try:N.restore_bytes(s,mutable)
    except TypeError:pass
    else:raise AssertionError('writable buffer accepted')
    mutable.extend(b'resize');del mutable;gc.collect();assert observed() is None
    d.openTransaction('Session');C.restore(s,encoded);s.solve();d.commitTransaction();d.recompute()
    d.saveAs(str(out/'restored.FCStd'));A.closeDocument(d.Name)
    d=A.openDocument(str(out/'restored.FCStd'));d.recompute();s=d.getObject('Sketch');s.solve()
    reopened=state(s);reopened_follower=state(d.getObject('Follower'))
    (out/'reopen-state.json').write_text(json.dumps(dict(accepted=accepted,reopened=reopened,external=external,reopened_follower=reopened_follower),indent=2))
    assert reopened==accepted and reopened_follower==external,{
        name:[expected.get(name),actual.get(name)] for expected,actual in [(accepted,reopened),(external,reopened_follower)]
        for name in expected if expected[name]!=actual.get(name)}
    A.closeDocument(d.Name)
    # Independent native transactions preserve A across B, without using adapter.
    d,s,f=make('LinuxNativeControl');d.openTransaction('A');s.addSymmetric([0],-1);s.solve();d.commitTransaction();direct=state(s)
    d.openTransaction('B');s.addSymmetric([0],-2);s.solve();d.abortTransaction();s.solve()
    assert state(s)==direct==accepted;assert state(f)==external;A.closeDocument(d.Name)
    (out/'results.json').write_text(json.dumps(dict(architecture=__import__('platform').machine(),available=True,
        exact_accepted=accepted,exact_direct=direct,external=external,shared_checkpoint_sha256=hashlib.sha256(raw).hexdigest(),
        shared_checkpoint_bytes=len(raw),same_immutable_payload_restores=40,refcounts=counts,errors=failures,
        native_history_growth=0,payload_property_sha256=property_hash,mutable_rejected_resized_collected=True,
        rss_bytes=rss,rss_warmup_restores=8,rss_growth_limit_bytes=8*1024*1024,
        initial_history=initial_history,pending_session_history=history,
        fresh_reopen_exact=True,documents_closed=not bool(A.listDocuments())),indent=2)+'\n')
    print('Linux native checkpoint: exact IDs/XML/mapping/refs, 40 shared-byte restores, exceptions and reopen passed.')
if __name__=='__main__':main()
