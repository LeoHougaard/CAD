"""Composed source validation, observer lifetime, quarantine and teardown."""
import hashlib
import json
import os
from pathlib import Path
from external_recorded_session import RecordedSession

root = Path(os.environ['CAD_EXTERNAL_ROOT'])
out = root / 'source-cleanup'
out.mkdir()
runtime = json.loads((root / 'runtime.json').read_text())
runtime['script'] = str(Path(__file__).parent / 'fixtures/open_cleanup_dependency_worker.py')
os.environ.update(R7_BACKEND=str(Path(__file__).resolve().parents[1] / 'backend'), R7_BINDINGS=str(out / 'bindings'))
session = RecordedSession(Path(os.environ['CAD_EXTERNAL_HOST']), runtime, out, 'native')


def call(op, **params):
    if 'document' in params:
        params.setdefault('expected_revision', session.revisions.get(params['document']))
    return session.call(op, **params)


def facts():
    return call('_native_facts')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def close_attempts():
    files = list((out / 'bindings').glob('*.json'))
    assert len(files) == 1
    return [event for event in json.loads(files[0].read_text())['events']
        if event.get('event') == 'close_attempt' and event['op'] == 'assembly_sources']


try:
    source = out / 'Source.FCStd'
    source.write_bytes((root / 'fixtures2/hole-invalid.FCStd').read_bytes())
    invalid_bytes = sha(source)
    keeper = call('new', name='SourceKeeper')['document']
    call('transaction', document=keeper, edits=[dict(op='assembly_create', name='Assembly', joints='Joints'),
        dict(op='add_object', type='Part::Box', name='Box')])
    call('save', document=keeper, path=str(out / 'Keeper.FCStd'))
    call('transaction', document=keeper, edits=[dict(op='set', object='Box', property='Length', value=23)])
    call('undo', document=keeper)
    keep = call('inspect', document=keeper)
    native = facts()
    assert keep['redo_count'] == 1
    suppressed = root / 'suppression-source/Fillet.FCStd'
    suppressed_bytes = sha(suppressed)
    suppressed_choices = call('assembly_sources', document=keeper, path=str(suppressed))['objects']
    assert any(obj['name'] == 'Body' for obj in suppressed_choices)
    assert call('inspect', document=keeper) == keep
    assert facts()['documents'] == native['documents'] and facts()['observers'] == native['observers']
    assert sha(suppressed) == suppressed_bytes
    attempts = len(close_attempts())
    error = call('assembly_sources', document=keeper, path=str(source), expect_error='invalid_model',
        _independent_fault=dict(close_fail_files=['Source.FCStd']))
    assert 'Load cleanup:' in error['message'] and 'independent Close fault' in error['message']
    after = facts()
    assert len(close_attempts()) == attempts + 1
    assert call('inspect', document=keeper) == keep
    assert after['documents'][keeper] == native['documents'][keeper]
    assert (after['active'], after['native_active']) == (native['active'], native['native_active'])
    quarantined = next(name for name, doc in after['documents'].items() if Path(doc['filename']) == source)
    assert quarantined in after['observers'] and quarantined in after['failed_loads']
    unavailable = call('inspect', document=quarantined)
    assert unavailable['available'] is False and unavailable['objects'] == []
    call('mesh', document=quarantined, object='Body', expect_error='document_unavailable')
    alias = out / 'SourceAlias.FCStd'
    os.link(source, alias)
    before = facts()
    call('open', path=str(alias), expect_error='document_unavailable')
    assert facts() == before and sha(source) == invalid_bytes
    alias.unlink()
    # An explicit repeated Load attempts this failed native owner once, keeps
    # the original diagnostics, and preserves the unrelated keeper/Redo.
    attempts = len(close_attempts())
    call('assembly_sources', document=keeper, path=str(source), expect_error='invalid_model',
        _independent_fault=dict(close_fail_files=['Source.FCStd']))
    assert len(close_attempts()) == attempts + 1
    assert call('inspect', document=keeper) == keep
    source.write_bytes((root / 'fixtures2/hole-stale.FCStd').read_bytes())
    valid_bytes = sha(source)
    choices = call('assembly_sources', document=keeper, path=str(source))['objects']
    value = next(obj['source'] for obj in choices if obj['name'] == 'Body')
    settled = facts()
    assert not settled['failed_loads'] and set(settled['documents']) == {keeper}
    assert settled['observers'] == [keeper] and sha(source) == valid_bytes
    assert call('inspect', document=keeper) == keep
    call('redo', document=keeper)
    call('undo', document=keeper)
    call('transaction', document=keeper, edits=[dict(op='assembly_insert', assembly='Assembly', name='HoleInstance', external_source=value)])
    loaded = facts()
    dependency = next(name for name in loaded['documents'] if name != keeper)
    assert dependency in loaded['observers']
    result = call('close', document=keeper, _independent_fault=dict(close_fail_files=['Source.FCStd']))
    assert result['closed'] and 'independent Close fault' in result['cleanup_error']
    leftover = facts()
    assert keeper not in leftover['documents'] and keeper not in leftover['observers']
    assert dependency in leftover['failed_loads'] and dependency in leftover['observers']
    recovered = call('inspect', document=dependency)
    assert recovered['available'] is False and recovered['objects'] == []
    call('close', document=dependency)
    assert not facts()['documents'] and not facts()['observers']
    assert sha(source) == valid_bytes
    (out / 'passed.json').write_text(json.dumps(dict(original_source_error_preserved=True,
        failed_close_retains_observer_and_registered_quarantine=True, physical_alias_denied=True,
        keeper_content_history_redo_exact=True, exact_path_retry_and_hole_regeneration=True,
        real_suppressed_dependency_validation_and_teardown=True,
        actual_root_close_teardown_failure_reported=True, explicit_recovery_releases_observers=True), indent=2) + '\n')
finally:
    session.close()
