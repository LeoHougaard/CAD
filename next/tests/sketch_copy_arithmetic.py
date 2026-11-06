"""Native-value admission spies at unsafe extremes; bounded actual native actions."""
import hashlib
import io
import json
import os
from pathlib import Path
import sys
import zipfile

import FreeCAD as App
import Part

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'backend'))
import sketch_operations as O
import sketch_geometry as G

out=Path(sys.argv[1]); out.mkdir(parents=True,exist_ok=False); report={'version':list(App.Version()),'invalid':[],'finite_spies':[],'native':[]}

def prop(sketch,name):
    with zipfile.ZipFile(io.BytesIO(sketch.dumpPropertyContent(name))) as z:
        return z.read('Persistence.xml').decode()

def facts(sketch):
    return {'properties':{p:prop(sketch,p) for p in ('Geometry','Constraints','ExpressionEngine')},
        'ids':[sketch.getGeometryId(i) for i in range(sketch.GeometryCount)],
        'construction':[sketch.getConstruction(i) for i in range(sketch.GeometryCount)],
        'undo':sketch.Document.UndoCount,'redo':sketch.Document.RedoCount}

class Spy:
    def __init__(self,source,geometry):
        self.source=source; self.geometries=geometry; self.calls=[]
    def __getattr__(self,name): return getattr(self.source,name)
    @property
    def Geometry(self): return self.geometries
    @Geometry.setter
    def Geometry(self,value): self.calls.append('Geometry registration')
    def addCopy(self,*args): self.calls.append('addCopy')
    def addMove(self,*args): self.calls.append('addMove')
    def addRectangularArray(self,*args): self.calls.append('addRectangularArray')
    def getPoint(self,index,point):
        geo=self.geometries[index]
        if isinstance(geo,Part.Point): return App.Vector(geo.X,geo.Y,geo.Z)
        return self.source.getPoint(index,point)

doc=App.newDocument('Admission');doc.UndoMode=1
try:
    sketch=doc.addObject('Sketcher::SketchObject','Sketch')
    sketch.addGeometry(Part.Point(App.Vector(1,2,0)),True);assert sketch.solve()==0;doc.recompute()
    path=out/'source.FCStd';doc.saveAs(str(path));source_hash=hashlib.sha256(path.read_bytes()).hexdigest()
    baseline=facts(sketch)
    def case(name,location,extra,valid=False):
        geometry=G.restore_point_extensions(sketch,'Geometry',sketch.Geometry)
        geometry[0].X,geometry[0].Y,geometry[0].Z=location
        content=[bytes(g.dumpContent()).hex() for g in geometry]
        spy=Spy(sketch,geometry)
        edit=dict(op='copy_geometry',object='Sketch',indices=[0],displacement=[1e308,0,0]);edit.update(extra)
        error=None
        try: O.apply(spy,edit)
        except O.OperationError as e:error=str(e)
        assert bool(spy.calls)==valid,(name,error,spy.calls)
        assert (error is None)==valid,(name,error)
        assert [bytes(g.dumpContent()).hex() for g in geometry]==content
        assert facts(sketch)==baseline
        assert hashlib.sha256(path.read_bytes()).hexdigest()==source_hash
        item=dict(name=name,source=location,edit=edit,error=error,calls=spy.calls,
            native_action_called=bool(spy.calls),source_native_content_unchanged=True,
            geometry_metadata_history_unchanged=True,source_file_unchanged=True)
        report['finite_spies' if valid else 'invalid'].append(item)
    array=dict(op='rectangular_array',columns=2,rows=2,clone=False,constrain_displacement=False,perpendicular_scale=1)
    case('source plus offset overflow',(1e308,0,0),{})
    case('Move before registration overflow',(1e308,0,0),dict(op='move_geometries'))
    case('column plus row composed overflow',(0,0,0),dict(array,displacement=[1e308,1e308,0]))
    case('left intermediate overflows before cancelling row',(1e308,0,0),dict(array,displacement=[1e308,-1e308,0]))
    case('perpendicular overflow even with one row',(0,0,0),dict(array,rows=1,displacement=[2,2,0],perpendicular_scale=1e308))
    case('column product overflow',(0,0,0),dict(array,columns=3,rows=1,displacement=[1e308,0,0]))
    case('required column native length overflow',(0,0,0),dict(array,rows=1,displacement=[1e155,0,0],constrain_displacement=True))
    case('required row native length overflow',(0,0,0),dict(array,displacement=[1,0,0],constrain_displacement=True,perpendicular_scale=1e155))
    case('required native length underflow',(0,0,0),dict(array,rows=1,displacement=[1e-200,0,0],constrain_displacement=True))
    case('finite large opposite signs',(-1e308,1e308,0),dict(displacement=[1e308,-1e308,0]),True)
    case('finite large translated point',(0,0,0),{},True)
    case('finite large array sums',(0,0,0),dict(array,displacement=[5e307,5e307,0]),True)
    case('finite mixed sign array sums',(0,0,0),dict(array,columns=3,displacement=[-4e307,4e307,0],perpendicular_scale=-0.5),True)
    case('finite tiny unconstrained',(1e-308,-1e-308,0),dict(array,displacement=[1e-308,-1e-308,0],perpendicular_scale=-0.5),True)
    case('zero scale single row constrained',(1,2,0),dict(array,rows=1,displacement=[4,-3,0],perpendicular_scale=0,constrain_displacement=True),True)
    report['unchanged_source_sha256']=source_hash
finally: App.closeDocument(doc.Name)

# Real native operations stay at bounded, well-behaved coordinates. No unsafe
# extreme above is sent to Sketcher, OCC segment construction or the solver.
for label,position,delta in [('large safe',[1e6,-1e6,0],[1e6,-1e6,0]),
    ('small safe',[1e-6,-1e-6,0],[1e-6,1e-6,0]),('mixed signs',[10,-20,0],[-10,20,0])]:
    doc=App.newDocument('SafePoint');doc.UndoMode=1
    try:
        s=doc.addObject('Sketcher::SketchObject','Sketch');s.addGeometry(Part.Point(App.Vector(*position)),True)
        assert s.solve()==0;doc.recompute();before=facts(s)
        doc.openTransaction('Safe native boundary')
        O.apply(s,dict(op='copy_geometry',object='Sketch',indices=[0],displacement=delta))
        assert s.solve()==0;doc.recompute();doc.commitTransaction()
        point=s.getPoint(1,1);assert [point.x,point.y,point.z]==[a+b for a,b in zip(position,delta)]
        doc.undo();assert s.solve()==0;doc.recompute()
        restored=facts(s);assert restored['properties']==before['properties'] and restored['ids']==before['ids']
        report['native'].append(dict(name=label,point=[point.x,point.y,point.z],undo_exact=True))
    finally: App.closeDocument(doc.Name)
(out/'report.json').write_text(json.dumps(report,indent=2))
print(json.dumps(dict(invalid=len(report['invalid']),finite_spies=len(report['finite_spies']),safe_native=len(report['native']))))

report=json.loads((out/'report.json').read_text());report['new_independent']=[]
class Spy:
    Name='Spy';ConstraintCount=0;Constraints=[]
    def __init__(self,geo):self.geos=[geo];self.calls=[];self.GeometryCount=1
    def isDerivedFrom(self,t):return True
    @property
    def Geometry(self):return self.geos
    @Geometry.setter
    def Geometry(self,v):self.calls.append('registration')
    def getPoint(self,i,p):
        g=self.geos[i]
        if isinstance(g,Part.Point):return App.Vector(g.X,g.Y,g.Z)
        return g.Center if p==3 else g.StartPoint
    def addCopy(self,*a):self.calls.append('copy')
    def addMove(self,*a):self.calls.append('move')
    def addRectangularArray(self,*a):self.calls.append('array')
def probe(name,geo,extra,valid):
    spy=Spy(geo);before=bytes(geo.dumpContent())
    edit=dict(op='rectangular_array',object='Spy',indices=[0],columns=2,rows=2,
              clone=False,constrain_displacement=False,perpendicular_scale=1,displacement=[4,-3,0])
    edit.update(extra);error=None
    try:O.apply(spy,edit)
    except O.OperationError as e:error=str(e)
    item=dict(name=name,edit=edit,error=error,calls=spy.calls,expected_admission=valid,geometry_unchanged=bytes(geo.dumpContent())==before)
    report['new_independent'].append(item)
    (out/'report.json').write_text(json.dumps(report,indent=2))
    assert bool(spy.calls)==valid and (error is None)==valid,item
    assert item['geometry_unchanged']
point=lambda x=0,y=0:Part.Point(App.Vector(x,y,0))
probe('row product overflow',point(),dict(rows=3,displacement=[0,1e308,0]),False)
probe('negative scale perpendicular preproduct overflow one row',point(),dict(rows=1,displacement=[2,0,0],perpendicular_scale=-1e308),False)
probe('finite final value with overflowing left intermediate',point(1e308),dict(displacement=[1e308,-1e308,0]),False)
probe('positive constrained separation lost to rounding',point(1e20),dict(rows=1,displacement=[1,0,0],constrain_displacement=True),False)
probe('translated line endpoints collapse',Part.LineSegment(App.Vector(0,0,0),App.Vector(1,0,0)),dict(rows=1,displacement=[1e20,0,0]),False)
probe('native squared length underflow threshold',point(),dict(rows=1,displacement=[1e-162,0,0],constrain_displacement=True),False)
probe('finite native squared length above underflow',point(),dict(rows=1,displacement=[1e-161,0,0],constrain_displacement=True),True)
probe('smallest subnormal unconstrained',point(),dict(displacement=[5e-324,0,0],perpendicular_scale=-1),True)
probe('one row finite huge unused negative perpendicular',point(),dict(rows=1,displacement=[1e154,0,0],constrain_displacement=True,perpendicular_scale=-1e154),True)
probe('negative scale exact cancellation',point(-1e308,0),dict(displacement=[1e308,0,0],perpendicular_scale=-0.5),True)
probe('circle center overflow',Part.Circle(App.Vector(0,0,0),App.Vector(0,0,1),2),dict(rows=3,displacement=[0,1e308,0]),False)
print(json.dumps(dict(new_independent=len(report['new_independent']),all_pass=True)))
