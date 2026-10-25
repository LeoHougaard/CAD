"""Independent native controls for final observer, placement formulas and faults."""
import json
from pathlib import Path
import sys
import FreeCAD as App
import Part

here=Path(__file__).resolve().parent
sys.path.insert(0,str(here.parent/'backend'))
from suppression import register, PROPERTY, metadata

out=Path(sys.argv[1]);out.mkdir()
observer=register();report={}


def create(name,repaired):
    d=App.newDocument(name);d.UndoMode=1
    if repaired:observer.attach(d)
    b=d.addObject('PartDesign::Body','Body')
    for name,size,pos in [('Base',[10,10,10],[0,0,0]),('A',[2,2,4],[1,1,10]),('B',[3,3,3],[6,6,10])]:
        a=b.newObject('PartDesign::AdditiveBox',name)
        a.Length,a.Width,a.Height=size;a.Placement.Base=App.Vector(*pos);d.recompute()
    return d


def check(d):
    d.recompute();observer.check()
    assert d.Body.isValid() and d.A.isValid()


try:
    reference=None
    for repaired in [False,True]:
        d=create('PlacementFormula'+str(repaired),repaired)
        d.A.setExpression('Placement.Base.x','Base.Length / 10');check(d)
        expression_before=list(d.A.ExpressionEngine)
        report['native_expression_contract']=expression_before
        d.openTransaction('Suppress');d.A.Suppressed=True;check(d);d.commitTransaction()
        d.openTransaction('Parent edit');d.Base.Length=20;check(d);d.commitTransaction()
        if repaired:
            path=out/'placement-formula.FCStd';d.saveAs(str(path));observer.detach(d);App.closeDocument(d.Name)
            d=App.openDocument(str(path));observer.attach(d);d.UndoMode=1
        d.openTransaction('Unsuppress');d.A.Suppressed=False;check(d);d.commitTransaction()
        assert list(d.A.Placement.Base)==[2.,1.,10.],d.A.Placement
        assert list(d.A.ExpressionEngine)==expression_before, (d.A.ExpressionEngine,expression_before)
        if repaired:
            difference=d.Body.Shape.cut(reference).Volume+reference.cut(d.Body.Shape).Volume
            assert difference==0,difference
            report['placement_expression_reopen_native_difference']=difference
        else:reference=d.Body.Shape.copy()
        observer.detach(d);App.closeDocument(d.Name)

    d=create('CaptureCollision',True);before=d.Body.Shape.copy();undo=d.UndoCount
    d.A.addProperty('App::PropertyString',PROPERTY);setattr(d.A,PROPERTY,'foreign content')
    assert metadata(d.A)['suppression']['can_toggle'] is False
    d.openTransaction('Rejected collision');d.A.Suppressed=True;d.recompute()
    try:observer.check()
    except RuntimeError as error:assert 'collision' in str(error)
    else:raise AssertionError('type collision was accepted')
    with observer.history():d.abortTransaction()
    check(d)
    assert not d.A.Suppressed and getattr(d.A,PROPERTY)=='foreign content'
    assert d.UndoCount==undo
    assert d.Body.Shape.cut(before).Volume+before.cut(d.Body.Shape).Volume==0
    d.A.removeProperty(PROPERTY)
    observer.detach(d);App.closeDocument(d.Name)
    report['native_collision_error_and_abort_exact']=True

    d=create('CallbackFailure',True);before=d.Body.Shape.copy();undo=d.UndoCount
    class BrokenPending(dict):
        def __setitem__(self,key,value):raise RuntimeError('injected capture failure')
    observer.pending=BrokenPending()
    d.openTransaction('Failed native callback');d.A.Suppressed=True;d.recompute()
    try:observer.check()
    except RuntimeError as error:assert 'injected capture failure' in str(error)
    else:raise AssertionError('native callback failure was silently accepted')
    observer.pending={}
    with observer.history():d.abortTransaction()
    check(d)
    assert not d.A.Suppressed and PROPERTY not in d.A.PropertiesList
    assert d.UndoCount==undo
    assert d.Body.Shape.cut(before).Volume+before.cut(d.Body.Shape).Volume==0
    d.openTransaction('Corrected capture');d.A.Suppressed=True;check(d);d.commitTransaction()
    assert list(getattr(d.A,PROPERTY).Base)==[1.,1.,10.]
    report['before_callback_failure_abort_and_recovery']=True
    observer.detach(d);App.closeDocument(d.Name)

    (out/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
finally:
    App.removeDocumentObserver(observer)
    for key in list(App.listDocuments()):App.closeDocument(key)
