"""Host-only production descriptor/worker dispatch tests with explicit doubles."""
import ast
import math
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace as NS
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
from plane_offset import describe

class V:
    def __init__(self,x,y,z): self.x,self.y,self.z=x,y,z
class Frame:
    def __init__(self,origin,sign=1): self.Base=V(*origin);self.sign=sign
    Rotation=property(lambda s:s)
    Matrix=property(lambda s:NS(A=[s.Base.x,s.Base.y,s.Base.z,s.sign]))
    def multVec(self,v): return V(v.z*self.sign,v.y,-v.x*self.sign)
    def multiply(self,offset):
        return Frame((self.Base.x-self.sign*offset.Base.z,self.Base.y,self.Base.z),self.sign)
def fixture(length=38.1,flipped=False):
    source=NS(TypeId='PartDesign::Plane',Name='Source',ID=2)
    offset=NS(Base=V(0,0,length),Rotation=NS(Angle=0,Q=(0.,0.,0.,1.)))
    offset.inverse=lambda:offset
    sign=-1 if flipped else 1
    return NS(TypeId='PartDesign::Plane',Name='Plane',ID=3,MapMode='FlatFace',AttachmentSupport=[(source,('',))],
        AttachmentOffset=offset,MapReversed=flipped,getGlobalPlacement=lambda:Frame((20+sign*length,30,40),sign))

class Tests(unittest.TestCase):
    def test_world_frame_nested_transformed_flip_and_readonly(self):
        for flip in (False,True):
            o=fixture(flipped=flip);before=vars(o).copy();d=describe(o,7)
            self.assertEqual(d['origin'],[20,30,40]);self.assertEqual(d['axis'],[-1 if flip else 1,0,0])
            self.assertEqual(d['length'],38.1);self.assertEqual(d['support_id'],2);self.assertEqual(vars(o),before)
    def test_other_native_placements_not_reinterpreted(self):
        for length in (-1,float('nan'),float('inf')): self.assertIsNone(describe(fixture(length),7))
        o=fixture();o.AttachmentOffset.Base.x=1;self.assertIsNone(describe(o,7))
        o=fixture();o.AttachmentOffset.Rotation.Q=(0.5,0.,0.,0.8660254037844386);self.assertIsNone(describe(o,7))
        o=fixture();o.MapMode='ParallelPlane';self.assertIsNone(describe(o,7))
    def test_tiny_native_rotation_is_not_silently_discarded(self):
        o=fixture()
        # Even when the reported angle is zero, native quaternion vector
        # components identify an actual offset rotation that this task cannot edit.
        o.AttachmentOffset.Rotation.Q=(5e-13,0.,0.,1.)
        self.assertIsNone(describe(o,7))
        o.AttachmentOffset.Rotation.Q=(0.,0.,0.,-1.)
        self.assertIsNotNone(describe(o,7))

    def test_production_worker_negative_native_parser_permissions_and_setter_dispatch(self):
        worker=Path(__file__).resolve().parents[1]/'backend/freecad_worker.py'
        tree=ast.parse(worker.read_text());function=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='apply_native_edit')
        # Execute only the production offset branch, with explicit host doubles.
        branch=next(n for n in ast.walk(function) if isinstance(n,ast.If) and ast.unparse(n.test)=="op == 'plane_offset'")
        module=ast.Module(body=[ast.FunctionDef(name='offset',args=ast.arguments(posonlyargs=[],args=[ast.arg(arg='doc'),ast.arg(arg='edit')],kwonlyargs=[],kw_defaults=[],defaults=[]),body=[ast.Assign(targets=[ast.Name(id='obj',ctx=ast.Store())],value=ast.Call(func=ast.Name(id='object_in',ctx=ast.Load()),args=[ast.Name(id='doc',ctx=ast.Load()),ast.Subscript(value=ast.Name(id='edit',ctx=ast.Load()),slice=ast.Constant('object'),ctx=ast.Load())],keywords=[]))]+branch.body,decorator_list=[])],type_ignores=[])
        obj=fixture();body=NS(ID=4);objects={'Plane':obj,'Source':obj.AttachmentSupport[0][0],'Body':body};obj.ExpressionEngine=[]
        calls=[];parsed=[];checked=[]
        def fail(code,message): raise ValueError(message)
        def parse(value,unit): parsed.append((value,unit));return -1 if isinstance(value,dict) else value
        env=dict(object_in=lambda doc,name:objects[name],owner_body=lambda o:body,fail=fail,math=math,
            DocumentUnits=NS(value=parse),writable_property=lambda o,p:checked.append(p),
            apply_native_edit=lambda doc,edit:calls.append(edit))
        exec(compile(ast.fix_missing_locations(module),'production-offset-branch','exec'),env)
        edit=dict(object='Plane',body='Body',body_id=4,support='Source',support_id=2,feature_id=3,magnitude={'quantity_input':'-1 in','default_unit':'mm'},flipped=True)
        with self.assertRaisesRegex(ValueError,'Flip'):env['offset'](None,edit)
        self.assertEqual(parsed[0][0],edit['magnitude']);self.assertEqual(calls,[])
        edit['magnitude']=38.1;env['offset'](None,edit)
        self.assertEqual(checked,['AttachmentOffset','MapReversed']);self.assertEqual(calls[-1]['property'],'AttachmentOffset')
        self.assertEqual(calls[-1]['value']['position'],[0,0,38.1])
        edit['support_id']=99
        with self.assertRaisesRegex(ValueError,'support changed'):env['offset'](None,edit)

if __name__=='__main__':unittest.main()
