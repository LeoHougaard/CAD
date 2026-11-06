"""Keep the unchanged frozen observer, with one output per native process."""
import os,runpy
from pathlib import Path
report=Path(os.environ['CAD_UNITS_PREFS_REPORT'])
os.environ['CAD_UNITS_PREFS_REPORT']=str(report.with_name(report.stem+'-'+str(os.getpid())+report.suffix))
runpy.run_path(str(Path(__file__).resolve().parents[2]/'tests/fixtures/document_units_observer.py'),run_name='__main__')
