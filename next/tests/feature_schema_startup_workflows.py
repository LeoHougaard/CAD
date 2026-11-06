"""Schema reads preserve the first model and independent native histories."""
import argparse
import json
from pathlib import Path
from native_workflows import Session, add, objects, set_value, transaction


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ('host', 'runtime', 'output'):
        parser.add_argument('--' + key, type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    session = Session(args.host.resolve(), json.loads(args.runtime.read_text()))
    try:
        schemas = {kind: session.call('feature_schema', type=kind)
                   for kind in session.capabilities['object_types']}
        session.call('new', name='SchemaA')
        transaction(session, 'SchemaA', 'First model', [add('Part::Box', 'Box')])
        session.call('new', name='SchemaB')
        transaction(session, 'SchemaB', 'Other model', [add('Part::Box', 'Box')])
        other = session.call('inspect', document='SchemaB')
        transaction(session, 'SchemaA', 'Change first model', [set_value('Box', 'Length', 25.4)])
        assert session.call('inspect', document='SchemaB') == other
        state = session.call('undo', document='SchemaA')
        assert state['redo_count'] == 1
        for kind in schemas:
            assert session.call('feature_schema', type=kind) == schemas[kind]
            session.call('feature_schema', type=kind, document='SchemaA',
                         expected_revision=state['revision'])
        assert session.call('inspect', document='SchemaA') == state
        state = session.call('redo', document='SchemaA')
        assert objects(state)['Box']['properties']['Length'] == 25.4
        state = session.call('preview', document='SchemaA', expected_revision=state['revision'],
                             token='schema-preview', edits=[set_value('Box', 'Length', 50.8)])['state']
        for kind in schemas:
            session.call('feature_schema', type=kind, document='SchemaA',
                         expected_revision=state['revision'])
        session.call('feature_schema', type='Part::Box', document='SchemaA',
                     expected_revision=state['revision'] - 1, expect_error='stale_revision')
        session.call('feature_schema', type='Part::Box', document='SchemaA',
                     expected_revision=state['revision'], values={'Length': '2 s'},
                     expect_error='native_error')
        assert session.call('inspect', document='SchemaA') == state
        state = session.call('preview_commit', document='SchemaA',
                             expected_revision=state['revision'], token='schema-preview')
        state = session.call('undo', document='SchemaA')
        assert objects(state)['Box']['properties']['Length'] == 25.4
        assert session.call('inspect', document='SchemaB') == other
        for doc in ('SchemaA', 'SchemaB'):
            session.call('close', document=doc)
        (out / 'passed.json').write_text(json.dumps(dict(types=len(schemas), first_transaction=True,
            independent_histories=True, schema_reads_preserve_redo_and_preview=True), indent=2))
    finally:
        session.close()


if __name__ == '__main__':
    main()
