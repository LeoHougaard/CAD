import hashlib,json,os,sys
from pathlib import Path
R=Path(os.environ['CAD_EXTERNAL_ROOT']);sys.path.insert(0,str(Path(__file__).resolve().parent))
from external_recorded_session import RecordedSession
from native_workflows import objects
out=R/sys.argv[1];out.mkdir();destinations=R/sys.argv[2]
runtime=json.loads((R/'runtime.json').read_text());runtime['script']=str(Path(__file__).resolve().parent/'fixtures/external_save_fault_worker.py')
os.environ['CAD_SAVE_INJECT_TRACE']=str(out/'native-trace.ndjson')
s=RecordedSession(Path(os.environ['CAD_EXTERNAL_HOST']),runtime,out,'worker');results=[]
def call(op,**kw):
    if 'document' in kw:kw.setdefault('expected_revision',s.revisions.get(kw['document']))
    return s.call(op,**kw)
def inventory(folder):return {str(p.relative_to(folder)):hashlib.sha256(p.read_bytes()).hexdigest() for p in folder.rglob('*') if p.is_file()}
def content(state):return {o['name']:{k:o.get(k) for k in ('type','properties','expressions','shape','global_placement')} for o in state['objects']}
try:
    for stage in ['after_save_as','clear_rebind','native_save','serialized_paths']:
        doc=call('open',path=str(R/'native/Final.FCStd'))['document']
        call('transaction',document=doc,label='Dirty root content',edits=[dict(op='add_object',type='Part::Box',name='DirtyContent')])
        call('transaction',document=doc,label='Redo marker',edits=[dict(op='add_object',type='Part::Box',name='RedoMarker')])
        call('undo',document=doc)
        before=call('inspect',document=doc);folder=destinations/stage;destination=folder/'Destination.FCStd';files=inventory(folder)
        call('save',document=doc,path=str(destination),expect_error='save_recovery')
        after=call('inspect',document=doc);recovery=after['save_recovery']
        assert recovery['destination_restored'] and recovery['dirty'] and recovery['history_preserved']
        assert inventory(folder)==files,(stage,'destination or earlier native backups changed')
        assert Path(after['path']).resolve()==destination.resolve()
        assert content(after)==content(before),(stage,'root content changed')
        assert (after['undo_count'],after['redo_count'])==(before['undo_count'],before['redo_count'])
        trace=json.loads((out/'native-trace.ndjson').read_text().splitlines()[-1]);assert trace['before']['ids']==trace['after']['ids']
        # Retry the SAME destination through normal Save. The live native path
        # has already changed, so a cross-folder-only repair would fail here.
        call('save',document=doc,path=str(destination));saved=call('inspect',document=doc)
        assert saved['save_recovery'] is None and content(saved)==content(before)
        assert (saved['undo_count'],saved['redo_count'])==(before['undo_count'],before['redo_count'])
        assert all(inventory(folder)[n]==v for n,v in files.items() if n!='Destination.FCStd')
        call('redo',document=doc);assert 'RedoMarker' in objects(call('inspect',document=doc))
        call('undo',document=doc);assert content(call('inspect',document=doc))==content(before)
        call('close',document=doc);reopened=call('open',path=str(destination));doc=reopened['document']
        assert content(reopened)==content(before);assert 'DirtyContent' in objects(reopened)
        call('close',document=doc)
        results.append(dict(stage=stage,existing_destination_and_backups_restored=True,native_ids_and_history_unchanged=True,same_destination_retry_and_reopen=True))
    source_manifest=json.loads((R/'fixtures2/manifest.json').read_text())['files'];assert all(hashlib.sha256((R/'fixtures2'/n).read_bytes()).hexdigest()==v for n,v in source_manifest.items())
    (out/'results.json').write_text(json.dumps(dict(passed=True,cases=results,limits='Native-save injection reports an error after a genuine native final write. Not a disk-full, process-crash or failed-filesystem-restoration test.'),indent=2));print('all SaveAs failure stages, existing backups and same-destination retries passed')
finally:s.close()
