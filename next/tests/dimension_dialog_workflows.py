"""Capture native ordinary-dimension failures and corrections on synthetic docs.

Uses the public host and shared command compiler. Outputs must be new; fixtures
contain solved native responses rather than invented geometry or revisions.
"""
import argparse
import json
import math
import os
from pathlib import Path
import sys

from angle_via_point_workflows import compile_command, e, fixture, geo, same_object, session_restored, sketch
from native_workflows import add, constraint, near, objects, transaction
from sketch_editor_workflows import SketchSession


def run(host, compiler, runtime, output):
    output.mkdir(parents=True, exist_ok=False)
    config = output / 'native-config'
    config.mkdir()
    os.environ['FREECAD_USER_HOME'] = str(config.resolve())
    os.environ['PYTHONDONTWRITEBYTECODE'] = '1'
    states = {}
    calls = []
    session = SketchSession(host, runtime)

    def keep(name, state):
        states[name] = state
        (output / 'states.json').write_text(json.dumps(states, indent=2) + '\n', encoding='utf-8')
        return state

    def call(op, **parameters):
        result = session.call(op, **parameters)
        calls.append(dict(op=op, parameters=parameters, result=result))
        (output / 'calls.json').write_text(json.dumps(calls, indent=2) + '\n', encoding='utf-8')
        return result

    def begin(doc, name):
        path = output / (name + '.FCStd')
        call('save', document=doc, path=str(path.resolve()))
        call('close', document=doc)
        state = call('open', path=str(path.resolve()))
        doc = state['document']
        return doc, call('sketch_begin', document=doc, object='Sketch', token=name + '-session')

    try:
        doc, state = fixture(session, 'OrdinaryDimensionRecovery')
        doc, state = begin(doc, 'existing')
        state = transaction(session, doc, 'Accepted angle for ordinary datum editing',
                            compile_command(compiler, state, [e(0), e(1), e(2, 1)], value='60 deg'))
        state = transaction(session, doc, 'Accepted expression',
                            compile_command(compiler, state, [], index=4, value='55 deg', expression=True))
        baseline = keep('ordinary_base', state)
        edits = compile_command(compiler, state, [], index=4, value='Missing.Angle', expression=True)
        call('transaction', document=doc, edits=edits, expect_error='invalid_model')
        state = keep('ordinary_expression_failed', call('inspect', document=doc))
        assert same_object(objects(state)['Sketch'], objects(baseline)['Sketch'])
        assert state['undo_count'] == baseline['undo_count']
        state = transaction(session, doc, 'Correct expression',
                            compile_command(compiler, state, [], index=4, value='75 deg', expression=True))
        keep('ordinary_expression_corrected', state)
        near(sketch(state)['constraints'][4]['value'], math.radians(75))
        # Via-point angles intentionally retain native quantity policy. Use a
        # malformed literal here; unit mismatch is covered by Distance below.
        edits = compile_command(compiler, state, [], index=4, value='10 deg +')
        call('transaction', document=doc, edits=edits, expect_error='native_error')
        failed = keep('ordinary_literal_failed', call('inspect', document=doc))
        assert same_object(objects(failed)['Sketch'], objects(state)['Sketch'])
        assert failed['undo_count'] == state['undo_count']
        state = transaction(session, doc, 'Correct explicit angle units',
                            compile_command(compiler, failed, [], index=4, value='65 deg'))
        keep('ordinary_literal_corrected', state)
        near(sketch(state)['constraints'][4]['value'], math.radians(65))
        assert objects(state)['Sketch']['properties']['ExpressionEngine'] == []
        edits = compile_command(compiler, state, [], index=2, value='10 deg')
        call('transaction', document=doc, edits=edits, expect_error='dimension_mismatch')
        failed = keep('distance_literal_failed', call('inspect', document=doc))
        state = transaction(session, doc, 'Correct ordinary length units',
                            compile_command(compiler, failed, [], index=2, value='8 mm'))
        keep('distance_literal_corrected', state)
        near(sketch(state)['constraints'][2]['value'], 8.)
        # A real native row replacement for the retry identity guard. This is a
        # deliberate external refresh injection, not a concurrent UI action.
        state = transaction(session, doc, 'Replace angle row with unrelated Radius', [
            dict(op='delete_constraint', object='Sketch', index=4),
            geo('circle', center=[20, 20, 0], radius=3, construction=True),
            constraint('Sketch', 'Radius', 3, 3.)])
        keep('ordinary_reused_row', state)
        assert sketch(state)['constraints'][4]['type'] == 'Radius'
        call('sketch_cancel', document=doc, token='existing-session')
        call('close', document=doc)

        doc = call('new', name='NewDimensionRecovery')['document']
        state = transaction(session, doc, 'Unconstrained line', [
            add('Sketcher::SketchObject', 'Sketch'), geo('line', start=[0, 0, 0], end=[5, 5, 0])])
        doc, state = begin(doc, 'creation')
        baseline = keep('creation_base', state)
        edits = compile_command(compiler, state, [e(0)], value='10 mm')
        call('transaction', document=doc, edits=edits, expect_error='dimension_mismatch')
        state = keep('creation_failed', call('inspect', document=doc))
        session_restored(objects(state)['Sketch'], objects(baseline)['Sketch'])
        assert state['undo_count'] == 0 and sketch(state)['constraint_count'] == 0
        state = transaction(session, doc, 'Correct new angle',
                            compile_command(compiler, state, [e(0)], value='60 deg'))
        keep('creation_corrected', state)
        assert sketch(state)['constraint_count'] == 1 and state['undo_count'] == 1
        near(sketch(state)['constraints'][0]['value'], math.radians(60))
        call('sketch_cancel', document=doc, token='creation-session')
        call('close', document=doc)
    finally:
        session.close()
    (output / 'report.json').write_text(json.dumps(dict(
        passed=True, native_cases=['existing_expression_retry', 'angle_literal_unit_retry',
                                  'distance_literal_unit_retry', 'new_angle_unit_retry', 'row_reuse'],
        states=len(states)), indent=2) + '\n', encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', type=Path, required=True)
    parser.add_argument('--compiler', type=Path, required=True)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    run(args.host.resolve(), args.compiler.resolve(), json.loads(args.runtime.read_text()), args.output.resolve())


if __name__ == '__main__':
    sys.dont_write_bytecode = True
    main()
