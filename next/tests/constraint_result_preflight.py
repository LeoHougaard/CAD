"""Nonnative tests of real worker function bodies and pre-history entry ordering."""
import ast,copy,json,types,unittest
from pathlib import Path
P=Path(__file__).resolve().parents[1]/'backend/freecad_worker.py'
class Rejected(ValueError):pass
def fail(code,message):raise Rejected(code+': '+message)
class Control(unittest.TestCase):
 def setUp(self):
  self.calls=[];self.mutations=[]
  names={'preflight_constraint_results','apply_edits','sketch_index','preflight_constraints',
         'transaction','sketch_transaction','sketch_edits','preview','sketch_preview','replay_sketch'}
  tree=ast.parse(P.read_text());nodes=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in names]
  self.doc=types.SimpleNamespace(Name='D')
  for method in ['openTransaction','abortTransaction','commitTransaction']:
   setattr(self.doc,method,lambda *a,m=method:self.mutate(m))
  self.sketch=types.SimpleNamespace(ConstraintCount=35,Constraints=[types.SimpleNamespace(Type='Angle')]*35)
  self.ns=dict(fail=fail,apply_edit=self.apply,object_in=lambda d,n:self.sketch,
     _failed_loads={},_native_previews={},_native_sketch_sessions={'D':{'opened':True,'object':'Sketch'}},
     lease=lambda *a,**k:None,sketch_metadata=lambda d:False,
     SketchOperations=types.SimpleNamespace(preflight=lambda e:None),
     SketchGeometry=types.SimpleNamespace(validate_constraint=lambda *a:None,GeometryError=RuntimeError),
     validate_revolve_axis_preview=lambda e:None,journal=lambda *a,**k:self.mutate('journal'),
     abort_transaction=lambda *a:self.mutate('abort'))
  exec(compile(ast.Module(body=nodes,type_ignores=[]),str(P),'exec'),self.ns)
  self.create={'op':'constraint','object':'Sketch','kind':'Angle','args':[0,1,1,1,1.]}
  self.datum={'op':'datum','object':'Sketch','index_from':0,'value':'45 deg'}
 def mutate(self,event):self.mutations.append(event);raise AssertionError('history touched: '+event)
 def apply(self,d,e):self.calls.append(e);return {'object':e['object'],'index':34}
 def invalid_batches(self):
  for ref in [True,False,-1,1,99,0.0,'0',None]:yield [self.create,dict(self.datum,index_from=ref)]
  for bad in [dict(self.datum,index=0),dict(self.datum,object='Foreign'),dict(self.datum,object=None),dict(self.datum,op='constraint_state')]:yield [self.create,bad]
  for source in [None,[],dict(self.create,op='geometry'),dict(self.create,kind='Horizontal'),dict(self.create,kind=[])]:yield [source,self.datum]
  yield [self.create,{'op':'delete_constraint','object':'Sketch','index':0},self.datum]
  yield [self.create,{'op':'datum','object':'Sketch','index':0,'value':'35 deg'},self.datum]
 def test_invalid_references_before_each_history_entry(self):
  for edits in self.invalid_batches():
   for route in ['transaction','sketch_transaction','preview','sketch_preview','replay_sketch']:
    with self.subTest(edits=edits,route=route):
     req={'op':'preview' if 'preview' in route else 'transaction','edits':edits}
     fn=self.ns[route]
     with self.assertRaises(Rejected):
      if route=='sketch_preview':fn(self.doc,req,None)
      elif route=='replay_sketch':fn(self.doc,{'opened':True},{'edits':edits})
      else:fn(self.doc,req)
     self.assertEqual(self.mutations,[]);self.assertEqual(self.calls,[])
 def test_active_preview_malformed_reference_before_abort(self):
  self.ns['_native_previews']['D']={'edits':[self.create,dict(self.datum,index_from=True)]}
  with self.assertRaises(Rejected):self.ns['preview'](self.doc,{'op':'preview','edits':[self.create,self.datum]})
  self.assertEqual(self.mutations,[])
 def test_append_constraint_preserves_native_result_not_last_result(self):
  edits=[self.create,dict(self.create,kind='Parallel'),self.datum];before=copy.deepcopy(edits)
  def apply(d,e):
   self.calls.append(e)
   return {'object':e['object'],'index':34 if e.get('kind')=='Angle' else 35}
  self.ns['apply_edits'](self.doc,edits,apply)
  self.assertEqual(self.calls[-1]['index'],34);self.assertNotIn('index_from',self.calls[-1]);self.assertEqual(edits,before)
 def test_bad_native_result_ownership_and_type(self):
  results=[None,[],{}, {'object':'Foreign','index':34}]
  results += [{'object':'Sketch','index':i} for i in [True,False,None,-1,35,34.0,'34']]
  for result in results:
   calls=[]
   def apply(d,e):calls.append(e);return result
   with self.subTest(result=result),self.assertRaises(Rejected):self.ns['apply_edits'](self.doc,[self.create,self.datum],apply)
   self.assertEqual(len(calls),1)
  self.sketch.Constraints[34]=types.SimpleNamespace(Type='Distance')
  with self.assertRaises(Rejected):self.ns['apply_edits'](self.doc,[self.create,self.datum])
  self.assertEqual(len(self.calls),1)
 def test_all_production_batches_resolve_result_references(self):
  tree=ast.parse(P.read_text())
  for n in ast.walk(tree):
   if isinstance(n,ast.ListComp):
    self.assertFalse(any(isinstance(c,ast.Call) and isinstance(c.func,ast.Name) and c.func.id in {'apply_edit','apply_preview_edit'} for c in ast.walk(n)))
  # Explicit stored-datum policy stays on the existing native setter path.
  explicit={'op':'datum','object':'Sketch','index':3,'value':'45 deg'}
  self.ns['apply_edits'](self.doc,[explicit]);self.assertEqual(self.calls,[explicit])
if __name__=='__main__':unittest.main(verbosity=2)
