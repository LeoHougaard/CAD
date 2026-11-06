"""Host topology and state contracts only. No geometry or application proof."""
import ast
import importlib.util
import itertools
import json
import math
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / 'backend'))
sys.path.insert(0, str(HERE))
import region_graph as G
from support_face_regions_portable import R, Contracts


class Topology(unittest.TestCase):
    def graph(self):
        # Two analytic arc *topology* records plus a diameter. Native-free
        # tangents are stipulated test inputs; they do not prove native splits.
        return [('circle:0',0,1,math.pi/2,math.pi/2),
                ('circle:1',1,0,-math.pi/2,-math.pi/2),
                ('diameter:0',1,0,0,math.pi)]

    def test_semicell_topology_has_two_cells_and_exterior(self):
        walks = G.cycles(self.graph())
        self.assertEqual(len(walks), 3)
        bounded = [w for w in walks if any(g == 'diameter:0' for g,d in w)]
        self.assertEqual(len(bounded),2)
        keys = {tuple(sorted(g for g,d in w)) for w in bounded}
        self.assertEqual(len(keys),2)
        self.assertEqual({tuple(sorted('circle' if g.startswith('circle') else 'diameter' for g,d in w)) for w in bounded},
                         {('circle','diameter')})

    def test_native_enumeration_independent(self):
        baseline = sorted(G.cycles(self.graph()))
        for permutation in itertools.permutations(self.graph()):
            self.assertEqual(sorted(G.cycles(permutation)), baseline)

    def test_multijunction_and_tail(self):
        # A square with two crossing internal diagonals, already split at O.
        xy = [(0,0),(2,0),(2,2),(0,2),(1,1),(3,0)]
        pairs = [(0,1),(1,2),(2,3),(3,0),(0,4),(4,2),(1,4),(4,3),(1,5)]
        edges=[]
        for i,(a,b) in enumerate(pairs):
            x,y=xy[a];xx,yy=xy[b]
            edges.append((str(i),a,b,math.atan2(yy-y,xx-x),math.atan2(y-yy,x-xx)))
        walks=G.cycles(edges)
        self.assertEqual(sorted(map(len,walks)),[3,3,3,3,4])
        self.assertTrue(all('8' not in [g for g,d in w] for w in walks))

    def test_junction_signature_and_changed_incidence(self):
        a={'c0':(0,1),'c1':(1,0),'d0':(1,0)}
        b={'d0':(9,7),'c1':(9,7),'c0':(7,9)}
        self.assertEqual(G.junction_signature(a), G.junction_signature(b))
        b['d0']=(7,9)
        self.assertNotEqual(G.junction_signature(a),G.junction_signature(b))

    def test_tangency_is_explicitly_ambiguous(self):
        with self.assertRaisesRegex(ValueError,'Ambiguous'):
            G.cycles([('a',0,1,0,math.pi),('b',0,2,0,math.pi)])


class Version3(Contracts):
    # Inherits all legacy v1/v2 support and state contracts unchanged.
    def partition(self):
        loops=self.augment()
        with patch.object(R,'support_face',return_value=(self.support,self.identity,self.face)):
            R.bind_partition(self.source,loops,[101,205,309],
                             {'junctions':G.junction_signature({'c0':(0,1),'d0':(1,0)})})
        return loops

    def test_v3_binding_and_key_roundtrip(self):
        loops=self.partition()
        for v in loops.values():
            parsed=json.loads(v['region'])
            self.assertEqual(parsed['version'],3)
            self.assertEqual(parsed['binding']['support']['mapped_face'],'native-face')
        old=loops[()]['region']
        with patch.object(self.source,'ID',999):
            with self.assertRaises(R.RegionError):R.selection(self.partition(),[old])
        with patch.object(self.doc,'Uid','foreign'):
            with self.assertRaises(R.RegionError):R.selection(self.partition(),[old])
        self.assertEqual(json.loads(old)['boundary']['outer'],[])
        self.assertEqual(R.selection(loops,[v['region'] for v in loops.values()]).cells,set(range(12)))

    def test_new_version_does_not_rewrite_saved_v1_v2_keys(self):
        original=R._key((101,),[(205,)])
        self.assertEqual(original,'{"outer":[101],"holes":[[205]]}')
        self.assertEqual(json.loads(self.augment()[(101,)]['region'])['version'],2)

    def test_source_assignment_absent_in_partition(self):
        tree=ast.parse(Path(R.__file__).read_text())
        fn=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='derived_fragments')
        for node in ast.walk(fn):
            if isinstance(node,(ast.Assign,ast.AugAssign,ast.AnnAssign)):
                targets=node.targets if isinstance(node,ast.Assign) else [node.target]
                self.assertFalse(any(isinstance(t,ast.Attribute) and isinstance(t.value,ast.Name)
                                     and t.value.id=='source' for t in targets))
            if isinstance(node,ast.Call) and isinstance(node.func,ast.Attribute):
                self.assertNotIn(node.func.attr,{'addGeometry','delGeometry','setGeometryId','setDatum','movePoint','setConstruction','fixTolerance'})

del Contracts  # Legacy suite is already inherited by Version3.
if __name__=='__main__':unittest.main()
