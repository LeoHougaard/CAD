"""Test-only loader fault, with production backend bytes left untouched."""
import importlib.abc
import os
from pathlib import Path
import runpy
import sys

class Block(importlib.abc.MetaPathFinder):
    def find_spec(self, fullname, path=None, target=None):
        if fullname == 'JointObject':
            raise ModuleNotFoundError('R2 injected unavailable shipped JointObject')

sys.meta_path.insert(0, Block())
backend=Path(os.environ['R2_BACKEND'])
sys.path.insert(0,str(backend))
runpy.run_path(str(backend/'freecad_worker.py'),run_name='__main__')
