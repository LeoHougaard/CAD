"""Independent readback. No worker or adapter imports."""
from pathlib import Path
import argparse, json, math
import FreeCAD as App
import Part, Sketcher

here=Path(__file__).resolve().parent
parser=argparse.ArgumentParser();parser.add_argument('--input',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
data=args.input
prefs=App.ParamGet('User parameter:BaseApp/Preferences/Units')
before=prefs.GetContents(); schema=App.Units.getSchema(); records=[]
for short in ['Fillet','Chamfer']:
    path=data/f'r1-01-{short}/accepted.FCStd'
    doc=App.openDocument(str(path))
    try:
        assert doc.UnitSystem==doc.getEnumerationsOfProperty('UnitSystem')[3]
        assert doc.getObject(short).Edges==[(1,App.Units.Quantity('0.1 in').Value,App.Units.Quantity('0.1 in').Value)]
        added=[o for o in doc.Objects if o.TypeId=='Part::'+short and o.Name!=short]
        assert len(added)==1
        obj=added[0]
        assert obj.Base==doc.Box and obj.Edges==[(3,App.Units.Quantity('0.06 in').Value,1.)]
        assert obj.Shape.isValid() and obj.Shape.Volume>0
        assert doc.Box.Label=='Redo control'
        records.append({'path':str(path),'units':doc.UnitSystem,'object':obj.Name,'base':obj.Base.Name,'edges':obj.Edges,'volume':obj.Shape.Volume,'valid':True})
    finally:App.closeDocument(doc.Name)
path=data/'r1-02-radians/radian-legacy.FCStd'
doc=App.openDocument(str(path))
try:
    idx=doc.Sketch.ConstraintCount-1
    c=doc.Sketch.Constraints[idx]
    assert c.Type=='Angle' and c.Value==1.0
    assert math.isclose(doc.Sketch.getDatum(idx).Value,App.Units.Quantity('1 rad').Value,rel_tol=1e-15)
    assert '1 rad' in dict(doc.Sketch.ExpressionEngine).values()
    records.append({'path':str(path),'constraint_radians':c.Value,'datum_degrees':doc.Sketch.getDatum(idx).Value,'expressions':list(doc.Sketch.ExpressionEngine)})
finally:App.closeDocument(doc.Name)
assert prefs.GetContents()==before and App.Units.getSchema()==schema
args.output.write_text(json.dumps({'passed':True,'files':records,'global_preferences_unchanged':True},indent=2))
print('3 saved native controls pass without adapter imports')
