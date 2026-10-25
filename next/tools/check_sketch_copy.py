"""Portable native Copy/Clone/Move/array controls and persistent-host checks."""
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
    for name in ['runtime', 'host', 'compiler', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--editor-tests', type=Path)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    source = Path(__file__).resolve().parents[1]
    runtime = json.loads(args.runtime.read_text())
    runtime['script'] = str(source / 'backend/freecad_worker.py')
    runtime_path = out / 'runtime.json'
    runtime_path.write_text(json.dumps(runtime, indent=2) + '\n')
    prefix = [runtime['executable'], *runtime['args']]
    env = dict(os.environ, PYTHONDONTWRITEBYTECODE='1', PYTHONUTF8='1', PYTHONIOENCODING='utf-8')
    env.setdefault('FREECAD_USER_HOME', str(out / 'config'))
    Path(env['FREECAD_USER_HOME']).mkdir(parents=True, exist_ok=True)

    def sha(path):
        with Path(path).open('rb') as stream:
            return hashlib.file_digest(stream, 'sha256').hexdigest()

    files = [path for folder in ['src', 'backend', 'tools', 'tests']
        for path in (source / folder).rglob('*') if path.is_file() and '__pycache__' not in path.parts]
    files += [args.host.resolve(), args.compiler.resolve(), Path(runtime['executable'])]
    if args.editor_tests:
        files.append(args.editor_tests.resolve())
    hashes = {str(path.resolve()): sha(path) for path in files}
    (out / 'executed-inputs.json').write_text(json.dumps(dict(files=hashes, runtime=runtime), indent=2) + '\n')
    commands = []

    def run(name, command, extra=None):
        command = list(map(str, command))
        with (out / (name + '.log')).open('x', encoding='utf-8') as log:
            result = subprocess.run(command, env=dict(env, **(extra or {})), stdout=log,
                stderr=subprocess.STDOUT, creationflags=getattr(subprocess, 'BELOW_NORMAL_PRIORITY_CLASS', 0))
        commands.append(dict(name=name, command=command, exit_code=result.returncode, environment=extra))
        (out / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
        print(name, result.returncode, flush=True)
        if result.returncode:
            raise SystemExit((out / (name + '.log')).read_text())

    for name in ['native', 'arithmetic', 'native_paths', 'native_repeat', 'native_coupled']:
        run(name, [*prefix, source / ('tests/sketch_copy_' + name + '.py'), out / name])
    for name in ['workflows', 'cli_workflows']:
        command = [sys.executable, '-B', source / ('tests/sketch_copy_' + name + '.py'),
            '--host', args.host.resolve(), '--runtime', runtime_path, '--output', out / name]
        if name == 'cli_workflows':
            command += ['--compiler', args.compiler.resolve()]
        run(name, command)
    observed = dict(runtime, script=str(source / 'tests/fixtures/suppression_checkpoint_worker.py'))
    observed_path = out / 'observed-runtime.json'
    observed_path.write_text(json.dumps(observed, indent=2) + '\n')
    trace = out / 'native-checkpoint.jsonl'
    run('integration', [sys.executable, '-B', source / 'tests/sketch_copy_integration_workflows.py',
        '--host', args.host.resolve(), '--runtime', observed_path, '--output', out / 'integration'],
        dict(CAD_SUPPRESSION_TRACE=str(trace)))
    events = [json.loads(line) for line in trace.read_text().splitlines()]
    restores = [event for event in events if event['event'] == 'checkpoint_restore']
    assert restores and all(event['history_depth'] > 0 for event in restores)
    assert all(event['exact_native_input'] for event in events if event['event'] == 'suppression_capture')
    assert all(event['history_depth'] == 0 and not event['callback_errors'] for event in events if event['event'] == 'dispatch')
    if args.editor_tests:
        # The debounce is 150 ms. Exercise ordinary replies, that boundary and
        # the independently reproduced 250 ms quantity-reply delay.
        delayed = dict(runtime, test_worker=str(source / 'tests/fixtures/copy_quantity_delay_worker.py'))
        delayed_path = out / 'editor-runtime.json'
        delayed_path.write_text(json.dumps(delayed, indent=2) + '\n')
        for delay in [0, 150, 250]:
            name = 'editor-' + str(delay)
            editor = out / name
            editor.mkdir()
            run(name, [args.editor_tests.resolve(), 'editor::sketch::lifecycle_tests::copy_tests::',
                '--ignored', '--test-threads=1', '--nocapture'],
                dict(CAD_LIFECYCLE_RUNTIME=str(delayed_path), CAD_LIFECYCLE_OUTPUT=str(editor),
                    CAD_COPY_QUANTITY_DELAY_MS=str(delay)))
            assert re.search(r'test result: ok\. 2 passed', (out / (name + '.log')).read_text())
            delays = [json.loads(line) for path in editor.glob('quantity-delay-*.jsonl') for line in path.read_text().splitlines()]
            assert delays and all(item['delay_ms'] == delay and item['before_sha256'] == item['after_sha256'] for item in delays)
    assert all(sha(path) == digest for path, digest in hashes.items())
    repeats = json.loads((out / 'native_repeat/report.json').read_text())
    (out / 'passed.json').write_text(json.dumps(dict(native_and_host=True,
        actual_editor_cases=2 if args.editor_tests else 'covered by the CI lifecycle step',
        editor_quantity_delay_ms=[0, 150, 250] if args.editor_tests else [],
        checkpoint_restores=len(restores), sources_and_binaries_unchanged=True,
        raw_post_solve_mismatches=[item for item in repeats['comparisons'] if not item['post_solve_exact']],
        physical_gui=False, resources=False), indent=2) + '\n')


if __name__ == '__main__':
    main()
