"""Portable native external-source lifecycle, recovery, Editor and stock readback."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['runtime', 'host', 'compiler', 'editor-tests', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    source = Path(__file__).resolve().parents[1]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    runtime = json.loads(args.runtime.read_text())
    runtime['script'] = str(source / 'tests/fixtures/external_observed_worker.py')
    runtime_file = out / 'runtime.json'
    runtime_file.write_text(json.dumps(runtime, indent=2) + '\n')
    native = [runtime['executable'], *runtime['args']]
    base = dict(os.environ, PYTHONDONTWRITEBYTECODE='1', PYTHONUTF8='1', PYTHONIOENCODING='utf-8',
        CAD_EXTERNAL_ROOT=str(out), CAD_EXTERNAL_HOST=str(args.host.resolve()),
        CAD_EXTERNAL_COMPILER=str(args.compiler.resolve()),
        R7_BACKEND=str(source / 'backend'))
    files = [p for folder in ['backend', 'src', 'tests', 'tools'] for p in (source / folder).rglob('*')
        if p.is_file() and '__pycache__' not in p.parts]
    files += [args.host.resolve(), args.compiler.resolve(), args.editor_tests.resolve(), Path(runtime['executable'])]

    def sha(path):
        with Path(path).open('rb') as stream:
            return hashlib.file_digest(stream, 'sha256').hexdigest()

    inputs = {str(path.resolve()): sha(path) for path in files}
    (out / 'executed-inputs.json').write_text(json.dumps(dict(files=inputs, runtime=runtime), indent=2) + '\n')
    commands = []

    def run(name, command, extra=None):
        env = dict(base, **(extra or {}))
        folder = out / ('run-' + name)
        folder.mkdir()
        for key, value in [('FREECAD_USER_HOME', 'config'), ('TMP', 'tmp'), ('TEMP', 'tmp'),
                           ('APPDATA', 'appdata'), ('LOCALAPPDATA', 'localappdata')]:
            env[key] = str(folder / value)
            Path(env[key]).mkdir(exist_ok=True)
        env.setdefault('CAD_EXTERNAL_NATIVE_TRACE', str(folder / 'native.ndjson'))
        command = list(map(str, command))
        with (folder / 'raw.log').open('x', encoding='utf-8') as log:
            process = subprocess.Popen(command, cwd=source.parent, env=env, stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, 'BELOW_NORMAL_PRIORITY_CLASS', 0))
            code = process.wait()
        commands.append(dict(name=name, command=command, pid=process.pid, exit_code=code,
            priority='BelowNormal', environment={k: v for k, v in env.items() if k.startswith(('CAD_', 'R7_'))}))
        (out / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
        print(name, code, flush=True)
        if code:
            raise SystemExit((folder / 'raw.log').read_text())
        if '--exact' in command:
            assert re.search(r'test result: ok\. 1 passed', (folder / 'raw.log').read_text())

    fixtures = out / 'partial-input'
    fixtures.mkdir()
    run('component-fixtures', [*native, source / 'tests/fixtures/external_native_fixtures.py', out / 'fixtures2'])
    run('nested-fixtures', [*native, source / 'tests/fixtures/external_nested_fixtures.py', fixtures])
    run('deep-fixtures', [*native, source / 'tests/fixtures/external_deep_fixtures.py', fixtures])
    run('suppression-fixtures', [*native, source / 'tests/suppression_native_references.py', out / 'suppression-source'])
    run('native', [sys.executable, '-B', source / 'tests/external_assembly_workflows.py', 'native'])
    run('guards', [sys.executable, '-B', source / 'tests/external_assembly_guards.py', 'guards'])
    run('partial', [sys.executable, '-B', source / 'tests/external_partial_workflows.py'])
    run('source-cleanup', [sys.executable, '-B', source / 'tests/external_cleanup_workflows.py'])
    run('save-failures', [sys.executable, '-B', source / 'tests/external_save_failures.py', 'save-failures', fixtures / 'destinations'])
    cases = [
        ('editor', 'external_tests::native_external_assembly_editor', None),
        ('editor-pick', 'external_tests::native_external_affine_pick_and_measure', None),
        ('editor-save', 'external_tests::native_external_save_recovery', 'external_save_fault_worker.py'),
        ('editor-setters', 'r1_tests::editor_restoring_setter_recovery', 'external_setter_fault_worker.py'),
        ('editor-nested', 'r1_tests::editor_nested_source_revision_guard', 'external_setter_fault_worker.py'),
        ('editor-alias', 'r1_tests::editor_hardlink_source_ownership', 'external_setter_fault_worker.py'),
        ('editor-partial', 'partial_tests::partial_source_open_correction_history_reopen', None),
        ('editor-teardown', 'external_tests::source_teardown_failure_reports_actual_close_and_reachable_retry', 'open_cleanup_dependency_worker.py')]
    for name, test, wrapper in cases:
        selected = dict(runtime)
        if wrapper:
            selected['script'] = str(source / 'tests/fixtures' / wrapper)
        selected_path = out / (name + '-runtime.json')
        selected_path.write_text(json.dumps(selected, indent=2) + '\n')
        control = out / (name + '-control.json')
        control.write_text('{"mode":"off"}')
        run(name, [args.editor_tests.resolve(), 'editor::assembly::tests::' + test,
            '--exact', '--ignored', '--test-threads=1', '--nocapture'], dict(
            CAD_ASSEMBLY_RUNTIME=str(selected_path), CAD_ASSEMBLY_OUTPUT=str(out / name),
            CAD_EXTERNAL_FIXTURES=str(out / 'fixtures2'), CAD_EXTERNAL_SCENE=str(out / 'native/Final.FCStd'),
            CAD_EXTERNAL_AFFINE=str(out / 'native/affine.json'), CAD_SAVE_DESTINATIONS=str(fixtures / 'destinations'),
            CAD_SAVE_INJECT_TRACE=str(out / (name + '-save.ndjson')), CAD_FIX_CONTROL=str(control),
            CAD_FIX_TRACE=str(out / (name + '-setter.ndjson')), CAD_FIX_EDITOR_INPUTS=str(fixtures),
            CAD_PARTIAL_INPUT=str(fixtures), R7_BINDINGS=str(out / (name + '-bindings'))))
    run('stock-readback', [*native, source / 'tests/external_native_readback.py', 'stock', 'native', 'guards', 'editor'])
    run('stock-partial-readback', [*native, source / 'tests/external_partial_readback.py', fixtures, out / 'stock-partial.json'])
    assert all(sha(path) == value for path, value in inputs.items())
    (out / 'passed.json').write_text(json.dumps(dict(source_and_binary_inputs_exact=True,
        native_full_graph_and_recovery=True, actual_editor_cases=len(cases), stock_native_readback=True,
        private_fixture_inputs=False, physical_gui=False, full_assembly_parity=False,
        native_child_graceful_exit_behind_rust_certified=False), indent=2) + '\n')


if __name__ == '__main__':
    main()
