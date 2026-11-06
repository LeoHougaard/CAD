"""External-root SaveAs rollback and explicit native-path recovery.

Native SaveAs changes the live document path before all stages can succeed.
Destination bytes and native backup sidecars are restored on rejection; the live
path change is reported, never disguised as an atomic native transaction.
"""
import os
from pathlib import Path
import re
import shutil
import tempfile
import assembly_sources as Sources

_recoveries = {}


class SaveError(Exception):
    code = 'save_recovery'


def checkpoint(stage):
    """No-op production boundary. Isolated fault-injection wrappers replace it."""


def native_save(doc):
    doc.save()


def validate_paths(path):
    Sources.validate_saved_paths(path)


def metadata(doc):
    recovery = _recoveries.get(doc.Name)
    if recovery is None:
        return None
    pending = Sources.recovery_metadata(doc)
    return {**recovery, 'live_graph_restored': pending is None,
            'pending_edges': [] if pending is None else pending['pending_edges'],
            'edge_errors': [] if pending is None else pending['errors']}


def close(key):
    _recoveries.pop(key, None)


def destination_files(path):
    path = Path(path)
    legacy = re.compile(re.escape(path.name) + r'\d+', re.IGNORECASE)
    return {p.name: p for p in path.parent.iterdir() if p.is_file() and (
        p.name == path.name or legacy.fullmatch(p.name)
        or (p.name.lower().startswith(path.stem.lower() + '.') and p.suffix.lower() == '.fcbak'))}


def save(doc, path):
    destination = Path(path)
    previous_path = doc.FileName
    history = (doc.UndoCount, doc.RedoCount)
    before = destination_files(path)
    rollback = Path(tempfile.mkdtemp(prefix='.cad-save-before-', dir=destination.parent))
    try:
        for name, original in before.items():
            shutil.copy2(original, rollback / name)
    except Exception:
        for copied in rollback.iterdir():
            copied.unlink()
        rollback.rmdir()
        raise
    try:
        doc.saveAs(str(destination))
        checkpoint('after_save_as')
        # Rebase on every external-root save, including a same-destination retry
        # after native SaveAs already moved the live path during a rejection.
        Sources.rebase_after_saveas(doc, lambda: checkpoint('clear_rebind'))
        native_save(doc)
        validate_paths(str(destination))
        Sources.check(doc)
        if history != (doc.UndoCount, doc.RedoCount):
            raise RuntimeError('Native SaveAs changed root history')
        # Two native writes must not rotate away earlier backup bytes or leave
        # an intermediate stale-path save as the user's new native backup.
        for name in before:
            if name != destination.name:
                shutil.copy2(rollback / name, destination.parent / name)
        if destination.name in before:
            for name, backup in destination_files(path).items():
                if name not in before and name != destination.name:
                    shutil.copy2(rollback / destination.name, backup)
    except Exception as error:
        restore_errors = []
        for name, current in destination_files(path).items():
            if name not in before:
                try:
                    current.unlink()
                except OSError as failure:
                    restore_errors.append(str(failure))
        for name in before:
            try:
                os.replace(rollback / name, destination.parent / name)
            except OSError as failure:
                restore_errors.append(str(failure))
        if not restore_errors:
            rollback.rmdir()
        recovery = dict(previous_path=previous_path, path=doc.FileName,
                        destination=str(destination), dirty=True,
                        destination_restored=not restore_errors,
                        history_preserved=history == (doc.UndoCount, doc.RedoCount),
                        backup_path=str(rollback) if restore_errors else None,
                        restore_errors=restore_errors)
        _recoveries[doc.Name] = recovery
        message = ('Save did not complete. Destination and native backup bytes were restored. '
                   if not restore_errors else 'Save did not complete and destination restoration failed. Retained recovery copies at ' + str(rollback) + '. ')
        raise SaveError(message + 'The live native root remains unsaved at ' + doc.FileName
                        + '. Its path may have changed. Inspect save_recovery, correct the error and retry Save or Save as. Cause: ' + str(error)) from error
    else:
        for copied in rollback.iterdir():
            copied.unlink()
        rollback.rmdir()
        _recoveries.pop(doc.Name, None)
