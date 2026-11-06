"""Isolated one-shot native SaveAs failures. Not a production protocol option."""
import json,os,sys,zipfile
from pathlib import Path
import xml.etree.ElementTree as ET
R=Path(__file__).resolve().parent;sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'backend'))
import freecad_worker as W
S=W.NativeSaves
original=W.dispatch;native_save=S.native_save;validate=S.validate_paths
stage=None;fired=set()
trace=Path(os.environ['CAD_SAVE_INJECT_TRACE'])
stages={'after_save_as','clear_rebind','native_save','serialized_paths'}
def fail(kind):
    global stage
    if stage==kind:
        stage=None
        raise RuntimeError('injected one-shot '+kind)
def checkpoint(kind):fail(kind)
def save(doc):
    native_save(doc)
    fail('native_save')  # actual native final write completed before the injected error
def validate_paths(path):
    global stage
    if stage=='serialized_paths':
        stage=None
        temporary=Path(path).with_suffix('.injected.tmp')
        with zipfile.ZipFile(path) as source,zipfile.ZipFile(temporary,'w',zipfile.ZIP_DEFLATED) as target:
            for name in source.namelist():
                data=source.read(name)
                if name=='Document.xml':
                    xml=ET.fromstring(data);edge=xml.find('.//Property[@name="LinkedObject"]/XLink')
                    assert edge is not None;edge.set('file','injected-missing-source.FCStd');data=ET.tostring(xml,encoding='utf-8')
                target.writestr(name,data)
        os.replace(temporary,path)
    validate(path)
S.checkpoint=checkpoint;S.native_save=save;S.validate_paths=validate_paths
def snapshot(doc):
    return dict(path=doc.FileName,undo=doc.UndoCount,redo=doc.RedoCount,
                ids={o.Name:o.ID for o in doc.Objects},state=W.snapshot(doc))
def dispatch(request):
    global stage
    path=Path(request.get('path',''))
    inject=request.get('op')=='save' and path.parent.name in stages and str(path) not in fired
    doc=W.App.getDocument(request['document']) if inject else None
    before=snapshot(doc) if doc is not None else None
    if inject:stage=path.parent.name;fired.add(str(path))
    try:return original(request)
    finally:
        if doc is not None:
            with trace.open('a',encoding='utf-8') as f:f.write(json.dumps(dict(stage=path.parent.name,before=before,after=snapshot(doc)))+'\n')
        stage=None
W.dispatch=dispatch;W.main()
