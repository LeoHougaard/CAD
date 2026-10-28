"""Read-only native FCStd dependencies. Native App::Link remains authoritative."""
import hashlib
import io
import json
import os
import re
from pathlib import Path
import xml.etree.ElementTree as ET
import zipfile
import FreeCAD as App

PROPERTY = 'CadExternalSource'
_dependencies = {}  # native name -> immutable file revision, never a writable root
_readonly_paths = set()  # survive scratch close and root lifecycle for this worker
_edge_recoveries = {}  # native targets captured before any destructive rebind


class SourceError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def reject(message, code='assembly_source'):
    raise SourceError(code, message)


def path_key(path):
    return os.path.normcase(str(Path(path).resolve()))


def same_file(first, second):
    if path_key(first) == path_key(second):
        return True
    try:
        return os.path.samefile(first, second)
    except OSError:
        return False


def protected_path(path):
    return any(same_file(path, source) for source in _readonly_paths)


def digest(path):
    try:
        return hashlib.sha256(Path(path).read_bytes()).hexdigest()
    except OSError:
        reject('Source file is missing or unreadable: ' + str(path), 'source_missing')


def descriptor(doc, obj, sha):
    return dict(version=1, path=str(Path(doc.FileName).resolve()), sha256=sha,
                document_uid=str(doc.Uid), object=obj.Name, object_id=obj.ID, type=obj.TypeId,
                dependencies=dependency_revisions(doc))


def dependency_revisions(doc):
    seen = {doc.Name}; pending = [doc]; result = []
    while pending:
        current = pending.pop()
        if current.Partial:
            reject('A partially loaded native source cannot establish complete dependency revisions', 'source_identity')
        for obj in current.Objects:
            for target in obj.OutList:
                other = target.Document
                if other.Name not in seen:
                    seen.add(other.Name); pending.append(other)
                    value = _dependencies.get(other.Name)
                    if value is None or not same_file(value['path'], other.FileName) or value['document_uid'] != str(other.Uid):
                        reject('External source depends on an unmanaged or writable root', 'source_ownership')
                    result.append(value.copy())
    return sorted(result, key=lambda v: v['path'])


def revision_list(value):
    if not isinstance(value, dict) or not isinstance(value.get('dependencies'), list):
        reject('Complete native dependency revisions are required. Load the source again.', 'source_identity')
    revisions = value['dependencies']
    if 'dependency_count' in value and (type(value['dependency_count']) is not int or value['dependency_count'] != len(revisions)):
        reject('Native dependency count does not match its complete revision list', 'source_identity')
    for index, entry in enumerate(revisions):
        if (not isinstance(entry, dict) or set(entry) != {'path', 'sha256', 'document_uid'}
                or any(not isinstance(entry.get(k), str) or not entry[k] for k in ('path', 'sha256', 'document_uid'))
                or not re.fullmatch('[0-9a-fA-F]{64}', entry['sha256'])):
            reject('Invalid native dependency revision', 'source_identity')
        if same_file(entry['path'], value['path']) or any(same_file(entry['path'], old['path']) for old in revisions[:index]):
            reject('Duplicate native dependency file identity', 'source_identity')
    return revisions


def check_files(value):
    for revision in [value, *revision_list(value)]:
        if digest(revision['path']) != revision['sha256']:
            reject('Source revision changed. Explicitly update/relink to a new revision path.', 'source_changed')


def check_native_revisions(value, doc):
    supplied = revision_list(value)
    loaded = _dependencies.get(doc.Name)
    if (loaded is None or loaded['document_uid'] != str(doc.Uid)
            or loaded['document_uid'] != value['document_uid'] or loaded['sha256'] != value['sha256']
            or not same_file(loaded['path'], doc.FileName) or not same_file(doc.FileName, value['path'])):
        reject('Loaded native source identity/revision differs from the accepted source', 'source_identity')
    expected = dependency_revisions(doc)
    if len(supplied) != len(expected) or any(not any(
            same_file(actual['path'], claimed['path']) and actual['sha256'] == claimed['sha256']
            and actual['document_uid'] == claimed['document_uid'] for claimed in supplied) for actual in expected):
        reject('Source descriptor does not contain the complete authoritative native dependency revisions', 'source_identity')
    # Check the native graph's revisions, never just the caller's subset. This
    # also detects a changed-then-reverted file behind an already loaded leaf.
    for revision in [loaded, *expected]:
        if digest(revision['path']) != revision['sha256']:
            reject('Native source dependency changed on disk', 'source_changed')


def same_revision(first, second):
    if not isinstance(first, dict) or not isinstance(second, dict):
        return False
    if any(first.get(k) != second.get(k) for k in ('version', 'document_uid', 'object', 'object_id', 'type', 'sha256')):
        return False
    if not same_file(first['path'], second['path']):
        return False
    before, after = revision_list(first), revision_list(second)
    return len(before) == len(after) and all(any(
        same_file(a['path'], b['path']) and a['sha256'] == b['sha256'] and a['document_uid'] == b['document_uid']
        for b in after) for a in before)


def status(link):
    value = stored(link)
    if value is None:
        return None
    if link.LinkedObject is None:
        return 'assembly_recovery_required' if link.Document.Name in _edge_recoveries else 'source_identity'
    try:
        if link.LinkedObject is not None:
            check_native_revisions(value, link.LinkedObject.Document)
        check_files(value)
        return 'current'
    except SourceError as error:
        return error.code


def supported(obj):
    return (obj.TypeId == 'PartDesign::Body' or obj.isDerivedFrom('Part::Feature')
            or (obj.TypeId == 'App::Link' and obj.LinkedObject is not None)) and hasattr(obj, 'Shape') and not obj.Shape.isNull()


def identity(value, obj):
    return (value.get('version') == 1 and value.get('document_uid') == str(obj.Document.Uid)
            and value.get('object') == obj.Name and value.get('object_id') == obj.ID
            and value.get('type') == obj.TypeId)


def stored(link):
    if PROPERTY not in link.PropertiesList:
        return None
    try:
        value = json.loads(getattr(link, PROPERTY))
        if value['version'] != 1:
            raise ValueError('version')
        return value
    except (ValueError, KeyError, TypeError):
        reject(link.Name + ': invalid external source identity')


def put(link, value):
    if PROPERTY not in link.PropertiesList:
        link.addProperty('App::PropertyString', PROPERTY, 'External source', 'Accepted native source identity and file revision')
    # Only this dedicated native operation writes this property. Generic native
    # property forms keep its readonly status, including after save and reopen.
    link.setPropertyStatus(PROPERTY, '-ReadOnly')
    setattr(link, PROPERTY, json.dumps(value, sort_keys=True))
    link.setPropertyStatus(PROPERTY, 'ReadOnly')


def close_native(name):
    # The worker binds its existing guarded Close/observer lifecycle here.
    # Standalone native controls retain a direct native implementation.
    App.closeDocument(name)


def restore_active(name):
    App.setActiveDocument(name if name in App.listDocuments() else '')
    App.ActiveDocument = App.activeDocument()


def guard_existing(doc):
    # Bound to the worker's authoritative native dependency/quarantine guard.
    pass


def cleanup(before):
    errors = []
    for name in sorted(set(App.listDocuments()) - before):
        try:
            close_native(name)
            if name in App.listDocuments():
                raise RuntimeError('Native Close left the source open')
            _dependencies.pop(name, None)
        except Exception as error:
            errors.append(dict(document=name, code=getattr(error, 'code', 'native_error'), message=str(error)))
    if errors:
        reject('Source cleanup: ' + json.dumps(errors, sort_keys=True), 'source_cleanup')


def cleanup_failure(before, error):
    try:
        cleanup(before)
    except Exception as cleanup_error:
        raise SourceError(getattr(error, 'code', 'native_error'), str(error) + '\n' + str(cleanup_error)) from error


def source_files(path):
    """Capture native XLink files before loading; never derive trust from metadata."""
    pending, files = [str(Path(path).resolve())], {}
    while pending:
        path = pending.pop()
        if any(same_file(path, old) for old in files):
            continue
        data = digest(path)
        with zipfile.ZipFile(path) as archive:
            xml = ET.fromstring(archive.read('Document.xml'))
        files[path] = data
        for edge in xml.findall('.//XLink'):
            linked = edge.get('file')
            if linked and edge.get('name'):
                target = Path(linked)
                pending.append(str((target if target.is_absolute() else Path(path).parent / target).resolve()))
    return files


def native_content(obj):
    # Native dumpContent embeds current ZIP timestamps. Compare the original
    # member names/order and exact XML/BRep bytes, not those transport headers.
    with zipfile.ZipFile(io.BytesIO(obj.dumpContent())) as archive:
        return tuple((entry.filename, archive.read(entry)) for entry in archive.infolist())


def native_owners():
    # Retained dependencies also own references in native Undo/Redo. They are
    # never released while any root is open, so every historical source owner
    # is in this pre-request set, not in the initial-load completion exception.
    return {name: (doc, str(doc.Uid), doc.FileName, doc.UndoCount, doc.RedoCount,
                   [(obj, obj.Name, obj.ID, native_content(obj)) for obj in doc.Objects],
                   digest(doc.FileName) if doc.FileName and Path(doc.FileName).is_file() else None)
            for name, doc in App.listDocuments().items()}


def check_owners(owners):
    for name, (doc, uid, path, undo, redo, objects, file_hash) in owners.items():
        if (name not in App.listDocuments() or App.getDocument(name) != doc
                or str(doc.Uid) != uid or doc.FileName != path
                or (doc.UndoCount, doc.RedoCount) != (undo, redo)
                or len(doc.Objects) != len(objects)):
            reject('Initial source loading changed a pre-existing native owner', 'source_ownership')
        if (digest(path) if path and Path(path).is_file() else None) != file_hash:
            reject('A pre-existing native owner file changed during source loading', 'source_changed')
        for obj, object_name, native_id, content in objects:
            if (doc.getObject(object_name) != obj or obj.ID != native_id or native_content(obj) != content
                    or any(target.Document.Name not in owners for target in obj.OutList)):
                reject('A pre-existing native object or history owns the new load set', 'source_ownership')


def complete_new(owners, files, is_root):
    completed = set()
    while True:
        check_owners(owners)
        new = [doc for name, doc in App.listDocuments().items() if name not in owners]
        for doc in new:
            if (not doc.FileName or not any(same_file(doc.FileName, path) for path in files)
                    or any(old.FileName and same_file(old.FileName, doc.FileName) for old, *_ in owners.values())
                    or is_root(doc.Name) or doc.Name in _dependencies):
                reject('Native completion is restricted to this request\'s new unowned source files', 'source_ownership')
        partial = next((doc for doc in new if doc.Partial), None)
        if partial is None:
            break
        path = partial.FileName
        if path_key(path) in completed:
            reject('Pinned native initial loading did not produce a stable full source graph', 'source_identity')
        completed.add(path_key(path))
        uid = str(partial.Uid)
        identities = [(obj.Name, obj.ID, obj.TypeId) for obj in partial.Objects]
        # Pinned main-file Open recreates this NEW partial document and repairs
        # its new graph's native links. No previously admitted handle is touched.
        full = App.openDocument(path, True)
        if str(full.Uid) != uid or any(
                full.getObject(name) is None or (full.getObject(name).ID, full.getObject(name).TypeId) != (native_id, type_id)
                for name, native_id, type_id in identities):
            reject('Native initial completion changed persistent source identity', 'source_identity')
    if any(digest(path) != sha for path, sha in files.items()):
        reject('A source file changed during initial native completion', 'source_changed')


def claim(before, is_root, validate_proxies):
    """Validate every autoload, including nested sources, before retaining any."""
    pending = {}
    roots = [d for name, d in App.listDocuments().items() if is_root(name) and d.FileName]
    # Check physical identity for every autoload before validating or claiming
    # any of them. A hardlink must not revoke another root's write ownership.
    for name, doc in App.listDocuments().items():
        if not is_root(name) and doc.FileName and any(same_file(doc.FileName, root.FileName) for root in roots):
            reject('A writable root cannot also be a readonly external source, including file aliases', 'source_ownership')
    for name, doc in App.listDocuments().items():
        if is_root(name):
            continue
        if name in before and name not in _dependencies:
            reject('Unmanaged native document cannot become a source: ' + name)
        if name in before:
            continue
        if not doc.FileName:
            reject('Every external dependency must be a saved FCStd document')
        if doc.Partial:
            reject('Native source is only partially restored: ' + doc.FileName, 'source_identity')
        validate_proxies(doc)
        for obj in doc.Objects:
            if not obj.isValid() or (hasattr(obj, 'Shape') and not obj.Shape.isNull() and not obj.Shape.isValid()):
                reject('Invalid native source object: ' + obj.FullName)
        sha = digest(doc.FileName)
        pending[name] = dict(path=str(Path(doc.FileName).resolve()), sha256=sha, document_uid=str(doc.Uid))
    _dependencies.update(pending)
    _readonly_paths.update(path_key(value['path']) for value in pending.values())


def guard_prior_owners(is_root):
    documents = App.listDocuments()
    if any(not is_root(name) and name not in _dependencies for name in documents):
        reject('Unmanaged native history cannot participate in initial source completion', 'source_ownership')
    if any(doc.Partial for doc in documents.values()):
        reject('Pre-existing partial native documents cannot be recreated during source loading', 'source_identity')


def load(path, is_root, validate_proxies):
    path = str(Path(path).expanduser().resolve())
    if Path(path).suffix.lower() != '.fcstd':
        reject('Choose a native .FCStd source file')
    sha = digest(path)
    existing = next((d for d in App.listDocuments().values() if d.FileName and same_file(d.FileName, path)), None)
    if existing is not None:
        guard_existing(existing)
        if is_root(existing.Name) or existing.Name not in _dependencies:
            reject('A writable root cannot also be a readonly external source', 'source_ownership')
        if _dependencies[existing.Name]['sha256'] != sha:
            reject('Loaded source changed on disk. Close dependent roots before reopening, or relink to a new revision path.', 'source_changed')
        if existing.Partial:
            reject('An accepted native source cannot be reopened to replace a partial document', 'source_identity')
        return existing
    before = set(App.listDocuments())
    readonly_before = set(_readonly_paths)
    active = App.ActiveDocument.Name if App.ActiveDocument else ''
    guard_prior_owners(is_root)
    owners = native_owners()
    files = source_files(path)
    try:
        doc = App.openDocument(path, True)
        complete_new(owners, files, is_root)
        claim(before, is_root, validate_proxies)
        if digest(path) != sha:
            reject('Source changed while opening', 'source_changed')
        return doc
    except Exception as error:
        try:
            cleanup_failure(before, error)
        finally:
            _readonly_paths.clear()
            _readonly_paths.update(readonly_before)
        check_owners(owners)
        raise
    finally:
        restore_active(active)


def browse(path, is_root, validate_proxies):
    before = set(App.listDocuments())
    readonly_before = set(_readonly_paths)
    try:
        doc = load(path, is_root, validate_proxies)
        sha = _dependencies[doc.Name]['sha256']
        choices = [dict(name=o.Name, label=o.Label, type=o.TypeId, source=descriptor(doc, o, sha))
                   for o in doc.Objects if supported(o)]
        if not choices:
            reject('Source has no supported native Part, Body or Link with a shape')
        result = dict(path=str(Path(doc.FileName).resolve()), sha256=sha, objects=choices)
    except Exception as error:
        _readonly_paths.clear()
        _readonly_paths.update(readonly_before)
        cleanup_failure(before, error)
        raise
    else:
        # Browsing returns immutable identities, never a retained scratch doc.
        cleanup(before)
        return result


def resolve(value):
    if not isinstance(value, dict) or value.get('version') != 1:
        reject('Browse and select an external source before Apply')
    check_files(value)
    candidates = [d for n, d in App.listDocuments().items() if n in _dependencies
                  and path_key(d.FileName) == path_key(value['path'])]
    if len(candidates) != 1:
        reject('External source was not prepared in the guarded loading phase')
    doc = candidates[0]
    check_native_revisions(value, doc)
    if _dependencies[doc.Name]['sha256'] != value['sha256']:
        reject('Loaded source revision differs from the selected source', 'source_changed')
    obj = doc.getObject(value.get('object', ''))
    if obj is None or not identity(value, obj) or not supported(obj):
        reject('Source native UUID/object identity changed. Select the intended source explicitly.', 'source_identity')
    return obj


def prepare(edits, is_root, validate_proxies):
    for edit in edits:
        value = edit.get('external_source')
        if value is not None:
            revision_list(value)
            load(value.get('path', ''), is_root, validate_proxies)
            resolve(value)


def check(doc, files=True):
    if doc.Name in _edge_recoveries:
        reject('Native source edges need recovery. Retry Save or Update / relink external source.', 'assembly_recovery_required')
    for obj in doc.Objects:
        if obj.TypeId != 'App::Link':
            continue
        value = stored(obj)
        target = obj.LinkedObject
        if value is None:
            if target is not None and target.Document != doc:
                reject(obj.Name + ': external link has no registered accepted source identity', 'source_identity')
            continue
        if target is None or target.Document == doc or not identity(value, target):
            reject(obj.Name + ': external source identity does not match the native link', 'source_identity')
        if path_key(target.Document.FileName) != path_key(value['path']):
            # Native SaveAs may make serialized relative paths resolve elsewhere.
            reject(obj.Name + ': external source path differs from its accepted identity', 'source_identity')
        if files:
            check_native_revisions(value, target.Document)
            check_files(value)


def release(is_root, final=False):
    # Undo/Redo owns native links not visible in the current graph. Retain managed
    # dependencies while any root is open; final root close releases all of them.
    if not any(is_root(n) for n in App.listDocuments()):
        for name in list(_dependencies):
            if name in App.listDocuments():
                close_native(name)
            _dependencies.pop(name, None)


def close_root(key):
    _edge_recoveries.pop(key, None)


def recovery_metadata(doc):
    pending = _edge_recoveries.get(doc.Name)
    if pending is None:
        return None
    return {'pending_edges': [{'name': edge['name'], 'native_id': edge['native_id'],
                               'source': edge['source']} for edge in pending['edges']],
            'errors': list(pending['errors'])}


def recovery_guard(doc, request):
    if doc.Name not in _edge_recoveries:
        return
    op = request.get('op')
    if op in ('save', 'close', 'inspect', 'assembly_sources'):
        return
    edits = request.get('edits')
    if op == 'transaction' and isinstance(edits, list) and edits and all(e.get('op') == 'assembly_relink' for e in edits):
        return
    reject('Native link restoration is pending. Retry Save or Update / relink external source after correcting the native error; content and history remain owned by this root.', 'assembly_recovery_required')


def restore_edges(doc):
    pending = _edge_recoveries.get(doc.Name)
    if pending is None:
        return True
    remaining, errors = [], []
    for edge in pending['edges']:
        link, target = edge['link'], edge['target']
        try:
            if (link.Document.Name != doc.Name or link.ID != edge['native_id']
                    or stored(link) != edge['source'] or not identity(edge['source'], target)
                    or not same_file(target.Document.FileName, edge['source']['path'])):
                reject('Captured native edge/target identity changed during recovery', 'source_identity')
            # Use the captured immutable native target, never a name/topology
            # guess. Keep already restored edges untouched and try every edge.
            try:
                if link.LinkedObject != target:
                    link.setLink(target)
            finally:
                link.LinkPlacement = edge['placement']
            if link.LinkedObject != target or link.LinkPlacement != edge['placement']:
                raise RuntimeError('Native edge or placement did not restore')
        except Exception as error:
            remaining.append(edge)
            errors.append(edge['name'] + ': ' + str(error))
    if remaining:
        _edge_recoveries[doc.Name] = {'edges': remaining, 'errors': errors}
        return False
    _edge_recoveries.pop(doc.Name, None)
    return True


def recover_edges(doc):
    if not restore_edges(doc):
        reject('Native link restoration remains pending: ' + '; '.join(_edge_recoveries[doc.Name]['errors'])
               + '. Retry Save or Update / relink external source after the native error clears.', 'assembly_recovery_required')


def rebase_after_saveas(doc, after_clear=lambda: None):
    # Pinned PropertyXLink::Save retains its restored relative filePath. After
    # native saveAs establishes the destination, recreate only that native edge
    # to the exact same live target so native serialization rebases it. No source
    # object, instance, placement, connector or geometry is replaced.
    history = (doc.UndoCount, doc.RedoCount)
    recover_edges(doc)
    edges = []
    for link in doc.Objects:
        if link.TypeId == 'App::Link' and stored(link) is not None:
            if link.LinkedObject is None:
                reject('Cannot capture an unresolved native source edge', 'source_identity')
            edges.append({'name': link.Name, 'native_id': link.ID, 'link': link,
                          'target': link.LinkedObject, 'source': stored(link),
                          'placement': App.Placement(link.LinkPlacement)})
    _edge_recoveries[doc.Name] = {'edges': edges, 'errors': []}
    try:
        for edge in edges:
            edge['link'].setLink(None)
            after_clear()
        if not restore_edges(doc):
            reject('Native edge restoration failed: ' + '; '.join(_edge_recoveries[doc.Name]['errors']), 'assembly_recovery_required')
    except Exception:
        # A restoring setter can fail too. Keep all original targets before
        # clearing anything and recover every affected edge, retaining exact
        # pending identities when persistent native failure prevents restoration.
        restore_edges(doc)
        raise
    if history != (doc.UndoCount, doc.RedoCount):
        reject('Native source-path rebasing changed root history', 'source_save_state')
    check(doc)


def validate_saved_paths(path):
    with zipfile.ZipFile(path) as archive:
        xml = ET.fromstring(archive.read('Document.xml'))
    for obj in xml.findall('./ObjectData/Object'):
        prop = obj.find('./Properties/Property[@name="' + PROPERTY + '"]/String')
        if prop is None:
            continue
        value = json.loads(prop.attrib['value'])
        edge = obj.find('./Properties/Property[@name="LinkedObject"]/XLink')
        if edge is None or edge.get('name') != value['object']:
            reject('Saved native XLink identity differs from its accepted source', 'source_save_state')
        native_path = Path(edge.get('file', ''))
        if not native_path.is_absolute():
            native_path = Path(path).parent / native_path
        if path_key(native_path) != path_key(value['path']):
            reject('Saved native XLink source path was not rebased', 'source_save_state')


def serialized_preflight(path):
    """Fail closed before native autoload if an accepted dependency is stale."""
    with zipfile.ZipFile(path) as archive:
        xml = ET.fromstring(archive.read('Document.xml'))
    sources = []
    for prop in xml.findall('.//Property[@name="' + PROPERTY + '"]'):
        node = prop.find('String')
        if node is None:
            reject('Saved external source identity is malformed', 'source_identity')
        value = json.loads(node.attrib['value'])
        for revision in [value, *revision_list(value)]:
            try:
                current = digest(revision['path'])
            except SourceError:
                current = None
            if current != revision['sha256']:
                reject('Restore the accepted source revision at ' + revision['path']
                       + ' before Open. Expected SHA256 ' + revision['sha256']
                       + ', native document UUID ' + revision['document_uid']
                       + '. Then use the root Update / relink external source task to choose a new revision path.',
                       'source_missing' if current is None else 'source_changed')
        sources.append(value)
    return sources


def prepare_open(path, is_root, validate_proxies):
    # Root autoload can reach prior documents independently of source metadata,
    # including when the list is empty or every descriptor reuses a full source.
    guard_prior_owners(is_root)
    # Native root autoload may restore only the selected source object. Open
    # each top source as a main document in this guarded, transaction-free phase
    # first. Its full native graph, not the saved caller list, remains authority.
    sources = serialized_preflight(path)
    prepare([{'external_source': value} for value in sources], is_root, validate_proxies)
    if protected_path(path):
        reject('An assembly root cannot also be its own readonly dependency', 'source_ownership')
