"""Copy actions across native units, suppression, ownership and checkpoints."""
import argparse
import json
from pathlib import Path

from native_workflows import add, objects, set_value, transaction
from suppression_checkpoint_workflows import RecordedSession, feature, native, owned, seed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['host', 'runtime', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    session = RecordedSession(args.host.resolve(), json.loads(args.runtime.read_text()), args.output)
    records = []
    try:
        doc, _ = seed(session, 'CopyCombined')
        transaction(session, doc, 'Construction point and expression', [
            dict(op='geometry', object='Profile', kind='point', position=[1, 2, 0], construction=True),
            dict(op='expression', object='Profile', property='Constraints.Size', expression='2 mm')])
        other = session.call('new', name='CopyKeeper')['document']
        transaction(session, other, 'Keeper', [add('Part::Box', 'Box')])
        transaction(session, other, 'Keeper edit', [set_value('Box', 'Length', 12)])
        keeper = owned(session.call('undo', document=other), other)
        session.call('document_settings', document=doc, expected_revision=session.revisions[doc], unit_system=3)
        before = session.call('inspect', document=doc)
        units = before['units']
        uuid = owned(before, doc)['uuid']
        query = session.call('quantities', document=doc, expected_revision=session.revisions[doc],
            items=[dict(text='0.2', unit='mm', default_unit='in')])
        assert query['items'][0]['value'] == 5.08, query
        begin = session.call('sketch_begin', document=doc, object='Profile', token='copy-owned')
        original = native(begin, 'Profile')
        follower = native(begin, 'Follower')
        frozen_feature = feature(begin, 'A')
        action = dict(op='copy_geometry', object='Profile', indices=[1],
            displacement=[dict(quantity_input='0.2', default_unit='in'), '-2 mm', 0])
        candidate = session.call('preview', document=doc, token='copy-preview', edits=[action])['state']
        assert objects(candidate)['Profile']['sketch']['geometry'][2]['position'] == [6.08, 0, 0]
        session.call('preview', document=doc, token='copy-preview', edits=[
            dict(action, displacement=[20, 0, 0]), dict(action, indices=[-3])], expect_error='invalid_request')
        recovered = session.call('inspect', document=doc)
        assert native(recovered, 'Profile') == native(candidate, 'Profile')
        cancelled = session.call('preview_cancel', document=doc, token='copy-preview')
        assert native(cancelled, 'Profile') == original and native(cancelled, 'Follower') == follower
        assert feature(cancelled, 'A') == frozen_feature
        session.call('document_settings', document=doc, expected_revision=session.revisions[doc],
            unit_system=3, expect_error='sketch_active')
        session.call('transaction', document=other, edits=[set_value('Box', 'Length', 20)], expect_error='sketch_active')
        session.call('transaction', document=doc, edits=[set_value('A', 'Suppressed', True)], expect_error='sketch_active')
        for label, edit in [
            ('Copy', action),
            ('Clone', dict(action, op='clone_geometry')),
            ('Move', dict(action, op='move_geometries')),
            ('Array', dict(action, op='rectangular_array', columns=3, rows=2,
                clone=False, constrain_displacement=False, perpendicular_scale=0.5))]:
            accepted = transaction(session, doc, label, [edit])
            exact = native(accepted, 'Profile')
            if label == 'Move':
                assert exact['ids'] == original['ids']
            assert feature(accepted, 'A') == frozen_feature and native(accepted, 'Follower') == follower
            undone = session.call('undo', document=doc)
            assert native(undone, 'Profile') == original
            redone = session.call('redo', document=doc)
            assert native(redone, 'Profile') == exact
            session.call('undo', document=doc)
            records.append(dict(action=label, exact_ids_xml_expressions_elementmap=True))
        accepted = transaction(session, doc, 'Final Copy and suppression driver', [
            action, dict(op='datum', object='Profile', index=0, value='3 mm'),
            dict(op='expression', object='Profile', property='Constraints.Size', expression='3 mm')])
        exact = native(accepted, 'Profile')
        # Failure after the checkpoint restore must recover this accepted state.
        session.call('preview', document=doc, token='restore-fault', edits=[action],
            suppression_restore_failures=1, expect_error='native_error')
        recovered = session.call('inspect', document=doc)
        assert recovered['available'] and native(recovered, 'Profile') == exact
        finished = session.call('sketch_finish', document=doc, token='copy-owned')
        assert objects(finished)['A']['properties']['Suppressed']
        assert '_CadSuppressionShape' in objects(finished)['A']['property_types']
        assert finished['units'] == units and owned(finished, doc)['uuid'] == uuid
        assert owned(finished, other) == keeper
        session.call('undo', document=doc)
        baseline = session.call('redo', document=doc)
        assert native(baseline, 'Profile') == exact and feature(baseline, 'A') == feature(finished, 'A')
        session.call('sketch_begin', document=doc, object='Profile', token='later-copy')
        transaction(session, doc, 'Later Move', [dict(action, op='move_geometries')])
        cancelled = session.call('sketch_cancel', document=doc, token='later-copy')
        assert native(cancelled, 'Profile') == exact
        assert native(cancelled, 'Follower') == native(baseline, 'Follower')
        assert feature(cancelled, 'A') == feature(baseline, 'A')
        assert cancelled['units'] == units and owned(cancelled, other) == keeper
        path = args.output / 'combined.FCStd'
        session.call('save', document=doc, path=str(path))
        session.call('close', document=doc)
        reopened = session.call('open', path=str(path))
        assert native(reopened, 'Profile') == exact
        assert feature(reopened, 'A') == feature(baseline, 'A')
        assert reopened['units'] == units and owned(reopened, other) == keeper
        session.call('close', document=reopened['document'])
        session.call('close', document=other)
        records.append(dict(imperial_native_input=True, protected_indices=True, checkpoint_recovery=True,
            consumed_finish_suppression=True, later_cancel_exact=True, other_document_redo_exact=True,
            settings_guarded=True, fresh_open=True))
        (args.output / 'results.json').write_text(json.dumps(records, indent=2) + '\n')
        print(json.dumps(records, indent=2))
    finally:
        session.close()


if __name__ == '__main__':
    main()
