"""Native Independent Copy invariants for materialized and mixed internal points.

Creates its own native inputs. Uses the public Rust host and native APIs only.
No private fixture or helper is required. Outputs must use a fresh directory.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET
import zipfile

from native_workflows import objects, transaction
from sketch_editor_workflows import SketchSession


EXTERNAL = 'Sketcher::ExternalGeometryExtension'


def property_xml(obj, name):
    with zipfile.ZipFile(io.BytesIO(obj.dumpPropertyContent(name))) as archive:
        return ET.fromstring(archive.read('Persistence.xml'))


def restored_point(obj, slot):
    # Restore a real native cache record, not hand-authored IDs or flags.
    import Part
    content = ET.Element('Content')
    content.extend(list(property_xml(obj, 'ExternalGeo').find('GeometryList'))[slot])
    payload = io.BytesIO()
    with zipfile.ZipFile(payload, 'w') as archive:
        archive.writestr('Persistence.xml', ET.tostring(content, encoding='utf-8'))
    point = Part.Point()
    point.restoreContent(bytearray(payload.getvalue()))
    return point


def native_info(obj):
    # Direct native XML is the oracle. Never use the candidate's recovery helper.
    with zipfile.ZipFile(io.BytesIO(obj.dumpContent())) as archive:
        members = {name: hashlib.sha256(archive.read(name)).hexdigest() for name in archive.namelist()}
    return dict(content=members, geometry=ET.tostring(property_xml(obj, 'Geometry'), encoding='unicode'),
                ids=[obj.getGeometryId(i) for i in range(obj.GeometryCount)],
                construction=[obj.getConstruction(i) for i in range(obj.GeometryCount)],
                constraints=[{key: getattr(c, key) for key in (
                    'Type', 'First', 'FirstPos', 'Second', 'SecondPos', 'Third', 'ThirdPos',
                    'Value', 'Name', 'Driving', 'InVirtualSpace', 'IsActive')} for c in obj.Constraints],
                expressions=list(obj.ExpressionEngine))


def metadata(info, strip_external=False):
    records = list(ET.fromstring(info['geometry']).find('GeometryList'))
    result = []
    for record in records:
        attributes = dict(record.attrib)
        if strip_external:
            for key in ('ref', 'refIndex', 'flags'):
                attributes.pop(key, None)
        extensions = [dict(ext.attrib) for ext in record.findall('./GeoExtensions/GeoExtension')
                      if not strip_external or ext.get('type') != EXTERNAL]
        result.append(dict(attributes=attributes, extensions=extensions,
                           construction=dict(record.find('Construction').attrib)))
    return result


def check_copy(source, copy):
    assert copy['ids'] == source['ids'], (source['ids'], copy['ids'])
    assert copy['construction'] == source['construction']
    assert copy['constraints'] == source['constraints']
    assert copy['expressions'] == source['expressions']
    assert metadata(copy) == metadata(source, strip_external=True)
    root = ET.fromstring(copy['geometry'])
    assert not root.findall(".//GeoExtension[@type='"+EXTERNAL+"']")
    assert all(not record.get('ref') for record in root.find('GeometryList'))


def fixtures(output):
    import FreeCAD as App
    import Part
    import Sketcher
    manifest = []
    for case in ('MaterializedPoint', 'MixedOrdinaryPoint', 'MixedConstructionPoint'):
        doc = App.newDocument(case)
        master = doc.addObject('Sketcher::SketchObject', 'Master')
        source = doc.addObject('Sketcher::SketchObject', 'Source')
        doc.addObject('PartDesign::Body', 'TargetBody')
        master.addGeometry(Part.Point(App.Vector(15,20,0)) if case == 'MaterializedPoint'
                           else Part.Circle(App.Vector(5,5,0), App.Vector(0,0,1), 2), False)
        corners = [App.Vector(x,y,0) for x,y in [(0,0),(20,0),(20,10),(0,10)]]
        for i in range(4):
            source.addGeometry(Part.LineSegment(corners[i], corners[(i+1)%4]), False)
        for i in range(4):
            source.addConstraint(Sketcher.Constraint('Coincident', i,2,(i+1)%4,1))
            source.addConstraint(Sketcher.Constraint('Horizontal' if i%2 == 0 else 'Vertical', i))
        source.addConstraint(Sketcher.Constraint('Coincident',0,1,-1,1))
        source.addConstraint(Sketcher.Constraint('Distance',0,20.))
        source.addConstraint(Sketcher.Constraint('Distance',1,10.))
        source.setExpression('Constraints[10]', 'Constraints[9] * 0.5')
        if case != 'MaterializedPoint':
            index = source.addGeometry(Part.LineSegment(App.Vector(30,0,0), App.Vector(30,5,0)), True)
            source.addConstraint(Sketcher.Constraint('Block', index))
        doc.recompute()
        source.addExternal('Master', 'Vertex1' if case == 'MaterializedPoint' else 'Edge1', True, False)
        doc.recompute()
        geometry = restored_point(source, 2) if case == 'MaterializedPoint' else source.ExternalGeo[2].copy()
        index = source.addGeometry(geometry, False)
        source.setConstruction(index, False)
        source.delExternal(-3)
        if case != 'MaterializedPoint':
            index = source.addGeometry(Part.Point(App.Vector(15,20,0)), case == 'MixedConstructionPoint')
        source.addConstraint(Sketcher.Constraint('DistanceX',index,1,15.))
        source.addConstraint(Sketcher.Constraint('DistanceY',index,1,20.))
        doc.recompute()
        assert source.solve() == 0 and len(source.ExternalGeo) == 2
        info = native_info(source)
        assert property_xml(source, 'Geometry').findall(".//GeoExtension[@type='"+EXTERNAL+"']")
        assert not source.Geometry[index].hasExtensionOfType(EXTERNAL)
        assert source.getConstruction(index) == (case == 'MixedConstructionPoint')
        path = output/(case+'-source.FCStd')
        assert not path.exists()
        doc.saveAs(str(path))
        App.closeDocument(doc.Name)
        manifest.append(dict(case=case,path=str(path),point_index=index,
                             sha256=hashlib.sha256(path.read_bytes()).hexdigest(),ids=info['ids'],
                             construction=info['construction']))
    (output/'fixtures.json').write_bytes((json.dumps(manifest,indent=2)+'\n').encode())


def instrumented_runtime(output, runtime):
    # Test-only inspection; production protocol and source remain unchanged.
    wrapper = output/'inspection_worker.py'
    wrapper.write_bytes(('''import importlib.util,sys
sys.path.insert(0, {tests!r})
sys.path.insert(0, {backend!r})
from internal_point_copy_workflows import native_info
spec=importlib.util.spec_from_file_location('tested_worker',{script!r})
worker=importlib.util.module_from_spec(spec);spec.loader.exec_module(worker)
ordinary=worker.dispatch
def dispatch(request):
    if request.get('op')=='test_native_info':
        return native_info(worker.object_in(worker.document(request),request['object']))
    return ordinary(request)
worker.dispatch=dispatch
worker.main()
'''.format(tests=str(Path(__file__).resolve().parent), backend=str(Path(runtime['script']).resolve().parent),
           script=str(Path(runtime['script']).resolve()))).encode())
    return {**runtime, 'script': str(wrapper)}


def workflow(session, fixture, output):
    path = Path(fixture['path'])
    state = session.call('open', path=str(path))
    doc = state['document']
    baseline = objects(state)['Source']
    source = session.call('test_native_info', document=doc, object='Source')
    assert source['ids'] == fixture['ids'] and source['construction'] == fixture['construction']
    edit = dict(op='reference_copy',source='Source',name='PublicCopy',body='TargetBody',mode='independent')

    def unchanged(state):
        assert objects(state)['Source'] == baseline
        assert session.call('test_native_info',document=doc,object='Source') == source

    state = session.call('preview',document=doc,token='point-copy',edits=[edit])['state']
    preview = session.call('test_native_info',document=doc,object='PublicCopy')
    check_copy(source,preview)
    unchanged(state)
    state = session.call('preview_cancel',document=doc,token='point-copy')
    assert 'PublicCopy' not in objects(state)
    unchanged(state)

    # Real solver failure after the copy/cleanup must roll back the whole action.
    session.call('transaction',document=doc,label='Copy then conflicting point dimension',edits=[edit,
        dict(op='constraint',object='PublicCopy',kind='DistanceX',args=[fixture['point_index'],1,100.])],
        expect_error='invalid_model')
    state = session.call('inspect',document=doc)
    assert 'PublicCopy' not in objects(state)
    unchanged(state)

    state = transaction(session,doc,'Independent copy preserving internal native point metadata',[edit])
    copy = session.call('test_native_info',document=doc,object='PublicCopy')
    check_copy(source,copy)
    unchanged(state)
    assert not objects(state)['PublicCopy']['sketch']['externals']
    state = session.call('undo',document=doc)
    assert 'PublicCopy' not in objects(state)
    unchanged(state)
    state = session.call('redo',document=doc)
    check_copy(source,session.call('test_native_info',document=doc,object='PublicCopy'))
    unchanged(state)

    result = output/(fixture['case']+'-result.FCStd')
    session.call('save',document=doc,path=str(result))
    session.call('close',document=doc)
    state = session.call('open',path=str(result))
    doc = state['document']
    restored_source = session.call('test_native_info',document=doc,object='Source')
    restored_copy = session.call('test_native_info',document=doc,object='PublicCopy')
    assert metadata(restored_source) == metadata(source)
    assert restored_source['ids'] == source['ids'] and restored_source['construction'] == source['construction']
    check_copy(restored_source,restored_copy)
    session.call('close',document=doc)
    assert hashlib.sha256(path.read_bytes()).hexdigest() == fixture['sha256']
    return dict(case=fixture['case'],passed=True,result=str(result),source=str(path),sha256=fixture['sha256'],
                source_ids=source['ids'],copied_ids=restored_copy['ids'],construction=restored_copy['construction'],
                hidden_external_identity_removed=True,other_native_metadata_preserved=True,
                source_snapshot_and_native_bytes_preserved=True,preview_cancel=True,
                failed_edit_atomic=True,undo_redo=True,save_reopen=True)


def readonly_verify(inputs, report):
    import FreeCAD as App
    results = []
    for source_file in sorted(inputs.glob('*-source.FCStd')):
        result_file = inputs/(source_file.name.replace('-source.FCStd','-result.FCStd'))
        digests = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in [source_file,result_file]}
        doc = App.openDocument(str(source_file))
        source = native_info(doc.getObject('Source'))
        App.closeDocument(doc.Name)
        doc = App.openDocument(str(result_file))
        restored_source = native_info(doc.getObject('Source'))
        copy = native_info(doc.getObject('PublicCopy'))
        assert metadata(restored_source) == metadata(source)
        assert restored_source['ids'] == source['ids']
        assert restored_source['construction'] == source['construction']
        check_copy(restored_source,copy)
        assert len(doc.getObject('PublicCopy').ExternalGeo) == 2
        App.closeDocument(doc.Name)
        assert all(hashlib.sha256(Path(p).read_bytes()).hexdigest() == digest for p,digest in digests.items())
        results.append(dict(case=source_file.stem,passed=True,ids=copy['ids'],construction=copy['construction'],
                            source_extensions_preserved=True,copy_external_extensions=[],file_hashes=digests))
    assert len(results) == 3
    with report.open('x',encoding='utf-8') as stream:
        stream.write(json.dumps(results,indent=2)+'\n')
    print(json.dumps(dict(passed=True,native_file_pairs=len(results)),indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('host','runtime','output'):
        parser.add_argument('--'+name,type=Path,required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True,exist_ok=False)
    config = output/'native-config'
    config.mkdir()
    os.environ['FREECAD_USER_HOME'] = str(config)
    runtime = json.loads(args.runtime.read_text(encoding='utf-8-sig'))
    subprocess.run([runtime['executable'],*runtime['args'],str(Path(__file__).resolve()),
                    '--native-fixtures',str(output)],check=True)
    fixture_list = json.loads((output/'fixtures.json').read_text())
    session = SketchSession(args.host.resolve(),instrumented_runtime(output,runtime))
    report = []
    try:
        for fixture in fixture_list:
            report.append(workflow(session,fixture,output))
            print(fixture['case'],'passed',flush=True)
    finally:
        session.close()
        (output/'report.json').write_bytes((json.dumps(report,indent=2)+'\n').encode())
    subprocess.run([runtime['executable'],*runtime['args'],str(Path(__file__).resolve()),
                    '--native-readonly',str(output),str(output/'native-comparison.json')],check=True)


if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--native-fixtures':
        fixtures(Path(sys.argv[2]))
    elif len(sys.argv) == 4 and sys.argv[1] == '--native-readonly':
        readonly_verify(Path(sys.argv[2]),Path(sys.argv[3]))
    else:
        main()
