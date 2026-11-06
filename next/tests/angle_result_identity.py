"""Real native regression of the worker edit bodies, isolated from protocol callbacks.

Run with the pinned FreeCAD Python, not system Python. This is a deliberately
stale-count control, not a reconstruction of the uncaptured Pixel request.
"""
import ast, copy, ctypes, hashlib, json, math, os, sys, traceback, types
from pathlib import Path
k=ctypes.WinDLL('kernel32');k.GetPriorityClass.restype=ctypes.c_uint32
k.GetPriorityClass.argtypes=[ctypes.c_void_p];k.GetCurrentProcess.restype=ctypes.c_void_p
assert k.GetPriorityClass(k.GetCurrentProcess())==16384
import FreeCAD as App
import Part, Sketcher
BACKEND=Path(__file__).resolve().parents[1]/'backend'
sys.path.insert(0,str(BACKEND))
import document_units as DocumentUnits
import sketch_geometry as SketchGeometry
OUT=Path(os.environ['ANGLE_OUTPUT'])
report={'checks':[], 'scope':'native worker function bodies; no full worker or Editor execution'}
def write(): (OUT/'result.json').write_text(json.dumps(report,indent=2))
def check(label,value):
 report['checks'].append({'label':label,'pass':bool(value)});write();assert value,label
def fail(code,message):raise ValueError(code+': '+message)
def load(path):
 tree=ast.parse(path.read_text())
 wanted={'apply_native_edit','apply_edit','apply_edits','preflight_constraint_results','datum_quantity','number','sketch_index','vector'}
 nodes=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in wanted]
 ns=dict(App=App,Part=Part,Sketcher=Sketcher,DocumentUnits=DocumentUnits,SketchGeometry=SketchGeometry,
         math=math,fail=fail,object_in=lambda d,n:d.getObject(n),check_suppression=lambda:None)
 # Unrelated operation registries and protocol suppression are outside this
 # isolated function-body control. No native geometry/setter is substituted.
 for name in ['AssemblyAuthoring','ProfileReferences','FeatureHistory','SketchOperations']:
  ns[name]=types.SimpleNamespace(OPERATIONS=())
 exec(compile(ast.Module(body=nodes,type_ignores=[]),str(path),'exec'),ns)
 return ns
def raw(s):return {'geometry':[g.Content for g in s.Geometry],'constraints':[c.Content for c in s.Constraints], 'ids':[s.getGeometryId(i) for i in range(s.GeometryCount)]}
def fixture():
 d=App.newDocument();d.UndoMode=1;s=d.addObject('Sketcher::SketchObject','Sketch')
 s.addGeometry(Part.LineSegment(App.Vector(0,0,0),App.Vector(10,0,0)),False)
 s.addGeometry(Part.LineSegment(App.Vector(0,0,0),App.Vector(5,8,0)),False)
 s.addConstraint(Sketcher.Constraint('Horizontal',0));d.recompute();assert s.solve()==0
 return d,s
def angle(s):
 a,b=s.Geometry
 u=a.EndPoint-a.StartPoint;v=b.EndPoint-b.StartPoint
 return math.acos(u.dot(v)/(u.Length*v.Length))
def main():
 report['runtime']={'pid':os.getpid(),'priority':16384,'profiles':{k:os.environ[k] for k in ['HOME','USERPROFILE','FREECAD_USER_HOME','APPDATA','LOCALAPPDATA','TMP','TEMP']},'modules':{n:{'path':m.__file__,'sha256':hashlib.sha256(Path(m.__file__).read_bytes()).hexdigest()} for n,m in [('FreeCAD',App),('Part',Part),('Sketcher',Sketcher),('document_units',DocumentUnits),('sketch_geometry',SketchGeometry)]}}
 fixed=load(BACKEND/'freecad_worker.py');old=load(Path(os.environ['ANGLE_BASELINE']))
 create={'op':'constraint','object':'Sketch','kind':'Angle','args':[0,1,1,1,math.atan2(8,5)]}
 edits=[create,{'op':'datum','object':'Sketch','index_from':0,'value':'45 deg'}]
 d,s=fixture();before=raw(s);d.openTransaction('old stale count')
 try:
  old['apply_edit'](d,create)
  old['apply_edit'](d,{'op':'datum','object':'Sketch','index':0,'value':'45 deg'})
 except Exception as e:report['old_failure']={'error':repr(e),'traceback':traceback.format_exc()};write()
 else:raise AssertionError('old stale-count control must reject')
 d.abortTransaction();d.recompute();check('old rejection restores geometry constraints IDs',raw(s)==before)
 d.openTransaction('fixed result binding');result=fixed['apply_edits'](d,edits);d.recompute()
 check('native returned index owns pending datum',result[0]['index']==1)
 report['fixed_native']={'raw':raw(s),'angle_radians':angle(s),'solver':s.solve()};write()
 check('solver valid and physical45',report['fixed_native']['solver']==0 and abs(angle(s)-math.pi/4)<1e-12 and all(g.toShape().isValid() for g in s.Geometry))
 c=s.Constraints[1];check('native Angle exact references', (c.Type,c.First,c.FirstPos,c.Second,c.SecondPos)==('Angle',0,1,1,1))
 check('old constraint unchanged',s.Constraints[0].Content==before['constraints'][0]);d.commitTransaction();accepted=raw(s)
 d.undo();d.recompute();check('Undo removes only angle',raw(s)==before)
 d.redo();d.recompute();check('Redo exact geometry constraints IDs',raw(s)==accepted)
 # A cancelled native action and a bad value must preserve the accepted model.
 for label,value in [('cancel','35 deg'),('invalid','not-a-quantity')]:
  baseline=raw(s);d.openTransaction(label)
  try:fixed['apply_edits'](d,[{'op':'datum','object':'Sketch','index':1,'value':value}])
  except Exception as e:report[label+'_error']=repr(e)
  else:check(label+' setter result',label=='cancel')
  d.abortTransaction();d.recompute();check(label+' atomic geometry constraints IDs',raw(s)==baseline)
 for ref in [True,-1,1,99]:
  baseline=raw(s)
  try:fixed['apply_edits'](d,[create,dict(edits[1],index_from=ref)])
  except ValueError:pass
  else:raise AssertionError('invalid identity accepted')
  check('invalid result reference '+str(ref)+' rejected before mutation',raw(s)==baseline)
 path=OUT/'angle.FCStd';d.saveAs(str(path));name=d.Name;App.closeDocument(name);d=App.openDocument(str(path));s=d.getObject('Sketch')
 check('SaveReopen native Angle references and physical45',(s.Constraints[1].Type,s.Constraints[1].First,s.Constraints[1].FirstPos,s.Constraints[1].Second,s.Constraints[1].SecondPos)==('Angle',0,1,1,1) and abs(angle(s)-math.pi/4)<1e-12)
 report['save_readback_raw']={'before':accepted,'after':raw(s)};write()
 check('SaveReopen solver validity',s.solve()==0)
 App.closeDocument(d.Name);check('all documents closed',not App.listDocuments());write()
if __name__=='__main__':
 try:main()
 except Exception:report['fatal']=traceback.format_exc();write();raise
