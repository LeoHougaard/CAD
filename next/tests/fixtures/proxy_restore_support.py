"""Native fixtures and test-only faults. Never installed in the production worker."""
import json
import os
from pathlib import Path
import runpy
import sys


def fixtures(backend, output):
    import FreeCAD as A
    import Part
    sys.path.insert(0,str(backend))
    import assembly as adapter
    d=A.newDocument('PartCase')
    try:
        box=d.addObject('Part::Box','Source');d.recompute()
        link=d.addObject('App::Link','Outside');link.setLink(box)
        adapter.apply(d,dict(op='assembly_create',name='EmptyAssembly',joints='EmptyJoints'))
        custom=d.addObject('App::FeaturePython','Custom')
        custom.addProperty('App::PropertyString','ObjectToGround','Notes');custom.ObjectToGround='not a joint'
        custom2=d.addObject('App::FeaturePython','CustomType')
        custom2.addProperty('App::PropertyEnumeration','JointType','Custom');custom2.JointType=['Note']
        adapter.validate_proxies(d)
        assert 'JointObject' not in sys.modules
        d.recompute();d.saveAs(str(output/'part.FCStd'))
    finally:A.closeDocument(d.Name)
    d=A.newDocument('AssemblyCase')
    try:
        box=d.addObject('Part::Box','Source');d.recompute()
        adapter.apply(d,dict(op='assembly_create',name='Assembly',joints='Joints'))
        for name,x in [('First',0),('Second',40)]:
            adapter.apply(d,dict(op='assembly_insert',assembly='Assembly',name=name,source='Source',
                placement=dict(position=[x,0,0],rotation=[0,0,0,1])))
        assert 'JointObject' not in sys.modules
        adapter.apply(d,dict(op='assembly_ground',assembly='Assembly',object='First',name='Ground',grounded=True))
        adapter.apply(d,dict(op='assembly_joint',assembly='Assembly',name='Joint',create=True,joint_type='Fixed',
            reference1=dict(link='First',subelements=['','']),reference2=dict(link='Second',subelements=['','']),
            properties={},expressions={'Offset1.Base.z':'Source.Length / 5'}))
        d.recompute();adapter.solve_document(d);adapter.validate_proxies(d)
        native=adapter.joint_module()
        assert tuple(native.JointTypes)==adapter.JOINT_TYPES
        assert list(d.Joint.getEnumerationsOfProperty('JointType'))==list(adapter.JOINT_TYPES)
        d.saveAs(str(output/'healthy.FCStd'))
        class DerivedJoint(native.Joint):pass
        class DerivedGround(native.GroundedJoint):pass
        joint_proxy=d.Joint.Proxy;ground_proxy=d.Ground.Proxy
        joint_proxy.__class__=DerivedJoint;ground_proxy.__class__=DerivedGround
        adapter.validate_proxies(d)
        joint_proxy.__class__=native.Joint;ground_proxy.__class__=native.GroundedJoint
        # The shipped constructor can also use another native Python feature
        # container. Its serialized signature still needs its real proxy.
        container=A.newDocument('NativePythonContainer')
        try:
            ground=container.addObject('Part::FeaturePython','NativeGround')
            native.GroundedJoint(ground,None)
            adapter.validate_proxies(container)
            ground.Proxy=None
            try:
                adapter.validate_proxies(container)
                raise AssertionError('Missing native proxy on Part::FeaturePython was accepted')
            except adapter.AssemblyError as error:assert error.code=='assembly_proxy_restore'
        finally:A.closeDocument(container.Name)
        # An external XLink reference remains native-owned. Validation only checks
        # proxy identity, without imposing local authoring restrictions.
        external=A.newDocument('ExternalSource')
        try:
            source=external.addObject('Part::Box','ExternalBox');external.recompute()
            external.saveAs(str(output/'External.FCStd'))
            original=d.Joint.Reference1
            d.Joint.Reference1=(source,['',''])
            adapter.validate_proxies(d)
            d.Joint.Reference1=original
        finally:A.closeDocument(external.Name)
        (output/'native-contract.json').write_text(json.dumps(dict(passed=True,
            joint_types=list(native.JointTypes),subclasses_accepted=True,external_reference_not_rejected=True,
            native_python_container_checked=True,
            custom_nonjoint_objects_deferred=True,freecad_version=list(A.Version()),occt=Part.OCC_VERSION,
            joint_module=native.__file__),indent=2))
    finally:A.closeDocument(d.Name)
    for label,names in [('missing_joint',['Joint']),('missing_ground',['Ground']),('missing_both',['Joint','Ground'])]:
        d=A.openDocument(str(output/'healthy.FCStd'))
        try:
            for name in names:d.getObject(name).Proxy=None
            d.saveAs(str(output/(label+'.FCStd')))
        finally:A.closeDocument(d.Name)


def check_native(path, output):
    import FreeCAD as A
    import JointObject as native
    d=A.openDocument(str(path))
    try:
        joint=isinstance(d.Joint.Proxy,native.Joint);ground=isinstance(d.Ground.Proxy,native.GroundedJoint)
        result=dict(joint_is_native=joint,ground_is_native=ground,
            joint_proxy_none=d.Joint.Proxy is None,ground_proxy_none=d.Ground.Proxy is None,
            reference1=[d.Joint.Reference1[0].Name,list(d.Joint.Reference1[1])],
            reference2=[d.Joint.Reference2[0].Name,list(d.Joint.Reference2[1])],
            expressions=list(d.Joint.ExpressionEngine),offset1_z=d.Joint.Offset1.Base.z)
        Path(output).write_text(json.dumps(result,indent=2))
    finally:A.closeDocument(d.Name)


def worker():
    import FreeCAD as A
    backend=Path(os.environ['PROXY_TEST_BACKEND'])
    sys.path.insert(0,str(backend))
    mode=os.environ.get('PROXY_TEST_MODE','healthy')
    if mode=='editor_failed_open':
        import importlib.abc
        class Block(importlib.abc.MetaPathFinder):
            def find_spec(self,fullname,path=None,target=None):
                if fullname=='JointObject':
                    raise ModuleNotFoundError('R2 injected unavailable shipped JointObject')
        sys.meta_path.insert(0,Block())
    if mode in ['partial_joint','partial_ground']:
        import JointObject as native
        cls=native.Joint if mode=='partial_joint' else native.GroundedJoint
        def failed_restore(self,obj):
            obj.Proxy=None
        cls.onDocumentRestored=failed_restore
    module=runpy.run_path(str(backend/'freecad_worker.py'),run_name='proxy_test_worker')
    original=module['dispatch'];g=original.__globals__
    diagnostic=Path(os.environ['PROXY_TEST_DIAGNOSTIC'])
    trigger=Path(os.environ['PROXY_TEST_TRIGGER'])
    def dispatch(request):
        failure=None
        if (mode=='editor_failed_open' and request.get('op')=='open'
                and Path(request.get('path','')).name=='missing.FCStd'):
            request=dict(request,path=os.environ['PROXY_EDITOR_TARGET'])
        if trigger.exists():
            fault=json.loads(trigger.read_text());trigger.unlink()
            A.getDocument(fault['document']).getObject(fault['object']).Proxy=None
        try:
            if mode=='open_raises' and request.get('op')=='open':
                open_document=A.openDocument
                def fail_after_create(path):
                    open_document(path)
                    raise RuntimeError('Injected native open failure after document creation')
                A.openDocument=fail_after_create
                try:return original(request)
                finally:A.openDocument=open_document
            return original(request)
        except Exception as error:
            failure=dict(code=getattr(error,'code',None),message=str(error))
            raise
        finally:
            states=[]
            for doc in A.listDocuments().values():
                states.append(dict(name=doc.Name,path=doc.FileName,touched=doc.isTouched(),
                    undo=doc.UndoCount,redo=doc.RedoCount,
                    proxies={o.Name:type(o.Proxy).__name__ for o in doc.Objects if o.TypeId=='App::FeaturePython'}))
            with diagnostic.open('a',encoding='utf-8') as log:
                log.write(json.dumps(dict(op=request.get('op'),documents=states,
                    active=A.ActiveDocument.Name if A.ActiveDocument else None,
                    error=failure,
                    joint_module_loaded='JointObject' in sys.modules))+'\n')
    g['dispatch']=dispatch
    module['main']()


if __name__=='__main__':
    if len(sys.argv)>1 and sys.argv[1]=='--fixtures':fixtures(Path(sys.argv[2]),Path(sys.argv[3]))
    elif len(sys.argv)>1 and sys.argv[1]=='--check-native':check_native(sys.argv[2],sys.argv[3])
    else:worker()
