import argparse,json,os,subprocess,sys,time,zipfile,xml.etree.ElementTree as E,traceback
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--fixtures',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
F=a.fixtures;out=a.output;out.mkdir(parents=True,exist_ok=False)
import FreeCAD as A
import Part,PartDesign,Sketcher
assert A.Version()[:3]==['1','1','4'];assert Part.OCC_VERSION=='7.8.1'
fields=['Depth','Diameter','ThreadType','ThreadSize','Threaded','HoleCutType','HoleCutCustomValues','HoleCutDepth','HoleCutDiameter','HoleCutCountersinkAngle']
def snap(d):
 return {h.Name:dict(label=h.Label,properties={k:(getattr(h,k).Value if hasattr(getattr(h,k),'Value') else getattr(h,k)) for k in fields},expressions=dict(h.ExpressionEngine),profile=h.Profile[0].Name,base=h.BaseFeature.Name if h.BaseFeature else None,valid=h.isValid(),volume=h.Shape.Volume,area=h.Shape.Area) for h in d.Objects if h.TypeId=='PartDesign::Hole'}
def restore_direct(d,p):
 with zipfile.ZipFile(p) as z:r=E.fromstring(z.read('Document.xml'))
 for h in d.Objects:
  if h.TypeId!='PartDesign::Hole':continue
  q=r.find("./ObjectData/Object[@name='"+h.Name+"']/Properties")
  prop=q.find("Property[@name='ThreadSize']");ls=prop.find('CustomEnumList');choices=[e.get('value') for e in ls] if ls is not None else h.getEnumerationsOfProperty('ThreadSize')
  desired=choices[int(prop.find('Integer').get('value'))]
  if h.ThreadSize!=desired:h.ThreadSize=desired
  if q.find("Property[@name='HoleCutCustomValues']/Bool").get('value') in ['true','1']:
   if not h.HoleCutCustomValues:h.HoleCutCustomValues=True
   for k in ['HoleCutCountersinkAngle','HoleCutDiameter','HoleCutDepth']:
    value=float(q.find("Property[@name='"+k+"']/Float").get('value'))
    if getattr(h,k).Value!=value:setattr(h,k,value)
 for h in d.Objects:
  if h.TypeId=='PartDesign::Hole':h.touch()
 d.recompute()
def prop(r,n,k):return r.find("./ObjectData/Object[@name='"+n+"']/Properties/Property[@name='"+k+"']")
def derive(src,name,mutate):
 p=out/(name+'.FCStd')
 with zipfile.ZipFile(src) as r,zipfile.ZipFile(p,'x',zipfile.ZIP_DEFLATED) as w:
  for info in r.infolist():
   data=r.read(info.filename)
   if info.filename=='Document.xml':
    root=E.fromstring(data);mutate(root);data=E.tostring(root,encoding='utf-8',xml_declaration=True)
   w.writestr(info,data)
 return p
base=F/'native-controls3/multi-valid.FCStd'
def index_bad(r):prop(r,'HoleB','ThreadSize').find('Integer').set('value','99999')
def missing_integer(r):
 q=prop(r,'HoleB','ThreadSize');q.remove(q.find('Integer'))
def enum_type(r):prop(r,'HoleB','ThreadSize').set('type','App::PropertyString')
def bad_bool(r):prop(r,'HoleB','HoleCutCustomValues').find('Bool').set('value','TRUE')
def nan(r):prop(r,'HoleB','HoleCutDepth').find('Float').set('value','nan')
def missing_angle(r):
 q=r.find("./ObjectData/Object[@name='HoleB']/Properties");q.remove(prop(r,'HoleB','HoleCutCountersinkAngle'))
def removed_hole(r):
 q=r.find('ObjectData');q.remove(q.find("Object[@name='HoleB']"));q.set('Count',str(len(q)))
def reorder(r):
 q=r.find('ObjectData');q[:]=list(reversed(list(q)))
def bool_numeric(r):prop(r,'HoleB','HoleCutCustomValues').find('Bool').set('value','0')
def without_enumlist(r):
 q=prop(r,'HoleB','ThreadSize');ls=q.find('CustomEnumList')
 if ls is not None:q.remove(ls)
cases=[dict(name='reverse-objectdata',input=str(derive(base,'reverse-objectdata',reorder)),accepted=True),
 dict(name='numeric-bool',input=str(derive(base,'numeric-bool',bool_numeric)),accepted=True),
 dict(name='builtin-enum-domain',input=str(derive(base,'builtin-enum-domain',without_enumlist)),accepted=True)]
for name,fn in [('bad-enum-index',index_bad),('missing-enum-value',missing_integer),('wrong-enum-type',enum_type),('bad-bool',bad_bool),('nonfinite',nan),('late-missing-angle',missing_angle),('missing-hole-objectdata',removed_hole)]:
 cases.append(dict(name=name,input=str(derive(base,name,fn)),accepted=False))
# Lexically reversed feature names, top-face supports, dependent expressions and profile placement.
d=A.newDocument('SupportedHoles')
try:
 body=d.addObject('PartDesign::Body','Body');b=body.newObject('PartDesign::AdditiveBox','Base');b.Length=40;b.Width=40;b.Height=20;d.recompute()
 face=next('Face'+str(i) for i,f in enumerate(b.Shape.Faces,1) if f.normalAt(0,0).z>.9)
 for name,x,depth in [('HoleZ',10,6),('HoleA',30,3)]:
  p=body.newObject('Sketcher::SketchObject','Profile'+name[-1]);p.AttachmentSupport=(b,[face]);p.MapMode='FlatFace';p.AttachmentOffset.Base=A.Vector(x,10,0)
  p.addGeometry(Part.Circle(A.Vector(),A.Vector(0,0,1),2 if name=='HoleZ' else 1),False);d.recompute()
  h=body.newObject('PartDesign::Hole',name);h.Profile=(p,[]);h.Diameter=4 if name=='HoleZ' else 2;h.Depth=depth;h.DrillPoint='Flat'
  if name=='HoleA':h.setExpression('Depth','HoleZ.Depth / 2')
  d.recompute();assert h.isValid(),h.getStatusString()
 source=out/'supported.FCStd';d.saveAs(str(source))
finally:A.closeDocument(d.Name)
cases.append(dict(name='supported',input=str(source),accepted=True))
def stale_support(r):
 prop(r,'HoleZ','Depth').find('Float').set('value','8')
 prop(r,'Base','Height').find('Float').set('value','24')
 reorder(r)
cases.append(dict(name='supported-stale-parent',input=str(derive(source,'supported-stale-parent',stale_support)),accepted=True))
def stale_hole(r):prop(r,'HoleZ','Depth').find('Float').set('value','8')
cases.append(dict(name='supported-stale-hole',input=str(derive(source,'supported-stale-hole',stale_hole)),accepted=True))
d=A.openDocument(str(source))
try:
 d.getObject('Base').Height=24;d.getObject('HoleZ').Depth=8;d.recompute();assert all(o.isValid() for o in d.Objects)
 parent=out/'supported-parent-edited.FCStd';d.saveAs(str(parent))
finally:A.closeDocument(d.Name)
cases.append(dict(name='supported-parent-edited',input=str(parent),accepted=True))
for c in cases:
 if not c['accepted']:continue
 before=set(A.listDocuments())
 try:
  d=A.openDocument(c['input']);restore_direct(d,c['input'])
  c['native_status']={o.Name:o.getStatusString() for o in d.Objects}
  c['accepted']=all(o.isValid() for o in d.Objects)
  if not c['accepted']:
   print(c['name'],'native rejected',c['native_status'],flush=True);continue
  c['expected']=snap(d);c['body_tip']=d.getObject('Body').Tip.Name
  p=out/('control-'+c['name']+'.FCStd');d.saveAs(str(p));c['control']=str(p)
 finally:
  for n in set(A.listDocuments())-before:A.closeDocument(n)
(out/'cases.json').write_text(json.dumps(dict(version=A.Version(),occt=Part.OCC_VERSION,cases=cases,remaining_documents=list(A.listDocuments())),indent=2))
print('Created',len(cases),'challenges;',sum(c['accepted'] for c in cases),'direct-native valid controls; no documents left',not A.listDocuments())
