"""Portable failed-Open quarantine and terminal native recovery regressions."""
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
    for name in ['runtime', 'host', 'editor-tests', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    source = Path(__file__).resolve().parents[1]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    runtime = json.loads(args.runtime.read_text())

    def sha(path):
        with Path(path).open('rb') as stream:
            return hashlib.file_digest(stream, 'sha256').hexdigest()

    files = [p for folder in ['src', 'backend', 'tools', 'tests'] for p in (source / folder).rglob('*')
        if p.is_file() and '__pycache__' not in p.parts]
    files += [args.host.resolve(), args.editor_tests.resolve(), Path(runtime['executable'])]
    inputs = {str(path.resolve()): sha(path) for path in files}
    (out / 'executed-inputs.json').write_text(json.dumps(dict(files=inputs, runtime=runtime), indent=2) + '\n')
    commands = []
    cases = [('original', 'open_cleanup', None), ('graph', 'open_cleanup_dependency', None),
        ('terminal', 'terminal_recovery', None), ('scope', 'terminal_removal_scope', None),
        ('editor-original', 'open_cleanup', 'native_failed_open_cleanup_retries_from_reachable_editor_open'),
        ('editor-close', 'open_cleanup_dependency', 'native_terminal_dependent_close_confirmation_retry_and_reopen'),
        ('editor-cycle', 'open_cleanup_dependency', 'native_terminal_cycle_reference_removal_cancel_failure_retry')]
    for name, fixture, test in cases:
        folder = out / name
        folder.mkdir()
        worker = 'open_cleanup_dependency' if fixture == 'terminal_recovery' else fixture
        observed = dict(runtime, script=str(source / ('tests/fixtures/' + worker + '_worker.py')))
        runtime_path = folder / 'runtime.json'
        runtime_path.write_text(json.dumps(observed, indent=2) + '\n')
        env = dict(os.environ, PYTHONDONTWRITEBYTECODE='1', PYTHONUTF8='1', PYTHONIOENCODING='utf-8',
            R7_BACKEND=str(source / 'backend'), R7_BINDINGS=str(folder / 'bindings'), R7_OUTPUT=str(folder / 'outputs'),
            R7_HOST=str(args.host.resolve()), R7_RUNTIME=str(runtime_path), R7_TEST_HOST=str(args.editor_tests.resolve()),
            CAD_UNITS_RUNTIME=str(runtime_path), CAD_UNITS_OUTPUT=str(folder / 'outputs'))
        for key, directory in [('FREECAD_USER_HOME', 'config'), ('TMP', 'tmp'), ('TEMP', 'tmp'),
            ('APPDATA', 'appdata'), ('LOCALAPPDATA', 'localappdata')]:
            env[key] = str(folder / directory)
            Path(env[key]).mkdir(exist_ok=True)
        (folder / 'bindings').mkdir()
        cwd = source.parent
        if name == 'editor-close':
            cwd = folder / 'outputs/terminal-recovery'
            cwd.mkdir(parents=True)
        if name == 'editor-original':
            env.update(R7_EDITOR_FAULT_PLAN=str(folder / 'editor-plan.json'), R7_EDITOR_DEPENDENCIES='1')
        if test:
            command = [str(args.editor_tests.resolve()), 'editor::units::tests::open_cleanup_tests::' + test,
                '--exact', '--ignored', '--test-threads=1', '--nocapture']
        else:
            command = [sys.executable, '-B', str(source / ('tests/' + fixture + '_workflows.py'))]
        with (folder / 'raw.log').open('x', encoding='utf-8') as log:
            process = subprocess.Popen(command, cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, 'BELOW_NORMAL_PRIORITY_CLASS', 0))
            code = process.wait()
        commands.append(dict(name=name, command=command, cwd=str(cwd), pid=process.pid, exit_code=code,
            runtime=observed, environment={key: value for key, value in env.items() if key.startswith(('R7_', 'CAD_'))}))
        (out / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
        print(name, code, flush=True)
        if code:
            raise SystemExit((folder / 'raw.log').read_text())
        if test:
            assert re.search(r'test result: ok\. 1 passed', (folder / 'raw.log').read_text())
    assert all(sha(path) == value for path, value in inputs.items())
    (out / 'passed.json').write_text(json.dumps(dict(public_workflows=4, actual_editor=3,
        native_model_and_identity_oracles_retained=True, inputs_unchanged=True,
        physical_gui=False, native_child_exit0_behind_rust_certified=False), indent=2) + '\n')


if __name__ == '__main__':
    main()
