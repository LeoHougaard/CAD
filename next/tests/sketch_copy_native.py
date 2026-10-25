"""Bounded direct-native controls, full native properties and Undo/Redo."""
import io
import json
import os
from pathlib import Path
import sys
import zipfile

import FreeCAD as App
import Part
import Sketcher

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'backend'))
import sketch_geometry as G
import sketch_operations as O

out = Path(sys.argv[1]); out.mkdir(parents=True,exist_ok=False)
report = {'version': list(App.Version()), 'cases': [], 'invalid': []}

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

for operation, options in [('copy_geometry',{}),('clone_geometry',{}),('move_geometries',{}),
    ('rectangular_array',dict(columns=3,rows=2,clone=False,constrain_displacement=False,perpendicular_scale=0.5)),
    ('rectangular_array',dict(columns=2,rows=2,clone=True,constrain_displacement=True,perpendicular_scale=1.0))]:
    doc=App.newDocument('MatchedCopy'); doc.UndoMode=1
    try:
        s=setup(doc); baseline=bytes(s.dumpContent()); before=facts(s)
        indices=list(range(s.GeometryCount))
        edit=dict(op=operation,object=s.Name,indices=indices,displacement=['10 mm','3 mm',0],**options)
        doc.openTransaction('Adapter'); O.apply(s,edit); assert s.solve()==0; doc.recompute()
        adapter=facts(s); doc.commitTransaction()
        doc.undo(); doc.recompute(); assert facts(s)==before, operation+' Undo'
        doc.redo(); doc.recompute(); assert facts(s)==adapter, operation+' Redo'
        path=out/(str(len(report['cases']))+'-'+operation+'.FCStd'); doc.saveAs(str(path))
        # Fresh native Open is independent of the worker's cached labels.
        App.closeDocument(doc.Name); doc=None
        opened=App.openDocument(str(path)); fresh=facts(opened.Sketch); App.closeDocument(opened.Name)
        assert fresh==adapter, operation+' fresh Open'
        doc=App.newDocument('DirectCopy'); doc.UndoMode=1
        s=setup(doc)
        doc.openTransaction('Direct')
        displacement=App.Vector(10,3,0)
        if operation=='move_geometries':
            s.Geometry=G.restore_point_extensions(s,'Geometry',s.Geometry)
            s.addMove(indices,displacement)
        elif operation=='rectangular_array':
            s.addRectangularArray(indices,displacement,options['clone'],options['columns'],options['rows'],
                options['constrain_displacement'],options['perpendicular_scale'])
        else: s.addCopy(indices,displacement,operation=='clone_geometry')
        assert s.solve()==0; doc.recompute(); direct=facts(s)
        assert adapter==direct, operation+' direct control'
        if operation in ('copy_geometry','clone_geometry'):
            s.setExpression('Constraints.SourceRadius','3 mm'); assert s.solve()==0; doc.recompute()
            assert s.Geometry[1].Radius==3
            # Exposed spline controls are circles too; compare the copied source
            # circle by the native type-order slot, not coordinate matching.
            assert s.Geometry[len(before['ids'])+1].Radius==(3 if operation=='clone_geometry' else 2)
        report['cases'].append(dict(edit=edit,before=before,adapter=adapter,direct=direct,
            exact_native_properties=True,undo=True,redo=True,fresh_open=True,path=str(path)))
    finally:
        if doc is not None: App.closeDocument(doc.Name)

doc=App.newDocument('InvalidCopy')
try:
    s=setup(doc); baseline=facts(s)
    for extra in [dict(indices=[-1]),dict(indices=[-3]),dict(indices=[999]),dict(indices=[True]),
        dict(indices=[0,0]),dict(indices=[]),dict(displacement=[0,0,1]),dict(displacement=['2 deg',0,0]),
        dict(columns=2147483648),dict(columns=100),dict(rows=0),dict(perpendicular_scale=float('inf')),
        dict(clone='yes'),dict(unknown=2)]:
        edit=dict(op='rectangular_array',object=s.Name,indices=[0],displacement=[2,3,0],
            columns=2,rows=2,clone=False,constrain_displacement=True,perpendicular_scale=1)
        edit.update(extra)
        try: O.apply(s,edit)
        except (O.OperationError,ValueError) as error:
            assert facts(s)==baseline
            report['invalid'].append(dict(input=repr(extra),error=str(error),preserved=True))
        else: raise AssertionError('admitted invalid input '+repr(extra))
    for indices in [[-3],[-1],[999],[True],[0,0],[]]:
        try: O.apply(s,dict(op='move_geometries',object=s.Name,indices=indices,displacement=[1,2,0]))
        except O.OperationError as error:
            assert facts(s)==baseline
            report['invalid'].append(dict(op='move_geometries',indices=indices,error=str(error),preserved=True))
        else: raise AssertionError('unsafe Move admitted')
    # Selected spline parent must bring its actual exposed controls, with native
    # link IDs preserved in the result, without selecting them individually.
    controls=[c.First for c in s.Constraints if c.Type=='InternalAlignment' and c.Second==8]
    count=s.GeometryCount
    alignment_count=sum(c.Type=='InternalAlignment' for c in s.Constraints)
    O.apply(s,dict(op='copy_geometry',object=s.Name,indices=[8],displacement=[10,0,0]))
    assert s.solve()==0; doc.recompute()
    assert s.GeometryCount==count+1+len(controls)
    assert sum(c.Type=='InternalAlignment' for c in s.Constraints)==alignment_count+len(controls)
    report['selected_spline_expansion']=dict(source=8,controls=controls,added=1+len(controls))
finally: App.closeDocument(doc.Name)
(out/'report.json').write_text(json.dumps(report,indent=2))
print(json.dumps(dict(matched_cases=len(report['cases']),invalid_preserved=len(report['invalid'])),indent=2))
