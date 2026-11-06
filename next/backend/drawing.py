"""Private, bounded native TechDraw operations. No general feature admission."""
import hashlib
import math
import os
from pathlib import Path
import xml.etree.ElementTree as ET
import zipfile
import drawing_admission as Admission

INITIALIZATION_ERROR=None

def initialize(doc):
    global INITIALIZATION_ERROR
    try:
        Admission.initialize(doc)
        INITIALIZATION_ERROR=None
    except Exception as error:
        Admission.READY=False
        INITIALIZATION_ERROR=str(error)
        # Only this optional feature is unavailable; ordinary startup stays usable.

admit_file = Admission.restore_inputs

TYPES = {'TechDraw::DrawPage', 'TechDraw::DrawSVGTemplate', 'TechDraw::DrawViewPart'}
OPERATIONS = {'drawing_create_page', 'drawing_insert_view', 'drawing_layout'}
TEMPLATE_ID = 'ANSI-A-ASME-landscape-v1'
TEMPLATE_SHA = '0677f004af63c4be28e32a9bcd675542cab58d7a2878624b80cf3c38e4fba5ee'
# Registry is installation-owned. No request can supply or amend this path.
TEMPLATE = Path(__file__).resolve().parents[2] / 'templates' / 'ANSIA_Landscape.svg'
AXES = {'Front': ((0,-1,0),(1,0,0)), 'Right': ((1,0,0),(0,1,0)),
        'Top': ((0,0,1),(1,0,0)), 'Iso': ((1,-1,1),(1,1,0))}


def capabilities():
    return dict(version=3, restore_field="drawing_restore", restore_value="bounded-v1", ordinary_restore="legacy_unqualified_for_drawing_security", available=Admission.READY, initialization_error=INITIALIZATION_ERROR, template_sharing=False, x_source=False, schema_preflight=True, protected_transitive_restore=True, templates=[dict(id=TEMPLATE_ID, sha256=TEMPLATE_SHA,
                paper_mm=[279.4,215.9])], views=list(AXES), projection='Third angle',
                position_unit='mm', origin='lower_left', scale='dimensionless',
                svg='native_per_view_fragment', whole_page_svg=False, shaded=False,
                shaded_test_needed=True, native_projection_group=False)


def checked_xml(data):
    if b'<!DOCTYPE' in data.upper() or b'<!ENTITY' in data.upper():
        raise ValueError('XML entities and DTDs are forbidden')
    return ET.fromstring(data)


def registry():
    data = TEMPLATE.read_bytes()
    if hashlib.sha256(data).hexdigest() != TEMPLATE_SHA:
        raise ValueError('Registered drawing template hash changed')
    checked_xml(data)
    return str(TEMPLATE)


def check_file(path):
    Admission.require(Admission.READY, 'Native drawing schema prototypes are not verified')
    Admission.serialized(Path(path).read_bytes())


def quantity(q):
    return dict(value=q.Value, unit=str(q.Unit), signature=list(q.Unit.Signature))


def identity(obj):
    return dict(object=obj.Name, native_id=obj.ID, document_uid=str(obj.Document.Uid))


def ref(doc, value, type_id, fail):
    if not isinstance(value, dict) or set(value) != {'object','native_id','document_uid'}:
        fail('stale_reference', 'Expected a current native drawing identity')
    obj = doc.getObject(value['object']) if isinstance(value['object'],str) else None
    if (obj is None or obj.TypeId != type_id or type(value['native_id']) is not int
            or obj.ID != value['native_id'] or str(doc.Uid) != value['document_uid']):
        fail('stale_reference', 'Native drawing identity changed')
    return obj


def scalar(value, low, high, fail):
    if type(value) not in (int,float) or not math.isfinite(value) or not low <= value <= high:
        fail('invalid_request', 'Drawing number is nonfinite or outside the supported range')
    return value


def position(value, fail):
    if not isinstance(value,list) or len(value) != 2:
        fail('invalid_request','Drawing position requires X/Y in mm')
    return [scalar(value[0],0,279.4,fail),scalar(value[1],0,215.9,fail)]


def keys(value, required, optional, fail):
    if not isinstance(value,dict) or not required <= value.keys() or value.keys() - required - optional:
        fail('invalid_request','Unknown or missing drawing request fields')


def preflight(doc, edits, w):
    selected = [e for e in edits if isinstance(e,dict) and e.get('op') in OPERATIONS]
    # Generic property writes/removals must also fail before history/preview abort.
    for e in edits:
        if not isinstance(e,dict):
            continue
        obj = doc.getObject(e.get('object')) if isinstance(e.get('object'),str) else None
        if obj is not None and obj.TypeId in TYPES and e.get('op') not in OPERATIONS:
            w.fail('read_only_property','Use dedicated typed drawing operations')
        if e.get('op') == 'add_object' and e.get('type') in TYPES:
            w.fail('unsupported_object_type','Use dedicated typed drawing operations')
    if not selected:
        return
    if not Admission.READY:w.fail('drawing_unavailable',INITIALIZATION_ERROR or 'Trusted drawing persistence schemas are unavailable')
    validate(doc,w.fail)
    if len(selected) != 1 or len(edits) != 1:
        w.fail('invalid_request','Drawing edits require one dedicated edit per request')
    e=selected[0]; op=e['op']
    if op == 'drawing_create_page':
        keys(e,{'op','name','template_id','source','scale','projection'},set(),w.fail)
        if e['template_id'] != TEMPLATE_ID:
            w.fail('untrusted_template','Choose a registered drawing template')
        try: registry()
        except (OSError,ValueError) as error: w.fail('untrusted_template',str(error))
        w.name(e['name']); w.name(e['name']+'Template')
        if doc.getObject(e['name']) or doc.getObject(e['name']+'Template'):
            w.fail('name_conflict','Drawing page or template already exists')
        ref(doc,e['source'],'Assembly::AssemblyObject',w.fail)
        scalar(e['scale'],1e-6,1000,w.fail)
        if e['projection'] != 'Third angle':
            w.fail('invalid_request','Only Third angle projection is supported')
    else:
        required = {'op','page','source'}
        required |= {'name','orientation','position_mm'} if op == 'drawing_insert_view' else {'views','scale','projection'}
        keys(e,required,set(),w.fail)
        p=ref(doc,e['page'],'TechDraw::DrawPage',w.fail)
        s=ref(doc,e['source'],'Assembly::AssemblyObject',w.fail)
        if not p.Template or p.Template.TypeId != 'TechDraw::DrawSVGTemplate':
            w.fail('untrusted_template','Drawing page has no registered native template')
        if op == 'drawing_insert_view':
            w.name(e['name'])
            if doc.getObject(e['name']): w.fail('name_conflict','Drawing view already exists')
            if e['orientation'] not in AXES: w.fail('invalid_request','Unsupported drawing orientation')
            position(e['position_mm'],w.fail)
        else:
            scalar(e['scale'],1e-6,1000,w.fail)
            if e['projection'] != 'Third angle': w.fail('invalid_request','Only Third angle is supported')
            if not isinstance(e['views'],list) or not e['views']: w.fail('invalid_request','Layout needs views')
            seen=set()
            for item in e['views']:
                keys(item,{'view','position_mm'},set(),w.fail)
                v=ref(doc,item['view'],'TechDraw::DrawViewPart',w.fail)
                if v not in p.Views or v.ID in seen or v.Source != [s]:
                    w.fail('stale_reference','Drawing view belongs to another page/source or is repeated')
                seen.add(v.ID); position(item['position_mm'],w.fail)
        if p.DrawingSource != s or any(v.Source != [s] for v in p.Views):
            w.fail('stale_reference','Drawing page source does not match')


def apply(doc,e,w):
    op=e['op']
    if op=='drawing_create_page':
        import TechDraw
        p=doc.addObject('TechDraw::DrawPage',e['name'])
        t=doc.addObject('TechDraw::DrawSVGTemplate',e['name']+'Template')
        t.Template=registry(); p.Template=t; p.Scale=e['scale']; p.ProjectionType=e['projection']
        p.addProperty('App::PropertyLink','DrawingSource','Drawing'); p.DrawingSource=doc.getObject(e['source']['object'])
        fields={k:'' for k in t.EditableTexts}
        fields.update(DrawingTitle2='DIMENSIONS ARE IN INCHES',DrawingTitle3='THIRD ANGLE PROJECTION',Sheet='1 of 1',scale='1:'+format(1/e['scale'], 'g'))
        t.EditableTexts=fields
        # Native PropertyLink also preserves the chosen source on an empty page.
        return dict(page=identity(p),template=identity(t),source=e['source'])
    p=doc.getObject(e['page']['object']); s=doc.getObject(e['source']['object'])
    if op=='drawing_insert_view':
        import TechDraw
        v=doc.addObject('TechDraw::DrawViewPart',e['name']); v.Source=[s]; result=p.addView(v)
        v.ScaleType='Custom'; v.Scale=p.Scale
        direction,xdir=AXES[e['orientation']]; v.Direction=w.App.Vector(*direction); v.XDirection=w.App.Vector(*xdir)
        v.X,v.Y=e['position_mm']; v.Rotation=0
        for prop in ('Perspective','CoarseView','HardHidden','SmoothHidden','SeamHidden','SmoothVisible','SeamVisible','IsoVisible','IsoHidden','LockPosition'):
            setattr(v,prop,False)
        v.XSource=[]; v.IsoCount=0; v.Focus=100; v.ScrubCount=1
        return dict(view=identity(v),page=identity(p),source=identity(s),add_view_result=result)
    p.Scale=e['scale']; p.ProjectionType=e['projection']
    for v in p.Views: v.Scale=e['scale']
    for item in e['views']:
        v=doc.getObject(item['view']['object']); v.X,v.Y=item['position_mm']
    fields=dict(p.Template.EditableTexts); fields['scale']='1:'+format(1/e['scale'], 'g'); p.Template.EditableTexts=fields
    return dict(page=identity(p))


def validate(doc,fail):
    try:
        types={obj.Name:obj.TypeId for obj in doc.Objects}
        records={}
        for obj in doc.Objects:
            if not obj.TypeId.startswith('TechDraw::'): continue
            Admission.require(obj.TypeId in TYPES,'Unqualified native drawing class')
            Admission.require(Admission.schema(obj)==Admission.SCHEMAS[obj.TypeId],'Native drawing property schema differs')
            Admission.require(not obj.ExpressionEngine,'Drawing expressions are forbidden')
            r={}
            for key,typ in Admission.SCHEMAS[obj.TypeId].items():
                value=getattr(obj,key)
                if typ=='App::PropertyLink':
                    Admission.require(value is not None and value.Document==doc,'Drawing link must be local')
                    r[key]=value.Name
                elif typ in ('App::PropertyLinkList','App::PropertyXLinkList'):
                    Admission.require(all(v.Document==doc for v in value),'Drawing source/view links must be local')
                    r[key]=[v.Name for v in value]
                elif typ=='App::PropertyVector':r[key]=[value.x,value.y,value.z]
                elif typ in ('App::PropertyAngle','App::PropertyDistance','App::PropertyLength'):r[key]=value.Value
                elif key in Admission.EMPTY:r[key]=list(value)
                else:r[key]=value
            if obj.TypeId=='TechDraw::DrawSVGTemplate':
                Admission.require(Admission.canonical(obj.Template)==Admission.canonical(registry()),'Unregistered native template path')
                Admission.require(hashlib.sha256(Path(obj.PageResult).read_bytes()).hexdigest()==TEMPLATE_SHA,'Unregistered included template bytes')
                Admission.require(obj.Width.Value==279.4 and obj.Height.Value==215.9,'Unsupported native paper dimensions')
            records[obj.Name]=r
        Admission.configuration(types,records)
    except (ValueError,OSError,KeyError,TypeError,AttributeError) as error:
        fail('drawing_contract',str(error))


def metadata(doc):
    result=[]
    for obj in doc.Objects:
        if obj.TypeId not in TYPES: continue
        item=dict(identity=identity(obj),type=obj.TypeId)
        if obj.TypeId=='TechDraw::DrawPage':
            item.update(source=identity(obj.DrawingSource),template=identity(obj.Template),views=[identity(v) for v in obj.Views],scale=obj.Scale,projection=obj.ProjectionType)
        elif obj.TypeId=='TechDraw::DrawSVGTemplate':
            item.update(template_id=TEMPLATE_ID,sha256=TEMPLATE_SHA,width=quantity(obj.Width),height=quantity(obj.Height),editable_fields=dict(obj.EditableTexts))
        else:
            item.update(source=[identity(s) for s in obj.Source],x_source=[identity(s) for s in obj.XSource],scale=obj.Scale,scale_type=obj.ScaleType,
                        x=quantity(obj.X),y=quantity(obj.Y),rotation=quantity(obj.Rotation),
                        direction=[obj.Direction.x,obj.Direction.y,obj.Direction.z],
                        x_direction=[obj.XDirection.x,obj.XDirection.y,obj.XDirection.z])
        result.append(item)
    return dict(document_uid=str(doc.Uid),objects=result)


def export(doc,request,w):
    # Existing export_exchange read/write lease semantics; no new host operation.
    keys(request,{'version','id','op','document','expected_revision','format','page','view','source','path'},set(),w.fail)
    p=ref(doc,request['page'],'TechDraw::DrawPage',w.fail)
    v=ref(doc,request['view'],'TechDraw::DrawViewPart',w.fail)
    s=ref(doc,request['source'],'Assembly::AssemblyObject',w.fail)
    if v not in p.Views or v.Source != [s]: w.fail('stale_reference','Wrong page or source')
    value=request['path']
    if not isinstance(value,str) or not value or '\0' in value: w.fail('invalid_request','Choose a new SVG export path')
    path=Path(value)
    if not path.is_absolute() or path.suffix.lower()!='.svg' or not path.parent.is_dir() or path.parent.is_symlink(): w.fail('invalid_request','Choose an absolute new SVG file in an existing directory')
    if path.exists() or path.is_symlink(): w.fail('file_exists','SVG export requires a new exclusive destination')
    import TechDraw
    fragment=TechDraw.viewPartAsSvg(v)
    # Preserve native bytes. Wrapper is a per-view sheet-positioned SVG, not a whole-page exporter.
    checked_xml(('<svg xmlns="http://www.w3.org/2000/svg">'+fragment+'</svg>').encode())
    lower=fragment.lower()
    if any(x in lower for x in ('href','url(','<script','<foreignobject','<image','<!','<style',' on')):
        w.fail('unsafe_svg','Native fragment contains unsupported active/external content')
    svg=('<svg xmlns="http://www.w3.org/2000/svg" width="279.4mm" height="215.9mm" viewBox="0 0 279.4 215.9"><g transform="translate(%s %s)">%s</g></svg>'%(v.X.Value,215.9-v.Y.Value,fragment))
    data=svg.encode('utf8')
    try:
        with path.open('xb') as f: f.write(data)
    except FileExistsError:
        w.fail('file_exists','SVG destination was created by another writer')
    return dict(path=str(path),format='SVG',kind='native_per_view_sheet_positioned',svg=svg,
                fragment_sha256=hashlib.sha256(fragment.encode()).hexdigest(),bytes=len(data),
                coordinate_contract=dict(unit='mm',viewBox=[0,0,279.4,215.9],native_origin='lower_left',svg_origin='upper_left',native_fragment_y='down'))
