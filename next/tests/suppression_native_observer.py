"""Reviewer-owned native checks; use the extracted frozen module without edits."""
import hashlib, json, pathlib, sys, traceback
import FreeCAD as App
import Part
import PartDesign

here=pathlib.Path(__file__).resolve().parent
sys.path.insert(0,str(here.parent/'backend'))
import suppression as S

out=pathlib.Path(sys.argv[1]);out.mkdir()
observer=S.register();report={}
def digest(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
(out/'loaded-binding.json').write_text(json.dumps({
    'version':list(App.Version()),'occt':Part.OCC_VERSION,
    'modules':{m.__name__:{'path':m.__file__,'sha256':digest(m.__file__)} for m in [App,Part,PartDesign,S]}},indent=2))

def create(name):
    d=App.newDocument(name);d.UndoMode=1;observer.attach(d)
    b=d.addObject('PartDesign::Body','Body')
    for key,size,pos in [('Base',[10,10,10],[0,0,0]),('A',[2,2,4],[1,1,10]),('B',[3,3,3],[6,6,10])]:
        a=b.newObject('PartDesign::AdditiveBox',key)
        a.Length,a.Width,a.Height=size;a.Placement.Base=App.Vector(*pos);d.recompute()
    check(d);return d
def check(d):
    assert d.recompute() is not False
    observer.check()
    for obj in d.Objects:
        assert obj.isValid(),(obj.Name,obj.getStatusString())
    assert not observer.pending,observer.pending
    assert not observer.errors
def state(d):
    return {o.Name:{'suppressed':bool(o.Suppressed),'placement':list(o.Placement.Base)+list(o.Placement.Rotation.Q),
                   'capture':S.PROPERTY in o.PropertiesList,'map':S.SHAPE_PROPERTY in o.PropertiesList,
                   'volume':o.Shape.Volume,'expressions':list(o.ExpressionEngine)}
            for o in [d.Base,d.A,d.B]}
def same_shape(a,b):
    delta=a.cut(b).Volume+b.cut(a).Volume
    assert abs(delta)<1e-9,delta
def close(d):observer.detach(d);App.closeDocument(d.Name)
def case(name,fn):
    try:report[name]={'passed':True,'result':fn()}
    except Exception:
        report[name]={'passed':False,'traceback':traceback.format_exc()}
        traceback.print_exc()
    (out/'report.json').write_text(json.dumps(report,indent=2))

def batch():
    d=create('Batch');baseline=state(d);shape=d.Body.Shape.copy()
    d.openTransaction('Many transitions in one batch')
    d.A.Suppressed=True;d.A.Suppressed=False;d.A.Height=6
    d.A.Suppressed=True;d.B.Suppressed=True;d.B.Suppressed=False
    check(d);d.commitTransaction();suppressed=state(d)
    assert d.A.Suppressed and not d.B.Suppressed
    d.openTransaction('Unsuppress updated input');d.A.Suppressed=False;check(d);d.commitTransaction()
    assert abs(d.Body.Shape.Volume-1051)<1e-8,d.Body.Shape.Volume
    final=state(d)
    with observer.history():d.undo()
    check(d);assert state(d)==suppressed
    with observer.history():d.undo()
    check(d);assert state(d)==baseline;same_shape(d.Body.Shape,shape)
    with observer.history():d.redo()
    check(d);assert state(d)==suppressed
    with observer.history():d.redo()
    check(d);assert state(d)==final
    close(d);return 'four transitions, two features, edited input, exact two-step history'

def deletion():
    d=create('DeleteReuse');d.openTransaction('Suppress deleted tip');d.B.Suppressed=True;check(d);d.commitTransaction()
    suppressed=state(d)
    d.openTransaction('Delete and replace same native name')
    d.removeObject('B');b=d.Body.newObject('PartDesign::AdditiveBox','B')
    b.Length=4;b.Width=3;b.Height=3;b.Placement.Base=App.Vector(6,6,10)
    check(d);d.commitTransaction()
    assert b.Name=='B' and not b.Suppressed
    assert S.PROPERTY not in b.PropertiesList and S.SHAPE_PROPERTY not in b.PropertiesList
    assert abs(d.Body.Shape.Volume-1052)<1e-8,d.Body.Shape.Volume
    replacement=state(d)
    with observer.history():d.undo()
    check(d);assert state(d)==suppressed
    with observer.history():d.redo()
    check(d);assert state(d)==replacement
    d.openTransaction('Suppress replacement');d.B.Suppressed=True;check(d);d.commitTransaction()
    assert list(getattr(d.B,S.PROPERTY).Base)==[6.,6.,10.]
    close(d)
    d=create('DeleteReuse');assert not observer.pending
    close(d);return 'same-name replacement plus document-name reuse has no state inheritance'

def fault():
    d=create('ChangedCallbackFault');before=state(d);shape=d.Body.Shape.copy();undo=d.UndoCount
    class BrokenPending(dict):
        def pop(self,*args):raise RuntimeError('review injected after-change pop failure')
    observer.pending=BrokenPending()
    d.openTransaction('Fail after before-change capture');d.A.Suppressed=True
    try:observer.check()
    except RuntimeError as e:assert 'review injected' in str(e)
    else:raise AssertionError('callback error did not drain')
    # Retain the pending entry as production code does, rather than resetting it.
    observer.pending=dict(observer.pending)
    with observer.history():d.abortTransaction()
    assert d.recompute() is not False;observer.check()
    assert state(d)==before and d.UndoCount==undo;same_shape(d.Body.Shape,shape)
    pending_after_abort=list(observer.pending)
    d.openTransaction('Correct next toggle');d.A.Suppressed=True;check(d);d.commitTransaction()
    assert list(getattr(d.A,S.PROPERTY).Base)==[1.,1.,10.]
    close(d);assert not observer.pending
    return {'pending_after_injected_fault_abort':pending_after_abort,'recovery':True}

def collisions():
    for key,kind in [(S.PROPERTY,'App::PropertyString'),(S.SHAPE_PROPERTY,'App::PropertyString')]:
        d=create('Collision');d.A.addProperty(kind,key);setattr(d.A,key,'review foreign value')
        before=state(d);undo=d.UndoCount
        assert not S.metadata(d.A)['suppression']['can_toggle']
        d.openTransaction('Reject foreign type');d.A.Suppressed=True;d.recompute()
        try:observer.check()
        except RuntimeError as e:assert 'collision' in str(e)
        else:raise AssertionError('accepted collision')
        with observer.history():d.abortTransaction()
        check(d);assert state(d)==before and getattr(d.A,key)=='review foreign value' and d.UndoCount==undo
        close(d)
    unmanaged=App.newDocument('Unmanaged');b=unmanaged.addObject('PartDesign::Body','Body')
    a=b.newObject('PartDesign::AdditiveBox','A');unmanaged.recompute();a.Suppressed=True;unmanaged.recompute()
    assert S.PROPERTY not in a.PropertiesList and S.SHAPE_PROPERTY not in a.PropertiesList
    App.closeDocument(unmanaged.Name)
    return 'both property-type collisions abort; unmanaged document untouched by observer'

try:
    for name,fn in [('batch',batch),('delete_replace_lifetime',deletion),('after_callback_fault',fault),('collision_and_unmanaged',collisions)]:
        case(name,fn)
finally:
    App.removeDocumentObserver(observer)
    for name in list(App.listDocuments()):App.closeDocument(name)
print(json.dumps(report,indent=2))
raise SystemExit(0 if all(x['passed'] for x in report.values()) else 1)
