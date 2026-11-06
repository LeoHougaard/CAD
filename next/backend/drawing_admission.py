import rectangular_draft as RectangularDraft
"""Bounded TechDraw serialized/runtime contract and protected restore inputs."""
from contextlib import contextmanager, ExitStack, nullcontext
from contextvars import ContextVar
import drawing_persistence as Persistence
import ctypes
import hashlib
import io
import json
import math
import os
from pathlib import Path
import stat
import xml.etree.ElementTree as ET
import zipfile

SCHEMAS = json.loads(Path(__file__).with_name('drawing_schema.json').read_bytes())
ENUMS = json.loads(Path(__file__).with_name('drawing_enums.json').read_bytes())
TEMPLATE = Path(__file__).resolve().parents[2] / 'templates/ANSIA_Landscape.svg'
TEMPLATE_SHA = '0677f004af63c4be28e32a9bcd675542cab58d7a2878624b80cf3c38e4fba5ee'
AXES = {'Front': ((0,-1,0),(1,0,0)), 'Right': ((1,0,0),(0,1,0)),
        'Top': ((0,0,1),(1,0,0)), 'Iso': ((1,-1,1),(1,1,0))}
FLAGS = ('Perspective','CoarseView','HardHidden','SmoothHidden','SeamHidden',
         'SmoothVisible','SeamVisible','IsoVisible','IsoHidden','LockPosition')
EMPTY = ('CenterLines','CosmeticEdges','CosmeticVertexes','GeomFormats')
READY = False
_ROUTE = ContextVar("drawing_restore_route",default=False)


def require(test, message):
    if not test:
        raise ValueError(message)


def xml(data):
    require(len(data) <= 16*1024*1024, 'Drawing XML size limit exceeded')
    require(b'<!DOCTYPE' not in data.upper() and b'<!ENTITY' not in data.upper(), 'DTD/entities forbidden')
    return ET.fromstring(data)


def schema(obj):
    return {p:obj.getTypeIdOfProperty(p) for p in obj.PropertiesList}


def initialize(doc):
    """Called in the existing trusted schema scratch document, never by Open."""
    global READY
    READY=False
    Persistence.CONTRACTS.clear()
    import TechDraw
    for typ, expected in SCHEMAS.items():
        obj=doc.addObject(typ,'DrawingPrototype')
        if typ=='TechDraw::DrawPage': obj.addProperty('App::PropertyLink','DrawingSource','Drawing')
        require(obj.TypeId==typ and schema(obj)==expected, 'Trusted native drawing prototype schema differs: '+typ)
        for prop,choices in ENUMS[typ].items():
            require(list(obj.getEnumerationsOfProperty(prop))==choices,'Native drawing enumeration differs: '+prop)
        owned_names=[obj.Name]
        try:
            if typ=='TechDraw::DrawViewPart':
                # DrawView::onChanged(ScaleType) returns early without a parent.
                # Learn the supported Custom contract in its real page context.
                parent=doc.addObject('TechDraw::DrawPage','DrawingPrototypePage')
                owned_names.append(parent.Name)
                require(hashlib.sha256(TEMPLATE.read_bytes()).hexdigest()==TEMPLATE_SHA,'Trusted scratch template bytes changed')
                template=doc.addObject('TechDraw::DrawSVGTemplate','DrawingPrototypeTemplate')
                owned_names.append(template.Name)
                template.Template=str(TEMPLATE)
                parent.Template=template
                parent.addView(obj)
                obj.ScaleType='Page'
                obj.ScaleType='Custom'
                require('ReadOnly' not in obj.getPropertyStatus('Scale'),'Trusted Custom view scale must be writable')
            Persistence.learn(obj)
            require(set(Persistence.CONTRACTS[typ]['normal'])==set(expected),'Current drawing contract requires all captured properties persisted')
        finally:
            # Native page removal may also delete its view and template wrappers.
            for name in reversed(owned_names):
                if doc.getObject(name) is not None:doc.removeObject(name)
    READY=True


def finite(value,low,high):
    require(type(value) in (int,float) and math.isfinite(value) and low<=value<=high,'Invalid drawing numeric range')


def configuration(types, records):
    """Same pure semantic predicates for serialized and live native state."""
    pages={n:r for n,r in records.items() if types[n]=='TechDraw::DrawPage'}
    templates={n:r for n,r in records.items() if types[n]=='TechDraw::DrawSVGTemplate'}
    views={n:r for n,r in records.items() if types[n]=='TechDraw::DrawViewPart'}
    used_templates=[]; used_views=[]
    for n,p in pages.items():
        require(p['Template'] in templates,'Template must belong to the same document')
        used_templates.append(p['Template'])
        require(types.get(p['DrawingSource'])=='Assembly::AssemblyObject','Page source must be a local native Assembly')
        finite(p['Scale'],1e-6,1000)
        require(p['ProjectionType']=='Third angle' and p['KeepUpdated'] is True,'Unsupported page projection/update mode')
        require(type(p['Views']) is list and len(set(p['Views']))==len(p['Views']),'Duplicate native page view')
        require(templates[p['Template']]['EditableTexts'].get('scale')=='1:'+format(1/p['Scale'],'g'),'Template scale text differs from page')
        for name in p['Views']:
            require(name in views,'Page references a foreign or unsupported view')
            used_views.append(name);v=views[name]
            require(v['Source']==[p['DrawingSource']] and v['XSource']==[],'View source/XSource exceeds local Assembly scope')
            require(v['ScaleType']=='Custom' and v['Scale']==p['Scale'],'View/page scales differ')
    require(len(set(used_templates))==len(used_templates) and set(used_templates)==set(templates),'Templates must be owned by exactly one local page')
    require(len(set(used_views))==len(used_views) and set(used_views)==set(views),'Views must be owned by exactly one local page')
    for v in views.values():
        require((tuple(v['Direction']),tuple(v['XDirection'])) in AXES.values(),'Unaudited projection basis')
        require(all(v[k] is False for k in FLAGS),'Unsupported view projection flags')
        require(v['Rotation']==0 and v['IsoCount']==0 and v['Focus']==100 and v['ScrubCount']==1,'Unsupported native view settings')
        require(all(v[k]==[] for k in EMPTY),'Cosmetic/computed drawing additions are unsupported')
        finite(v['X'],0,279.4);finite(v['Y'],0,215.9)
        finite(v['Scale'],1e-6,1000)


def canonical(path):
    return os.path.normcase(os.path.abspath(path))


def read_member(z,name,limit=64*1024*1024):
    info=z.getinfo(name);require(info.file_size<=limit,'FCStd member exceeds admission limit')
    return z.read(name)


def serialized(data):
    """No native deserialization. Return actual native XLink and descriptor paths."""
    require(len(data)<=64*1024*1024,'FCStd exceeds bounded admission limit')
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        names=z.namelist();require(len(names)==len(set(names)) and len(names)<=4096,'Ambiguous/oversized FCStd archive')
        root=xml(read_member(z,'Document.xml',16*1024*1024))
        declarations=root.findall('./Objects/Object')
        types={o.get('name'):o.get('type') for o in declarations}
        require(len(types)==len(declarations) and all(isinstance(t,str) for t in types.values()),'Ambiguous object declarations')
        drawing={n:t for n,t in types.items() if t.startswith('TechDraw::')}
        require(set(drawing.values())<=SCHEMAS.keys(),'Unqualified native drawing class')
        objects=root.findall('./ObjectData/Object');byname={o.get('name'):o for o in objects}
        require(len(byname)==len(objects),'Duplicate serialized object data')
        records={}
        for n,typ in drawing.items():
            obj=byname[n]
            require(all(c.tag in ('Properties','Extensions') for c in obj),'Unexpected drawing object payload')
            pd=Persistence.properties(obj,typ)
            persisted=Persistence.CONTRACTS[typ]['normal']
            require({k:p.get('type') for k,p in pd.items()}=={k:v['type'] for k,v in persisted.items()},'Unexpected drawing property schema/Python state')
            exts=[(e.get('type'),e.get('name')) for e in obj.findall('./Extensions/Extension')]
            require(exts==([('TechDraw::CosmeticExtension','CosmeticExtension')] if typ=='TechDraw::DrawViewPart' else []),'Unexpected native drawing extension')
            require(all(len(e)==0 for e in obj.findall('./Extensions/Extension')),'Unexpected drawing extension state')
            r={}
            for key,p in pd.items():
                require(len(p)==1,'Malformed native drawing property')
                node=p[0];kind=p.get('type')
                if kind=='App::PropertyExpressionEngine':
                    require(node.tag=='ExpressionEngine' and len(node)==0 and node.get('count')=='0','Drawing expressions forbidden');continue
                if kind=='App::PropertyLink':
                    require(node.tag=='Link' and set(node.attrib)=={'value'},'Only local drawing links are supported');r[key]=node.get('value')
                elif kind=='App::PropertyLinkList':
                    require(node.tag=='LinkList' and int(node.get('count'))==len(node),'Malformed drawing link list')
                    require(all(e.tag=='Link' and set(e.attrib)=={'value'} for e in node),'Only local drawing source/view links are supported');r[key]=[e.get('value') for e in node]
                elif kind=='App::PropertyXLinkList':
                    require(node.tag=='XLinkSubList' and node.get('count')=='0' and len(node)==0,'Nonempty XSource forbidden');r[key]=[]
                elif kind=='App::PropertyBool': require(node.tag=='Bool' and node.get('value') in ('true','false'),'Malformed Bool');r[key]=node.get('value')=='true'
                elif kind in ('App::PropertyFloatConstraint','App::PropertyLength','App::PropertyDistance','App::PropertyAngle'):
                    require(node.tag=='Float','Malformed numeric drawing property');r[key]=float(node.get('value'))
                elif kind in ('App::PropertyInteger','App::PropertyEnumeration'):
                    require(node.tag=='Integer','Malformed integer drawing property');r[key]=int(node.get('value'))
                elif kind=='App::PropertyVector':
                    require(node.tag=='PropertyVector','Malformed projection vector');r[key]=[float(node.get('value'+a)) for a in 'XYZ']
                elif key in EMPTY:
                    require(len(node)==0 and node.get('count')=='0','Computed/cosmetic drawing content forbidden');r[key]=[]
                elif kind=='App::PropertyMap':
                    require(node.tag=='Map' and int(node.get('count'))==len(node),'Malformed template field map')
                    r[key]={e.get('key'):e.get('value') for e in node};require(len(r[key])==len(node),'Duplicate template fields')
                elif kind in ('App::PropertyString','App::PropertyFile'):
                    require(node.tag=='String','Malformed drawing string');r[key]=node.get('value')
                elif kind=='App::PropertyFileIncluded':
                    require(node.tag=='FileIncluded' and node.get('file')=='ANSIA_Landscape.svg','Unregistered drawing resource')
                    require(hashlib.sha256(read_member(z,node.get('file'))).hexdigest()==TEMPLATE_SHA,'Included SVG bytes differ');r[key]=node.get('file')
                else: raise ValueError('Unparsed drawing property: '+key)
            if typ=='TechDraw::DrawPage':require(r['ProjectionType']==1,'Only Third angle supported');r['ProjectionType']='Third angle'
            if typ=='TechDraw::DrawViewPart':require(r['ScaleType']==2,'Only Custom scale supported');r['ScaleType']='Custom'
            if typ=='TechDraw::DrawSVGTemplate':
                require(canonical(r['Template'])==canonical(TEMPLATE),'Unregistered external template path')
                require(r['Width']==279.4 and r['Height']==215.9 and r['Orientation']==1,'Unregistered template dimensions/orientation')
            records[n]=r
        configuration(types,records)
        paths=[]
        for node in root.iter():
            if node.tag.startswith('XLink') and node.get('file'):
                require(node.tag=='XLink' and bool(node.get('name')),'Unsupported external native link serialization')
                paths.append(node.get('file'))
        for p in root.findall('.//Property[@name="CadExternalSource"]'):
            require(p.get('type')=='App::PropertyString','Malformed separately qualified Assembly descriptor')
            value=json.loads(p.find('String').get('value'))
            paths.extend(r['path'] for r in [value,*value['dependencies']])
        return paths,bool(drawing)


@contextmanager
def hold(path,directory=False):
    """Windows mandatory read sharing: deny writers and replacement while restoring."""
    require(os.name=='nt','Protected native restore currently requires Windows file sharing')
    k=ctypes.WinDLL('kernel32',use_last_error=True)
    k.CreateFileW.argtypes=[ctypes.c_wchar_p,ctypes.c_uint32,ctypes.c_uint32,ctypes.c_void_p,ctypes.c_uint32,ctypes.c_uint32,ctypes.c_void_p];k.CreateFileW.restype=ctypes.c_void_p
    k.CloseHandle.argtypes=[ctypes.c_void_p];k.CloseHandle.restype=ctypes.c_int
    handle=k.CreateFileW(str(path),0x80 if directory else 0x80000000,1,None,3,0x02000000 if directory else 0,None)
    if handle==ctypes.c_void_p(-1).value:raise OSError(ctypes.get_last_error(),'Cannot protect restore input',str(path))
    try:yield
    finally:require(bool(k.CloseHandle(handle)),'Restore input handle close failed')


@contextmanager
def admit(path):
    """Protect and check the whole actual transitive input set before ANY Open."""
    require(READY,'Native constructor prototype schemas have not been verified')
    files={};pending=[Path(path).absolute()];dirs=set();drawing=False
    with ExitStack() as stack:
        # Protect each ancestor against rename/reparse replacement. Refuse reparse traversal.
        def protect(p):
            for parent in reversed(p.parents):
                if canonical(parent) in dirs:continue
                stack.enter_context(hold(parent,True));dirs.add(canonical(parent))
                require(not (parent.stat().st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT),'Reparse ancestor forbidden for restore inputs')
            stack.enter_context(hold(p))
            require(not (p.stat().st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT),'Reparse restore input forbidden')
        while pending:
            p=pending.pop();key=canonical(p)
            if key in files:continue
            require(len(files)<64 and p.suffix.lower()=='.fcstd','Bounded native dependency graph exceeded')
            protect(p);require(p.stat().st_size<=64*1024*1024,'FCStd exceeds admission byte limit')
            data=p.read_bytes();links,hasdrawing=serialized(data);drawing|=hasdrawing
            before=hashlib.sha256(data).hexdigest()
            RectangularDraft.serialized_preflight(p)
            require(hashlib.sha256(p.read_bytes()).hexdigest()==before,"Rectangular preflight input changed")
            files[key]=before
            for value in links:
                target=Path(value);pending.append((target if target.is_absolute() else p.parent/target).absolute())
        if drawing:
            protect(TEMPLATE)
            require(hashlib.sha256(TEMPLATE.read_bytes()).hexdigest()==TEMPLATE_SHA,'External registered template bytes changed')
        # All files stay locked through root/dependency prepare, native Open and complete.
        yield files
        require(all(hashlib.sha256(Path(p).read_bytes()).hexdigest()==h for p,h in files.items()),'Restore input bytes changed')


@contextmanager
def request_route(request):
    value=request.get('drawing_restore')
    require(value is None or value=='bounded-v1','Unknown drawing restore route')
    require(value is None or request.get('op') in ('open','assembly_sources','transaction','preview'),'Drawing restore route is not supported by this operation')
    token=_ROUTE.set(value=='bounded-v1')
    try:yield
    finally:_ROUTE.reset(token)


def route_name():
    return 'bounded-v1' if _ROUTE.get() else 'legacy'


def restore_inputs(path):
    # No graph scan, platform requirement, size limit or lock on the legacy route.
    # Legacy Open is not a security-certified drawing restore.
    return admit(path) if _ROUTE.get() else nullcontext(None)
