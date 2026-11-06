"""Read saved Hole BRep before Open, against current independent native setters."""
import argparse,json,zipfile,xml.etree.ElementTree as E
from pathlib import Path
import FreeCAD as A
import Part,PartDesign,Sketcher
p=argparse.ArgumentParser();p.add_argument('--input',type=Path,required=True);a=p.parse_args();folder=a.input
def raw(path):
    with zipfile.ZipFile(path) as z:
        root=E.fromstring(z.read('Document.xml'));node=root.find("./ObjectData/Object[@name='Hole']")
        shape=Part.Shape();shape.importBrepFromString(z.read(node.find("./Properties/Property[@name='Shape']/Part").get('file')).decode())
        return node,shape
before,unused=raw(folder/'hole-configured.FCStd');node,saved=raw(folder/'hole-current.FCStd')
assert all(node.find("./Properties/Property[@name='"+k+"']") is None for k in ['_CadSuppressionShape','_CadSuppressionPlacement'])
d=A.openDocument(str(folder/'hole-configured.FCStd'))
try:
    h=d.Hole
    for key in ['ThreadSize','HoleCutType','HoleCutCustomValues','HoleCutCountersinkAngle','HoleCutDiameter','HoleCutDepth']:
        prop=before.find("./Properties/Property[@name='"+key+"']");kind=prop.get('type')
        if kind=='App::PropertyEnumeration':
            values=prop.find('CustomEnumList');choices=[v.get('value') for v in values] if values is not None else h.getEnumerationsOfProperty(key)
            value=choices[int(prop.find('Integer').get('value'))]
        elif kind=='App::PropertyBool':value=prop.find('Bool').get('value') in ['true','1']
        else:value=float(prop.find('Float').get('value'))
        current=getattr(h,key);current=current.Value if hasattr(current,'Value') else current
        if current!=value:
            assert 'ReadOnly' not in h.getEditorMode(key) and 'ReadOnly' not in h.getPropertyStatus(key)
            setattr(h,key,value)
    h.HoleCutDepth=A.Units.Quantity('0.02 in').Value;h.touch();assert d.recompute() is not False
    assert all(o.isValid() for o in d.Objects)
    direct=h.Shape;delta=saved.cut(direct).Volume+direct.cut(saved).Volume
    report=dict(before_saved_open_or_repair=True,csg=delta,volume_delta=saved.Volume-direct.Volume,area_delta=saved.Area-direct.Area,
        native_cut_depth=h.HoleCutDepth.Value,saved_cut_depth=float(node.find("./Properties/Property[@name='HoleCutDepth']/Float").get('value')))
    assert report['saved_cut_depth']==report['native_cut_depth'] and delta==0
    assert abs(report['volume_delta'])<1e-7 and abs(report['area_delta'])<1e-7
    (folder/'native-shape-report.json').write_text(json.dumps(report,indent=2)+'\n')
finally:A.closeDocument(d.Name)
