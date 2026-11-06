"""New bounded native corners. Never writes into the frozen bundle."""
import io,json,os,sys,zipfile,math,traceback
from pathlib import Path
import FreeCAD as A
import Part,Sketcher
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'backend'))
import sketch_operations as O
out=Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=False); report={'version':list(A.Version()),'cases':[],'admission':[]}
def xml(o,p):
    with zipfile.ZipFile(io.BytesIO(o.dumpPropertyContent(p))) as z:
        return z.read('Persistence.xml').decode()
def facts(d):
    return {s.Name:dict(properties={p:xml(s,p) for p in ('Geometry','Constraints','ExpressionEngine','ExternalGeo')},
        ids=[s.getGeometryId(i) for i in range(s.GeometryCount)],
        construction=[s.getConstruction(i) for i in range(s.GeometryCount)],dof=s.DoF,
        externals=[(ob.Name,list(ns)) for ob,ns in s.ExternalGeometry],
        maps={kind+str(i):s.Shape.getElementMappedName(kind+str(i))
            for kind,count in [('Edge',len(s.Shape.Edges)),('Vertex',len(s.Shape.Vertexes))]
            for i in range(1,count+1)}) for s in d.Objects if s.TypeId=='Sketcher::SketchObject'}
def settle(d):
    for s in d.Objects:
        if s.TypeId=='Sketcher::SketchObject':assert s.solve()==0,(s.Name,s.SolverMessages if hasattr(s,'SolverMessages') else s.State)
    d.recompute()
def coupled(name):
    d=A.newDocument(name);d.UndoMode=1
    s=d.addObject('Sketcher::SketchObject','Sketch')
    s.addGeometry(Part.LineSegment(A.Vector(0,0,0),A.Vector(10,0,0)),False)
    s.addGeometry(Part.Point(A.Vector(0,0,0)),True)
    s.addGeometry(Part.Point(A.Vector(0,0,0)),False)
    s.addConstraint(Sketcher.Constraint('Distance',0,10.0))
    s.renameConstraint(0,'SourceLength');s.setExpression('Constraints.SourceLength','10 mm')
    s.addConstraint(Sketcher.Constraint('Coincident',1,1,0,1))
    settle(d)
    f=d.addObject('Sketcher::SketchObject','Follower')
    f.addExternal('Sketch','Edge1');f.addExternal('Sketch','Vertex1')
    settle(d)
    return d,s
def record(name,fn):
    try:
        details=fn();report['cases'].append(dict(name=name,ok=True,details=details))
    except Exception:
        report['cases'].append(dict(name=name,ok=False,error=traceback.format_exc()))
    (out/'report.json').write_text(json.dumps(report,indent=2))

def move_coupled():
    d,s=coupled('Coupled');before=facts(d)
    try:
        d.openTransaction('Move coupled source')
        O.apply(s,dict(op='move_geometries',object=s.Name,indices=[0],displacement=[3,4,0]));settle(d)
        moved=facts(d);d.commitTransaction()
        assert moved['Sketch']['ids']==before['Sketch']['ids']
        assert moved['Sketch']['construction']==before['Sketch']['construction']
        assert (s.Geometry[1].X,s.Geometry[1].Y)!=(0.,0.),'coincident point did not follow'
        assert (s.Geometry[2].X,s.Geometry[2].Y)==(0.,0.),'unselected duplicate point changed'
        d.undo();settle(d);assert facts(d)==before,'coupled Undo property mismatch'
        d.redo();settle(d);assert facts(d)==moved,'coupled Redo property mismatch'
        d.openTransaction('Two moves then abort')
        for delta in ([2,1,0],[-1,3,0]):
            O.apply(s,dict(op='move_geometries',object=s.Name,indices=[0,2],displacement=delta));settle(d)
        d.abortTransaction();settle(d);assert facts(d)==moved,'Abort failed'
        path=out/'coupled.FCStd';d.saveAs(str(path));A.closeDocument(d.Name);d=None
        d=A.openDocument(str(path));fresh=facts(d)
        settle(d);recomputed=facts(d)
        (out/'coupled-phases.json').write_text(json.dumps(dict(before=before,moved=moved,fresh=fresh,recomputed=recomputed),indent=2))
        report['fresh_raw_matches_moved']=fresh==moved
        assert recomputed==moved,'fresh recompute mismatch'
        return dict(before=before,moved=moved,fresh=fresh,undo=True,redo=True,multi_move_abort=True)
    finally:
        if d:A.closeDocument(d.Name)
record('move_coupled_points_and_external_follower',move_coupled)

for clone,constrain,scale,rows in [(False,True,-0.75,2),(True,True,1,2),(True,False,0,2),(False,True,0,1)]:
    def array_case(clone=clone,constrain=constrain,scale=scale,rows=rows):
        d=A.newDocument('ArrayCorner');d.UndoMode=1
        try:
            s=d.addObject('Sketcher::SketchObject','Sketch')
            s.addGeometry(Part.Point(A.Vector(2,3,0)),True)
            s.addGeometry(Part.Circle(A.Vector(8,7,0),A.Vector(0,0,1),2),False)
            settle(d);before=facts(d)
            edit=dict(op='rectangular_array',object=s.Name,indices=[0,1],displacement=[4,-3,0],
                columns=3,rows=rows,clone=clone,constrain_displacement=constrain,perpendicular_scale=scale)
            d.openTransaction('Point first array');O.apply(s,edit);settle(d);after=facts(d);d.commitTransaction()
            expected=2+(3*rows-1)*(2+int(constrain));assert s.GeometryCount==expected
            expected_positions=[(2+x*4+y*scale*-3,3+x*-3+y*scale*-4) for y in range(rows) for x in range(3)]
            actual_positions=[(g.X,g.Y) for g in s.Geometry if isinstance(g,Part.Point)]
            assert all(abs(a-b)<1e-7 for aa,bb in zip(actual_positions,expected_positions) for a,b in zip(aa,bb)),(actual_positions,expected_positions)
            d.undo();settle(d);assert facts(d)==before
            d.redo();settle(d);assert facts(d)==after
            return dict(edit=edit,count=expected,point_positions=actual_positions,undo=True,redo=True,
                ids_unique=len(set(after['Sketch']['ids']))==expected)
        finally:A.closeDocument(d.Name)
    record('point_first_array_'+str((clone,constrain,scale,rows)),array_case)

# Spy forwarding proves whether admission occurs before native mutation, without
# sending extreme coordinates into OCC or the solver.
class Spy:
    Name='Spy';GeometryCount=1;ConstraintCount=0;Constraints=[]
    def __init__(self):self.Geometry=[Part.Point(A.Vector(1e308,0,0))];self.calls=[]
    def isDerivedFrom(self,t):return True
    def addCopy(self,*a):self.calls.append(('addCopy',repr(a)))
    def addRectangularArray(self,*a):self.calls.append(('addRectangularArray',repr(a)))
for name,extra,calculation in [
    ('overflow_source_plus_offset',{},lambda:1e308+1e308),
    ('overflow_array_sum',dict(op='rectangular_array',columns=2,rows=2,clone=False,
        constrain_displacement=False,perpendicular_scale=1,displacement=[1e308,1e308,0]),lambda:1e308+1e308+1e308),
    ('overflow_unused_perpendicular_rows1',dict(op='rectangular_array',columns=2,rows=1,clone=False,
        constrain_displacement=False,perpendicular_scale=1e308,displacement=[2,2,0]),lambda:0*float('inf'))]:
    spy=Spy();edit=dict(op='copy_geometry',object='Spy',indices=[0],displacement=[1e308,0,0]);edit.update(extra)
    error=None
    try:O.apply(spy,edit)
    except O.OperationError as e:error=str(e)
    report['admission'].append(dict(name=name,edit=edit,error=error,native_called=bool(spy.calls),
        resulting_scalar=repr(calculation()),nonfinite=not math.isfinite(calculation())))
(out/'report.json').write_text(json.dumps(report,indent=2))
print(json.dumps({k:v for k,v in report.items() if k!='cases'},indent=2))
print(json.dumps([{k:v for k,v in c.items() if k!='details'} for c in report['cases']],indent=2))

assert all(c["ok"] for c in report["cases"])
assert all(not c["native_called"] for c in report["admission"])
