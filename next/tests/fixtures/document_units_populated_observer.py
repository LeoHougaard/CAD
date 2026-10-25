import pathlib,sys,json,os,atexit
report=pathlib.Path(os.environ['CAD_UNITS_PREFS_REPORT'])
os.environ['CAD_UNITS_PREFS_REPORT']=str(report.with_name(report.stem+'-'+str(os.getpid())+report.suffix))
HERE=report.parent
sys.dont_write_bytecode=True
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[2]/'backend'))
import freecad_worker as w
prefs=w.App.ParamGet('User parameter:BaseApp/Preferences/Units');before=prefs.GetContents();schema=w.App.Units.getSchema()
dispatch=w.dispatch;prepare=w.prepare_state;bump=w.bump;fired=set()
def fault_prepare(doc):
    if doc.Name in {'ReviewSettingPrepare','ReviewSettingRestore'} and w.DocumentUnits.index(doc)==3 and doc.Name not in fired:
        fired.add(doc.Name);raise RuntimeError('review injected settings preparation failure')
    if doc.Name=='ReviewSettingRestore' and doc.Name in fired:
        raise RuntimeError('review injected settings restoration verification failure')
    return prepare(doc)
def fault_bump(doc):
    if doc.Name=='ReviewSettingPublish' and w.DocumentUnits.index(doc)==3 and doc.Name not in fired:
        fired.add(doc.Name);raise RuntimeError('review injected pre-publication bump failure')
    return bump(doc)
w.prepare_state=fault_prepare;w.bump=fault_bump
settings_observations=[]
def touched(doc):
    result={'object_states':{o.Name:list(o.State) for o in doc.Objects}}
    for name in ['isTouched','Touched']:
        try:
            v=getattr(doc,name);result[name]=v() if callable(v) else v
        except Exception:pass
    return result
def observed(request):
    doc=w.App.getDocument(request.get('document')) if request.get('document') in w.App.listDocuments() else None
    initial=touched(doc) if doc and request.get('op')=='document_settings' else None
    try:return dispatch(request)
    finally:
        after=prefs.GetContents()
        record=dict(before=before,after=after,schema_before=schema,schema_after=w.App.Units.getSchema(),unchanged=before==after and schema==w.App.Units.getSchema())
        pathlib.Path(os.environ.get('CAD_UNITS_PREFS_REPORT',str(HERE/'preferences-host.json'))).write_text(json.dumps(record,indent=2))
        if initial is not None:
            settings_observations.append(dict(request=request,before=initial,after=touched(doc)))
            (HERE/('native-touched-'+doc.Name+'.json')).write_text(json.dumps(settings_observations,indent=2))
w.dispatch=observed
w.main()
