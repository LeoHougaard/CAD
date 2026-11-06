"""Stock-native finite-model/identity readback. No product adapter or file writes."""
import hashlib
import json
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
import zipfile
import FreeCAD as App
import Part
import Assembly

root = Path(sys.argv[1])
output = Path(sys.argv[2])
files = sorted(root.rglob('*.FCStd'))
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
before = {str(p): sha(p) for p in files}
results = {}
for label in ['deep', 'cycle']:
    path = root / ('check4-' + label) / 'Saved.FCStd'
    with zipfile.ZipFile(path) as archive:
        xml = ET.fromstring(archive.read('Document.xml'))
    ids = {node.get('name'): int(node.get('id')) for node in xml.findall('./Objects/Object')}
    doc = App.openDocument(str(path))
    doc.recompute()
    assert {obj.Name: obj.ID for obj in doc.Objects} == ids
    link = doc.Instance
    value = json.loads(link.CadExternalSource)
    target = link.LinkedObject
    assert link.getTypeIdOfProperty('LinkedObject') == 'App::PropertyXLink'
    assert str(target.Document.Uid) == value['document_uid']
    assert (target.Name, target.ID, target.TypeId) == (value['object'], value['object_id'], value['type'])
    for revision in [value, *value['dependencies']]:
        assert sha(Path(revision['path'])) == revision['sha256']
    control = App.newDocument('FiniteControl')
    box = control.addObject('Part::Box', 'Box')
    control.recompute()
    shapes = {}
    for name in ['Instance', 'DirtyContent']:
        obj = doc.getObject(name)
        delta = obj.Shape.cut(box.Shape).Volume + box.Shape.cut(obj.Shape).Volume
        assert delta == 0, (label, name, delta)
        shapes[name] = dict(symmetric_difference=delta, volume=obj.Shape.Volume, area=obj.Shape.Area)
    results[label] = dict(native_ids=ids, source=value, finite_shapes=shapes,
        native_statuses={obj.Name: dict(state=list(obj.State), status=obj.getStatusString()) for obj in doc.Objects})
    for name in list(App.listDocuments()):
        App.closeDocument(name)
assert not App.listDocuments()
assert {str(p): sha(p) for p in files} == before
output.write_text(json.dumps(dict(native_version=list(App.Version()), occt=Part.OCC_VERSION,
    results=results, input_sha256=before, exact_source_bytes=True), indent=2) + '\n')
