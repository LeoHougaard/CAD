"""Portable contract tests. Set booleans are NOT OCCT/application proof."""
import importlib.util
import json
from pathlib import Path
import sys
from types import SimpleNamespace as NS
import unittest
from unittest.mock import patch

BACKEND = Path(__file__).resolve().parents[1] / 'backend'
spec = importlib.util.spec_from_file_location('portable_planar_regions', BACKEND / 'planar_regions.py')
R = importlib.util.module_from_spec(spec)
with patch.dict(sys.modules, {'FreeCAD': NS(Vector=lambda *v: v), 'Part': NS()}):
    spec.loader.exec_module(R)


class Face:
    """Exact finite sets exercise selection algebra and copy ownership only."""
    ShapeType = 'Face'
    Surface = NS(TypeId='Part::GeomPlane')
    Solids = []
    Wires = [NS(isClosed=lambda: True)]
    def __init__(self, cells): self.cells = set(cells)
    @property
    def Area(self): return len(self.cells)
    @property
    def Faces(self): return [self] if self.cells else []
    def copy(self): return Face(self.cells)
    def cut(self, other): return Face(self.cells - other.cells)
    def fuse(self, other): return Face(self.cells | other.cells)
    def removeSplitter(self): return self
    def isValid(self): return True
    def normalAt(self, *args): return NS(dot=lambda n: 1)


class Contracts(unittest.TestCase):
    def setUp(self):
        self.doc = NS(Uid='document-A')
        self.body = NS(ID=10, TypeId='PartDesign::Body', Group=[])
        self.source = NS(ID=20, Name='Sketch', Document=self.doc, GeometryCount=3,
                         getGeometryId=lambda i: [101, 205, 309][i],
                         getParentGeoFeatureGroup=lambda: self.body, OutListRecursive=[],
                         Placement=NS(Rotation=NS(multVec=lambda v: v)))
        self.support = NS(ID=30, Name='Pad', Document=self.doc,
                          getParentGeoFeatureGroup=lambda: self.body, OutListRecursive=[],
                          isDerivedFrom=lambda t: False, TypeId='PartDesign::Feature')
        self.body.Group = [self.source, self.support]
        self.identity = {'object':'Pad', 'native_id':30, 'mapped_face':'native-face',
                         'history':(30, 'origin', []), 'map_mode':'FlatFace'}
        self.face = Face(range(12))
        self.source.AttachmentSupport = [(self.support, ['Face6'])]
        self.source.MapMode = 'FlatFace'
        self.support.Shape = NS(Tag=30, isNull=lambda: False, isValid=lambda: True,
            getElementMappedName=lambda n: 'native-face',
            getElementIndexedName=lambda n: 'Face6', getElement=lambda n: self.face,
            getElementHistory=lambda n: (30, 'origin', []))

    def loops(self):
        # Adjacent top-level atoms and one nested hole; geometry IDs are opaque.
        return {(101,): {'face':Face([1,2,3]), 'children':[(205,)], 'parent':None, 'region':'a'},
                (205,): {'face':Face([2]), 'children':[], 'parent':(101,), 'region':'b'},
                (309,): {'face':Face([4,5]), 'children':[], 'parent':None, 'region':'c'}}

    def augment(self, loops=None):
        loops = self.loops() if loops is None else loops
        with patch.object(R, 'support_face', return_value=(self.support, self.identity, self.face.copy())):
            R.add_support_boundary(self.source, loops, [101,205,309])
        return loops

    def test_partition_nested_adjacent_and_union(self):
        loops = self.augment()
        outside = R.selection(loops, [loops[()]['region']])
        self.assertEqual(outside.cells, set(range(12)) - {1,2,3,4,5})
        self.assertEqual(R.selection(loops, [v['region'] for v in loops.values()]).cells, set(range(12)))
        self.assertEqual(R.selection(loops, [loops[(101,)]['region']]).cells, {1,3})
        self.assertEqual(self.face.cells, set(range(12)))
        key = json.loads(loops[()]['region'])
        self.assertEqual(key['boundary']['outer'], [])
        self.assertEqual(key['binding']['source_geometry_ids'], [101,205,309])

    def test_identity_denies_old_face_source_document_body_and_geometry(self):
        old = self.augment()[()]['region']
        for obj, field, changed in [(self.source, 'ID', 21), (self.doc, 'Uid', 'other'),
                                    (self.body, 'ID', 11), (self.source, 'getGeometryId', lambda i: i+800)]:
            with patch.object(obj, field, changed):
                with self.assertRaises(R.RegionError): R.selection(self.augment(), [old])
        for field, value in [('native_id',31), ('mapped_face','replaced'), ('history',(31,'new',[]))]:
            with patch.dict(self.identity, {field:value}):
                with self.assertRaises(R.RegionError): R.selection(self.augment(), [old])

    def test_out_of_support_rejected(self):
        loops = self.loops(); loops[(309,)]['face'] = Face([99])
        with self.assertRaisesRegex(R.RegionError, 'inside'): self.augment(loops)

    def test_native_resolution_and_detached_copy(self):
        obj, identity, face = R.support_face(self.source)
        self.assertIs(obj, self.support); self.assertIsNot(face, self.face)
        self.assertEqual(identity, self.identity)

    def test_support_admission_denials(self):
        for obj, field, value in [(self.support, 'Document', NS()),
                (self.support, 'OutListRecursive', [self.source]),
                (self.source, 'MapMode', 'Deactivated'),
                (self.source, 'AttachmentSupport', [(self.support, ['Face6','Face7'])]),
                (self.support.Shape, 'Tag', 999),
                (self.support.Shape, 'getElementIndexedName', lambda n: 'Face7'),
                (Face, 'Surface', NS(TypeId='Part::GeomCylinder'))]:
            with patch.object(obj, field, value):
                with self.assertRaises(R.RegionError): R.support_face(self.source)

    def test_token_binds_live_frame_and_support_brep(self):
        import math
        def placement(x):
            return NS(Base=NS(x=x, y=0.0, z=0.0), Rotation=NS(Q=(0.0,0.0,0.0,1.0), multVec=lambda v:v))
        self.source.Shape = NS(exportBrepToString=lambda:'sketch')
        self.source.Placement = placement(0.0)
        self.source.getGlobalPlacement = lambda:placement(1234.000000001)
        self.support.Placement = placement(0.0)
        self.support.getGlobalPlacement = lambda:placement(0.0)
        self.support.Shape.exportBrepToString = lambda:'support-brep'
        old = R.source_token(self.source)
        with patch.object(self.source, 'getGlobalPlacement',
                          lambda:placement(math.nextafter(1234.000000001, math.inf))):
            self.assertNotEqual(R.source_token(self.source), old)
        with patch.object(self.support, 'getGlobalPlacement',
                          lambda:placement(math.nextafter(0.0, math.inf))):
            self.assertNotEqual(R.source_token(self.source), old)
        with patch.object(self.support.Shape, 'exportBrepToString', lambda:'changed'):
            self.assertNotEqual(R.source_token(self.source), old)

    def profile(self, version):
        obj = NS(TypeId='Part::Part2DObjectPython', Document=self.doc,
            PropertiesList=list(R.V1_FIELDS), RegionVersion=version,
            RegionDocumentUid='document-A', RegionSource=self.source, RegionSourceId=20,
            RegionBodyId=10, RegionGeometryIds=[101,205,309],
            getParentGeoFeatureGroup=lambda:self.body)
        self.body.Group.append(obj)
        return obj

    def test_v1_restore_uses_original_internal_boundaries(self):
        obj = self.profile(1)
        with patch.object(R, 'boundaries', return_value=(self.loops(), [101,205,309])) as b:
            R.resolve(obj); b.assert_called_once_with(self.source, include_support=False)
        self.assertIsNone(R.RegionProfile().__getstate__())
        R.RegionProfile().__setstate__(None)
        with self.assertRaises(R.RegionError): R.RegionProfile().__setstate__({'solid':'no'})

    def test_v2_fields_protection_restore_and_stale_support(self):
        obj = self.profile(2)
        with self.assertRaises(R.RegionError): R.resolve(obj)
        obj.PropertiesList += list(R.SUPPORT_FIELDS)
        obj.RegionSupport = self.support
        obj.getTypeIdOfProperty = lambda p: {'RegionSupport':'App::PropertyLink', 'RegionSupportIdentity':'App::PropertyString'}[p]
        obj.getPropertyStatus = lambda p: ['ReadOnly']
        obj.RegionSupportIdentity = json.dumps(self.identity, sort_keys=True, separators=(',', ':'))
        with patch.object(R, 'boundaries', return_value=(self.loops(), [101,205,309])) as b:
            R.resolve(obj); b.assert_called_once_with(self.source, include_support=True)
            for prop in R.FIELDS | {'Shape','Placement','Proxy'}: self.assertTrue(R.protected(obj, prop))
            obj.RegionSupportIdentity = '{}'
            with self.assertRaises(R.RegionError): R.resolve(obj)

    def test_open_tail_graph_unchanged(self):
        spec = importlib.util.spec_from_file_location('portable_graph', BACKEND / 'region_graph.py')
        graph = importlib.util.module_from_spec(spec); spec.loader.exec_module(graph)
        import math
        edges = [(101,0,1,0,math.pi), (205,1,2,math.pi/2,-math.pi/2),
                 (309,2,0,-3*math.pi/4,math.pi/4), (777,1,3,0,math.pi)]
        walks = graph.cycles(edges)
        self.assertEqual(len(walks), 2)
        self.assertTrue(all({gid for gid,d in w} == {101,205,309} for w in walks))

    def test_missing_native_api_is_explicit_unsupported(self):
        del self.support.Shape.getElement
        with self.assertRaisesRegex(R.RegionError, 'API unavailable'): R.support_face(self.source)


if __name__ == '__main__': unittest.main()
