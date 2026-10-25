"""Read-only saved BRep checks before Open or oracle repair, then native idempotence."""
import argparse,hashlib,json,zipfile,xml.etree.ElementTree as E,traceback
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--fixtures',type=Path,required=True);p.add_argument('--runs',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
F=a.fixtures;O=a.runs;out=a.output;out.mkdir(parents=True,exist_ok=False)
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
import FreeCAD as A
import Part,PartDesign,Sketcher
assert A.Version()[:3]==['1','1','4'];assert Part.OCC_VERSION=='7.8.1'
hashes={};checks=[]
def archive(p):
 p=Path(p);hashes[str(p)]=sha(p)
 with zipfile.ZipFile(p) as z:
  xml=z.read('Document.xml');assert b'hole_task_literals' not in xml
  root=E.fromstring(xml);types={n.get('name'):n.get('type') for n in root.findall('./Objects/Object')};holes={}
  for n in root.findall('./ObjectData/Object'):
   if types.get(n.get('name'))!='PartDesign::Hole':continue
   s=Part.Shape();ref=n.find("./Properties/Property[@name='Shape']/Part").get('file');s.importBrepFromString(z.read(ref).decode())
   holes[n.get('name')]=(n,s)
  return root,holes
def same(a,b):
 assert a.isValid() and b.isValid()
 diff=a.cut(b).Volume+b.cut(a).Volume
 # Record raw CSG, area and volume errors. No input/property drift whitelist.
 result=dict(symdiff=diff,volume_delta=a.Volume-b.Volume,area_delta=a.Area-b.Area)
 assert abs(diff)<1e-7 and abs(result['volume_delta'])<1e-7 and abs(result['area_delta'])<1e-7,result
 assert len(a.Solids)==len(b.Solids)
 return result
fields=list(json.loads((F/'native-controls2/controls.json').read_text())['baseline']['properties'])
def value(node,k,obj):
 p=node.find("./Properties/Property[@name='"+k+"']");kind=p.get('type')
 if kind=='App::PropertyEnumeration':
  ls=p.find('CustomEnumList');choices=[e.get('value') for e in ls] if ls is not None else obj.getEnumerationsOfProperty(k)
  return choices[int(p.find('Integer').get('value'))]
 if kind=='App::PropertyBool':return p.find('Bool').get('value') in ['true','1']
 child=p.find('Float')
 return float(child.get('value')) if child is not None else int(p.find('Integer').get('value'))
def verify(p,ref):
 _,saved=archive(p);_,control=archive(ref);assert set(saved)==set(control)
 # Complete this gate for every Hole before any native Open or repair setter.
 pre={name:same(s,control[name][1]) for name,(_,s) in saved.items()}
 before=set(A.listDocuments());d=None
 try:
  d=A.openDocument(str(p));contracts={};repairs={}
  for name,(node,_) in saved.items():
   h=d.getObject(name);vals={k:value(node,k,h) for k in fields};contracts[name]=vals;repairs[name]=[]
   def setter(k,v):
    current=getattr(h,k);current=current.Value if hasattr(current,'Value') else current
    if current!=v:
     assert 'ReadOnly' not in h.getEditorMode(k) and 'ReadOnly' not in h.getPropertyStatus(k)
     setattr(h,k,v);repairs[name].append(k)
   setter('ThreadSize',vals['ThreadSize'])
   if vals['HoleCutCustomValues']:
    setter('HoleCutCustomValues',True)
    for k in ['HoleCutCountersinkAngle','HoleCutDiameter','HoleCutDepth']:setter(k,vals[k])
  for name in saved:d.getObject(name).touch()
  d.recompute();assert all(o.isValid() for o in d.Objects)
  post={}
  for name,(node,s) in saved.items():
   h=d.getObject(name);post[name]=same(h.Shape,s)
   for k,v in contracts[name].items():
    actual=getattr(h,k);actual=actual.Value if hasattr(actual,'Value') else actual;assert actual==v,(name,k,actual,v)
   ee=node.find("./Properties/Property[@name='ExpressionEngine']/ExpressionEngine");expressions={x.get('path'):x.get('expression') for x in ee} if ee is not None else {}
   assert dict(h.ExpressionEngine)==expressions
   refnode=control[name][0]
   for k in ['Profile','BaseFeature']:
    assert E.tostring(node.find("./Properties/Property[@name='"+k+"']"))==E.tostring(refnode.find("./Properties/Property[@name='"+k+"']")),(name,k)
  # A second native invalidate/recompute must not drift inputs or geometry.
  for name in saved:d.getObject(name).touch()
  d.recompute()
  twice={name:same(d.getObject(name).Shape,s) for name,(_,s) in saved.items()}
  return dict(saved=str(p),reference=str(ref),passed=True,holes=len(saved),before_any_oracle_repair=pre,after_native_repair=post,second_touch=twice,native_setters=repairs)
 finally:
  for name in set(A.listDocuments())-before:A.closeDocument(name)

pairs=[]
records=json.loads((O/'fixed-regressions/fixed-open-results.json').read_text())
pairs += [(Path(c['saved']),Path(c['control'])) for c in records]
pairs += [(O/'fixed-regressions'/('fixed-multi-cycle'+str(i)+'.FCStd'),F/'native-controls3/control-multi-edited.FCStd') for i in range(4)]
pairs += [(O/'retained-persistence'/('Hole-persistence-'+str(i)+'.FCStd'),F/'persistence2/profile-control.FCStd') for i in range(4)]
records=json.loads((O/'independent-r2/r2-saved-pairs.json').read_text())
pairs += [(Path(c['saved']),Path(c['reference'])) for c in records]
pairs += [(O/'independent-r2/r2-after-history.FCStd',O/'independent-r2/r2-before-history.FCStd')]
for saved,ref in pairs:
 try:record=verify(saved,ref)
 except Exception as e:record=dict(saved=str(saved),reference=str(ref),passed=False,error=repr(e),traceback=traceback.format_exc())
 checks.append(record);print(saved.name,record['passed'],flush=True)
for saved,depth,diameter,total in [(O/'retained-recovery/Hole-recovered.FCStd','0.25 mm','5.5 mm',None),(O/'independent-r2/r2-widget-units.FCStd','0.25 mm','5.5 mm',None),(O/'imperial/hole-imperial.FCStd','0.01 in','0.25 in','0.25 in')]:
 try:
  _,raw=archive(saved);source=F/'fixtures/input.FCStd';root,original=archive(source);d=A.openDocument(str(source))
  try:
   h=d.Hole;node=original['Hole'][0]
   size=value(node,'ThreadSize',h)
   if h.ThreadSize!=size:h.ThreadSize=size
   h.HoleCutType='Counterbore';h.HoleCutCustomValues=True;h.HoleCutDepth=A.Units.Quantity(depth).Value;h.HoleCutDiameter=A.Units.Quantity(diameter).Value
   if total is not None:h.DepthType='Dimension';h.Depth=A.Units.Quantity(total).Value
   h.touch();d.recompute();direct=same(raw['Hole'][1],h.Shape)
  finally:A.closeDocument(d.Name)
  record=verify(saved,saved);record['direct_native_setter_match']=direct
 except Exception as e:record=dict(saved=str(saved),passed=False,error=repr(e),traceback=traceback.format_exc())
 checks.append(record);print(saved.name,record['passed'],flush=True)
preserved=all(sha(p)==h for p,h in hashes.items())
errors=[x for c in checks if c['passed'] for stage in ['before_any_oracle_repair','after_native_repair','second_touch'] for x in c[stage].values()]
errors += [c['direct_native_setter_match'] for c in checks if c.get('direct_native_setter_match')]
raw_zero=all(all(v==0 for v in e.values()) for e in errors)
raw_csg_zero=all(e['symdiff']==0 for e in errors)
# Keep the independent review's original same() bounds unchanged. Report raw
# summation differences too; equal BReps can differ in the last volume bit.
passed=all(c['passed'] for c in checks) and preserved and not A.listDocuments() and raw_csg_zero
(out/'report.json').write_text(json.dumps(dict(passed=passed,checks=checks,raw_csg_all_zero=raw_csg_zero,raw_csg_volume_area_all_zero=raw_zero,read_only_hashes=hashes,all_input_files_unchanged=preserved,remaining_documents=list(A.listDocuments()),version=A.Version(),occt=Part.OCC_VERSION),indent=2))
raise SystemExit(0 if passed else 1)
