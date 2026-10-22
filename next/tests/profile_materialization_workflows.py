"""Native circular defining/intersection copy ownership and history checks."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import traceback
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'next/tests'))
from native_workflows import objects, solid, transaction, set_value
from preview_workflows import PreviewSession
from profile_reference_workflows import compile_properties, source_state

OUTPUT = None


def fixtures():
    import FreeCAD as App
    import Part
    import PartDesign
    import Sketcher
    OUTPUT.mkdir(exist_ok=True)
    manifest = []
    for kind in ['defining', 'intersection', 'materialized']:
        for moved in ([False] if kind == 'materialized' else [False, True]):
            case = kind + ('-moved' if moved else '')
            doc = App.newDocument('Materialization' + case.replace('-', ''))
            body = doc.addObject('PartDesign::Body', 'SourceBody')
            source = body.newObject('Sketcher::SketchObject', 'Source')
            points = [(0, 0), (10, 0), (10, 5), (0, 5)]
            for a, b in zip(points, points[1:] + points[:1]):
                source.addGeometry(Part.LineSegment(App.Vector(*a, 0), App.Vector(*b, 0)), False)
            construction = source.addGeometry(Part.LineSegment(App.Vector(20, 0, 0), App.Vector(20, 5, 0)), True)
            source.addConstraint(Sketcher.Constraint('Block', construction))
            for i in range(4):
                source.addConstraint(Sketcher.Constraint('Coincident', i, 2, (i + 1) % 4, 1))
            for i, name in enumerate(['Horizontal', 'Vertical', 'Horizontal', 'Vertical']):
                source.addConstraint(Sketcher.Constraint(name, i))
            source.addConstraint(Sketcher.Constraint('Coincident', 0, 1, -1, 1))
            if kind != 'intersection':
                master = body.newObject('Sketcher::SketchObject', 'Master')
                master.addGeometry(Part.Circle(App.Vector(5, 2.5, 0), App.Vector(0, 0, 1), 1), False)
                master.addConstraint(Sketcher.Constraint('Radius', 0, 1.0))
                reference = master
            else:
                master = doc.addObject('Part::Cylinder', 'Master')
                master.Radius, master.Height = 1, 10
                master.Placement = App.Placement(App.Vector(5, 2.5, 0), App.Rotation())
                source.Placement = App.Placement(App.Vector(0, 0, 5), App.Rotation())
                reference = body.newObject('PartDesign::ShapeBinder', 'Binder')
                reference.Support = [(master, ['Face1'])]
            doc.recompute()
            source.addExternal(reference.Name, 'Face1' if kind == 'intersection' else 'Edge1', True, kind == 'intersection')
            measurement = source.addConstraint(Sketcher.Constraint('Radius', -3, 1.0))
            source.setDriving(measurement, False)
            source.renameConstraint(measurement, 'ExternalRadius')
            width = source.addConstraint(Sketcher.Constraint('Distance', 0, 10.0))
            source.renameConstraint(width, 'Width')
            height = source.addConstraint(Sketcher.Constraint('Distance', 1, 5.0))
            source.renameConstraint(height, 'Height')
            source.setExpression('Constraints[' + str(height) + ']', 'Constraints[' + str(width) + '] * 0.5', 'Materialized \u03b4 reference')
            if kind == 'materialized':
                # Reproduce the previous native materialization, including its
                # stale external identity on an already-internal source curve.
                legacy = source.ExternalGeo[2].copy()
                index = source.addGeometry(legacy, False)
                source.setConstruction(index, False)
                source.delExternal(-3)
                doc.recompute()
                assert source.Geometry[index].getExtensionOfType('Sketcher::ExternalGeometryExtension').Ref
            target = doc.addObject('PartDesign::Body', 'TargetBody')
            if moved:
                # Native TraceSupport=False keeps the Binder in its local frame.
                # Do not add a client world-placement compensation.
                placement = App.Placement(App.Vector(30, 4, 0), App.Rotation(App.Vector(0, 0, 1), 90))
                body.Placement = placement
                target.Placement = App.Placement(App.Vector(-7, 10, 0), App.Rotation(App.Vector(0, 0, 1), 30))
            doc.recompute()
            assert source.solve() == 0
            assert len(source.Shape.Wires) == 2 and len(source.Shape.Edges) == 5
            assert source.getConstruction(4)
            external = source.ExternalGeo[2:]
            assert len(external) == (0 if kind == 'materialized' else 1)
            defining = source.Geometry[5] if kind == 'materialized' else external[0]
            assert defining.getExtensionOfType('Sketcher::ExternalGeometryExtension').testFlag('Defining')
            path = OUTPUT / (case + '-baseline.FCStd')
            assert not path.exists(), path
            doc.saveAs(str(path))
            manifest.append(dict(case=case, path=str(path), kind=kind, moved=moved, width=width, height=height))
            App.closeDocument(doc.Name)
    (OUTPUT / 'fixtures.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')


def read_native():
    import FreeCAD as App
    sys.path.insert(0, str(NATIVE_BACKEND))
    import expression_links as E
    report = []
    for entry in json.loads((OUTPUT / 'saved.json').read_text(encoding='utf-8')):
        path = Path(entry['path'])
        before = hashlib.sha256(path.read_bytes()).hexdigest()
        with zipfile.ZipFile(path) as archive:
            root = ET.fromstring(archive.read('Document.xml'))
        copied_xml = next(obj for obj in root.iter('Object') if obj.attrib.get('name') == 'CopySource' and obj.find('Properties') is not None)
        geometry_xml = next(prop for prop in copied_xml.iter('Property') if prop.attrib['name'] == 'Geometry')
        assert all(not geo.attrib.get('ref') and not any(ext.attrib.get('type') == 'Sketcher::ExternalGeometryExtension' for ext in geo.iter('GeoExtension')) for geo in geometry_xml.iter('Geometry'))
        doc = App.openDocument(str(path))
        try:
            doc.recompute()
            source, copy = doc.getObject('Source'), doc.getObject('CopySource')
            materialized = entry['case'] == 'materialized'
            assert len(source.ExternalGeo) == (2 if materialized else 3)
            defining = source.Geometry[5] if materialized else source.ExternalGeo[2]
            ext = defining.getExtensionOfType('Sketcher::ExternalGeometryExtension')
            assert ext.testFlag('Defining')
            assert ext.Ref
            assert copy.GeometryCount == 6 and copy.getConstruction(4) and not copy.getConstruction(5)
            assert copy.ExternalGeometry == [] and len(copy.ExternalGeo) == 2
            assert all(not geometry.hasExtensionOfType('Sketcher::ExternalGeometryExtension') for geometry in copy.Geometry)
            assert not any(o.Name in ('Master', 'Binder', 'Source') for o in copy.OutListRecursive)
            assert copy.Placement == source.Placement
            expressions = E.entries(copy)
            assert len(expressions) == 1 and expressions[0]['comment'] == 'Materialized \u03b4 reference'
            assert source.getConstruction(4) and source.ConstraintCount == (12 if materialized else 13)
            report.append(dict(case=entry['case'], expressions=expressions,
                               copy_construction=[copy.getConstruction(i) for i in range(copy.GeometryCount)],
                               source_defining=True, source_already_materialized=materialized, copy_geometry_extension_refs=[geometry.getExtensionOfType('Sketcher::ExternalGeometryExtension').Ref if geometry.hasExtensionOfType('Sketcher::ExternalGeometryExtension') else None for geometry in copy.Geometry], copy_cache=len(copy.ExternalGeo), copy_dependency_names=[o.Name for o in copy.OutListRecursive]))
        finally:
            App.closeDocument(doc.Name)
        assert hashlib.sha256(path.read_bytes()).hexdigest() == before
    (OUTPUT / 'native-inspection.json').write_text(json.dumps(report, indent=2), encoding='utf-8')


def changed_members(before, after):
    assert before['expressions'] == after['expressions']
    return sorted(name for name in before['content'] if before['content'][name] != after['content'][name])


def source_only_reopen(session, fixture):
    state = session.call('open', path=fixture['path'])
    doc = state['document']
    parent = ([dict(op='datum', object='Master', index=0, value='2 mm')] if fixture['kind'] != 'intersection'
              else [set_value('Master', 'Radius', 2)])
    state = transaction(session, doc, 'Matched source-only parent change', parent)
    semantic = source_state(state)
    before = session.call('private_content', document=doc, object='Source')
    path = OUTPUT / (fixture['case'] + '-source-only.FCStd')
    session.call('save', document=doc, path=str(path))
    session.call('close', document=doc)
    state = session.call('open', path=str(path))
    assert source_state(state) == semantic
    after = session.call('private_content', document=state['document'], object='Source')
    session.call('close', document=state['document'])
    return changed_members(before, after), after


def workflow(session, compiler, fixture):
    native_restore_changes, restored_baseline = source_only_reopen(session, fixture)
    state = session.call('open', path=fixture['path'])
    doc = state['document']
    source = source_state(state)
    canonical = session.call('private_content', document=doc, object='Source')
    source_ids = [item['geometry_id'] for item in session.call('private_identity', document=doc, object='Source')]
    baseline_names = set(objects(state))
    history = state['undo_count']
    request = dict(source='Source', name='CopySource', body='TargetBody', mode='independent')
    parameters = dict(type='PartDesign::Pad', name='Pad', body='TargetBody', create=True,
                      properties=dict(Profile={'link': 'Source'}, Length=5), expressions={},
                      reference_choice=dict(state=state, request=request))
    edits = compile_properties(compiler, parameters)
    expected = (50 - math.pi) * 5

    identity_checks = 0
    def unchanged(current):
        nonlocal identity_checks
        if 'CopySource' in objects(current):
            identity = session.call('private_identity', document=doc, object='CopySource')
            assert len(identity) == 6 and all(item['extension_ref'] is None for item in identity), identity
            assert [item['geometry_id'] for item in identity[:len(source_ids)]] == source_ids
            identity_checks += 1
        assert source_state(current) == source
        actual_canonical = session.call('private_content', document=doc, object='Source')
        assert actual_canonical == canonical, {'expected': canonical, 'actual': actual_canonical}

    for token in ['cancel', 'apply']:
        candidate = session.call('preview', document=doc, token=token, edits=edits)['state']
        solid(candidate, 'Pad', expected)
        unchanged(candidate)
        candidate = session.call('preview', document=doc, token=token, edits=edits)['state']
        assert set(objects(candidate)) == baseline_names | {'CopySource', 'Pad'}
        unchanged(candidate)
        if token == 'cancel':
            state = session.call('preview_cancel', document=doc, token=token)
            assert set(objects(state)) == baseline_names and state['undo_count'] == history
            unchanged(state)
        else:
            state = session.call('preview_commit', document=doc, token=token)
    assert state['undo_count'] == history + 1
    unchanged(state)
    original_copy = objects(state)['CopySource']
    assert original_copy['sketch']['geometry_count'] == 6
    assert original_copy['sketch']['externals'] == []
    assert len(original_copy['sketch']['constraints']) == 12
    assert original_copy['properties']['Placement'] == objects(state)['Source']['properties']['Placement']
    state = session.call('undo', document=doc)
    assert set(objects(state)) == baseline_names
    unchanged(state)
    state = session.call('redo', document=doc)
    solid(state, 'Pad', expected)
    unchanged(state)
    session.call('transaction', document=doc, label='Atomic materialized failure', edits=[
        dict(op='datum', object='CopySource', index=10, value='9 mm'),
        set_value('Pad', 'MissingProperty', 1)], expect_error='unsupported_property')
    state = session.call('inspect', document=doc)
    solid(state, 'Pad', expected)
    unchanged(state)
    # Intended original-parent change must not move the newly native internal circle.
    parent = ([dict(op='datum', object='Master', index=0, value='2 mm')] if fixture['kind'] != 'intersection'
              else [set_value('Master', 'Radius', 2)])
    state = transaction(session, doc, 'Source-only defining geometry change', parent)
    solid(state, 'Pad', expected)
    source = source_state(state)
    canonical = session.call('private_content', document=doc, object='Source')
    # Local expression survived removal/remapping of the preceding external
    # measurement. Editing copied Width must still drive copied Height.
    width = next(c['index'] for c in objects(state)['CopySource']['sketch']['constraints'] if c['name'] == 'Width')
    state = transaction(session, doc, 'Copied local expression', [dict(op='datum', object='CopySource', index=width, value='12 mm')])
    expected_width = (72 - math.pi) * 5
    solid(state, 'Pad', expected_width)
    unchanged(state)
    undo_state = session.call('undo', document=doc)
    solid(undo_state, 'Pad', expected)
    unchanged(undo_state)
    state = session.call('redo', document=doc)
    solid(state, 'Pad', expected_width)
    unchanged(state)
    radius = dict(op='constraint', object='CopySource', kind='Radius', args=[5, 2.0])
    state = transaction(session, doc, 'Independent native materialized circle edit', [radius])
    expected_final = (72 - 4 * math.pi) * 5
    solid(state, 'Pad', expected_final)
    unchanged(state)
    undo_state = session.call('undo', document=doc)
    solid(undo_state, 'Pad', expected_width)
    unchanged(undo_state)
    state = session.call('redo', document=doc)
    solid(state, 'Pad', expected_final)
    unchanged(state)
    path = OUTPUT / (fixture['case'] + '-final.FCStd')
    session.call('save', document=doc, path=str(path))
    session.call('close', document=doc)
    state = session.call('open', path=str(path))
    doc = state['document']
    solid(state, 'Pad', expected_final)
    assert source_state(state) == source
    restored = session.call('private_content', document=doc, object='Source')
    restore_changes = changed_members(canonical, restored)
    assert restore_changes == native_restore_changes, (restore_changes, native_restore_changes)
    # Native restore can normalize persisted BRep/XML bytes. Compare source-only
    # restore first, then enforce byte preservation in this new live session.
    assert restored == restored_baseline
    canonical = restored
    unchanged(state)
    assert objects(state)['CopySource']['sketch']['externals'] == []
    session.call('close', document=doc)
    return dict(case=fixture['case'], path=str(path), initial_volume=expected,
                copied_width12_volume=expected_width, copied_radius2_volume=expected_final,
                source_canonical_preserved=True, source_snapshot_preserved=True,
                preview_cancel_commit_history=True, failed_batch_atomic=True,
                original_parent_does_not_propagate=True, native_geometry_extension_identity_checks=identity_checks,
                ordinary_native_restore_normalized_members=native_restore_changes,
                candidate_restored_source_matches_source_only_bytes=True, original_internal_geometry_ids_preserved=source_ids)


def instrumented_worker(output, runtime):
    # Test-only inspections use native serialization/extension APIs. No private
    # operation is added to the production backend or published protocol.
    wrapper = output / 'inspection_worker.py'
    wrapper.write_text("""import importlib.util,sys
sys.path.insert(0, {backend!r})
spec=importlib.util.spec_from_file_location('native_test_worker', {script!r})
worker=importlib.util.module_from_spec(spec);spec.loader.exec_module(worker)
ordinary=worker.dispatch
def dispatch(request):
    if request.get('op')=='private_identity':
        obj=worker.object_in(worker.document(request),request['object'])
        return [{{'index':index,'geometry_id':obj.getGeometryId(index),'extension_ref':(geometry.getExtensionOfType('Sketcher::ExternalGeometryExtension').Ref if geometry.hasExtensionOfType('Sketcher::ExternalGeometryExtension') else None)}} for index,geometry in enumerate(obj.Geometry)]
    if request.get('op')=='private_content':
        import hashlib,io,zipfile
        obj=worker.object_in(worker.document(request),request['object'])
        with zipfile.ZipFile(io.BytesIO(bytes(obj.dumpContent()))) as archive:
            return {{'content':{{name:hashlib.sha256(archive.read(name)).hexdigest() for name in archive.namelist()}},'expressions':worker.ExpressionLinks.entries(obj)}}
    return ordinary(request)
worker.dispatch=dispatch
worker.main()
""".format(script=str(Path(runtime['script']).resolve()),backend=str(Path(runtime['script']).resolve().parent)),encoding='utf-8')
    return {**runtime, 'script': str(wrapper)}


def main():
    global OUTPUT
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['host','compiler','runtime','output']:
        parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args()
    runtime=json.loads(args.runtime.read_text(encoding='utf-8-sig'))
    OUTPUT=args.output.resolve()
    assert not OUTPUT.exists(), 'Refuse to overwrite native proof output'
    OUTPUT.mkdir(parents=True)
    user_dir = OUTPUT / 'native-config'
    user_dir.mkdir()
    os.environ['FREECAD_USER_HOME'] = str(user_dir)
    subprocess.run([runtime['executable'],*runtime['args'],str(Path(__file__).resolve()),'--native-fixtures',str(OUTPUT)],check=True)
    manifest=json.loads((OUTPUT/'fixtures.json').read_text(encoding='utf-8'))
    originals={f['path']:hashlib.sha256(Path(f['path']).read_bytes()).hexdigest() for f in manifest}
    session=PreviewSession(args.host.resolve(),instrumented_worker(OUTPUT,runtime))
    report=[]
    try:
        for fixture in manifest:
            try:
                report.append(dict(passed=True,**workflow(session,args.compiler.resolve(),fixture)))
            except BaseException:
                report.append(dict(case=fixture['case'],passed=False,error=traceback.format_exc()))
                break
    finally:
        session.close()
        (OUTPUT/'report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
    assert len(report)==len(manifest) and all(c['passed'] for c in report),report
    assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest()==digest for path,digest in originals.items())
    (OUTPUT/'saved.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
    subprocess.run([runtime['executable'],*runtime['args'],str(Path(__file__).resolve()),'--native-read',str(OUTPUT),str(Path(runtime['script']).resolve().parent)],check=True)
    print(json.dumps({'passed':True,'circular_workflows':len(report),'source_fixture_hashes_unchanged':True},indent=2))


if __name__=='__main__':
    if len(sys.argv)==3 and sys.argv[1]=='--native-fixtures':
        OUTPUT=Path(sys.argv[2]);fixtures()
    elif len(sys.argv)==4 and sys.argv[1]=='--native-read':
        OUTPUT=Path(sys.argv[2]);NATIVE_BACKEND=Path(sys.argv[3]);read_native()
    else:
        main()
