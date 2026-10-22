"""Native circular dimension creation, editing, presentation-input and session acceptance.

Run the same Rust compiler as the editor against pinned native geometry. Output
FCStd documents are disposable and belong in a caller-selected directory.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess
import traceback
from native_workflows import add, constraint, objects, transaction, near, solid, set_value
from sketch_editor_workflows import SketchSession

def geo(kind, **fields):
    return dict(op='geometry', object='Sketch', kind=kind, **fields)

def c(kind, *args):
    return constraint('Sketch', kind, *args)

def e(index, point=0):
    return dict(index=index, point=point)

def same(a, b):
    if isinstance(a, float) or isinstance(b, float):
        return type(a) in (float, int) and type(b) in (float, int) and math.isclose(a, b, rel_tol=1e-10, abs_tol=1e-09)
    if type(a) is not type(b):
        return False
    if isinstance(a, dict):
        return a.keys() == b.keys() and all((same(a[k], b[k]) for k in a))
    if isinstance(a, list):
        return len(a) == len(b) and all((same(x, y) for x, y in zip(a, b)))
    return a == b

def compile(compiler, snapshot, kind, selection, value=None):
    request = dict(op='constraint', snapshot=snapshot, kind=kind, selection=selection)
    if value is not None:
        request['value'] = value
    return compile_request(compiler, request)

def compile_request(compiler, request):
    result = subprocess.run([str(compiler)], input=json.dumps(request) + '\n', capture_output=True, text=True, encoding='utf-8', check=True)
    response = json.loads(result.stdout)
    assert response['ok'], response
    return response['edits']

def fixture(session, name, base, sector=False):
    state = session.call('new', name='DimensionCandidate' + name)
    doc = state['document']
    initial = [add('PartDesign::Body', 'Body'), add('Sketcher::SketchObject', 'Sketch', 'Body')] if sector else [add('Sketcher::SketchObject', 'Sketch')]
    if sector:
        state = transaction(session, doc, 'Native directed arc', initial + [base[0]])
        arc = objects(state)['Sketch']['sketch']['geometry'][0]
        handles = {h['point']: h['position'] for h in arc['handles']}
        start, end = (handles[1], handles[2])
        anchor = 1 if abs(start[1]) < 1e-08 else 2
        base = [geo('line', start=end, end=arc['center']), geo('line', start=arc['center'], end=start), c('Coincident', 0, 3, -1, 1), c('Radius', 0, 5.0), c('DistanceY', 0, anchor, 0.0), c('Coincident', 0, 2, 1, 1), c('Coincident', 1, 2, 2, 1), c('Coincident', 2, 2, 0, 1), c('Coincident', 1, 2, 0, 3)]
        initial = base + [add('PartDesign::Pad', 'Pad', 'Body'), set_value('Pad', 'Profile', {'link': 'Sketch'}), set_value('Pad', 'Length', 5)]
    else:
        initial += base
    state = transaction(session, doc, 'Native dimension fixture', initial)
    return state

def workflow(session, compiler, output, name, base, kind, selection, value, edited, check, edit_check, sector=False):
    state = fixture(session, name, base, sector)
    doc = state['document']
    before = objects(state)['Sketch']
    history = state['undo_count']
    names = set(objects(state))
    edits = compile(compiler, before, kind, selection, value)
    assert edits[-1]['op'] == 'datum' and edits[-1]['value'] == value
    index = before['sketch']['constraint_count']
    first = session.call('preview', document=doc, token='dimension', edits=edits)['state']
    assert objects(first)['Sketch']['sketch']['degrees_of_freedom'] == 0
    check(session, first, doc)
    again = session.call('preview', document=doc, token='dimension', edits=edits)['state']
    check(session, again, doc)
    state = session.call('preview_cancel', document=doc, token='dimension')
    assert state['undo_count'] == history and same(objects(state)['Sketch']['sketch'], before['sketch']) and (set(objects(state)) == names)
    session.call('preview', document=doc, token='apply', edits=edits)
    state = session.call('preview_commit', document=doc, token='apply')
    check(session, state, doc)
    assert state['undo_count'] == history + 1
    state = session.call('undo', document=doc)
    assert same(objects(state)['Sketch']['sketch'], before['sketch'])
    state = session.call('redo', document=doc)
    check(session, state, doc)
    # Existing dimensions use the same native datum/expression edits as the UI.
    state = transaction(session, doc, 'Existing native dimension expression', [dict(op='expression', object='Sketch', property=f'Constraints[{index}]', expression=edited)])
    edit_check(session, state, doc)
    state = transaction(session, doc, 'Return existing dimension to explicit units', [dict(op='expression', object='Sketch', property=f'Constraints[{index}]', expression=None), dict(op='datum', object='Sketch', index=index, value=value)])
    check(session, state, doc)
    state = session.call('undo', document=doc)
    edit_check(session, state, doc)
    state = session.call('redo', document=doc)
    check(session, state, doc)
    state = transaction(session, doc, 'Native reference dimension', [dict(op='constraint_state', object='Sketch', index=index, driving=False)])
    assert objects(state)['Sketch']['sketch']['constraints'][index]['driving'] is False
    assert objects(state)['Sketch']['sketch']['degrees_of_freedom'] == 1
    state = session.call('undo', document=doc)
    assert objects(state)['Sketch']['sketch']['degrees_of_freedom'] == 0
    state = session.call('redo', document=doc)
    assert objects(state)['Sketch']['sketch']['constraints'][index]['driving'] is False
    path = output / (name + '.FCStd')
    session.call('save', document=doc, path=str(path))
    session.call('close', document=doc)
    state = session.call('open', path=str(path))
    doc = state['document']
    check(session, state, doc)
    assert objects(state)['Sketch']['sketch']['constraints'][index]['driving'] is False
    state = transaction(session, doc, 'Edit reopened reference as driving', [dict(op='constraint_state', object='Sketch', index=index, driving=True), dict(op='datum', object='Sketch', index=index, value=value)])
    check(session, state, doc)
    session.call('close', document=doc)
    return dict(case=name, passed=True, path=str(path), native_args=edits[0]['args'], value=value, expression=edited, preview_cancel_apply=True, undo_redo=True, reference_driving=True, existing_edit_and_reopen=True)

def external_and_session_workflows(session, compiler, output):
    report = []
    state = session.call('new', name='ExternalArcDimensions')
    doc = state['document']
    state = transaction(session, doc, 'Native reference arc', [add('Sketcher::SketchObject', 'Master'), dict(op='geometry', object='Master', kind='arc', start=[2, 0, 0], mid=[math.sqrt(2), math.sqrt(2), 0], end=[0, 2, 0]), constraint('Master', 'Coincident', 0, 3, -1, 1), constraint('Master', 'Radius', 0, 2.0), constraint('Master', 'DistanceY', 0, 1, 0.0), constraint('Master', 'Angle', 0, math.pi / 2)])
    state = transaction(session, doc, 'Project recomputed native arc', [add('Sketcher::SketchObject', 'Sketch'), dict(op='external_geometry', object='Sketch', source='Master', subelement='Edge1')])
    sketch = objects(state)['Sketch']
    edits = []
    for index, kind in enumerate(['Angle', 'Distance']):
        edits.extend(compile(compiler, sketch, kind, [e(-3)]))
        edits.append(dict(op='constraint_state', object='Sketch', index=index, driving=False))
    candidate = session.call('preview', document=doc, token='external-measure', edits=edits)['state']
    assert all((not c['driving'] for c in objects(candidate)['Sketch']['sketch']['constraints']))
    state = session.call('preview_cancel', document=doc, token='external-measure')
    assert objects(state)['Sketch']['sketch']['constraint_count'] == 0
    state = transaction(session, doc, 'Reference-only native arc measurements', edits)
    state = transaction(session, doc, 'Reference parent arc change', [dict(op='datum', object='Master', index=3, value='120 deg')])
    constraints = objects(state)['Sketch']['sketch']['constraints']
    near(constraints[0]['value'], 2 * math.pi / 3)
    near(constraints[1]['value'], 4 * math.pi / 3)
    session.call('undo', document=doc)
    state = session.call('redo', document=doc)
    near(objects(state)['Sketch']['sketch']['constraints'][1]['value'], 4 * math.pi / 3)
    path = output / 'ExternalArcDimensions.FCStd'
    session.call('save', document=doc, path=str(path))
    session.call('close', document=doc)
    state = session.call('open', path=str(path))
    doc = state['document']
    assert all((not c['driving'] for c in objects(state)['Sketch']['sketch']['constraints']))
    near(objects(state)['Sketch']['sketch']['constraints'][0]['value'], 2 * math.pi / 3)
    session.call('close', document=doc)
    report.append(dict(case='ExternalArcMeasurements', passed=True, parent_updates=True, preview_cancel=True, undo_redo_reopen=True))
    state = session.call('new', name='ClockwiseEndpointIdentity')
    doc = state['document']
    state = transaction(session, doc, 'Clockwise native arc', [add('Sketcher::SketchObject', 'Sketch'), geo('arc', start=[5, 0, 0], mid=[5 / math.sqrt(2), -5 / math.sqrt(2), 0], end=[0, -5, 0])])
    geometry = objects(state)['Sketch']['sketch']['geometry'][0]
    handles = {h['point']: h['position'] for h in geometry['handles']}
    near(geometry['start'][0], 5)
    near(geometry['end'][1], -5)
    near(handles[1][0], 0)
    near(handles[1][1], -5)
    near(handles[2][0], 5)
    near(handles[2][1], 0)
    edits = compile(compiler, objects(state)['Sketch'], 'DistanceX', [e(0, 1)], '2 mm')
    near(edits[0]['args'][-1], 0)
    state = transaction(session, doc, 'Dimension the actual native selected endpoint', edits)
    handles = {h['point']: h['position'] for h in objects(state)['Sketch']['sketch']['geometry'][0]['handles']}
    near(handles[1][0], 2)
    session.call('undo', document=doc)
    state = session.call('redo', document=doc)
    handles = {h['point']: h['position'] for h in objects(state)['Sketch']['sketch']['geometry'][0]['handles']}
    near(handles[1][0], 2)
    session.call('close', document=doc)
    report.append(dict(case='ClockwiseEndpointIdentity', passed=True, raw_geometry_differs_from_solver_handles=True, native_point1_drives_selected_endpoint=True, undo_redo=True))
    state = session.call('open', path=str(output / 'ArcLength.FCStd'))
    doc = state['document']
    solid(state, 'Pad', 187.5)
    for token in ['cancel-consumed', 'finish-consumed']:
        state = session.call('sketch_begin', document=doc, object='Sketch', token=token)
        index = objects(state)['Sketch']['sketch']['constraint_count'] - 1
        baseline = objects(state)['Sketch']['sketch']
        edits = [dict(op='constraint_state', object='Sketch', index=index, driving=True), dict(op='datum', object='Sketch', index=index, value='20 mm')]
        state = session.call('preview', document=doc, token='length-task', edits=edits)['state']
        near(objects(state)['Sketch']['sketch']['constraints'][index]['value'], 20)
        # Downstream context stays unchanged until native Sketch Finish.
        solid(state, 'Pad', 187.5)
        state = session.call('preview_cancel', document=doc, token='length-task')
        assert objects(state)['Sketch']['sketch']['constraints'][index]['driving'] is False
        state = transaction(session, doc, 'Accepted dimension action', edits)
        state = session.call('undo', document=doc)
        assert objects(state)['Sketch']['sketch']['constraints'][index]['driving'] is False
        state = session.call('redo', document=doc)
        near(objects(state)['Sketch']['sketch']['constraints'][index]['value'], 20)
        if token == 'cancel-consumed':
            state = session.call('sketch_cancel', document=doc, token=token)
            solid(state, 'Pad', 187.5)
            assert objects(state)['Sketch']['sketch']['constraints'][index]['driving'] is False
        else:
            state = session.call('sketch_finish', document=doc, token=token)
            solid(state, 'Pad', 250)
            assert state['sketch_edit'] is None and state['undo_count'] == 1
    solid(session.call('undo', document=doc), 'Pad', 187.5)
    solid(session.call('redo', document=doc), 'Pad', 250)
    session.call('close', document=doc)
    report.append(dict(case='ConsumedArcSketchSession', passed=True, task_preview_cancel=True, action_undo_redo=True, session_cancel=True, finish_single_document_undo=True))
    return report

def snapped_arc_workflows(session, compiler, output):
    results = []
    cases = [('CCWMinor', 1, 90, 0, [0, 0]), ('CWMinor', -1, 90, 0, [0, 0]),
             ('CCWMajor', 1, 270, 0, [0, 0]), ('CWMajor', -1, 270, 0, [0, 0]),
             ('CWMinorShifted', -1, 90, 0.6, [31, -7]), ('CWMajorShifted', -1, 270, 1.1, [-13, 8]),
             ('CWCurveTarget', -1, 90, 0, [0, 0])]
    for name, sign, degrees, rotation, center in cases:
        sweep = sign * math.radians(degrees)
        points = [[center[0] + 5 * math.cos(rotation + angle), center[1] + 5 * math.sin(rotation + angle), 0]
                  for angle in [0, sweep / 2, sweep]]
        curve_target = name == 'CWCurveTarget'
        if curve_target:
            base = [geo('line', start=[5, -3, 0], end=[5, 3, 0]), geo('point', position=points[2]),
                    c('Block', 0), c('DistanceX', 1, 1, points[2][0]), c('DistanceY', 1, 1, points[2][1])]
        else:
            base = [geo('point', position=points[0]), geo('point', position=points[2])]
            for index, point in enumerate([points[0], points[2]]):
                base.extend([c('DistanceX', index, 1, point[0]), c('DistanceY', index, 1, point[1])])
        state = fixture(session, name, base)
        doc = state['document']
        history = state['undo_count']
        baseline_path = output / ('snap-' + name + '-baseline.FCStd')
        session.call('save', document=doc, path=str(baseline_path))
        original_hash = hashlib.sha256(baseline_path.read_bytes()).hexdigest()
        state = session.call('sketch_begin', document=doc, object='Sketch', token='snap-' + name)
        raw_request = dict(op='draw', object='Sketch', tool='arc', points=points, index=2)
        raw = compile_request(compiler, raw_request)
        state = session.call('preview', document=doc, token='native-oracle', edits=raw)['state']
        handles = {h['point']: h['position'] for h in objects(state)['Sketch']['sketch']['geometry'][2]['handles']}
        native_ids = [next(point for point in [1, 2] if math.dist(handles[point], clicked) < 1e-7)
                      for clicked in [points[0], points[2]]]
        def arc_length():
            return next(edge['length'] for edge in session.call('topology', document=doc, object='Sketch')['edges'] if edge['geometry_type'] == 'Part::GeomCircle')
        native_length = arc_length()
        near(native_length, 5 * math.radians(degrees))
        session.call('preview_cancel', document=doc, token='native-oracle')
        # Exact old editor behavior: first click->point1, last click->point2.
        legacy = raw + [c('PointOnObject', 2, 1, 0) if curve_target else c('Coincident', 2, 1, 0, 1), c('Coincident', 2, 2, 1, 1)]
        legacy_record = {}
        try:
            state = session.call('preview', document=doc, token='legacy-snap', edits=legacy)['state']
            handles = {h['point']: h['position'] for h in objects(state)['Sketch']['sketch']['geometry'][2]['handles']}
            correct = all(math.dist(handles[point], clicked) < 1e-7 for point, clicked in zip(native_ids, [points[0], points[2]]))
            legacy_record = dict(accepted=True, preserves_clicked_native_endpoints=correct,
                                 length=arc_length())
            assert correct == (sign > 0), legacy_record
        except AssertionError as error:
            response = error.args[0] if error.args else None
            if not isinstance(response, dict) or response.get('ok') is not False:
                raise
            assert sign < 0 and response['error']['code'] == 'invalid_model', response
            legacy_record = dict(accepted=False, error=response['error'])
        state = session.call('inspect', document=doc)
        if state['preview']:
            session.call('preview_cancel', document=doc, token='legacy-snap')
        request = dict(raw_request, snaps=[e(0, 0 if curve_target else 1), None, e(1, 1)])
        corrected = compile_request(compiler, request)
        assert corrected[-2]['args'] == ([2, native_ids[0], 0] if curve_target else [2, native_ids[0], 0, 1])
        assert corrected[-1]['args'] == [2, native_ids[1], 1, 1]
        state = transaction(session, doc, 'Shared editor/compiler snapped arc', corrected)
        def check(current):
            native = objects(current)['Sketch']['sketch']
            handles = {h['point']: h['position'] for h in native['geometry'][2]['handles']}
            assert all(math.dist(handles[point], clicked) < 1e-7 for point, clicked in zip(native_ids, [points[0], points[2]]))
            near(arc_length(), native_length)
            assert native['degrees_of_freedom'] == (2 if curve_target else 1) and native['constraint_count'] == (5 if curve_target else 6)
        check(state)
        state = session.call('undo', document=doc)
        assert objects(state)['Sketch']['sketch']['geometry_count'] == 2
        state = session.call('redo', document=doc)
        check(state)
        state = session.call('sketch_finish', document=doc, token='snap-' + name)
        assert state['undo_count'] == history + 1
        assert objects(session.call('undo', document=doc))['Sketch']['sketch']['geometry_count'] == 2
        state = session.call('redo', document=doc)
        check(state)
        path = output / ('snap-' + name + '-final.FCStd')
        session.call('save', document=doc, path=str(path))
        session.call('close', document=doc)
        state = session.call('open', path=str(path)); doc = state['document']
        check(state)
        assert state['sketch_edit'] is None
        session.call('close', document=doc)
        assert hashlib.sha256(baseline_path.read_bytes()).hexdigest() == original_hash
        results.append(dict(case=name, native_input_endpoint_ids=native_ids, legacy=legacy_record,
                            corrected_preserves_endpoints_and_trace=True, native_expected_length=native_length, first_snap_kind='PointOnObject' if curve_target else 'Coincident',
                            action_and_document_history=True, reopen=True, points=points,
                            baseline=str(baseline_path), baseline_sha256=original_hash, final=str(path)))
    return results

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', type=Path, required=True)
    parser.add_argument('--compiler', type=Path, required=True)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    compiler = args.compiler.resolve()
    runtime = json.loads(args.runtime.read_text(encoding='utf-8-sig'))
    session = SketchSession(args.host.resolve(), runtime)
    report = []
    result = {'workflows': report}
    try:
        for clockwise in [False, True]:
            sign = -1 if clockwise else 1
            arc = [geo('arc', start=[5, 0, 0], mid=[5 / math.sqrt(2), sign * 5 / math.sqrt(2), 0], end=[0, sign * 5, 0])]
            suffix = 'Clockwise' if clockwise else ''

            def angle_check(s, state, doc):
                solid(state, 'Pad', 125 * math.pi / 3)

            def edited_angle(s, state, doc):
                solid(state, 'Pad', 125 * math.pi / 2)
            report.append(workflow(session, compiler, output, 'ArcAngle' + suffix, arc, 'Angle', [e(0)], '120 deg', '180 deg', angle_check, edited_angle, True))

            def length_check(s, state, doc):
                solid(state, 'Pad', 187.5)

            def edited_length(s, state, doc):
                solid(state, 'Pad', 250)
            report.append(workflow(session, compiler, output, 'ArcLength' + suffix, arc, 'Distance', [e(0)], '15 mm', '20 mm', length_check, edited_length, True))

        def px(expected):
            return lambda s, state, doc: near(objects(state)['Sketch']['sketch']['geometry'][0]['position'][0], expected)
        report.append(workflow(session, compiler, output, 'PointCircle', [geo('point', position=[10, 0, 0]), geo('circle', center=[0, 0, 0], radius=2), c('Block', 1), c('DistanceY', 0, 1, 0.0)], 'Distance', [e(0, 1), e(1)], '6 mm', '8 mm', px(8), px(10)))

        def cy(expected):
            return lambda s, state, doc: near(objects(state)['Sketch']['sketch']['geometry'][0]['center'][1], expected)
        report.append(workflow(session, compiler, output, 'CircleLine', [geo('circle', center=[0, 5, 0], radius=2), geo('line', start=[-10, 0, 0], end=[10, 0, 0]), c('Block', 1), c('Radius', 0, 2.0), c('DistanceX', 0, 3, 0.0)], 'Distance', [e(1), e(0)], '4 mm', '6 mm', cy(6), cy(8)))

        def cx(expected):
            return lambda s, state, doc: near(objects(state)['Sketch']['sketch']['geometry'][1]['center'][0], expected)
        for name, big, initial, value, edited, want, next_want in [('Separated', 2, 10, '7 mm', '9 mm', 12, 14), ('Contained', 10, 2, '4 mm', '3 mm', 3, 4), ('Overlapping', 2, 4, '-1 mm', '-0.5 mm', 4, 4.5), ('Subpixel', 2, 5.01, '0.01 mm', '0.02 mm', 5.01, 5.02)]:
            report.append(workflow(session, compiler, output, 'Circles' + name, [geo('circle', center=[0, 0, 0], radius=big), geo('circle', center=[initial, 0, 0], radius=3), c('Block', 0), c('Radius', 1, 3.0), c('DistanceY', 1, 3, 0.0)], 'Distance', [e(0), e(1)], value, edited, cx(want), cx(next_want)))
        state = fixture(session, 'CirclesZeroReference', [
            geo('circle', center=[0, 0, 0], radius=2),
            geo('circle', center=[5, 0, 0], radius=3), c('Block', 0),
            c('Radius', 1, 3.0), c('DistanceY', 1, 3, 0.0)])
        doc = state['document']
        before = objects(state)['Sketch']['sketch']
        edits = compile(compiler, objects(state)['Sketch'], 'Distance', [e(0), e(1)])
        edits.append(dict(op='constraint_state', object='Sketch', index=3, driving=False))
        state = transaction(session, doc, 'Native zero reference clearance', edits)
        near(objects(state)['Sketch']['sketch']['constraints'][3]['value'], 0)
        assert objects(state)['Sketch']['sketch']['degrees_of_freedom'] == 1
        session.call('undo', document=doc)
        state = session.call('redo', document=doc)
        near(objects(state)['Sketch']['sketch']['constraints'][3]['value'], 0)
        path = output / 'CirclesZeroReference.FCStd'
        session.call('save', document=doc, path=str(path))
        session.call('close', document=doc)
        state = session.call('open', path=str(path)); doc = state['document']
        measured = objects(state)['Sketch']['sketch']
        near(measured['constraints'][3]['value'], 0)
        assert measured['constraints'][3]['driving'] is False
        assert same(measured['geometry'], before['geometry'])
        session.call('close', document=doc)
        result['zero_reference_clearance'] = dict(passed=True, path=str(path), undo_redo_reopen=True)
        result['external_and_sessions'] = external_and_session_workflows(session, compiler, output)
        result['snapped_arcs'] = snapped_arc_workflows(session, compiler, output)
    except BaseException:
        result['error'] = traceback.format_exc()
    finally:
        session.close()
        (output / 'common-dimensions-report.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    assert 'error' not in result, result
    assert len(report) == 10 and len(result['external_and_sessions']) == 3 and len(result['snapped_arcs']) == 7
    print(json.dumps({'passed': True, 'workflows': len(report), 'external_and_sessions': len(result['external_and_sessions']), 'snapped_arcs': len(result['snapped_arcs'])}))
if __name__ == '__main__':
    main()
