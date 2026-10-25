"""Test observer and one-shot faults, entirely outside the production adapter."""
import os,sys,json
from pathlib import Path
here=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(here/'backend'))
import freecad_worker as w
prefs=w.App.ParamGet('User parameter:BaseApp/Preferences/Units')
before=prefs.GetContents();schema=w.App.Units.getSchema();fired=set()
prepare=w.prepare_state;bump=w.bump
def fault_prepare(doc):
    if doc.Name in {'FailPrepare','FailRestore'} and w.DocumentUnits.index(doc)==3 and doc.Name not in fired:
        fired.add(doc.Name);raise RuntimeError('test fault during settings state preparation')
    if doc.Name=='FailRestore' and doc.Name in fired:
        raise RuntimeError('test fault during restored state preparation')
    return prepare(doc)
def fault_bump(doc):
    if doc.Name=='FailBump' and w.DocumentUnits.index(doc)==3 and doc.Name not in fired:
        fired.add(doc.Name);raise RuntimeError('test fault during settings publication')
    return bump(doc)
w.prepare_state=fault_prepare;w.bump=fault_bump
dispatch=w.dispatch
observations=0
def record_preferences():
    global observations
    observations+=1
    after=prefs.GetContents();record=dict(before=before,after=after,schema_before=schema,schema_after=w.App.Units.getSchema(),unchanged=before==after and schema==w.App.Units.getSchema())
    record['observations']=observations
    target=Path(os.environ.get('CAD_UNITS_PREFS_REPORT',str(here/'preferences-report.json')));target.write_text(json.dumps(record,indent=2));assert record['unchanged']
def observed_dispatch(request):
    try:return dispatch(request)
    finally:record_preferences()
w.dispatch=observed_dispatch
w.main()
