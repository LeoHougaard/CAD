"""Negative integrity checks for the workflow ledger, stdlib only."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode=True

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('validate_workflows',ROOT/'next/tools/validate_workflows.py')
validator=importlib.util.module_from_spec(spec)
spec.loader.exec_module(validator)


class WorkflowLedgerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.inventory_bytes=(ROOT/'next/coverage/onshape.json').read_bytes()
        cls.inventory=json.loads(cls.inventory_bytes)
        cls.ledger=json.loads((ROOT/'next/coverage/workflows.json').read_text(encoding='utf-8'))
        cls.guide=(ROOT/'next/WORKFLOWS.md').read_text(encoding='utf-8')
        supplied=os.environ.get('CAD_WORKFLOW_PINNED_ROOT')
        cls.pinned=validator.PinnedSources(ROOT,cls.ledger['metadata'],Path(supplied) if supplied else None)

    def run_validation(self,mutate=None):
        ledger=copy.deepcopy(self.ledger)
        if mutate:
            mutate(ledger)
        return validator.validate(ledger,self.inventory,ROOT,self.guide,self.inventory_bytes,pinned=self.pinned)

    def rejects(self,mutate,expected):
        errors=self.run_validation(mutate)
        self.assertTrue(any(expected in e for e in errors),errors)

    def test_current_ledger(self):
        self.assertEqual(self.run_validation(),[])

    def test_lost_topic(self):
        self.rejects(lambda d:d['topics'].pop(),'lost/reordered/added topic')

    def test_lost_requirement(self):
        self.rejects(lambda d:d['topics'][0]['requirements'].pop(),'lost/reordered/added requirement')

    def test_duplicate_requirement(self):
        self.rejects(lambda d:d['topics'][0]['requirements'].append(d['topics'][0]['requirements'][0]),'duplicate requirement IDs')

    def test_requirement_text_rewrite(self):
        self.rejects(lambda d:d['topics'][0]['requirements'][0].update(requirement='redefined'),'requirement text lost/changed')

    def test_duplicate_route_id(self):
        self.rejects(lambda d:d['routes'].append(d['routes'][0]),'routes: duplicate IDs')

    def test_duplicate_visible_route(self):
        self.rejects(lambda d:d['routes'][1].update(path=d['routes'][0]['path']),'duplicate visible route')

    def test_missing_prerequisite(self):
        self.rejects(lambda d:d['families'][0].update(prerequisites=[]),'missing prerequisites')

    def test_missing_lifecycle_stage(self):
        self.rejects(lambda d:d['families'][0]['steps'].pop(),'missing/duplicate lifecycle')

    def test_missing_acceptance(self):
        self.rejects(lambda d:d['families'][0].pop('acceptance'),'missing acceptance')

    def test_false_topic_completion(self):
        self.rejects(lambda d:d['topics'][0].update(full_documented_workflow_accepted=True),'unsupported completed claim')

    def test_false_requirement_completion(self):
        self.rejects(lambda d:d['topics'][0]['requirements'][0].update(full_documented_workflow_accepted=True),'unsupported completed claim')

    def test_false_evidence_completion(self):
        self.rejects(lambda d:d['evidence'][0].update(claim='complete'),'unsupported evidence/completion')

    def test_fake_percentage(self):
        self.rejects(lambda d:d['metadata'].update(completion_percentage=99),'completion percentage')

    def test_lost_native_input(self):
        self.rejects(lambda d:d['native_tasks'][0]['declared_fields'].pop(),'declared native declared_fields')

    def test_lost_package_input(self):
        self.rejects(lambda d:d['package_tasks'][0]['parameters'].popitem(),'package parameter declaration')

    def test_unresolved_route(self):
        self.rejects(lambda d:d['families'][0]['route_ids'].append('invented'),'unknown route')

    def test_missing_guide_family(self):
        errors=validator.validate(self.ledger,self.inventory,ROOT,'',self.inventory_bytes,pinned=self.pinned)
        self.assertTrue(any('guide family missing' in e for e in errors))

    def test_changed_inventory(self):
        errors=validator.validate(self.ledger,self.inventory,ROOT,self.guide,b'changed',pinned=self.pinned)
        self.assertTrue(any('inventory hash changed' in e for e in errors))

    def test_candidate_cannot_be_promoted(self):
        self.rejects(lambda d:d['in_progress_candidates'][0].update(status='complete'),'candidate promoted without review')

    def test_candidate_needs_checks(self):
        self.rejects(lambda d:d['in_progress_candidates'][0].update(pending_checks=[]),'missing candidate checks')

    def test_performance_policy_cannot_be_removed(self):
        self.rejects(lambda d:d.pop('performance_policy'),'performance hillclimbing must wait')

    def test_markdown_table_rows_must_be_contiguous(self):
        guide=self.guide+'\n| a | b |\n\n| c | d |\n'
        errors=validator.validate(self.ledger,self.inventory,ROOT,guide,self.inventory_bytes,pinned=self.pinned)
        self.assertTrue(any('blank row breaks' in e for e in errors))


class HistoricalAndCurrentTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        WorkflowLedgerTests.setUpClass()
        cls.ledger=WorkflowLedgerTests.ledger
        cls.pinned=WorkflowLedgerTests.pinned
        cls.inventory=WorkflowLedgerTests.inventory
        cls.inventory_bytes=WorkflowLedgerTests.inventory_bytes
        cls.guide=WorkflowLedgerTests.guide
        cls.native,cls.packages=validator.current_catalogs(ROOT)

    def current(self,mutate=None):
        ledger,native,packages=map(copy.deepcopy,[self.ledger,self.native,self.packages])
        if mutate:
            mutate(ledger,native,packages)
        return validator.validate_current(ledger,native,packages)

    def rejects(self,mutate,needle):
        errors=self.current(mutate)
        self.assertTrue(any(needle in e for e in errors),errors)

    def historical(self,ledger=None,root=ROOT,check_current=True):
        return validator.validate(ledger or self.ledger,self.inventory,root,self.guide,
            self.inventory_bytes,pinned=self.pinned,check_current=check_current)

    def baseline_root(self,directory):
        root=Path(directory)
        for path in self.pinned.hashes:
            p=root/path;p.parent.mkdir(parents=True,exist_ok=True)
            p.write_bytes(self.pinned.read_bytes(path))
        return root

    def test_intentional_labels_and_removed_historical_anchor(self):
        self.assertEqual(self.current(),[])
        self.assertEqual(self.historical(),[])
        historical=validator.native_catalog(self.pinned.read_text('next/src/feature_commands.rs'))
        for id,current in [('part.fuse','Union of two objects'),('part.multi_fuse','Union of multiple objects')]:
            self.assertEqual(historical[id]['label'],'Union')
            self.assertEqual(self.native[id]['label'],current)
            self.assertEqual(next(t['label'] for t in self.ledger['native_tasks'] if t['id']==id),'Union')
        anchor='*value = property_commands::selected_links(selected, sub)[0].clone();'
        self.assertIn(anchor,self.pinned.read_text('next/src/editor/properties.rs'))
        self.assertNotIn(anchor,(ROOT/'next/src/editor/properties.rs').read_text())

    def test_current_native_id_loss(self):
        self.rejects(lambda d,c,p:c.pop('part.fuse'),'stable ID set lost/added')

    def test_current_native_id_addition(self):
        self.rejects(lambda d,c,p:c.update(invented=c['part.fuse']),'stable ID set lost/added')

    def test_current_native_type_change(self):
        self.rejects(lambda d,c,p:c['part.fuse'].update(type='Part::Cut'),'current native type')

    def test_current_native_group_change(self):
        self.rejects(lambda d,c,p:c['part.fuse'].update(group='Other'),'current native group')

    def test_current_native_input_loss(self):
        self.rejects(lambda d,c,p:c['part.fuse']['declared_fields'].pop(),'current native declared_fields')

    def test_unlisted_current_rename(self):
        self.rejects(lambda d,c,p:c['part.cut'].update(label='Invented'),'without exact presentation update')

    def test_historical_label_cannot_be_rewritten(self):
        ledger=copy.deepcopy(self.ledger)
        next(t for t in ledger['native_tasks'] if t['id']=='part.fuse')['label']='Invented'
        self.assertTrue(any('declared native label lost/changed' in e for e in self.historical(ledger)))

    def test_current_package_input_change(self):
        self.rejects(lambda d,c,p:next(iter(p.values()))['parameters'].popitem(),'current package parameter')

    def test_current_package_id_loss(self):
        self.rejects(lambda d,c,p:p.pop(next(iter(p))),'current package stable ID')

    def test_current_package_id_addition(self):
        self.rejects(lambda d,c,p:p.update(invented=next(iter(p.values()))),'current package stable ID')

    def test_alias_cannot_rewrite_historical_label(self):
        self.rejects(lambda d,c,p:d['current_compatibility']['presentation_updates']['part.fuse'].update(baseline='Changed'),
            'rewrites historical label')

    def test_alias_cannot_promote_acceptance(self):
        self.rejects(lambda d,c,p:d['current_compatibility'].update(acceptance_update=True),'provenance or acceptance')

    def test_unknown_alias_stable_id(self):
        self.rejects(lambda d,c,p:d['current_compatibility']['presentation_updates'].update(invented=dict(baseline='x',current='y')),
            'unknown stable ID')

    def test_alias_cannot_change_source_commit(self):
        self.rejects(lambda d,c,p:d['current_compatibility'].update(source_commit='0'*40),'provenance or acceptance')

    def test_explicit_root_verifies_all_pinned_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root=self.baseline_root(directory)
            pinned=validator.PinnedSources(ROOT,self.ledger['metadata'],root)
            self.assertEqual(pinned.kind,'verified root')
            self.assertEqual(validator.validate(self.ledger,self.inventory,ROOT,self.guide,self.inventory_bytes,pinned=pinned),[])

    def test_explicit_root_rejects_changed_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root=self.baseline_root(directory)
            with (root/'next/src/editor/properties.rs').open('ab') as f:
                f.write(b'\n// changed baseline\n')
            with self.assertRaisesRegex(ValueError,'pinned source hash mismatch'):
                validator.PinnedSources(ROOT,self.ledger['metadata'],root)

    def test_explicit_root_rejects_missing_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root=self.baseline_root(directory)
            (root/'next/src/editor/properties.rs').unlink()
            with self.assertRaises((OSError,ValueError)):
                validator.PinnedSources(ROOT,self.ledger['metadata'],root)

    def test_explicit_root_rejects_extra_package(self):
        with tempfile.TemporaryDirectory() as directory:
            root=self.baseline_root(directory)
            (root/'next/packages/extra.json').write_text('{}')
            with self.assertRaisesRegex(ValueError,'package file set'):
                validator.PinnedSources(ROOT,self.ledger['metadata'],root)

    def test_missing_git_commit_does_not_fall_back_to_current_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root=self.baseline_root(directory)
            metadata=copy.deepcopy(self.ledger['metadata']);metadata['source_commit']='0'*40
            manifest=json.loads((ROOT/metadata['pinned_source_manifest']).read_bytes())
            manifest['source_commit']=metadata['source_commit']
            (root/metadata['pinned_source_manifest']).write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError,'Pinned Git commit is unavailable'):
                validator.PinnedSources(root,metadata)

    def test_historical_anchor_not_found_only_in_current_source(self):
        ledger=copy.deepcopy(self.ledger)
        source=next(q for q in ledger['task_queue'] if q['id']=='Q09')['source_references'][0]
        source['anchor']='property_widget_with_selection'
        self.assertTrue(any('historical source anchor missing' in e for e in self.historical(ledger)))

    def test_historical_anchor_must_match_its_exact_line(self):
        ledger=copy.deepcopy(self.ledger)
        source=next(q for q in ledger['task_queue'] if q['id']=='Q09')['source_references'][0]
        source['line']=1;source['url']=source['url'].rsplit('#L',1)[0]+'#L1'
        self.assertTrue(any('historical source anchor missing' in e for e in self.historical(ledger)))

    def test_historical_only_is_separate_from_current_compatibility(self):
        with tempfile.TemporaryDirectory() as directory:
            # This root intentionally has no current catalog. Pinned reads are
            # still authenticated; only an explicit historical-only call passes.
            root=Path(directory)
            self.assertEqual(self.historical(root=root,check_current=False),[])
            self.assertTrue(any('current catalog unavailable' in e for e in self.historical(root=root)))

    def test_duplicate_current_native_or_package_ids_are_rejected(self):
        text=(ROOT/'next/src/feature_commands.rs').read_text()
        extra='feature!("part.fuse", "Duplicate", "Part::Fuse", "Part operations", ["Base", "Tool", "Refine"])'
        with self.assertRaisesRegex(ValueError,'duplicate native stable ID'):
            validator.native_catalog(text+extra)
        p=next(iter(self.packages.values()))
        with self.assertRaisesRegex(ValueError,'duplicate package stable ID'):
            validator.package_catalog([p,p])

    def test_spline_pole_description_matches_pinned_xy_controls(self):
        inputs=next(f for f in self.ledger['families'] if f['id']=='sketch-modify')['inputs']
        description=next(s for s in inputs if s.startswith('Spline pole position:'))
        self.assertIn('1-based Pole selector and X (mm)/Y (mm)',description)
        self.assertNotIn('x/y/z Position',description)
        self.assertIn(description,self.guide)
        self.assertIn('["X (mm)", "Y (mm)"]',self.pinned.read_text('next/src/editor/sketch_tools.rs'))


if __name__=='__main__':
    unittest.main()
