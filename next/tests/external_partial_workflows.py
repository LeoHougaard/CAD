"""Full native graph equality through Save/Open, including partial autoload."""
import json
import os
from pathlib import Path
import zipfile
import xml.etree.ElementTree as ET
from external_recorded_session import RecordedSession

root = Path(os.environ['CAD_EXTERNAL_ROOT'])
inputs = root / 'partial-input'
runtime = json.loads((root / 'runtime.json').read_text())
out = root / 'partial-public'
out.mkdir()
session = RecordedSession(Path(os.environ['CAD_EXTERNAL_HOST']), runtime, out, 'native')


def call(op, **params):
    if 'document' in params:
        params.setdefault('expected_revision', session.revisions.get(params['document']))
    return session.call(op, **params)


try:
    for label, path, obj, count in [('deep', inputs / 'deep/Top.FCStd', 'TopLink', 3),
                                  ('cycle', inputs / 'cycle/One.FCStd', 'ToTwo', 1)]:
        folder = inputs / ('check4-' + label)
        folder.mkdir()
        doc = call('new', name='Portable' + label)['document']
        saved = folder / 'Saved.FCStd'
        call('save', document=doc, path=str(saved))
        call('transaction', document=doc, edits=[dict(op='assembly_create', name='Assembly', joints='Joints')])
        choices = call('assembly_sources', document=doc, path=str(path))['objects']
        source = next(o['source'] for o in choices if o['name'] == obj)
        assert len(source['dependencies']) == count and 'dependency_count' not in source
        call('transaction', document=doc, edits=[dict(op='assembly_insert', assembly='Assembly', name='Instance', external_source=source),
            dict(op='add_object', type='Part::Box', name='DirtyContent')])
        call('save', document=doc, path=str(saved))
        before = call('inspect', document=doc)
        call('close', document=doc)
        reopened = call('open', path=str(saved))
        # Same-phase stock readback checks every saved ID and finite native
        # shape separately. Origin reference planes and native Touched status
        # are not an exact cross-phase modeled-geometry oracle.
        previous = {o['name']: o for o in before['objects']}
        current = {o['name']: o for o in reopened['objects']}
        assert {name: o['type'] for name, o in current.items()} == {name: o['type'] for name, o in previous.items()}
        for name in ['Instance', 'DirtyContent']:
            for field in ['properties', 'expressions', 'shape', 'global_placement', 'instance_state']:
                assert current[name].get(field) == previous[name].get(field), (label, name, field)
        call('close', document=reopened['document'])
        if label == 'deep':
            shortened = inputs / 'Shortened.FCStd'
            with zipfile.ZipFile(saved) as archive, zipfile.ZipFile(shortened, 'w', zipfile.ZIP_DEFLATED) as target:
                for entry in archive.infolist():
                    data = archive.read(entry)
                    if entry.filename == 'Document.xml':
                        xml = ET.fromstring(data)
                        node = xml.find('.//Property[@name="CadExternalSource"]/String')
                        value = json.loads(node.get('value'))
                        value['dependencies'] = [v for v in value['dependencies'] if Path(v['path']).name != 'Side.FCStd']
                        assert len(value['dependencies']) == 2
                        node.set('value', json.dumps(value, sort_keys=True))
                        data = ET.tostring(xml, encoding='utf-8')
                    target.writestr(entry, data)
            keeper = call('new', name='PartialKeeper')['document']
            call('transaction', document=keeper, edits=[dict(op='add_object', type='Part::Box', name='KeepContent')])
            call('transaction', document=keeper, edits=[dict(op='add_object', type='Part::Box', name='KeepRedo')])
            call('undo', document=keeper)
            keep = call('inspect', document=keeper)
            for missing in [False, True]:
                side = inputs / 'deep/Side.FCStd'
                original = side.read_bytes()
                try:
                    if missing:
                        side.unlink()
                    for rejected in [shortened, saved] if missing else [shortened]:
                        call('open', path=str(rejected), expect_error='source_missing' if missing else 'source_identity')
                        assert call('inspect', document=keeper) == keep
                finally:
                    if missing:
                        side.write_bytes(original)
            call('redo', document=keeper)
            call('close', document=keeper)
    (out / 'passed.json').write_text(json.dumps(dict(full_deep_and_cycle_reopen=True,
        shortened_metadata_missing_branch_denied=True, keeper_redo_preserved=True)) + '\n')
finally:
    session.close()
