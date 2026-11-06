"""Production AST callbacks with explicit native doubles; no FreeCAD execution."""
import ast, math, re, unittest
from pathlib import Path
from types import SimpleNamespace as N
ROOT=Path(__file__).parent
HELPER=ROOT/'source/next/backend/standalone_helix.py'
WORKER=ROOT/'source/next/backend/freecad_worker.py'
class Obj:
    def __init__(self, kind, name):
        self.TypeId,self.Name,self.PropertiesList,self.expressions=kind,name,[],{}
    def addProperty(self, kind, name, *args): self.PropertiesList.append(name)
    def setExpression(self, key, value): self.expressions[key]=value
    def setEditorMode(self,*args): pass
class Doc:
    def __init__(self): self.objects={};self.tip=object();self.events=[]
    def getObject(self,n): return self.objects.get(n)
    def addObject(self,k,n):
        obj=Obj(k,n);self.objects[n]=obj;self.events.append(('add',k,n));return obj
    def recompute(self): self.events.append(('recompute',))
def env():
    nodes=[n for n in ast.parse(HELPER.read_text()).body if not isinstance(n,(ast.Import,ast.ImportFrom))]
    e=dict(math=math,re=re,DocumentUnits=N(value=lambda v,u:float(v)),App=N(Placement=lambda *a:a, Vector=lambda *a:a,Rotation=lambda *a:a),Part=N())
    exec(compile(ast.Module(body=nodes,type_ignores=[]),str(HELPER),'exec'),e)
    return e
class Tests(unittest.TestCase):
    def test_native_creation_dependencies_and_expression_graph(self):
        e=env();doc=Doc();tip=doc.tip;source=object()
        e['reference']=lambda *a:(source,'Face1','Edge2',False)
        result=e['create'](doc,dict(name='Helix',face_binder='HF',edge_binder='HC'))
        self.assertEqual(result,{'object':'Helix'});self.assertIs(doc.tip,tip)
        obj=doc.objects['Helix'];self.assertEqual(obj.TypeId,'Part::Helix')
        self.assertEqual(obj.Style,'New style');self.assertEqual(obj.MapMode,'Concentric')
        self.assertEqual(obj.Turns,1);self.assertEqual(obj.StartAngle,0)
        self.assertEqual(obj.expressions['Pitch'],'Height / Turns')
        self.assertEqual(obj.expressions['AttachmentOffset.Rotation.Angle'],'StartAngle')
        self.assertEqual(obj.expressions['Radius'],'HC.Shape.Length / (2 * pi) * 1 mm')
        self.assertEqual(doc.objects['HF'].Support,[(source,['Face1'])])
        self.assertEqual(doc.objects['HC'].Support,[(source,['Edge2'])])
        self.assertFalse(doc.objects['HC'].Visibility)
        self.assertNotIn('Shape',obj.__dict__);self.assertNotIn('Proxy',obj.__dict__)
    def test_conflict_rejected_before_mutation(self):
        e=env();doc=Doc();doc.objects['HF']=object();e['reference']=lambda *a:(object(),'Face1','Edge1',False)
        with self.assertRaises(e['HelixError']): e['create'](doc,dict(name='H',face_binder='HF',edge_binder='HC'))
        self.assertEqual(doc.events,[])
    def test_stale_large_identity_rejected_without_geometry_access(self):
        e=env();doc=Doc();doc.objects['Pad']=N(ID=9007199254740993)
        with self.assertRaises(e['HelixError']): e['reference'](doc,dict(source='Pad',source_id='9007199254740992'))
    def test_mapped_identity_rejected(self):
        e=env();doc=Doc();doc.objects['Pad']=N(ID=1,Shape=N(getElementMappedName=lambda k:'new'))
        with self.assertRaises(e['HelixError']): e['reference'](doc,dict(source='Pad',source_id='1',subelement='Face1',mapped_name='old'))
    def test_preflight_distinct_names(self):
        e=env();e['reference']=lambda *a:None
        with self.assertRaises(e['HelixError']):e['preflight'](Doc(),[dict(op='create_standalone_helix',name='H',face_binder='H',edge_binder='E')])
    def test_actual_worker_callback_dispatch(self):
        e=env();doc=Doc();seen=[]
        worker=ast.parse(WORKER.read_text());node=next(n for n in worker.body if isinstance(n,ast.FunctionDef) and n.name=='apply_native_edit')
        e.update(FirstScalePolicy=N(validate=lambda e:None),RectangularDraft=N(OPS=()),AssemblyAuthoring=N(OPERATIONS=()),ProfileReferences=N(OPERATIONS=()),FeatureHistory=N(OPERATIONS=()),SketchOperations=N(OPERATIONS=()),Exchange=N(OPERATIONS=()),StandaloneHelix=N(create=lambda d,x:seen.append((d,x)) or {'object':'H'},HelixError=e['HelixError']))
        exec(compile(ast.Module(body=[node],type_ignores=[]),str(WORKER),'exec'),e)
        edit=dict(op='create_standalone_helix');self.assertEqual(e['apply_native_edit'](doc,edit),{'object':'H'});self.assertEqual(seen,[(doc,edit)])
    def test_preflight_precedes_preview_abort(self):
        tree=ast.parse(WORKER.read_text());node=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='preview')
        body=ast.get_source_segment(WORKER.read_text(),node)
        self.assertLess(body.index('StandaloneHelix.preflight'),body.index('abort_transaction',body.index('StandaloneHelix.preflight')))
    def test_no_custom_geometry_generation(self):
        tree=ast.parse(HELPER.read_text());attributes=[n.attr for n in ast.walk(tree) if isinstance(n,ast.Attribute)]
        self.assertNotIn('makeHelix',attributes);self.assertNotIn('Proxy',attributes)
class V:
    def __init__(self,x=0,y=0,z=0): self.x,self.y,self.z=x,y,z
    def dot(self,o): return self.x*o.x+self.y*o.y+self.z*o.z
    def __sub__(self,o):return V(self.x-o.x,self.y-o.y,self.z-o.z)
class Circle:
    def __init__(self,z):self.Center=V(0,0,z);self.Radius=25.400000000000006;self.Axis=V(0,0,1)
class Cylinder:
    Axis=V(0,0,1)
class Edge:
    Degenerated=False
    def __init__(self,z):self.Curve=Circle(z)
    def isClosed(self):return True
    def isSame(self,o):return self is o
class Face:
    Surface=Cylinder()
    def __init__(self):self.Edges=[Edge(127),Edge(0)];self.Area=2*math.pi*self.Edges[0].Curve.Radius*127
    def isSame(self,o):return self is o
class GeometryTests(unittest.TestCase):
    def fixture(self):
        e=env();e['Part']=N(Cylinder=Cylinder,Circle=Circle,Face=Face)
        face=Face();shape=N(Faces=[face],Edges=face.Edges)
        shape.getElementMappedName=lambda k:'mapped-'+k
        shape.getElement=lambda k:face if k=='Face1' else face.Edges[int(k[4:])-1]
        shape.ancestorsOfType=lambda *a:[face]
        doc=Doc();doc.objects['Pad']=N(ID=9007199254740993,Shape=shape)
        return e,doc,face
    def test_full_cylinder_selects_native_bottom_edge_without_rounding(self):
        e,d,f=self.fixture();result=e['reference'](d,dict(source='Pad',source_id='9007199254740993',subelement='Face1',mapped_name='mapped-Face1'))
        self.assertEqual(result[1:],('Face1','Edge2',False))
        self.assertEqual(f.Edges[0].Curve.Radius,25.400000000000006)
    def test_circular_edge_resolves_actual_adjacent_cylinder(self):
        e,d,f=self.fixture();result=e['reference'](d,dict(source='Pad',source_id='9007199254740993',subelement='Edge1',mapped_name='mapped-Edge1'))
        self.assertEqual(result[1:3],('Face1','Edge2'))
    def test_partial_cylinder_rejected(self):
        e,d,f=self.fixture();f.Area/=2
        with self.assertRaises(e['HelixError']):e['reference'](d,dict(source='Pad',source_id='9007199254740993',subelement='Face1',mapped_name='mapped-Face1'))
class CircleTests(GeometryTests):
    def edit(self):
        return dict(source='Pad',source_id='9007199254740993',subelement='Edge1',mapped_name='mapped-Edge1',
                    reference_mode='Circle',extent_mode='Height and pitch',name='H',face_binder='HF',edge_binder='HC')
    def test_circle_without_face_ancestor(self):
        e,d,f=self.fixture()
        d.objects['Pad'].Shape.ancestorsOfType=lambda *a: (_ for _ in ()).throw(AssertionError('ancestor lookup forbidden'))
        self.assertEqual(e['reference'](d,self.edit())[1:],(None,'Edge1',False))
    def test_three_acyclic_extent_graphs_and_persistent_support(self):
        for pair, (a,b,dependent,expression) in env()['EXTENTS'].items():
            e,d,f=self.fixture();edit=self.edit();edit['extent_mode']=pair
            e['preflight'](d,[dict(edit,op='create_standalone_helix')])
            e['create'](d,edit);obj=d.objects['H']
            self.assertNotIn('HF',d.objects)
            self.assertEqual(obj.ExtentMode,pair)
            self.assertEqual(obj.CircleReference,d.objects['HC'])
            self.assertEqual(obj.AttachmentSupport,[(d.objects['HC'],['Edge1'])])
            self.assertEqual(obj.expressions[dependent],expression)
            self.assertNotIn(a,obj.expressions);self.assertNotIn(b,obj.expressions)
            self.assertNotIn('Shape',obj.__dict__)
            for field,value in [('Height',2.),('Pitch',1.),('Radius',25.4)]: setattr(obj,field,N(Value=value))
            d.objects['HC'].Shape=N(Edges=[f.Edges[0]])
            e['validate'](obj)
            with self.assertRaises(e['HelixError']):
                e['preflight'](d,[dict(op='set',object='H',property=dependent,value=2)])
    def test_bad_circle_mode_and_partial_edge_fail(self):
        e,d,f=self.fixture();edit=self.edit();edit['extent_mode']='guess'
        with self.assertRaises(e['HelixError']): e['reference'](d,edit)
        edit=self.edit();f.Edges[0].isClosed=lambda:False
        with self.assertRaises(e['HelixError']): e['reference'](d,edit)
    def test_invalid_authored_lengths_preflight(self):
        for value in [0,-1,float('nan'),float('inf')]:
            e,d,f=self.fixture()
            with self.assertRaises(e['HelixError']):
                e['preflight'](d,[dict(self.edit(),op='create_standalone_helix'),dict(op='set',object='H',property='Pitch',value=value)])
if __name__=='__main__':unittest.main(verbosity=2)
