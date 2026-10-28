"""Native full/deep/cyclic source files for portable external-Open controls."""
import hashlib
import json
from pathlib import Path
import sys
import FreeCAD as App
import Part

root = Path(sys.argv[1])
deep = root / 'deep'
deep.mkdir()
leaf = App.newDocument('DeepLeaf')
box = leaf.addObject('Part::Box', 'Box')
leaf.recompute()
leaf.saveAs(str(deep / 'Leaf.FCStd'))
original = (deep / 'Leaf.FCStd').read_bytes()
box.Length = 20
leaf.recompute()
leaf.saveAs(str(deep / 'LeafRevision.FCStd'))
App.closeDocument(leaf.Name)
(deep / 'Leaf.FCStd').write_bytes(original)
leaf = App.openDocument(str(deep / 'Leaf.FCStd'))
middle = App.newDocument('Middle')
middle.saveAs(str(deep / 'Middle.FCStd'))
middle.addObject('App::Link', 'MiddleLink').setLink(leaf.Box)
middle.recompute()
middle.save()
side = App.newDocument('Side')
side.saveAs(str(deep / 'Side.FCStd'))
side.addObject('App::Link', 'SideLink').setLink(leaf.Box)
side.recompute()
side.save()
top = App.newDocument('DeepTop')
top.saveAs(str(deep / 'Top.FCStd'))
top.addObject('App::Link', 'TopLink').setLink(middle.MiddleLink)
top.addObject('App::Link', 'SharedBranch').setLink(side.SideLink)
top.recompute()
top.save()
for name in list(App.listDocuments()):
    App.closeDocument(name)
cycle = root / 'cycle'
cycle.mkdir()
one = App.newDocument('CycleOne')
two = App.newDocument('CycleTwo')
for doc, name in [(one, 'One'), (two, 'Two')]:
    doc.addObject('Part::Box', 'Box')
    doc.recompute()
    doc.saveAs(str(cycle / (name + '.FCStd')))
one.addObject('App::Link', 'ToTwo').setLink(two.Box)
two.addObject('App::Link', 'ToOne').setLink(one.Box)
one.recompute()
two.recompute()
one.save()
two.save()
for name in list(App.listDocuments()):
    App.closeDocument(name)
template = (root / 'destinations/after_save_as/Destination.FCStd').read_bytes()
for name in ['once-save', 'persistent-save', 'persistent-relink']:
    folder = root / name
    folder.mkdir()
    for filename in ['Destination.FCStd', 'Destination.20000101-000000.FCBak', 'Destination.FCStd1']:
        (folder / filename).write_bytes(template)
assert not App.listDocuments()
(root / 'deep-fixture-manifest.json').write_text(json.dumps({str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
    for folder in [deep, cycle] for p in folder.glob('*.FCStd')}, indent=2) + '\n')
