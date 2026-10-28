import hashlib, json, sys
from pathlib import Path
import FreeCAD as App
import Part
R = Path(sys.argv[1])
out = R / 'fixtures'
out.mkdir()
leaf = App.newDocument('Leaf')
box = leaf.addObject('Part::Box', 'Box')
box.Length = 10
leaf.recompute()
leaf.saveAs(str(out / 'Leaf.FCStd'))
original = (out / 'Leaf.FCStd').read_bytes()
box.Length = 20
leaf.recompute()
leaf.saveAs(str(out / 'LeafRevision.FCStd'))
App.closeDocument(leaf.Name)
(out / 'Leaf.FCStd').write_bytes(original)
leaf = App.openDocument(str(out / 'Leaf.FCStd'))
wrapper = App.newDocument('Wrapper')
wrapper.saveAs(str(out / 'Wrapper.FCStd'))
link = wrapper.addObject('App::Link', 'Wrapped')
link.setLink(leaf.Box)
link.LinkTransform = True
wrapper.recompute()
wrapper.save()
for name in list(App.listDocuments()): App.closeDocument(name)
destinations = R / 'destinations'
destinations.mkdir()
for stage in ['after_save_as', 'clear_rebind', 'native_save', 'serialized_paths']:
    folder = destinations / stage
    folder.mkdir()
    doc = App.newDocument('ExistingDestination')
    box = doc.addObject('Part::Box', 'ExistingBox')
    box.Length = 9
    doc.recompute()
    doc.saveAs(str(folder / 'Destination.FCStd'))
    App.closeDocument(doc.Name)
    data = (folder / 'Destination.FCStd').read_bytes()
    (folder / 'Destination.20000101-000000.FCBak').write_bytes(data)
    (folder / 'Destination.FCStd1').write_bytes(data)
manifest = {str(p.relative_to(R)): hashlib.sha256(p.read_bytes()).hexdigest() for directory in [out, destinations] for p in directory.rglob('*') if p.is_file()}
(R / 'fixtures-manifest.json').write_text(json.dumps(manifest, indent=2))
print('Generated independent nested native revisions and four existing save destinations; all native documents closed')
