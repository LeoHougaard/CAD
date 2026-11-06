"""Independent named faults and native facts. No production source changes."""
import hashlib, json, os, sys
from pathlib import Path
sys.dont_write_bytecode=True
B=Path(os.environ['R7_BACKEND']).resolve();sys.path.insert(0,str(B))
import freecad_worker as w
O=Path(os.environ['R7_BINDINGS']);O.mkdir(exist_ok=True)
dispatch,validate,close,detach=w.dispatch,w.validate,w.App.closeDocument,w._native_suppression.detach
callback=w.SessionCallbacks.call
fault={};events=[];callbacks=[];current=None;ui_close_target=None;ui_close_failures=0;ui_remove_failures=0

def native_content_hashes(obj):
    members=w.AssemblySources.native_content(obj)
    if obj.Document.Name=='properties' and obj.Name=='B':
        folder=O/'native-members';folder.mkdir(exist_ok=True)
        for name,data in members:
            target=folder/(hashlib.sha256(data).hexdigest()+'-'+name)
            if not target.exists():target.write_bytes(data)
    return [(name,hashlib.sha256(data).hexdigest()) for name,data in members]

def facts():
    documents={}
    for n,d in w.App.listDocuments().items():
        documents[n]=dict(uuid=str(d.Uid),filename=d.FileName,label=d.Label,undo=d.UndoCount,redo=d.RedoCount,
            undo_names=list(d.UndoNames),redo_names=list(d.RedoNames),objects={o.Name:dict(id=o.ID,type=o.TypeId,
                label=o.Label,expressions=list(o.ExpressionEngine) if hasattr(o,'ExpressionEngine') else [],
                link=(o.LinkedObject.Document.Name,o.LinkedObject.Name) if o.TypeId=='App::Link' and o.LinkedObject else None,
                volume=o.Shape.Volume if hasattr(o,'Shape') and not o.Shape.isNull() else None,
                content=native_content_hashes(o)) for o in d.Objects})
        documents[n]['revision']=w.revision_request('lookup',n)
        documents[n]['lease']=w.lease('metadata',n)
        for obj in d.Objects:
            documents[n]['objects'][obj.Name]['out']=[dict(document=x.Document.Name,uuid=str(x.Document.Uid),name=x.Name,id=x.ID) for x in obj.OutList]
            documents[n]['objects'][obj.Name]['native_references']=sorted({(x.Document.Name,x.Name,x.ID) for x in w.native_references(obj)})
            documents[n]['objects'][obj.Name]['state']=list(obj.State)
            documents[n]['objects'][obj.Name]['status']=obj.getStatusString()
    return dict(documents=documents,active=w.App.ActiveDocument.Name if w.App.ActiveDocument else None,
        native_active=w.App.activeDocument().Name if w.App.activeDocument() else None,
        transaction=w.App.getActiveTransaction(),native_pending={n:d.HasPendingTransaction for n,d in w.App.listDocuments().items()},
        observers=sorted(w._native_suppression.documents),failed_loads=w._failed_loads,
        source_dependencies=w.AssemblySources._dependencies,readonly_paths=sorted(w.AssemblySources._readonly_paths),
        source_recoveries={n:w.AssemblySources.recovery_metadata(d) for n,d in w.App.listDocuments().items()},
        errors=list(w._native_suppression.errors),pending=list(map(list,w._native_suppression.pending)),depth=w._native_suppression.replaying)
def dump():
    modules={}
    for n,m in list(sys.modules.items()):
        p=getattr(m,'__file__',None)
        if p and (Path(p).resolve().is_relative_to(B) or n in ('FreeCAD','Part','Sketcher')):
            p=Path(p).resolve();modules[n]=dict(path=str(p),sha256=hashlib.sha256(p.read_bytes()).hexdigest())
    (O/(str(os.getpid())+'.json')).write_text(json.dumps(dict(pid=os.getpid(),ppid=os.getppid(),modules=modules,events=events,callbacks=callbacks),indent=2))
def observed_callback(channel,action,document,**params):
    result=callback(channel,action,document,**params)
    callbacks.append(dict(op=current,channel=channel,action=action,document=document,result=result))
    return result

# Change only a named callback's correlated response after the real host answered.
class CallbackJson:
    def __getattr__(self,k):return getattr(json,k)
    def loads(self,value,*a,**k):
        reply=json.loads(value,*a,**k)
        target=fault.get('bad_callback')
        if target and reply.get('kind')==target['channel'] and callbacks and callbacks[-1].get('pending_action')==target['action']:
            events.append(dict(event='real_callback_reply_before_fault',reply=reply))
            fault.pop('bad_callback');reply['sequence']+=1
        return reply
def traced_callback(channel,action,document,**params):
    callbacks.append(dict(op=current,channel=channel,pending_action=action,document=document))
    return observed_callback(channel,action,document,**params)
w.SessionCallbacks.json=CallbackJson()
w.SessionCallbacks.call=traced_callback

def external_fixture_identity(link,target):
    """Saved test metadata follows the full native graph, not a supplied list."""
    doc=target.Document;seen={doc.Name};pending=[doc];dependencies=[]
    while pending:
        current_doc=pending.pop()
        for obj in current_doc.Objects:
            for other in obj.OutList:
                owner=other.Document
                if owner.Name not in seen:
                    seen.add(owner.Name);pending.append(owner)
                    dependencies.append(dict(path=str(Path(owner.FileName).resolve()),
                        sha256=hashlib.sha256(Path(owner.FileName).read_bytes()).hexdigest(),document_uid=str(owner.Uid)))
    value=dict(version=1,path=str(Path(doc.FileName).resolve()),
        sha256=hashlib.sha256(Path(doc.FileName).read_bytes()).hexdigest(),document_uid=str(doc.Uid),
        object=target.Name,object_id=target.ID,type=target.TypeId,dependencies=sorted(dependencies,key=lambda v:v['path']))
    link.addProperty('App::PropertyString','CadExternalSource','External source')
    link.CadExternalSource=json.dumps(value,sort_keys=True);link.setPropertyStatus('CadExternalSource','ReadOnly')

def recovery_link(doc,target,name='Target'):
    # A genuine saved local Link carries the hidden foreign recovery edge.
    # The old unannotated foreign LinkedObject format is a separate negative.
    link=doc.addObject('App::Link',name);link.LinkedObject=doc.Box
    link.addProperty('App::PropertyXLink','RecoveryOwner','Recovery fixture')
    link.RecoveryOwner=target;link.setPropertyStatus('RecoveryOwner','Hidden')
    return link

def recovery_link_proof(doc,name='Target'):
    link=doc.getObject(name)
    assert link.TypeId=='App::Link' and link.ElementCount==0
    assert link.LinkedObject==doc.Box and link.getTypeIdOfProperty('RecoveryOwner')=='App::PropertyXLink'
    assert 'Hidden' in link.getPropertyStatus('RecoveryOwner')
    target=link.RecoveryOwner
    assert target.Document!=doc
    link_edges={(o.Document.Name,o.ID) for o in w.native_references(link) if o.Document!=doc}
    assert link_edges=={(target.Document.Name,target.ID)}
    external_dependents=[other.Name for other in w.App.listDocuments().values() if other!=doc
        if any(obj.Document==doc and obj.ID==link.ID for obj in w.native_object_dependencies(other.Objects))]
    foreign_edges=sorted({(obj.Name,o.Document.Name,o.Name,o.ID) for obj in doc.Objects
        for o in w.native_references(obj) if o.Document!=doc})
    return dict(document=doc.Name,link=link.Name,native_id=link.ID,element_count=link.ElementCount,
        local_target=dict(name=doc.Box.Name,id=doc.Box.ID),hidden_foreign_edge=dict(document=target.Document.Name,
        object=target.Name,native_id=target.ID),foreign_edges=foreign_edges,sole_document_foreign_edge=len(foreign_edges)==1,
        dependency_closure=sorted(w.native_dependencies(doc)),external_dependents=external_dependents)

def native_fixture(folder,malformed=False):
    folder=Path(folder);folder.mkdir(exist_ok=True)
    shared=w.App.newDocument('Shared');shared.Label='SameLabel'
    part=shared.addObject('Part::Box','SharedBox');part.Length=7
    if malformed is True:
        broken=shared.addObject('App::FeaturePython','BrokenGround');broken.addProperty('App::PropertyLinkGlobal','ObjectToGround','Ground')
    shared.recompute();shared.saveAs(str(folder/'shared.FCStd'))
    for key,file in [('KeeperNative','keeperfile.FCStd'),('RootNative','source.FCStd')]:
        d=w.App.newDocument(key);d.Label='SameLabel';box=d.addObject('Part::Box','Box');box.Length=5;d.recompute();d.saveAs(str(folder/file))
        link=d.addObject('App::Link','SharedLink');link.LinkedObject=part;external_fixture_identity(link,part)
        if malformed and key=='RootNative':
            broken=d.addObject('App::FeaturePython','BrokenGround');broken.addProperty('App::PropertyLinkGlobal','ObjectToGround','Ground')
        d.recompute();d.saveAs(str(folder/file))
    for key in list(w.App.listDocuments()):close(key)
    w.App.setActiveDocument('');w.App.ActiveDocument=w.App.activeDocument()
    return dict(files=[str(folder/p) for p in ('shared.FCStd','keeperfile.FCStd','source.FCStd')])
def observed_validate(doc,*args,**kwargs):
    global ui_remove_failures
    result=validate(doc,*args,**kwargs)
    if current=='transaction' and doc.Name==ui_close_target and ui_remove_failures:
        ui_remove_failures-=1
        w.fail('invalid_model','independent reference removal validation fault')
    if fault.pop('invalid_dependency_proxy',False):
        dependencies=[d for d in w.App.listDocuments().values() if Path(d.FileName).name=='shared.FCStd']
        assert len(dependencies)==1
        obj=dependencies[0].addObject('App::FeaturePython','BrokenGround')
        obj.addProperty('App::PropertyLinkGlobal','ObjectToGround','Ground')
        w.validate_assembly_proxies(dependencies[0])
    if fault.pop('sibling',False):
        d=w.App.newDocument('Sibling');d.Label='SameLabel';d.addObject('Part::Box','Box');d.recompute();w._native_suppression.attach(d)
    if fault.pop('validate',False):w.fail('invalid_model','independent original validation failure')
    return result
def observed_close(key):
    global ui_close_failures
    events.append(dict(event='close_attempt',op=current,key=key,observer=key in w._native_suppression.documents))
    if key==ui_close_target and ui_close_failures:
        ui_close_failures-=1
        raise RuntimeError('independent Close fault '+key)
    if key in fault.get('close_fail',[]) or Path(w.App.getDocument(key).FileName).name in fault.get('close_fail_files',[]):raise RuntimeError('independent Close fault '+key)
    if key in fault.get('close_noop',[]):return None
    filename=Path(w.App.getDocument(key).FileName).name
    result=close(key);events.append(dict(event='native_closed',key=key,remaining=sorted(w.App.listDocuments())))
    if filename in fault.get('close_after_files',[]):
        raise RuntimeError('independent post-Close fault '+key)
    return result
def observed_detach(key):
    assert key not in w.App.listDocuments(),key
    events.append(dict(event='detach',key=key));return detach(key)
def bound(req):
    global fault,current,ui_close_target,ui_close_failures,ui_remove_failures
    current=req.get('op');fault=req.pop('_independent_fault',{})
    before=json.loads(json.dumps(facts()));error=None
    try:
        if current=='_review_fixture':return native_fixture(req['folder'],req.get('malformed',False))
        if current=='_native_facts':return facts()
        if current=='_recovery_link_proof':return recovery_link_proof(w.App.getDocument(req['document']),req.get('link_name','Target'))
        if current=='_terminal_editor_dependency':
            target=w.App.getDocument(req['document']).Box;d=w.App.newDocument('EditorDependentNative')
            path=Path(req['path']);d.addObject('Part::Box','Box');d.recompute();d.saveAs(str(path))
            recovery_link(d,target);d.recompute();d.saveAs(str(path));close(d.Name)
            result=dispatch(dict(version=1,id=req['id'],op='open',path=str(path)))
            child=result['document']
            proof=recovery_link_proof(w.App.getDocument(child))
            assert proof['sole_document_foreign_edge'] and not proof['external_dependents']
            events.append(dict(event='saved_recovery_link_proof',proof=proof))
            dispatch(dict(version=1,id=req['id'],op='transaction',document=child,expected_revision=result['revision'],
                edits=[dict(op='set',object='Box',property='Length',value=44)]))
            if req.get('cycle'):
                target.addProperty('App::PropertyXLink','NativeReference');target.NativeReference=w.App.getDocument(child).Box
            ui_close_target=child;ui_close_failures=0 if req.get('cycle') else 2;ui_remove_failures=1 if req.get('cycle') else 0
            return dict(dependent=child,path=str(path))
        if current=='_linked_file_fixture':
            target=w.App.getDocument(req['document']).getObject(req['object']);d=w.App.newDocument('LinkedNative')
            path=Path(req['path']);d.addObject('Part::Box','Box');d.Box.Length=req.get('box_length',10);d.recompute();d.saveAs(str(path))
            if req.get('local_link'):d.addObject('App::Link','LocalLink').LinkedObject=d.Box
            if req.get('legacy_external_link'):
                d.addObject('App::Link','Target').LinkedObject=target
            else:recovery_link(d,target,req.get('link_name','Target'))
            d.recompute();d.saveAs(str(path));close(d.Name)
            return dict(path=str(path))
        if current=='_cleanup_borrowed_fixture':
            before=facts()
            assert all(w.revision_request('lookup',key) is None for key in w.App.listDocuments())
            for key in list(w.App.listDocuments()):
                close(key);w._native_suppression.detach(key)
            return dict(before=before,after=facts(),test_fixture_cleanup_only=True)
        if current=='_poison_owner':w.lease('poison',req['document']);return facts()
        if current=='_follower_fixture':
            target=w.App.openDocument(req['path']);d=w.App.newDocument('FollowerNative');box=d.addObject('Part::Box','Box')
            path=Path(req['folder'])/req.get('filename','follower.FCStd');d.recompute();d.saveAs(str(path))
            box.setExpression('Length',target.Name+'#'+req.get('object','B')+'.Length');d.recompute();d.saveAs(str(path))
            for key in list(w.App.listDocuments()):close(key)
            w.App.setActiveDocument('');w.App.ActiveDocument=w.App.activeDocument()
            return dict(path=str(path))
        if current=='_late_expression':
            doc=w.App.getDocument(req['document']);obj=doc.getObject(req['object'])
            obj.setExpression(req['property'],req['expression']);doc.recompute()
            return facts()
        if current=='_nested_fixture':
            target=w.App.openDocument(req['path'])
            d=w.App.newDocument('NestedNative');d.addObject('Part::Box','Box');path=Path(req['folder'])/'nested.FCStd'
            d.recompute();d.saveAs(str(path));link=d.addObject('App::Link','NestedLink');link.LinkedObject=target.SharedLink
            external_fixture_identity(link,target.SharedLink)
            d.recompute();d.saveAs(str(path))
            for key in list(w.App.listDocuments()):close(key)
            w.App.setActiveDocument('');w.App.ActiveDocument=w.App.activeDocument()
            return dict(path=str(path))
        if current=='_late_adopt':
            doc=w.App.getDocument(req['document']);obj=doc.getObject(req['object'])
            prop=req.get('property','NativeReference');kind=req.get('type','App::PropertyXLink')
            if prop not in obj.PropertiesList:obj.addProperty(kind,prop)
            target=w.App.getDocument(req['target_document']).getObject(req['target_object'])
            if 'SubList' in kind:value=[(target,[''])]
            elif 'List' in kind:value=[target]
            elif 'Sub' in kind:value=(target,[''])
            else:value=target
            setattr(obj,prop,value)
            if req.get('recompute'):
                assert all(w.lease('metadata',key)['available'] for key in w.App.listDocuments())
                for owner in w.App.listDocuments().values():owner.recompute()
            return facts()
        if current=='_property_fixture':
            folder=Path(req['folder']);folder.mkdir()
            d=w.App.newDocument('PropertyNative');a=d.addObject('Part::Box','A');d.addObject('Part::Box','B')
            for i,kind in enumerate(['App::PropertyLink','App::PropertyLinkHidden','App::PropertyLinkSub','App::PropertyLinkList','App::PropertyXLink','App::PropertyXLinkSub','App::PropertyXLinkSubList']):
                a.addProperty(kind,'Ref'+str(i))
            d.recompute();path=folder/'properties.FCStd';d.saveAs(str(path));close(d.Name)
            return dict(path=str(path))
        if current=='_review_validate_owners':
            result={}
            for key,doc in w.App.listDocuments().items():
                try:w.validate_assembly_proxies(doc);result[key]=None
                except w.Failure as e:result[key]=dict(code=e.code,message=str(e))
            return result
        return dispatch(req)
    except BaseException as e:
        error=dict(type=type(e).__name__,message=str(e));raise
    finally:
        events.append(dict(event='request',request=req,before=before,after=json.loads(json.dumps(facts())),error=error));dump();fault={}
w.validate=observed_validate;w.App.closeDocument=observed_close;w._native_suppression.detach=observed_detach;w.dispatch=bound
w.main()
events.append(dict(event='normal_native_eof',facts=facts()));dump()
