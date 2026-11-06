"""Focused native checks for the new nine-branch admission and reference reads."""
import hashlib
import io
import json
import os
from pathlib import Path
import sys
import zipfile

import FreeCAD as App
import Part

here=Path(__file__).resolve().parent; out=Path(sys.argv[1])
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'backend'))
import sketch_operations as O
import sketch_geometry as G

def facts(sketch):
    result={}
    for prop in ('Geometry','Constraints','ExpressionEngine'):
        with zipfile.ZipFile(io.BytesIO(sketch.dumpPropertyContent(prop))) as z: result[prop]=z.read('Persistence.xml').decode()
    result['ids']=[sketch.getGeometryId(i) for i in range(sketch.GeometryCount)]
    result['dof']=sketch.DoF
    return result

out.mkdir(parents=True,exist_ok=False)
from sketch_copy_fixture import setup
source=out/'nine-types.FCStd'
d=App.newDocument('NineTypes');setup(d);d.saveAs(str(source));App.closeDocument(d.Name)
before_hash=hashlib.sha256(source.read_bytes()).hexdigest()
doc=App.openDocument(str(source)); report={'checks':[]}
try:
    s=doc.Sketch;assert s.solve()==0;doc.recompute();doc.UndoMode=1
    before=facts(s); count=s.GeometryCount
    doc.openTransaction('Nine native branches')
    O.apply(s,dict(op='copy_geometry',object=s.Name,all_geometry=True,indices=[],displacement=[3,-4,0]))
    assert s.solve()==0;doc.recompute();doc.commitTransaction();after=facts(s)
    assert s.GeometryCount==2*count
    kinds=sorted({g.TypeId for g in s.Geometry});assert len(kinds)==9
    doc.undo();assert s.solve()==0;doc.recompute();assert facts(s)==before
    doc.redo();assert s.solve()==0;doc.recompute();assert facts(s)==after
    report['checks'].append(dict(name='all nine native coordinate branches',types=kinds,undo_redo_exact=True))
finally:App.closeDocument(doc.Name)
assert hashlib.sha256(source.read_bytes()).hexdigest()==before_hash
report['original_fixture_unchanged']=before_hash

for kind in ['reversed_arc','spline','periodic_rational_spline']:
    doc=App.newDocument('NativeReference');doc.UndoMode=1
    try:
        s=doc.addObject('Sketcher::SketchObject','Sketch')
        if kind=='reversed_arc':
            curve=Part.ArcOfCircle(Part.Circle(App.Vector(1,2,0),App.Vector(0,0,-1),2),0.2,1.4)
        elif kind=='spline':
            curve=G.make_geometry(dict(kind='bspline',poles=[[1,2,0],[3,4,0],[6,2,0]],degree=2))
        else:
            curve=G.make_geometry(dict(kind='bspline',poles=[[1,2,0],[3,5,0],[6,3,0],[4,0,0]],
                degree=2,periodic=True,weights=[1,0.8,1.2,0.9]))
        s.addGeometry(curve,False);assert s.solve()==0;doc.recompute()
        before=facts(s);indices=[0];edit=dict(op='rectangular_array',object=s.Name,indices=indices,
            displacement=[7,-3,0],columns=2,rows=2,clone=False,constrain_displacement=True,perpendicular_scale=0.75)
        doc.openTransaction('Adapter reference');O.apply(s,edit);adapter_before_solve=facts(s)
        assert s.solve()==0;doc.recompute();doc.commitTransaction();adapter=facts(s)
        doc.undo();assert s.solve()==0;doc.recompute();assert facts(s)==before
        # Undo retains native identity allocation history. Use a fresh source
        # for the matched direct control, as the original matched suite does.
        App.closeDocument(doc.Name);doc=App.newDocument('DirectReference');doc.UndoMode=1
        s=doc.addObject('Sketcher::SketchObject','Sketch');s.addGeometry(curve,False)
        assert s.solve()==0;doc.recompute()
        doc.openTransaction('Direct reference')
        s.addRectangularArray(indices,App.Vector(7,-3,0),False,2,2,True,0.75)
        direct_before_solve=facts(s)
        assert s.solve()==0;doc.recompute();doc.commitTransaction();direct=facts(s)
        phases=dict(adapter_before_solve=adapter_before_solve,direct_before_solve=direct_before_solve,
            adapter_solved=adapter,direct_solved=direct)
        (out/(kind+'-comparison.json')).write_text(json.dumps(phases,indent=2))
        assert adapter_before_solve==direct_before_solve
        report['checks'].append(dict(name=kind+' native reference after center/poles',
            exact_before_solve=True,solved_properties_equal=adapter==direct,phases=phases))
    finally:App.closeDocument(doc.Name)
(out/'report.json').write_text(json.dumps(report,indent=2))
print(json.dumps(dict(focused_native_checks=len(report['checks']),source_unchanged=True)))
