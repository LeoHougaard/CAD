"""Independent stock-native rollback oracle for original ordered raw members."""
import hashlib
import io
import json
from pathlib import Path
import sys
import zipfile
import FreeCAD as App
import Part

input_path, members_path, output = map(Path, sys.argv[1:4])
output.mkdir()
expected = json.loads(input_path.read_text())
paths = [Path(expected['shared_path']), Path(expected['properties_path'])]
sha = lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
before_files = {str(path): sha(path) for path in paths}
assert 'freecad_worker' not in sys.modules
shared = App.openDocument(str(paths[0]))
doc = App.openDocument(str(paths[1]))
doc.UndoMode = 1
App.setActiveDocument(doc.Name)
for index in range(7):
    prop = 'Ref' + str(index)
    kind = doc.A.getTypeIdOfProperty(prop)
    value = [(doc.B, [''])] if 'SubList' in kind else [doc.B] if 'List' in kind else (doc.B, ['']) if 'Sub' in kind else doc.B
    doc.openTransaction('Edit')
    setattr(doc.A, prop, value)
    doc.recompute()
    doc.commitTransaction()
    for obj in doc.Objects:
        assert obj.Shape.isValid()
for obj in doc.Objects:
    for face in obj.Shape.Faces:
        face.tessellate(.1)
doc.B.addProperty('App::PropertyXLink', 'NativeReference')
doc.B.NativeReference = shared.SharedBox
doc.B.setExpression('Length', shared.Name + '#SharedBox.Length')
doc.recompute()
for obj in doc.Objects:
    assert obj.Shape.isValid()
doc.A.addProperty('App::PropertyXLink', 'SecondReference')
doc.A.SecondReference = shared.SharedBox


def capture(label):
    with zipfile.ZipFile(io.BytesIO(doc.B.dumpContent())) as archive:
        members = [(entry.filename, archive.read(entry)) for entry in archive.infolist()]
    for name, data in members:
        (output / (label + '-' + name)).write_bytes(data)
    return dict(members=members, native_id=doc.B.ID, uid=str(doc.Uid), undo=doc.UndoCount, redo=doc.RedoCount,
        undo_names=list(doc.UndoNames), redo_names=list(doc.RedoNames), state=list(doc.B.State),
        target=(doc.B.NativeReference.Document.Name, doc.B.NativeReference.Name, doc.B.NativeReference.ID),
        geometry=doc.B.Shape.exportBrepToString())


def compare_members(actual, claimed):
    assert [name for name, _ in actual] == [name for name, _ in claimed]
    for (name, data), (_, digest) in zip(actual, claimed):
        original = (members_path / (digest + '-' + name)).read_bytes()
        assert hashlib.sha256(original).hexdigest() == digest
        assert data == original


before = capture('before')
doc.openTransaction('Edit')
doc.B.NativeReference = None
# The real graph rejection precedes native recompute while SecondReference
# remains. Native abort itself restores the edge and marks the property touched.
doc.abortTransaction()
after = capture('after')
for phase, actual in [('before', before), ('after', after)]:
    value = expected[phase]
    compare_members(actual['members'], value['object']['content'])
    assert actual['native_id'] == value['object']['id'] and actual['uid'] == value['owner']['uuid']
    for field in ['undo', 'redo', 'undo_names', 'redo_names']:
        assert actual[field] == value['owner'][field]
    assert actual['state'] == value['object']['state']
    assert actual['target'] in [tuple(edge) for edge in value['object']['native_references']]
assert before['members'][1:] == after['members'][1:]
first = Part.Shape()
first.importBrepFromString(before['geometry'])
second = Part.Shape()
second.importBrepFromString(after['geometry'])
assert first.cut(second).Volume + second.cut(first).Volume == 0
negative = []
for index in range(len(after['members'])):
    name, data = after['members'][index]
    corrupted = bytes([data[0] ^ 1]) + data[1:]
    (output / ('corrupted-' + name)).write_bytes(corrupted)
    trial = list(after['members'])
    trial[index] = (name, corrupted)
    try:
        compare_members(trial, expected['after']['object']['content'])
    except AssertionError:
        negative.append(name)
    else:
        raise AssertionError('Raw member corruption was accepted: ' + name)
for name in list(App.listDocuments()):
    App.closeDocument(name)
assert not App.listDocuments() and {str(path): sha(path) for path in paths} == before_files
result = dict(raw_ordered_members_exact_both_phases=True, shape_members_exact=True, geometry_symmetric_difference=0,
    native_ids_history_and_target_exact=True, corrupted_private_members_rejected=negative,
    source_files=before_files, script_sha256=sha(__file__), input_sha256=sha(input_path),
    freecad_version=list(App.Version()), occt=Part.OCC_VERSION,
    native_modules={name: dict(path=module.__file__, sha256=sha(module.__file__)) for name, module in
        [('FreeCAD', App), ('Part', Part)] if getattr(module, '__file__', None)},
    product_adapter_imported=False)
(output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result))
