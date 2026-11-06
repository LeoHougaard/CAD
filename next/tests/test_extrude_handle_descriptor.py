"""Native-free execution of the production read-only descriptor with test doubles.

These tests do not qualify FreeCAD coordinate conventions or native previews.
Run: python -B tests/test_extrude_handle_descriptor.py
"""
import math
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace as NS

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
from extrude_handle import describe


class V:
    def __init__(self, x, y, z): self.x, self.y, self.z = x, y, z
    @property
    def Length(self): return math.sqrt(self.dot(self))
    def dot(self, v): return self.x*v.x+self.y*v.y+self.z*v.z
    def __truediv__(self, n): return V(self.x/n, self.y/n, self.z/n)


class Frame:
    # +90 degrees about Y: local normal becomes world X.
    Rotation = property(lambda self: self)
    def __init__(self, offset=(20, 30, 40)): self.offset = offset
    def multVec(self, v):
        # Rotation.multVec must omit translation.
        return V(v.z, v.y, -v.x)
    def multiply(self, inverse):
        frame = self
        return NS(Rotation=self, multVec=lambda v: V(v.z+frame.offset[0], v.y+frame.offset[1], -v.x+frame.offset[2]))


def fixture():
    frame = Frame()
    placement = NS(inverse=lambda: None)
    profile = NS(Name='Profile', ID=88, Shape=NS(isNull=lambda: False, CenterOfMass=V(2,3,0)),
                 Placement=placement, getGlobalPlacement=lambda: frame)
    return NS(TypeId='PartDesign::Pad', Name='Pad', ID=89, Type='Length',
              Profile=(profile, []), Length=NS(Value=25.4), Midplane=False, SideType='One side',
              Reversed=False, UseCustomVector=False, AlongSketchNormal=True, ReferenceAxis=None,
              Direction=V(1,1,1), Placement=placement, getGlobalPlacement=lambda: frame)


class DescriptorTests(unittest.TestCase):
    def test_transformed_support_and_native_quantity(self):
        obj=fixture(); before=vars(obj).copy(); result=describe(obj,17)
        self.assertEqual(result['origin'],[20,33,38])
        self.assertEqual(result['axis'],[1,0,0])
        self.assertEqual(result['signed_length'],25.4)
        self.assertEqual((result['feature_id'],result['profile_id'],result['revision']),(89,88,17))
        self.assertEqual(vars(obj),before)

    def test_signed_zero_and_symmetric(self):
        obj=fixture(); obj.Reversed=True; obj.SideType='Symmetric'
        self.assertEqual(describe(obj,2)['signed_length'],-25.4)
        self.assertEqual(describe(obj,2)['factor'],0.5)
        obj.Length.Value=0
        self.assertEqual(describe(obj,2)['signed_length'],0)

    def test_custom_direction_normal_length_projection(self):
        obj=fixture(); obj.UseCustomVector=True; obj.Direction=V(1,0,1)
        result=describe(obj,3)
        self.assertAlmostEqual(result['factor'],math.sqrt(2))
        obj.AlongSketchNormal=False
        self.assertEqual(describe(obj,3)['factor'],1)
        obj.AlongSketchNormal=True; obj.Direction=V(1,0,0)
        self.assertIsNone(describe(obj,3))

    def test_reference_direction(self):
        obj=fixture(); obj.AlongSketchNormal=False
        curve=NS(isDerivedFrom=lambda kind: kind=='Part::GeomLine')
        edge=NS(ShapeType='Edge',Curve=curve,FirstParameter=0,tangentAt=lambda p:V(0,1,0))
        obj.ReferenceAxis=(NS(getSubObject=lambda name:edge, getGlobalPlacement=obj.getGlobalPlacement,
                              Placement=obj.Placement),['Edge1'])
        self.assertEqual(describe(obj,3)['axis'],[0,1,0])

    def test_invalid_values_modes_and_missing_geometry(self):
        for value in [float('nan'),float('inf'),-1]:
            obj=fixture();obj.Length.Value=value;self.assertIsNone(describe(obj,1))
        obj=fixture();obj.Type='Up to face';self.assertIsNone(describe(obj,1))
        obj=fixture();obj.SideType='Two sides';self.assertIsNone(describe(obj,1))
        obj=fixture();obj.Profile=(None,[]);self.assertIsNone(describe(obj,1))


if __name__=='__main__': unittest.main()
