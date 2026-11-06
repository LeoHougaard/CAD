"""Fresh native controls retain all raw pre/post-solve differences."""
import json,sys
from pathlib import Path
import FreeCAD as App
import Part
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'backend'))
import sketch_geometry as G, sketch_operations as O
import io,zipfile,xml.etree.ElementTree as ET
out=Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=False);report={'cases':[],'comparisons':[]}
def facts(s):
    answer={}
    for p in ['Geometry','Constraints','ExpressionEngine']:
        with zipfile.ZipFile(io.BytesIO(s.dumpPropertyContent(p))) as z:answer[p]=z.read('Persistence.xml').decode()
    answer['ids']=[s.getGeometryId(i) for i in range(s.GeometryCount)]
    answer['construction']=[s.getConstruction(i) for i in range(s.GeometryCount)]
    answer['dof']=s.DoF
    return answer
def different_attrs(a,b):
    diffs=[]
    for prop in ['Geometry','Constraints','ExpressionEngine']:
        aa=list(ET.fromstring(a[prop]).iter());bb=list(ET.fromstring(b[prop]).iter())
        if len(aa)!=len(bb):diffs.append(dict(property=prop,structure_count=[len(aa),len(bb)]));continue
        for i,(x,y) in enumerate(zip(aa,bb)):
            for k in sorted(set(x.attrib)|set(y.attrib)):
                if x.attrib.get(k)!=y.attrib.get(k):diffs.append(dict(property=prop,element=i,tag=x.tag,attribute=k,a=x.attrib.get(k),b=y.attrib.get(k)))
    for k in ['ids','construction','dof']:
        if a[k]!=b[k]:diffs.append(dict(property=k,a=a[k],b=b[k]))
    return diffs
for kind in ['reversed_arc','periodic_rational_spline']:
    for index,mode in enumerate(['direct','direct','adapter','direct','adapter','adapter']):
        d=App.newDocument('Repeat');d.UndoMode=1
        try:
            s=d.addObject('Sketcher::SketchObject','Sketch')
            if kind=='reversed_arc':curve=Part.ArcOfCircle(Part.Circle(App.Vector(1,2,0),App.Vector(0,0,-1),2),0.2,1.4)
            else:curve=G.make_geometry(dict(kind='bspline',poles=[[1,2,0],[3,5,0],[6,3,0],[4,0,0]],degree=2,periodic=True,weights=[1,.8,1.2,.9]))
            s.addGeometry(curve,False);assert s.solve()==0;d.recompute();source=facts(s)
            d.openTransaction('Matched repeat')
            if mode=='direct':s.addRectangularArray([0],App.Vector(7,-3,0),False,2,2,True,.75)
            else:O.apply(s,dict(op='rectangular_array',object='Sketch',indices=[0],displacement=[7,-3,0],columns=2,rows=2,clone=False,constrain_displacement=True,perpendicular_scale=.75))
            pre=facts(s);solve=s.solve();assert solve==0;d.recompute();d.commitTransaction();post=facts(s)
            item=dict(kind=kind,mode=mode,index=index,source=source,pre_solve=pre,solve_result=solve,post_solve=post)
            report['cases'].append(item)
        finally:App.closeDocument(d.Name)
    cases=[c for c in report['cases'] if c['kind']==kind];base=cases[0]
    for c in cases[1:]:
        comp=dict(kind=kind,a=dict(mode=base['mode'],index=base['index']),b=dict(mode=c['mode'],index=c['index']),
                  source_exact=base['source']==c['source'],pre_solve_exact=base['pre_solve']==c['pre_solve'],post_solve_exact=base['post_solve']==c['post_solve'],
                  raw_differences=different_attrs(base['post_solve'],c['post_solve']))
        assert comp['source_exact'] and comp['pre_solve_exact'],comp
        report['comparisons'].append(comp)
(out/'report.json').write_text(json.dumps(report,indent=2))
print(json.dumps(report['comparisons'],indent=2))
