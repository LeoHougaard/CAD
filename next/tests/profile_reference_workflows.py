"""Native profile choices using the editor's Rust property compiler and leases."""
import argparse
import hashlib
import io
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET
import zipfile

from native_workflows import add, link, near, objects, rectangle, set_value, solid, transaction
from preview_workflows import PreviewSession


def native_fixtures(directory):
    """Use native APIs for source comments/control metadata absent from edits."""
    import FreeCAD as App
    import Part
    import PartDesign
    import Sketcher
    directory.mkdir(parents=True, exist_ok=True)
    paths = {}
    for case in ['plain', 'local', 'cross', 'attached', 'external', 'shifted', 'nondefault', 'spline', 'moved']:
        doc = App.newDocument('ReferenceFixture' + case)
        body = doc.addObject('PartDesign::Body', 'SourceBody')
        source = body.newObject('Sketcher::SketchObject', 'Source')
        if case == 'spline':
            poles = [App.Vector(x, y, 0) for x, y in [(0, 0), (2, 3), (5, 3), (7, 0)]]
            curve = Part.BSplineCurve()
            curve.buildFromPolesMultsKnots(poles, degree=3, weights=[1, 2, 1, 1], knots=[0, 1], mults=[4, 4])
            source.addGeometry(curve, False)
            source.addGeometry(Part.LineSegment(poles[-1], poles[0]), False)
            source.addConstraint(Sketcher.Constraint('Coincident', 0, 2, 1, 1))
            source.addConstraint(Sketcher.Constraint('Coincident', 0, 1, 1, 2))
            source.exposeInternalGeometry(0)
        else:
            points = [(0, 0), (10, 0), (10, 8), (0, 8)]
            for a, b in zip(points, points[1:] + points[:1]):
                source.addGeometry(Part.LineSegment(App.Vector(*a, 0), App.Vector(*b, 0)), False)
            for i in range(4):
                source.addConstraint(Sketcher.Constraint('Coincident', i, 2, (i+1) % 4, 1))
            for i, kind in enumerate(['Horizontal', 'Vertical', 'Horizontal', 'Vertical']):
                source.addConstraint(Sketcher.Constraint(kind, i))
            if case in ['external', 'shifted']:
                master = body.newObject('Sketcher::SketchObject', 'Master')
                master.addGeometry(Part.LineSegment(App.Vector(0, 0, 0), App.Vector(10, 0, 0)), False)
                doc.recompute()
                source.addExternal('Master', 'Edge1')
                source.addConstraint(Sketcher.Constraint('Coincident', 0, 1, -3, 1))
            else:
                source.addConstraint(Sketcher.Constraint('Coincident', 0, 1, -1, 1))
            width = source.addConstraint(Sketcher.Constraint('Distance', 0, 10.0))
            source.renameConstraint(width, 'Width')
            height = source.addConstraint(Sketcher.Constraint('Distance', 1, 8.0))
            source.renameConstraint(height, 'Height')
            if case in ['local', 'shifted']:
                source.setExpression('Constraints[' + str(height) + ']', 'Constraints[' + str(width) + '] * 0.5', 'Profile reference comment: \u03b4')
            elif case == 'cross':
                master = doc.addObject('Part::Box', 'Master')
                master.Length = 10
                source.setExpression('Constraints[' + str(width) + ']', 'Master.Length', 'Keep intentional external dependency')
            elif case == 'attached':
                master = doc.addObject('Part::Box', 'Master')
                master.Length, master.Width, master.Height = 20, 20, 5
                doc.recompute()
                source.AttachmentSupport = [(master, ['Face6'])]
                source.MapMode = 'FlatFace'
            elif case == 'nondefault':
                source.ArcFitTolerance = 0.03
                source.MakeInternals = True
                source.Label2 = 'Native metadata \u03b4'
                source.Placement = App.Placement(App.Vector(2, 3, 0), App.Rotation(App.Vector(0, 0, 1), 30))
                circle = source.addGeometry(Part.Circle(App.Vector(30, 30, 0), App.Vector(0, 0, 1), 2), True)
                radius = source.addConstraint(Sketcher.Constraint('Radius', circle, 2.0))
                source.setDriving(radius, False)
                source.setVirtualSpace(radius, True)
        doc.recompute()
        pad = body.newObject('PartDesign::Pad', 'SourcePad')
        pad.Profile, pad.Length = source, 5
        source.Visibility = False
        target = doc.addObject('PartDesign::Body', 'TargetBody')
        if case == 'moved':
            body.Placement = App.Placement(App.Vector(30, 4, 0), App.Rotation(App.Vector(0, 0, 1), 90))
            target.Placement = App.Placement(App.Vector(-7, 10, 0), App.Rotation(App.Vector(0, 0, 1), 30))
        doc.recompute()
        assert source.solve() == 0 and pad.isValid() and len(pad.Shape.Solids) == 1, case
        path = (directory / (case + '.FCStd')).resolve()
        doc.saveAs(str(path))
        paths[case] = {'path': str(path), 'volume': pad.AddSubShape.Volume, 'source_volume': pad.Shape.Volume}
        App.closeDocument(doc.Name)
    (directory / 'fixtures.json').write_text(json.dumps(paths, indent=2), encoding='utf-8')


def native_read(manifest, output):
    import FreeCAD as App
    records = []
    for entry in json.loads(manifest.read_text(encoding='utf-8')):
        doc = App.openDocument(entry['path'])
        try:
            doc.recompute()
            source, copy = doc.getObject('Source'), doc.getObject('CopySource')
            values = {}
            for name, obj in [('source', source), ('copy', copy)]:
                if obj and obj.isDerivedFrom('Sketcher::SketchObject'):
                    with zipfile.ZipFile(io.BytesIO(bytes(obj.dumpPropertyContent('ExpressionEngine')))) as archive:
                        xml = next(archive.read(n) for n in archive.namelist() if n.lower().endswith('.xml'))
                    values[name] = {'expressions': [dict(e.attrib) for e in ET.fromstring(xml).iter('Expression')],
                                    'cache_count': len(obj.ExternalGeo), 'constraint_count': obj.ConstraintCount,
                                    'construction': [obj.getConstruction(i) for i in range(obj.GeometryCount)]}
            records.append({'case': entry['case'], **values})
        finally:
            App.closeDocument(doc.Name)
    output.write_text(json.dumps(records, indent=2), encoding='utf-8')


def compile_properties(compiler, parameters):
    response = subprocess.run([str(compiler), '--native-properties', '-'], input=json.dumps(parameters),
                              text=True, encoding='utf-8', capture_output=True, check=True)
    return json.loads(response.stdout)


def source_state(state):
    return {name: objects(state)['Source'][name] for name in ['type', 'body', 'sketch', 'properties', 'expressions', 'global_placement']}


def close_file(session, doc, path):
    session.call('save', document=doc, path=str(path.resolve()))
    session.call('close', document=doc)
    return session.call('open', path=str(path.resolve()))


def workflow(session, compiler, fixture, case, mode, output, existing=False):
    state = session.call('open', path=fixture['path'])
    doc = state['document']
    if existing:
        state = transaction(session, doc, 'Native existing downstream feature', [
            add('Sketcher::SketchObject', 'Local', 'TargetBody'), *rectangle('Local', 4, 4),
            add('PartDesign::Pad', 'Pad', 'TargetBody'), set_value('Pad', 'Profile', link('Local')), set_value('Pad', 'Length', 5),
            add('PartDesign::AdditiveBox', 'Finish', 'TargetBody'), set_value('Finish', 'Length', 2), set_value('Finish', 'Width', 2),
            set_value('Finish', 'Height', 2), set_value('Finish', 'Placement', {'position': [0, 0, 4], 'rotation': [0, 0, 0, 1]})])
        solid(state, 'Finish', 84)
    baseline, original = state, source_state(state)
    names = set(objects(state))
    request = {'source': 'Source', 'name': 'CopySource', 'body': 'TargetBody', 'mode': mode}
    parameters = {'type': 'PartDesign::Pad', 'name': 'Pad', 'body': 'TargetBody', 'create': not existing,
                  'properties': {'Profile': link('Source'), 'Length': 5}, 'expressions': {},
                  'reference_choice': {'state': baseline, 'request': request}}
    if existing:
        parameters.update(tip='Finish', tip_visibility=True)
    edits = compile_properties(compiler, parameters)
    assert bool([e for e in edits if e['op'] == 'reference_copy']) == (mode != 'cross_reference')
    history = state['undo_count']
    preview = session.call('preview', document=doc, token='profile-choice', edits=edits)['state']
    assert source_state(preview) == original
    expected = fixture['volume'] + (4 if existing else 0)
    solid(preview, 'Finish' if existing else 'Pad', expected)
    # Replacement from the same baseline/name must not leave orphan references.
    preview = session.call('preview', document=doc, token='profile-choice', edits=edits)['state']
    assert set(objects(preview)) == names | ({'CopySource'} if mode != 'cross_reference' else set()) | (set() if existing else {'Pad'})
    state = session.call('preview_cancel', document=doc, token='profile-choice')
    assert set(objects(state)) == names and state['undo_count'] == history and source_state(state) == original
    assert objects(state)['TargetBody']['body_state'] == objects(baseline)['TargetBody']['body_state']
    session.call('preview', document=doc, token='profile-apply', edits=edits)
    state = session.call('preview_commit', document=doc, token='profile-apply')
    assert state['undo_count'] == history + 1 and source_state(state) == original
    profile = 'Source' if mode == 'cross_reference' else 'CopySource'
    assert objects(state)['Pad']['properties']['Profile']['link'] == profile
    if existing:
        assert objects(state)['TargetBody']['body_state']['tip'] == 'Finish'
        assert objects(state)['TargetBody']['body_state']['display_object'] == 'Finish'
    if mode == 'independent':
        copied = objects(state)['CopySource']
        assert copied['sketch']['externals'] == []
        assert copied['properties']['AttachmentSupport'] == [] and copied['properties']['MapMode'] == 'Deactivated'
        assert copied['properties']['Placement'] == objects(baseline)['Source']['properties']['Placement']
        if case == 'spline':
            assert copied['sketch']['spline_controls'] == objects(baseline)['Source']['sketch']['spline_controls']
            assert copied['sketch']['geometry'][0]['weights'] == [1, 2, 1, 1]
        if case == 'nondefault':
            for prop in ['ArcFitTolerance', 'MakeInternals', 'Label2', 'Visibility']:
                assert copied['properties'][prop] == objects(baseline)['Source']['properties'][prop]
    state = session.call('undo', document=doc)
    assert set(objects(state)) == names and source_state(state) == original
    state = session.call('redo', document=doc)
    solid(state, 'Finish' if existing else 'Pad', expected)
    assert source_state(state) == original
    # Preview-independent failed batch must restore the committed baseline.
    session.call('transaction', document=doc, label='Reject invalid native profile field',
                 edits=[set_value('Pad', 'MissingNativeProperty', 1)], expect_error='unsupported_property')
    state = session.call('inspect', document=doc)
    assert source_state(state) == original
    solid(state, 'Finish' if existing else 'Pad', expected)
    before_edit = expected
    if case in ['local', 'shifted'] and mode == 'independent':
        datum = next(c['index'] for c in objects(state)['CopySource']['sketch']['constraints'] if c['name'] == 'Width')
        state = transaction(session, doc, 'Copied expression immediately drives native feature', [dict(op='datum', object='CopySource', index=datum, value='12 mm')])
        expected = 360 + (4 if existing else 0)
        assert source_state(state) == original
    elif case == 'cross':
        state = transaction(session, doc, 'Intentional external expression', [set_value('Master', 'Length', 12)])
        expected = 480 + (4 if existing else 0)
    elif case in ['plain', 'moved']:
        datum = next(c['index'] for c in objects(state)['Source']['sketch']['constraints'] if c['name'] == 'Width')
        state = transaction(session, doc, 'Original profile parent edit', [dict(op='datum', object='Source', index=datum, value='12 mm')])
        expected = fixture['volume'] if mode == 'independent' else 480
        expected += 4 if existing else 0
    elif case == 'attached':
        state = transaction(session, doc, 'Attachment parent edit', [set_value('Master', 'Height', 9)])
        if mode == 'independent':
            near(objects(state)['CopySource']['properties']['Placement']['position'][2], 5)
        near(objects(state)['Source']['properties']['Placement']['position'][2], 9)
    elif case in ['external', 'shifted']:
        state = transaction(session, doc, 'External geometry parent edit', [dict(op='move_geometry', object='Master', index=0, point=1, position=[2, 1, 0])])
    elif case == 'nondefault':
        datum = next(c['index'] for c in objects(state)['CopySource']['sketch']['constraints'] if c['name'] == 'Width')
        state = transaction(session, doc, 'Edit copied nondefault profile', [dict(op='datum', object='CopySource', index=datum, value='12 mm')])
        expected = 480
    elif case == 'spline':
        state = transaction(session, doc, 'Edit native copied spline control', [dict(op='spline_pole', object='CopySource', index=0, pole=1, position=[2, 4, 0])])
        expected = solid(state, 'Pad')['volume']
        assert abs(expected - before_edit) > 1
    solid(state, 'Finish' if existing else 'Pad', expected)
    solid(session.call('undo', document=doc), 'Finish' if existing else 'Pad', before_edit)
    solid(session.call('redo', document=doc), 'Finish' if existing else 'Pad', expected)
    path = output / (case + '-' + mode + ('-existing' if existing else '') + '.FCStd')
    state = close_file(session, doc, path)
    solid(state, 'Finish' if existing else 'Pad', expected)
    if mode == 'independent':
        assert objects(state)['CopySource']['sketch']['externals'] == []
    if existing:
        assert objects(state)['TargetBody']['body_state']['tip'] == 'Finish'
        assert objects(state)['TargetBody']['body_state']['display_object'] == 'Finish'
    session.call('close', document=state['document'])
    return {'case': case, 'mode': mode, 'existing_downstream': existing, 'path': str(path.resolve()),
            'initial_volume': fixture['volume'], 'final_volume': expected,
            'source_preserved_copy_lifecycle': True, 'preview_replace_cancel_commit_undo_redo_reopen': True}


def expression_history(session, compiler, fixture, factory, direction):
    state = session.call('open', path=fixture['path'])
    doc, original = state['document'], source_state(state)
    if factory == 'copy':
        edits = compile_properties(compiler, dict(type='PartDesign::Pad', name='Pad', body='TargetBody', create=True,
            properties={'Profile': link('Source'), 'Length': 5}, expressions={}, reference_choice={'state': state,
            'request': dict(source='Source', name='CopySource', body='TargetBody', mode='independent')}))
        names, result, expected = ['CopySource', 'Pad'], 'Pad', 480
    elif factory == 'sketch':
        edits = [add('Sketcher::SketchObject', 'RawSketch', 'TargetBody'), *rectangle('RawSketch', 10, 8),
                 dict(op='expression', object='RawSketch', property='Constraints[9]', expression='Master.Length')]
        edits += compile_properties(compiler, dict(type='PartDesign::Pad', name='Pad', body='TargetBody', create=True,
                 properties={'Profile': link('RawSketch'), 'Length': 5}, expressions={}))
        names, result, expected = ['RawSketch', 'Pad'], 'Pad', 480
    else:
        edits = compile_properties(compiler, dict(type='Part::Box', name='Result', body=None, create=True,
                 properties={'Length': 10, 'Width': 10, 'Height': 10}, expressions={'Length': 'Master.Length'}))
        names, result, expected = ['Result'], 'Result', 1200
    state = transaction(session, doc, 'Ordinary native expression object', edits)
    baseline_count = state['undo_count']
    if direction == 'redo_creation':
        state = transaction(session, doc, 'A second Redo must survive expression repair', [set_value(result, 'Label', 'Pending second Redo')])
        session.call('undo', document=doc)
        session.call('undo', document=doc)
        state = session.call('redo', document=doc)
        assert state['undo_count'] == baseline_count and state['redo_count'] == 1
        assert source_state(state) == original
        state = session.call('redo', document=doc)
        assert state['undo_count'] == baseline_count + 1 and state['redo_count'] == 0
        assert objects(state)[result]['properties']['Label'] == 'Pending second Redo'
    else:
        state = transaction(session, doc, 'Remove expression owner', [dict(op='remove_object', object=n) for n in reversed(names)])
        assert all(n not in objects(state) for n in names)
        state = session.call('undo', document=doc)
        assert state['undo_count'] == baseline_count and state['redo_count'] == 1
        assert source_state(state) == original
    state = transaction(session, doc, 'Parent propagates after native object restoration', [set_value('Master', 'Length', 12)])
    solid(state, result, expected)
    state = session.call('undo', document=doc)
    solid(state, result, 400 if factory != 'box' else 1000)
    state = session.call('redo', document=doc)
    solid(state, result, expected)
    session.call('close', document=doc)
    return dict(factory=factory, direction=direction, ordinary_native_baseline=factory != 'copy',
                history_counts_preserved=True, second_redo_preserved=direction == 'redo_creation',
                original_source_unchanged_during_history=True, parent_volume=expected)


def guards(session, fixture):
    state = session.call('open', path=fixture['path'])
    doc, original, names = state['document'], source_state(state), set(objects(state))
    base = dict(op='reference_copy', source='Source', name='CopySource', body='TargetBody', mode='independent')
    checks = [('mode', 'bad', 'invalid_request'), ('body', 'Source', 'invalid_request'),
              ('source', 'Missing', 'not_found'), ('source', 'SourcePad', 'unsupported_reference'),
              ('name', 'Source', 'name_conflict'), ('name', 'bad name', 'invalid_request'),
              ('unexpected', True, 'invalid_request')]
    for key, value, code in checks:
        session.call('transaction', document=doc, edits=[{**base, key: value}], expect_error=code)
        state = session.call('inspect', document=doc)
        assert state['available'] and set(objects(state)) == names and source_state(state) == original
    revision = state['revision']
    session.call('transaction', document=doc, expected_revision=revision - 1, edits=[base], expect_error='stale_revision')
    session.call('transaction', document=doc, edits=[base, set_value('CopySource', 'MissingNativeProperty', 1)], expect_error='unsupported_property')
    state = session.call('inspect', document=doc)
    assert state['available'] and set(objects(state)) == names and source_state(state) == original
    session.call('preview', document=doc, token='reference-owner', edits=[base])
    session.call('transaction', document=doc, edits=[set_value('Source', 'Label', 'Forbidden concurrent edit')], expect_error='preview_active')
    state = session.call('preview_cancel', document=doc, token='reference-owner')
    assert set(objects(state)) == names and source_state(state) == original
    session.call('close', document=doc)
    return dict(invalid_requests_atomic=len(checks), stale_revision=True, failed_copy_batch_rolled_back=True, preview_lease=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['host', 'compiler', 'runtime', 'output']:
        parser.add_argument('--' + name, required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    runtime = json.loads(args.runtime.read_text(encoding='utf-8'))
    fixtures_dir = (args.output / 'fixtures').resolve()
    env = os.environ.copy()
    user_dir = (args.output / 'native-config').resolve()
    user_dir.mkdir(exist_ok=True)
    env['FREECAD_USER_HOME'] = str(user_dir)
    subprocess.run([runtime['executable'], *runtime['args'], str(Path(__file__).resolve()), '--native-fixtures', str(fixtures_dir)],
                   env=env, check=True, capture_output=True, text=True, encoding='utf-8')
    fixtures = json.loads((fixtures_dir / 'fixtures.json').read_text(encoding='utf-8'))
    fixture_hashes = {p['path']: hashlib.sha256(Path(p['path']).read_bytes()).hexdigest() for p in fixtures.values()}
    session = PreviewSession(args.host, runtime)
    results = []
    history = []
    try:
        for case, fixture in fixtures.items():
            results.append(workflow(session, args.compiler, fixture, case, 'independent', args.output))
        for mode in ['dependent', 'cross_reference']:
            results.append(workflow(session, args.compiler, fixtures['plain'], 'plain', mode, args.output))
        for mode in ['independent', 'dependent', 'cross_reference']:
            results.append(workflow(session, args.compiler, fixtures['plain'], 'plain', mode, args.output, existing=True))
        for factory in ['copy', 'sketch', 'box']:
            for direction in ['redo_creation', 'undo_deletion']:
                history.append(expression_history(session, args.compiler, fixtures['cross'], factory, direction))
        guard_results = guards(session, fixtures['plain'])
    finally:
        session.close()
    assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == sha for path, sha in fixture_hashes.items())
    manifest = (args.output / 'native-inspection-manifest.json').resolve()
    manifest.write_text(json.dumps(results, indent=2), encoding='utf-8')
    native_output = (args.output / 'native-inspection.json').resolve()
    subprocess.run([runtime['executable'], *runtime['args'], str(Path(__file__).resolve()), '--native-read', str(manifest), str(native_output)],
                   env=env, check=True, capture_output=True, text=True, encoding='utf-8')
    inspected = json.loads(native_output.read_text(encoding='utf-8'))
    for entry in inspected:
        if entry.get('copy'):
            assert entry['copy']['cache_count'] == 2
            if entry['case'] in ['local', 'shifted']:
                assert entry['copy']['expressions'][0]['comment'] == 'Profile reference comment: \u03b4'
    report = {'workflows': results, 'expression_history': history, 'guards': guard_results,
              'native_inspection': inspected, 'source_fixture_hashes_unchanged': True,
              'limits': 'Generated workflows, not complete native feature or GUI parity.'}
    (args.output / 'profile-reference-report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps({'passed': True, 'workflows': len(results)}, indent=2))


if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--native-fixtures':
        native_fixtures(Path(sys.argv[2]))
    elif len(sys.argv) == 4 and sys.argv[1] == '--native-read':
        native_read(Path(sys.argv[2]), Path(sys.argv[3]))
    else:
        main()
