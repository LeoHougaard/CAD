import io,zipfile
import FreeCAD as App
import Part,Sketcher
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'backend'))
import sketch_geometry as G

def xml(s, prop):
    with zipfile.ZipFile(io.BytesIO(s.dumpPropertyContent(prop))) as z:
        return z.read('Persistence.xml').decode()

def facts(s):
    return dict(properties={p:xml(s,p) for p in ('Geometry','Constraints','ExpressionEngine')},
        ids=[s.getGeometryId(i) for i in range(s.GeometryCount)],
        construction=[s.getConstruction(i) for i in range(s.GeometryCount)], dof=s.DoF,
        element_map={kind+str(i):s.Shape.getElementMappedName(kind+str(i))
            for kind,count in [('Edge',len(s.Shape.Edges)),('Vertex',len(s.Shape.Vertexes))]
            for i in range(1,count+1)})

def setup(doc):
    s=doc.addObject('Sketcher::SketchObject','Sketch')
    v=App.Vector
    curves=[Part.LineSegment(v(0,0,0),v(3,1,0)),Part.Circle(v(10,0,0),v(0,0,1),2),
        Part.ArcOfCircle(Part.Circle(v(20,0,0),v(0,0,1),2),0.2,2),Part.Point(v(30,0,0))]
    for kind,center in [('ellipse',40),('ellipse_arc',50),('hyperbola_arc',60)]:
        curves.append(G.make_geometry(dict(kind=kind,major=[center+4,0,0],minor=[center,2,0],
            center=[center,0,0],start_parameter=0.2,end_parameter=1.5)))
    curves.append(G.make_geometry(dict(kind='parabola_arc',center=[70,0,0],focal=2,
        start_parameter=-1,end_parameter=1)))
    curves.append(G.make_geometry(dict(kind='bspline',poles=[[80,0,0],[82,3,0],[85,0,0]],degree=2)))
    s.addGeometry(curves,True)
    s.addConstraint(Sketcher.Constraint('Radius',1,2))
    s.renameConstraint(0,'SourceRadius'); s.setExpression('Constraints.SourceRadius','2 mm')
    for i in range(4,9): s.exposeInternalGeometry(i)
    assert s.solve()==0
    doc.recompute()
    return s
