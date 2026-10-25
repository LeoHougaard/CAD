"""Validate the static workflow ledger without native tools or third-party packages.

This checks audit integrity, not application behavior or semantic parity.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys


STAGES = {
    'start', 'inputs', 'preview', 'apply', 'cancel', 'edit', 'parent_update',
    'history', 'save_reopen', 'failure', 'availability',
}
EVIDENCE_CLAIMS = {
    'static-audit', 'recorded-bounded', 'reviewed-bounded-handover',
}
FEATURE_PATTERN = re.compile(
    r'feature!\(\s*"([^"]+)",\s*"([^"]+)",\s*"([^"]+)",'
    r'\s*"([^"]+)",\s*\[(.*?)\]\s*\)', re.S,
)


def public_path(path: str) -> bool:
    return (isinstance(path,str) and path.startswith('next/')
            and not any(c in path for c in ('\\', ':', '\x00', '\n'))
            and '..' not in PurePosixPath(path).parts and PurePosixPath(path).as_posix()==path)


class PinnedSources:
    """Read historical bytes from local Git or an explicitly verified root.

    No network requests, worktree fallback or private artifact paths are used.
    The public manifest authenticates the exact source files used by the audit.
    """
    def __init__(self, root: Path, metadata: dict, pinned_root: Path | None = None):
        self.root=Path(root).resolve()
        self.commit=metadata.get('source_commit','')
        if not re.fullmatch('[0-9a-f]{40}',self.commit):
            raise ValueError('pinned source commit must be exact')
        manifest_path=metadata.get('pinned_source_manifest')
        if manifest_path!='next/coverage/workflow-sources.json':
            raise ValueError('missing public pinned source manifest')
        manifest=json.loads((self.root/manifest_path).read_bytes())
        if manifest.get('schema_version')!=1 or manifest.get('source_commit')!=self.commit:
            raise ValueError('pinned source manifest commit/schema mismatch')
        entries=manifest.get('files',[])
        self.hashes={}
        for entry in entries:
            path=entry.get('path')
            if not public_path(path) or path in self.hashes or not re.fullmatch('[0-9a-f]{64}',entry.get('sha256','')):
                raise ValueError('invalid or duplicate pinned source manifest path/hash')
            self.hashes[path]=entry['sha256']
        if not self.hashes:
            raise ValueError('empty pinned source manifest')
        self.pinned_root=Path(pinned_root).resolve() if pinned_root is not None else None
        self.cache={}
        self.kind='verified root' if self.pinned_root is not None else 'local Git'
        expected=sorted(p for p in self.hashes if p.startswith('next/packages/') and p.endswith('.json'))
        if self.pinned_root is None:
            try:
                resolved=self.git('rev-parse','--verify',self.commit+'^{commit}').decode().strip()
                if resolved!=self.commit:
                    raise ValueError('pinned commit did not resolve exactly')
            except (OSError,ValueError) as error:
                raise ValueError('Pinned Git commit is unavailable. Supply --pinned-root with source matching the public manifest. '+str(error)) from error
            package_paths=self.git('ls-tree','--full-tree','-r','--name-only',self.commit,'--','next/packages').decode('utf-8').splitlines()
            if sorted(p for p in package_paths if p.endswith('.json'))!=expected:
                raise ValueError('pinned package file set differs from the public manifest')
        elif sorted(p.relative_to(self.pinned_root).as_posix() for p in (self.pinned_root/'next/packages').glob('*.json'))!=expected:
            raise ValueError('verified-root package file set differs from the public manifest')
        # Verify all declared bytes once, including fields not reached after a
        # failed structural assertion. Later reads use this immutable cache.
        for path in self.hashes:
            self.read_bytes(path)

    def git(self,*args: str) -> bytes:
        result=subprocess.run(['git','-C',str(self.root),*args],capture_output=True,
            env=dict(os.environ,GIT_NO_REPLACE_OBJECTS='1',GIT_OPTIONAL_LOCKS='0'))
        if result.returncode:
            raise ValueError(result.stderr.decode('utf-8',errors='replace').strip())
        return result.stdout

    def read_bytes(self,path: str) -> bytes:
        if not public_path(path) or path not in self.hashes:
            raise ValueError('historical source is absent from the verified manifest: '+str(path))
        if path not in self.cache:
            if self.pinned_root is None:
                data=self.git('show',self.commit+':'+path)
            else:
                target=(self.pinned_root/path).resolve()
                if not target.is_relative_to(self.pinned_root):
                    raise ValueError('pinned source escapes the supplied root: '+path)
                data=target.read_bytes()
            if hashlib.sha256(data).hexdigest()!=self.hashes[path]:
                raise ValueError('pinned source hash mismatch: '+path)
            self.cache[path]=data
        return self.cache[path]

    def read_text(self,path: str) -> str:
        return self.read_bytes(path).decode('utf-8')


def native_catalog(text: str) -> dict:
    result={}
    for id,label,type_id,group,fields in FEATURE_PATTERN.findall(text):
        if id in result:
            raise ValueError('duplicate native stable ID: '+id)
        result[id]=dict(type=type_id,label=label,group=group,
                        declared_fields=re.findall(r'"([^"]+)"',fields))
    return result


def package_catalog(items) -> dict:
    result={}
    for item in items:
        if item['id'] in result:
            raise ValueError('duplicate package stable ID: '+item['id'])
        result[item['id']]=item
    return result


def current_catalogs(root: Path) -> tuple[dict,dict]:
    native=native_catalog((root/'next/src/feature_commands.rs').read_text(encoding='utf-8'))
    packages=package_catalog(json.loads(p.read_bytes()) for p in sorted((root/'next/packages').glob('*.json')))
    return native,packages


def validate_current(ledger: dict, native: dict, packages: dict) -> list[str]:
    """Strict live compatibility, independent of historical source anchors."""
    errors=[]
    tasks={t['id']:t for t in ledger['native_tasks']}
    baseline_packages={p['id']:p for p in ledger['package_tasks']}
    overlay=ledger.get('current_compatibility',{})
    if overlay.get('source_commit')!=ledger['metadata']['source_commit'] or overlay.get('acceptance_update') is not False:
        errors.append('current compatibility cannot change historical provenance or acceptance')
    updates=overlay.get('presentation_updates',{})
    if not isinstance(updates,dict):
        return errors+['invalid current presentation updates']
    if not set(updates)<=set(tasks):
        errors.append('current presentation update contains an unknown stable ID')
    if set(native)!=set(tasks):
        errors.append('current native stable ID set lost/added; deliberate catalog reaudit required')
    for id,baseline in tasks.items():
        if id not in native:
            continue
        for key in ('type','group','declared_fields'):
            if native[id].get(key)!=baseline[key]:
                errors.append(f'{id}: current native {key} lost/changed')
        expected=baseline['label']
        if id in updates:
            update=updates[id]
            if (not isinstance(update,dict) or set(update)!={'baseline','current'}
                    or update.get('baseline')!=baseline['label'] or not isinstance(update.get('current'),str) or not update['current']):
                errors.append(f'{id}: presentation update rewrites historical label or is invalid')
            else:
                expected=update['current']
        if native[id].get('label')!=expected:
            errors.append(f'{id}: current label changed without exact presentation update')
    if set(packages)!=set(baseline_packages):
        errors.append('current package stable ID set lost/added')
    for id,p in baseline_packages.items():
        if id in packages and packages[id].get('parameters')!=p['parameters']:
            errors.append(f'{id}: current package parameter declaration lost/changed')
    return errors


def validate(ledger: dict, inventory: dict, root: Path, guide: str,
             inventory_bytes: bytes | None = None, *, pinned: PinnedSources | None = None,
             pinned_root: Path | None = None, check_current: bool = True) -> list[str]:
    errors: list[str] = []

    def check(ok, message):
        if not ok:
            errors.append(message)

    def entries(name):
        value = ledger.get(name)
        check(isinstance(value, list) and bool(value), f'{name}: missing nonempty list')
        return value if isinstance(value, list) else []

    def unique(items, label):
        ids = [x.get('id') for x in items]
        check(all(isinstance(x, str) and x for x in ids), f'{label}: missing ID')
        duplicates = [x for x,n in Counter(ids).items() if n > 1]
        check(not duplicates, f'{label}: duplicate IDs {duplicates}')
        return {x.get('id'):x for x in items}

    def strings(value):
        return isinstance(value, list) and bool(value) and all(isinstance(x,str) and x.strip() for x in value)

    def sources(value, label):
        if isinstance(value, dict):
            if {'path','line','anchor','url'} <= value.keys():
                path = value['path']
                check(isinstance(path,str) and path.startswith('next/') and '..' not in Path(path).parts,
                      f'{label}: source must be relative public next path')
                if not isinstance(path,str) or not path.startswith('next/') or '..' in Path(path).parts:
                    return
                check(isinstance(value['line'],int) and value['line'] > 0, f'{label}: invalid source line')
                try:
                    lines=pinned.read_text(path).splitlines()
                    line=value['line']
                    check(isinstance(line,int) and 0<line<=len(lines) and value['anchor'] in lines[line-1],
                          f'{label}: historical source anchor missing at pinned line {path}: {value["anchor"]}')
                except (OSError,ValueError) as error:
                    check(False,f'{label}: historical source unavailable: {error}')
                expected = f'https://github.com/LeoHougaard/CAD/blob/{ledger.get("metadata",{}).get("source_commit")}/{path}#L{value["line"]}'
                check(value['url']==expected, f'{label}: source URL/commit/line mismatch')
            for k,v in value.items():
                sources(v,f'{label}.{k}')
        elif isinstance(value,list):
            for i,v in enumerate(value):
                sources(v,f'{label}[{i}]')

    def no_completion(value, label):
        if isinstance(value,dict):
            for key,item in value.items():
                check('percentage' not in key.lower() and key not in ('completion_percent','completed_count'),
                      f'{label}: completion percentage/count is unsupported')
                if key=='full_documented_workflow_accepted':
                    check(item is False, f'{label}: unsupported completed claim')
                if key in ('status','acceptance') and isinstance(item,str):
                    check(not re.fullmatch(r'(complete[d]?|accepted|passed|verified)',item,re.I),
                          f'{label}: unsupported completed claim')
                no_completion(item,f'{label}.{key}')
        elif isinstance(value,list):
            for i,item in enumerate(value):
                no_completion(item,f'{label}[{i}]')

    check(ledger.get('schema_version')==1, 'unsupported workflow schema')
    check(not re.search(r'(?m)^\|[^\n]+\|\n\n\|',guide), 'guide: blank row breaks a Markdown table')
    metadata = ledger.get('metadata',{})
    check(bool(re.fullmatch('[0-9a-f]{40}', metadata.get('source_commit',''))), 'source commit must be exact')
    check(metadata.get('original_inventory_commit')==inventory.get('metadata',{}).get('audited_commit'),
          'original inventory commit changed')
    if inventory_bytes is not None:
        check(metadata.get('inventory_sha256')==hashlib.sha256(inventory_bytes).hexdigest(),
              'original inventory hash changed; deliberate audit update required')
    check(ledger.get('acceptance_gates')==inventory.get('acceptance_gates'), 'original acceptance gates changed/lost')
    routes = entries('routes')
    route_map = unique(routes,'routes')
    paths = [r.get('path','').strip().casefold() for r in routes]
    check(all(paths), 'routes: missing visible path')
    check(len(paths)==len(set(paths)), 'routes: duplicate visible route paths')
    for r in routes:
        if len(r.get('native_types',[]))>1:
            check(bool(r.get('ambiguity')), f'{r.get("id")}: ambiguous catalog labels need an explicit limitation')

    evidence = unique(entries('evidence'),'evidence')
    for e in evidence.values():
        check(e.get('claim') in EVIDENCE_CLAIMS, f'{e.get("id")}: unsupported evidence/completion claim')
        check(e.get('kind') in ('implementation','headless','native','desktop'),f'{e.get("id")}: invalid evidence kind')
        check(bool(e.get('scope')), f'{e.get("id")}: missing bounded evidence scope')

    families = unique(entries('families'),'families')
    for f in families.values():
        id=f.get('id')
        for key in ('prerequisites','selection','inputs'):
            check(strings(f.get(key)), f'{id}: missing {key}')
        steps=f.get('steps',[])
        step_ids=[s.get('id') for s in steps]
        check(set(step_ids)==STAGES and len(step_ids)==len(STAGES), f'{id}: missing/duplicate lifecycle steps')
        check(all(isinstance(s.get('instruction'),str) and s['instruction'].strip() for s in steps),f'{id}: empty step')
        check(bool(f.get('missing_operation')),f'{id}: missing explicit remaining operation')
        check(all(r in route_map for r in f.get('route_ids',[])),f'{id}: unknown route')
        check(len(f.get('route_ids',[]))==len(set(f.get('route_ids',[]))),f'{id}: duplicate route binding')
        check((f.get('access')=='missing') == (not f.get('route_ids')), f'{id}: missing/access route mismatch')
        a=f.get('acceptance',{})
        check(a.get('status')=='open' and a.get('full_documented_workflow_accepted') is False,
              f'{id}: missing acceptance/open status or unsupported completed claim')
        check(set(a.get('required_gates',[]))==set(inventory['acceptance_gates']),f'{id}: missing acceptance gates')
        check(bool(a.get('next_check')), f'{id}: missing acceptance action')
        check(bool(f.get('evidence_ids')) and all(x in evidence for x in f['evidence_ids']),f'{id}: missing/unknown evidence')
        check(f'### {id}:' in guide, f'{id}: guide family missing')
        check(f'id="wf-{id}"' in guide, f'{id}: guide family target missing')

    topics=entries('topics')
    unique(topics,'topics')
    original=inventory['topics']
    check([t.get('id') for t in topics]==[t['id'] for t in original], 'lost/reordered/added topic IDs')
    check(metadata.get('topic_count')==len(original), 'topic inventory count mismatch')
    original_map={t['id']:t for t in original}
    all_req=[]
    for t in topics:
        id=t.get('id')
        o=original_map.get(id)
        if o is None:
            continue
        f=families.get(t.get('family_id'))
        check(f is not None,f'{id}: unknown family')
        if f is None:
            continue
        for key in ('title','area','official_url','required_gates','cloud_specific_dependency','cloud_note'):
            check(t.get(key)==o.get(key),f'{id}: original {key} lost/changed')
        check(isinstance(t.get('route_ids'),list) and all(r in f.get('route_ids',[]) for r in t['route_ids']), f'{id}: topic/family route mismatch')
        check(bool(t.get('route_ids'))==bool(f.get('route_ids')), f'{id}: topic route missing/access mismatch')
        check(len(t.get('route_ids',[]))==len(set(t.get('route_ids',[]))), f'{id}: duplicate topic route')
        check(t.get('access')==f.get('access'), f'{id}: topic/family access mismatch')
        check(bool(t.get('missing_operation')),f'{id}: explicit gap missing')
        check(t.get('acceptance')=='open' and t.get('full_documented_workflow_accepted') is False,
              f'{id}: missing acceptance or unsupported completed claim')
        check(f'[{id} {t.get("title")}]' in guide, f'{id}: guide topic missing')
        req=t.get('requirements',[])
        check([r.get('id') for r in req]==[r['id'] for r in o['option_checklist']],f'{id}: lost/reordered/added requirement IDs')
        om={r['id']:r for r in o['option_checklist']}
        for r in req:
            rid=r.get('id')
            all_req.append(rid)
            rr=om.get(rid)
            if rr is None:
                continue
            check(r.get('requirement')==rr['requirement'],f'{rid}: original requirement text lost/changed')
            check(r.get('acceptance')==rr['acceptance'],f'{rid}: original acceptance lost/changed')
            check(r.get('full_documented_workflow_accepted') is False,f'{rid}: unsupported completed claim')
            check(r.get('family_id')==t.get('family_id'),f'{rid}: family mismatch')
            check(r.get('candidate_route_ids')==t.get('route_ids'),f'{rid}: route mismatch')
            check(bool(r.get('missing_operation')),f'{rid}: missing operation absent')
            expected='candidate-family-only' if f.get('route_ids') else 'missing-operation'
            check(r.get('binding')==expected,f'{rid}: unsupported route satisfaction claim')
    check(len(all_req)==len(set(all_req)), 'duplicate requirement IDs')
    check(metadata.get('requirement_count')==sum(len(t['option_checklist']) for t in original), 'requirement inventory count mismatch')

    try:
        pinned=pinned or PinnedSources(root,metadata,pinned_root)
        if pinned.commit!=metadata.get('source_commit'):
            raise ValueError('resolver commit differs from ledger source_commit')
        if inventory_bytes is not None:
            check(inventory_bytes==pinned.read_bytes('next/coverage/onshape.json'),'original inventory differs from pinned source bytes')
        declared=native_catalog(pinned.read_text('next/src/feature_commands.rs'))
    except (OSError,ValueError) as error:
        return errors+['historical source resolution failed: '+str(error)]
    tasks=unique(entries('native_tasks'),'native tasks')
    check(set(tasks)==set(declared),'native task lost/added; deliberate catalog reaudit required')
    for id,t in tasks.items():
        d=declared.get(id)
        if d:
            for k in ('type','label','group','declared_fields'):
                check(t.get(k)==d[k],f'{id}: declared native {k} lost/changed')
        r=route_map.get(t.get('route_id'),{})
        check(t.get('type') in r.get('native_types',[]),f'{id}: native route identity mismatch')
        check(t.get('acceptance')=='open',f'{id}: unsupported native task acceptance')
    packages=unique(entries('package_tasks'),'packages')
    expected_packages=package_catalog(json.loads(pinned.read_bytes(p)) for p in sorted(pinned.hashes)
        if p.startswith('next/packages/') and p.endswith('.json'))
    check(set(packages)==set(expected_packages),'package input inventory lost/added')
    for id,p in packages.items():
        if id in expected_packages:
            check(p.get('parameters')==expected_packages[id]['parameters'],f'{id}: package parameter declaration lost/changed')
        check(p.get('acceptance')=='open',f'{id}: unsupported package acceptance')
    if check_current:
        try:
            native,live_packages=current_catalogs(root)
            errors.extend(validate_current(ledger,native,live_packages))
        except (OSError,ValueError,KeyError) as error:
            errors.append('current catalog unavailable: '+str(error))
    for q in entries('task_queue'):
        check(q.get('family_id') in families,f'{q.get("id")}: unknown queue family')
        check(all(q.get(k) for k in ('priority','owner','next_action','interface_seam','source_references')),
              f'{q.get("id")}: incomplete task queue item')
    unique(ledger.get('task_queue',[]),'queue')
    queue_map={q['id']:q for q in ledger.get('task_queue',[])}
    candidates=ledger.get('in_progress_candidates',[])
    check(isinstance(candidates,list),'candidate list invalid')
    if isinstance(candidates,list):
        unique(candidates,'candidates')
        for c in candidates:
            check(c.get('family_id') in families and c.get('queue_id') in queue_map,f'{c.get("id")}: unresolved candidate/queue seam')
            check(c.get('status')=='in progress; unreviewed' and c.get('full_documented_workflow_accepted') is False,
                  f'{c.get("id")}: candidate promoted without review')
            check(strings(c.get('pending_checks')),f'{c.get("id")}: missing candidate checks')
            check(bool(c.get('reported_by')) and bool(c.get('route_note')),f'{c.get("id")}: candidate provenance/baseline distinction missing')
    policy=ledger.get('performance_policy',{})
    check(bool(policy.get('hillclimbing')) and 'after verified functional parity' in policy['hillclimbing'].lower(),
          'performance hillclimbing must wait for verified functional parity')
    check(strings(policy.get('correctness_gates')) and strings(policy.get('benchmark_planning')),
          'correctness/benchmark planning gates missing')
    sources(ledger,'ledger')
    no_completion(ledger,'ledger')
    return errors


def main() -> int:
    parser=argparse.ArgumentParser(description=__doc__)
    root=Path(__file__).resolve().parents[2]
    parser.add_argument('--root',type=Path,default=root,help='repository or archived source root')
    parser.add_argument('--ledger',type=Path)
    parser.add_argument('--inventory',type=Path)
    parser.add_argument('--guide',type=Path)
    parser.add_argument('--pinned-root',type=Path,help='explicit historical source root; every public manifest hash is verified')
    parser.add_argument('--historical-only',action='store_true',help='validate pinned audit integrity without claiming current catalog compatibility')
    args=parser.parse_args()
    inventory_path=args.inventory or args.root/'next/coverage/onshape.json'
    ledger_path=args.ledger or args.root/'next/coverage/workflows.json'
    guide_path=args.guide or args.root/'next/WORKFLOWS.md'
    try:
        inventory_bytes=inventory_path.read_bytes()
        inventory=json.loads(inventory_bytes)
        ledger=json.loads(ledger_path.read_text(encoding='utf-8'))
        errors=validate(ledger,inventory,args.root,guide_path.read_text(encoding='utf-8'),inventory_bytes,
                        pinned_root=args.pinned_root,check_current=not args.historical_only)
    except (OSError,ValueError,KeyError,TypeError,AttributeError) as error:
        print(f'Invalid workflow ledger: {error}',file=sys.stderr)
        return 1
    if errors:
        for error in errors:
            print(error,file=sys.stderr)
        return 1
    print(f'Workflow structure valid: {len(ledger["topics"])} topics, '
          f'{sum(len(t["requirements"]) for t in ledger["topics"])} requirement IDs, '
          f'{len(ledger["families"])} families, {len(ledger["routes"])} routes. '+
          ('Historical integrity only; current catalog was not checked. ' if args.historical_only else 'Pinned historical integrity and strict current compatibility checked. ')+
          'All acceptance remains open; no behavior or parity certification.')
    return 0


if __name__=='__main__':
    raise SystemExit(main())
