"""Lifecycle fixture with unchanged native results and a quantity-reply delay."""
import hashlib
import json
import os
from pathlib import Path
import runpy
import sys
import time

sys.dont_write_bytecode = True
backend = Path(__file__).resolve().parents[2] / 'backend'
sys.path.insert(0, str(backend))
import freecad_worker as worker

delay_ms = int(os.environ.get('CAD_COPY_QUANTITY_DELAY_MS', '0'))
assert delay_ms in (0, 150, 250), 'Named debounce-boundary controls only'
output = Path(os.environ['CAD_LIFECYCLE_OUTPUT'])
dispatch = worker.dispatch


def delayed(request):
    result = dispatch(request)
    if request.get('op') == 'quantities':
        before = hashlib.sha256(json.dumps(result, sort_keys=True).encode()).hexdigest()
        time.sleep(delay_ms / 1000)
        after = hashlib.sha256(json.dumps(result, sort_keys=True).encode()).hexdigest()
        assert before == after
        with (output / ('quantity-delay-%s.jsonl' % os.getpid())).open('a', encoding='utf-8') as stream:
            stream.write(json.dumps(dict(op='quantities', id=request.get('id'), delay_ms=delay_ms,
                native_completed=True, before_sha256=before, after_sha256=after)) + '\n')
        bindings()
    return result


def bindings():
    modules = {name: dict(path=str(Path(module.__file__).resolve()),
        sha256=hashlib.sha256(Path(module.__file__).read_bytes()).hexdigest())
        for name, module in list(sys.modules.items()) if getattr(module, '__file__', None)
        and Path(module.__file__).is_file() and Path(module.__file__).resolve().is_relative_to(backend)}
    (output / ('copy-modules-%s.json' % os.getpid())).write_text(json.dumps(dict(
        pid=os.getpid(), modules=modules, wrapper_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()), indent=2) + '\n')


worker.dispatch = delayed
runpy.run_path(str(Path(__file__).with_name('lifecycle_fault_worker.py')), run_name='__main__')
