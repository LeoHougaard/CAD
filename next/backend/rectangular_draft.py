"""Private bounded New/Blind rectangle feature. Native imports occur only at use."""
import base64
import json
import math
import re
import zipfile
import xml.etree.ElementTree as ET

OPS = ('create_rectangular_draft', 'update_rectangular_draft')
GROUP = 'Rectangular draft'
BASE_SCHEMA = None
SOURCE_SCHEMA = None
BODY_SCHEMA = None
ORIGIN_SCHEMA = None
ORIGIN_FEATURE_SCHEMAS = None
MAP_MODES = None
PERSISTENCE = {}
DOWNSTREAM_SCHEMAS = {}

def initialize_schema(doc, trusted_types=()):
    global BASE_SCHEMA, SOURCE_SCHEMA, BODY_SCHEMA, ORIGIN_SCHEMA, ORIGIN_FEATURE_SCHEMAS, MAP_MODES
    require(BASE_SCHEMA is None,'Base schema may only be initialized once')
    obj=doc.addObject('PartDesign::FeaturePython','RectangularDraftSchema')
    BASE_SCHEMA=property_schema(obj)
    learn_persistence(obj)
    doc.removeObject(obj.Name)
    body=doc.addObject('PartDesign::Body','RectangularDraftBodySchema')
    source=body.newObject('Sketcher::SketchObject','RectangularDraftSourceSchema')
    SOURCE_SCHEMA=property_schema(source)
    BODY_SCHEMA=property_schema(body)
    ORIGIN_SCHEMA=(body.Origin.TypeId,property_schema(body.Origin))
    ORIGIN_FEATURE_SCHEMAS={str(f.Role):(f.TypeId,property_schema(f)) for f in body.Origin.OriginFeatures}
    require(set(ORIGIN_FEATURE_SCHEMAS)=={'X_Axis','Y_Axis','Z_Axis','XY_Plane','XZ_Plane','YZ_Plane','Origin'},'Trusted native origin roles changed')
    MAP_MODES=list(source.getEnumerationsOfProperty('MapMode'))
    for prototype in (source,body,body.Origin,*body.Origin.OriginFeatures):learn_persistence(prototype)
    # Only stock constructors already admitted by the worker can extend history.
    for native_type in sorted(trusted_types):
        if native_type != 'Sketcher::SketchObject' and not native_type.startswith('PartDesign::'):
            continue
        if native_type in ('PartDesign::Body','PartDesign::FeaturePython'):
            continue
        prototype=body.newObject(native_type,'RectangularDownstreamSchema')
        schema=property_schema(prototype)
        if (prototype.isDerivedFrom('PartDesign::Feature') or prototype.isDerivedFrom('App::DatumElement')
                or prototype.isDerivedFrom('App::LocalCoordinateSystem') or native_type in ('Sketcher::SketchObject','PartDesign::Plane','PartDesign::Line','PartDesign::Point','PartDesign::CoordinateSystem')):
            require('App::PropertyPythonObject' not in schema.values(),'Native downstream constructor contains Python state')
            DOWNSTREAM_SCHEMAS[native_type]=schema
            learn_persistence(prototype)
        doc.removeObject(prototype.Name)
    doc.removeObject(source.Name)
    doc.removeObject(body.Name)

SCHEMA = {'Profile':'App::PropertyLinkSub','DraftBody':'App::PropertyLinkHidden',
          'Length':'App::PropertyLength','TaperAngle':'App::PropertyAngle',
          'Reversed':'App::PropertyBool','DraftVersion':'App::PropertyInteger',
          'DraftIdentity':'App::PropertyString'}
# Pinned OCCT Precision::Confusion and Precision::Angular, in mm/radians.
CONFUSION = 1e-7
ANGULAR = 1e-12

class DraftError(ValueError):
    code = 'rectangular_draft_unsupported'

def require(ok, message):
    if not ok:
        raise DraftError(message)

def strict_json(value):
    def pairs(items):
        result={}
        for k,v in items:
            require(k not in result,'Duplicate serialized JSON key')
            result[k]=v
        return result
    return json.loads(value,object_pairs_hook=pairs,parse_constant=lambda x:require(False,'Nonfinite saved JSON'))

def state(value):
    require(type(value) is dict and set(value)=={'version'} and type(value['version']) is int and value['version']==1,
            'Only stateless rectangular draft JSON version 1 is supported')

class RectangularDraftV1:
    __slots__ = ()
    def dumps(self):
        return {'version':1}
    def loads(self, value):
        state(value)
    def onDocumentRestored(self, obj):
        live(obj)
    def execute(self, obj):
        import Part
        import FreeCAD as App
        source, body, identity = live(obj)
        p,u,v,W,H = rectangle(source)
        L,A,R = scalars(obj.Length.Value,obj.TaperAngle.Value,obj.Reversed,W,H)
        # Exactly one source-global to feature-local transform.
        frame=obj.getGlobalPlacement().inverse().multiply(source.getGlobalPlacement())
        n=u.cross(v)
        if R:n=-n
        point=lambda x:frame.multVec(x)
        direction=lambda x:frame.Rotation.multVec(x)
        def face(points):
            return Part.Face(Part.makePolygon([point(x) for x in [*points,points[0]]]))
        wire=source.Shape.Wires[0].copy()
        wire.Placement=source.Placement.inverse().multiply(wire.Placement)
        wire.Placement=frame.multiply(wire.Placement)
        original=Part.Face(wire)
        if A==0:
            result=original.extrude(direction(L*n))
        elif A<0:
            t=math.tan(math.radians(-A))
            # A full roof is bounded by aspect ratio, checked before history.
            X=face([p,p+W*u,p+W/2*u+W/(2*t)*n]).extrude(direction(H*v))
            Y=face([p,p+H*v,p+H/2*v+H/(2*t)*n]).extrude(direction(W*u))
            result=X.common(Y).common(original.extrude(direction(L*n)))
        else:
            d=L*math.tan(math.radians(A))
            top=Part.makePolygon([point(x+L*n) for x in [p-d*u-d*v,p+(W+d)*u-d*v,p+(W+d)*u+(H+d)*v,p-d*u+(H+d)*v,p-d*u-d*v]])
            result=Part.makeLoft([wire,top],True,True)
        require(not result.isNull() and result.isValid() and len(result.Solids)==1,'Native rectangular draft did not produce one valid solid')
        obj.Shape=result

def scalars(L,A,R,W,H):
    require(all(type(x) in (int,float) and math.isfinite(x) for x in (L,A,W,H)) and type(R) is bool,'Finite native quantities and Boolean Reversed required')
    require(min(L,W,H)>CONFUSION and abs(A)<90,'Positive kernel-resolvable dimensions and absolute angle below 90 required')
    if A!=0:
        t=abs(math.tan(math.radians(A)))
        require(abs(math.radians(A))>ANGULAR,'Nonzero angle is below pinned OCCT angular resolution')
        # Bound all roof slopes and outward side lengths by Precision::Angular.
        require(t>=math.sqrt(ANGULAR) and t<=1/math.sqrt(ANGULAR),'Draft slope outside bounded v1 kernel conditioning domain')
        require(L*t>CONFUSION,'Draft displacement below pinned OCCT linear resolution')
    return L,A,R

def constraint_rows(source):
    keys=('Type','First','FirstPos','Second','SecondPos','Third','ThirdPos','Driving','IsActive')
    if len(source.Constraints)==11:keys+=('Name','InVirtualSpace')
    return [{k:getattr(c,k) for k in keys} for c in source.Constraints]

def dimension_authority(rows):
    """Classify recorded native selectors, never source labels or dimension values."""
    if len(rows)==12:
        # Preserve the qualified controlled family's admission and saved identity.
        require([r['Type'] for r in rows]==['Coincident']*4+['Horizontal','Vertical','Horizontal','Vertical','DistanceX','DistanceY','Distance','Distance'], 'Unsupported controlled rectangle constraints')
        expected=[(i,2,(i+1)%4,1) for i in range(4)]
        require(all((r['First'],r['FirstPos'],r['Second'],r['SecondPos'])==e for r,e in zip(rows,expected)),'Original coincidence identities changed')
        require([r['First'] for r in rows[4:8]]==list(range(4)) and [r['First'] for r in rows[10:]]==[0,1] and all(r['Driving'] and r['IsActive'] for r in rows),'Original dimensional/axis constraints required')
        return {10:'width',11:'height'}
    require(len(rows)==11,'Unsupported rectangle source family')
    keys=('Type','First','FirstPos','Second','SecondPos','Third','ThirdPos','Driving','IsActive')
    expected=[('Coincident',i,2,(i+1)%4,1,-2000,0,True,True) for i in range(4)]
    expected += [(kind,i,0,-2000,0,-2000,0,True,True) for i,kind in enumerate(('Horizontal','Vertical','Horizontal','Vertical'))]
    expected += [('Symmetric',0,1,2,1,-1,1,True,True)]
    require(all(type(r) is dict and set(r)==set(keys)|{'Name','InVirtualSpace'} and all(type(r[k]) is int for k in keys[1:7]) and type(r['Driving']) is bool and type(r['IsActive']) is bool and type(r['Name']) is str and r['InVirtualSpace'] is False for r in rows),'Unsupported origin-symmetric rectangle selectors or flags')
    require(all(tuple(r[k] for k in keys)==e for r,e in zip(rows[:9],expected)),'Unsupported origin-symmetric rectangle topology constraints')
    # Both edge lengths and distances between opposite parallel edges express
    # the same width/height. Classify the native selectors, not a particular
    # mouse-selection recipe. Original records and their identities are kept.
    axes={i:('width' if i%2==0 else 'height') for i in range(4)}
    dimensions={}
    for index,r in enumerate(rows[9:],9):
        require(r['Type']=='Distance' and r['Driving'] and r['IsActive'] and r['Third']==-2000 and r['ThirdPos']==0,'Only active driving width/height distances supported')
        first,position,second,second_position=(r[k] for k in ('First','FirstPos','Second','SecondPos'))
        require(first in axes,'Distance must reference an original boundary edge')
        if position==0 and second==-2000 and second_position==0:
            dimensions[index]=axes[first]
        else:
            require(position in (1,2) and second==(first+2)%4 and second_position==0,'Distance must measure an edge length or the gap between opposite parallel edges')
            dimensions[index]='height' if axes[second]=='width' else 'width'
    require(set(dimensions.values())=={'width','height'},'Independent width and height distances required')
    return dimensions


def identity_of(source):
    require(source.TypeId=='Sketcher::SketchObject' and source.GeometryCount==4 and not source.ExternalGeometry,'One internal four-line sketch required')
    ids=[source.getGeometryId(i) for i in range(4)]
    require(len(set(ids))==4,'Distinct original native geometry IDs required')
    maps=[source.Shape.getElementMappedName('Edge'+str(i+1)) for i in range(4)]
    require(all(type(x) is str and x for x in maps) and len(set(maps))==4,'Four persistent mapped boundary identities required')
    require(all(sum(m.lstrip(';').startswith('g'+str(g)+';') for g in ids)==1 for m in maps),'Mapped boundary must resolve to original geometry IDs')
    rows=constraint_rows(source)
    dimensions = dimension_authority(rows)
    if len(rows)==11:
        # Bind the new family to solved geometry as well as constraint records.
        p,u,v,W,H=rectangle(source)
        require(abs(u.y)<ANGULAR and abs(v.x)<ANGULAR,'Axis constraints disagree with geometry')
        require((p+W/2*u+H/2*v).Length<CONFUSION,'Origin symmetry disagrees with geometry')
        for index,axis in dimensions.items():
            value=source.Constraints[index].Value
            require(type(value) in (int,float) and math.isfinite(value) and value>CONFUSION and abs(value-(W if axis=='width' else H))<CONFUSION,'Distance authority disagrees with bound geometry')
    require(not source.ExpressionEngine,'v1 source expressions require independent qualification; existing expressions are never cleared')
    return {'version':1,'geometry_ids':ids,'mapped_edges':maps,'constraints':rows}

def rectangle(source):
    require(source.GeometryCount==4 and len(source.Shape.Wires)==1 and len(source.Shape.Edges)==4 and source.Shape.isClosed(),'One closed live rectangular wire required')
    g=source.Geometry
    require(all(x.TypeId=='Part::GeomLineSegment' and not source.getConstruction(i) for i,x in enumerate(g)),'Only original nonconstruction lines supported')
    p=g[0].StartPoint;u=g[0].EndPoint-p;v=g[1].EndPoint-g[1].StartPoint
    W=u.Length;H=v.Length
    require(min(W,H)>CONFUSION,'Rectangle below native linear resolution')
    u=u/W;v=v/H
    require(abs(u.dot(v))<ANGULAR and abs(u.z)<ANGULAR and abs(v.z)<ANGULAR,'Native profile must be planar and rectangular')
    points=[p,p+W*u,p+W*u+H*v,p+H*v]
    require(all((x.StartPoint-points[i]).Length<CONFUSION and (x.EndPoint-points[(i+1)%4]).Length<CONFUSION for i,x in enumerate(g)),'Original boundary no longer closes as a rectangle')
    require(source.DoF==0,'Fully constrained original source required')
    return p,u,v,W,H

def property_schema(obj):
    return {p:obj.getTypeIdOfProperty(p) for p in obj.PropertiesList}

def origin_schema(body):
    require(ORIGIN_SCHEMA is not None and ORIGIN_FEATURE_SCHEMAS is not None,'Trusted origin schemas unavailable')
    origin=body.Origin
    require(origin is not None and origin.Document==body.Document and (origin.TypeId,property_schema(origin))==ORIGIN_SCHEMA,'Unqualified native Origin dependency')
    require([owner for owner in origin.InList if owner.TypeId=='PartDesign::Body']==[body],'Origin must belong exclusively to its Body')
    require(not origin.ExpressionEngine,'Origin expressions are unsupported')
    features=list(origin.OriginFeatures)
    require(len(features)==len(ORIGIN_FEATURE_SCHEMAS) and len({str(f.Role) for f in features})==len(features),'Exact original native origin roles required')
    for feature in features:
        role=str(feature.Role)
        require(feature.Document==body.Document and role in ORIGIN_FEATURE_SCHEMAS and (feature.TypeId,property_schema(feature))==ORIGIN_FEATURE_SCHEMAS[role],'Unqualified native origin feature dependency')
        require(not feature.ExpressionEngine,'Origin feature expressions are unsupported')
    require(all(t!='App::PropertyPythonObject' for t in ORIGIN_SCHEMA[1].values()) and all(t!='App::PropertyPythonObject' for _,schema in ORIGIN_FEATURE_SCHEMAS.values() for t in schema.values()),'Origin Python state is forbidden')
    return origin

# Pinned FreeCAD Property::Status indices, commit4fd3bf320d9566a27e60069fc8387448aaa3a094.
STATUS_BITS={'Immutable':1,'ReadOnly':2,'Hidden':3,'Transient':4,'MaterialEdit':5,'NoMaterialListEdit':6,'Output':7,'LockDynamic':8,'NoModify':9,'PartialTrigger':10,'NoRecompute':11,'CopyOnChange':16,'UserEdit':17}

def status_value(obj,prop):
    raw=0
    for flag in obj.getPropertyStatus(prop):
        if flag=='AllowPartial':continue # Link flag, not a Property::Status bit.
        index=STATUS_BITS.get(flag) if isinstance(flag,str) else flag
        require(type(index) is int and 1<=index<32,'Unknown native property status')
        raw|=1<<index
    return raw

def learn_persistence(obj):
    record={'properties':{},'transients':{},'omitted':{}}
    for prop in obj.PropertiesList:
        typ=obj.getTypeIdOfProperty(prop);status=status_value(obj,prop)
        # Match PropertyContainer::Save: NoPersist, then non-dynamic transient.
        if status&(1<<22):record['omitted'][prop]=typ
        elif not status&(1<<21) and status&((1<<4)|(1<<25)):
            record['transients'][prop]={'type':typ,'status':status}
        else:record['properties'][prop]=typ
    previous=PERSISTENCE.get(obj.TypeId)
    require(previous is None or previous==record,'Trusted constructor persistence schema changed within a native type')
    PERSISTENCE[obj.TypeId]=record

def serialized_schema(obj,native_type,extra=None):
    require(native_type in PERSISTENCE,'Trusted persistent schema unavailable')
    schema=PERSISTENCE[native_type]
    parent=obj.find('Properties')
    require(parent is not None and all(n.tag in ('Property','_Property') for n in parent),'Unsupported saved property record')
    rows=parent.findall('Property');props={n.get('name'):n for n in rows}
    transient_rows=parent.findall('_Property');transients={n.get('name'):n for n in transient_rows}
    require(len(rows)==len(props) and len(transient_rows)==len(transients) and not (props.keys()&transients.keys()),'Duplicate saved property identities')
    expected=dict(schema['properties'],**(extra or {}))
    require({n:p.get('type') for n,p in props.items()}==expected,'Unqualified persistent native property schema')
    require(set(transients)==set(schema['transients']),'Unqualified native transient metadata schema')
    for name,node in transients.items():
        metadata=schema['transients'][name]
        require(set(node.attrib)=={'name','type','status'} and len(node)==0 and node.get('type')==metadata['type'],'Malformed native transient metadata')
        value=node.get('status')
        require(value is not None and value.isdecimal() and int(value)<2**32 and (int(value)&~1)==metadata['status'],'Changed native transient status')
    for node in props.values():
        value=node.get('status','0')
        require(value.isdecimal() and int(value)<2**32 and not int(value)&((1<<4)|(1<<22)|(1<<25)),'Persistent value has transient/no-persist status')
    return props

def linked_schema(source,body):
    require(SOURCE_SCHEMA is not None and BODY_SCHEMA is not None,'Trusted linked schemas unavailable')
    require(source.TypeId=='Sketcher::SketchObject' and property_schema(source)==SOURCE_SCHEMA,'Unqualified source property schema')
    require(body.TypeId=='PartDesign::Body' and property_schema(body)==BODY_SCHEMA,'Unqualified Body property schema')
    require(all(t!='App::PropertyPythonObject' for t in (*SOURCE_SCHEMA.values(),*BODY_SCHEMA.values())),'Linked Python state is not supported')
    require(source.Document==body.Document and source in body.Group,'Original source/Body ownership required')
    origin_schema(body)
    require(not body.ExpressionEngine,'Body expressions are unsupported')
    mode=str(source.MapMode)
    support=source.AttachmentSupport
    require(mode in ('Deactivated','FlatFace'),'Only unattached or native Body-origin plane source frames supported')
    if mode=='Deactivated':
        require(not support,'Inactive source must not retain attachment support')
    else:
        require(len(support)==1,'One native origin-plane attachment required')
        plane,subs=support[0]
        require(plane in body.Origin.OriginFeatures and plane.isDerivedFrom('App::Plane') and plane.Role in ('XY_Plane','XZ_Plane','YZ_Plane') and list(subs)==[''],'Source frame must be its Body origin plane')

def empty_solid_history(body,source):
    # This bounded New route admits only the selected sketch's empty history.
    # Tip may have been cleared while old solid features remain in Group.
    require(list(body.Group)==[source],'New requires source-only Body history')

def prepare_placement(edit,decoder,target=None):
    value=edit.get('value')
    require(type(value) is dict and 'position' in value and set(value)<= {'position','rotation','axis_angle'},'Bounded rigid Placement input required')
    require(not ('rotation' in value and 'axis_angle' in value),'Choose quaternion or axis-angle, not both')
    rotation=value.get('rotation',[0,0,0,1])
    require(type(rotation) is list and len(rotation)==4 and all(type(x) in (int,float) and math.isfinite(x) for x in rotation) and any(x!=0 for x in rotation),'Placement quaternion requires four finite nonzero components')
    require(callable(decoder),'Native Placement preparation decoder required')
    placement=decoder(target,value)
    require(all(math.isfinite(x) for x in (*placement.Base,*placement.Rotation.Q)),'Finite decoded native Placement required')
    return placement

def feature_family(obj):
    return obj.TypeId=='PartDesign::FeaturePython' or 'DraftVersion' in obj.PropertiesList or 'DraftIdentity' in obj.PropertiesList


def ordered_history(members,source,feature,dependencies,origins,tip):
    """Shared live/saved admission. Forward edges also reject all history cycles."""
    require(len(members)>=2 and members[:2]==[source,feature] and len(set(members))==len(members),
            'Original source/feature prefix and distinct ordered Body members required')
    require(tip is None or tip in members,'Tip must remain in owned history')
    earlier=set(origins)
    for member in members:
        require(set(dependencies(member))<=earlier,'History dependency is foreign, cyclic or later than its owner')
        earlier.add(member)


def history(obj,source,body,tip_override=False,tip=None):
    members=list(body.Group)
    for member in members:
        require(member.Document==body.Document,'Cross-document Body history forbidden')
        owners=[o for o in body.Document.Objects if o.TypeId=='PartDesign::Body' and member in o.Group]
        require(owners==[body],'History member must belong exclusively to original Body')
        if member not in (source,obj):
            require(member.TypeId in DOWNSTREAM_SCHEMAS and property_schema(member)==DOWNSTREAM_SCHEMAS[member.TypeId],
                    'Unqualified downstream native constructor/schema')
            require(not member.ExpressionEngine,'Downstream expressions require separate saved dependency qualification')
    def dependencies(member):
        # _Body is the stock transient ownership backlink, not a history edge.
        refs=[r for r in member.OutList if r!=body]
        for prop in member.PropertiesList:
            if prop=='_Body' or (member==obj and prop=='DraftBody'):continue
            if not member.getTypeIdOfProperty(prop).startswith(('App::PropertyLink','App::PropertyXLink')):continue
            pending=[getattr(member,prop)]
            while pending:
                value=pending.pop()
                if isinstance(value,(tuple,list)):pending.extend(value)
                elif hasattr(value,'Document') and hasattr(value,'TypeId'):refs.append(value)
        if member==obj:
            require(getattr(member,'BaseFeature',None) is None,'Rectangular creation cannot acquire a replacement BaseFeature')
        require(all(r.Document==body.Document for r in refs),'Cross-document history dependency forbidden')
        return [r.Name for r in refs]
    chosen_tip=tip if tip_override else body.Tip
    require(chosen_tip is None or chosen_tip in members,'Tip must remain in owned history')
    ordered_history([m.Name for m in members],source.Name,obj.Name,
                    lambda name:dependencies(next(m for m in members if m.Name==name)),
                    [body.Origin.Name,*[f.Name for f in body.Origin.OriginFeatures]],
                    None if chosen_tip is None else chosen_tip.Name)


def saved_references(props):
    refs=[]
    for prop,node in props.items():
        typ=node.get('type','')
        if typ.startswith('App::PropertyXLink'):
            require(False,'Cross-document downstream links require separate qualification')
        if not typ.startswith('App::PropertyLink'):continue
        root='LinkSubList' if 'LinkSubList' in typ else 'LinkList' if 'LinkList' in typ else 'LinkSub' if 'LinkSub' in typ else 'Link'
        require(len(node)==1 and node[0].tag==root,'Malformed saved native link container')
        # These are the stock local Link/LinkSub/List formats, not Python data.
        require(all(n.tag in ('Link','LinkSub','LinkList','LinkSubList','Sub') for n in node.iter() if n is not node),
                'Unsupported saved native link representation')
        for n in node.iter():
            if n.tag in ('Link','LinkSub'):
                value=n.get('obj',n.get('value',''))
                if value:refs.append(value)
    return refs


def serialized_history(objects,types,body_name,source_name,name,body_props,members,origins):
    require(all(member in objects for member in members),'Saved Body member link unresolved')
    props_by_name={}
    for member in members:
        owners=[n for n,t in types.items() if t=='PartDesign::Body' and any(
            link.get('value')==member for link in objects[n].findall('./Properties/Property[@name="Group"]/LinkList/Link'))]
        require(owners==[body_name],'Saved history member must belong exclusively to original Body')
        if member in (source_name,name):continue
        native_type=types.get(member)
        require(native_type in DOWNSTREAM_SCHEMAS,'Saved downstream native family is not constructor-qualified')
        props=serialized_schema(objects[member],native_type)
        require(all(p.get('type')!='App::PropertyPythonObject' for p in props.values()),'Downstream Python state forbidden before restore')
        require(not objects[member].findall('./Properties/Property[@name="ExpressionEngine"]/ExpressionEngine/Expression'),
                'Downstream expressions require separate saved dependency qualification')
        props_by_name[member]=props
    # Source and proxy keep their existing stronger identity/frame guards.
    props_by_name[source_name]={p.get('name'):p for p in objects[source_name].findall('./Properties/Property')}
    feature_props={p.get('name'):p for p in objects[name].findall('./Properties/Property')}
    base=feature_props.get('BaseFeature')
    require(base is None or not saved_references({'BaseFeature':base}),'Saved rectangular BaseFeature replacement forbidden')
    props_by_name[name]={p:n for p,n in feature_props.items() if p!='DraftBody'}
    tip_node=body_props.get('Tip')
    tip=None if tip_node is None else tip_node.find('Link')
    tip=None if tip is None else tip.get('value') or None
    ordered_history(members,source_name,name,lambda m:saved_references(props_by_name[m]),origins,tip)

def live(obj):
    require(BASE_SCHEMA is not None and {p:obj.getTypeIdOfProperty(p) for p in obj.PropertiesList}==dict(BASE_SCHEMA,**SCHEMA),'Exact native and semantic property schema required')
    require(obj.TypeId=='PartDesign::FeaturePython' and type(obj.Proxy) is RectangularDraftV1,'Exact bundled rectangular draft proxy required')
    require(not hasattr(obj.Proxy,'__dict__'),'Proxy must remain stateless')
    require({p:obj.getTypeIdOfProperty(p) for p in obj.PropertiesList if obj.getGroupOfProperty(p)==GROUP}==SCHEMA,'Exact rectangular draft property schema required')
    require(all(obj.getTypeIdOfProperty(p)!='App::PropertyPythonObject' or p=='Proxy' for p in obj.PropertiesList),'Extra Python state is forbidden')
    require(obj.DraftVersion==1 and not obj.ExpressionEngine,'Unsupported draft version or feature expression ownership')
    source,subs=obj.Profile;body=obj.DraftBody
    require(source is not None and body is not None and not subs and source.Document==obj.Document and body.Document==obj.Document,'Same-document native Profile and Body links required')
    linked_schema(source,body)
    history(obj,source,body)
    identity=strict_json(obj.DraftIdentity)
    require(identity==identity_of(source),'Original geometry, constraint or mapped boundary identity changed; no index repair')
    _,_,_,W,H=rectangle(source)
    scalars(obj.Length.Value,obj.TaperAngle.Value,obj.Reversed,W,H)
    return source,body,identity

def validate_document(doc):
    for obj in doc.Objects:
        if feature_family(obj):live(obj)

def native_ref(doc,value):
    require(type(value) is dict and set(value)=={'name','native_id'} and type(value['native_id']) is int,'Exact native object identity required')
    obj=doc.getObject(value['name'])
    require(obj is not None and obj.ID==value['native_id'],'Stale native object identity')
    return obj

def quantities(edit,W,H):
    import FreeCAD as App
    values=[]
    for key,unit in [('length','mm'),('angle','deg')]:
        value=edit.get(key)
        require(type(value) is str,'Authoritative quantity input strings required')
        q=App.Units.Quantity(value)
        require(q.Unit==App.Units.Quantity('1 '+unit).Unit,'Wrong native quantity dimension')
        values.append(q.Value)
    return scalars(*values,edit.get('reversed'),W,H)

def prepare(doc,edit,preview_object=None):
    op=edit.get('op')
    fields={'op','operation','termination','side','length','angle','reversed'}
    fields|={'source','body','name','boundary'} if op==OPS[0] else {'target'}
    require(op in OPS and set(edit)==fields,'Exact semantic rectangular draft fields required')
    require((edit['operation'],edit['termination'],edit['side'])==('New','Blind','one-side'),'Convergence Add/Pocket and other terminations are unsupported in v1')
    if op==OPS[0]:
        source=native_ref(doc,edit['source']);body=native_ref(doc,edit['body'])
        linked_schema(source,body)
        if preview_object is None:
            empty_solid_history(body,source)
            require(body.Tip is None or body.Tip==source,'New requires a Body with no existing solid feature')
        else:
            # Only the worker's current lease-owned creation may occupy New's slot.
            old_source,old_body,_=live(preview_object)
            require(old_source==source and old_body==body and list(body.Group)==[source,preview_object]
                    and body.Tip==preview_object,'Preview replacement ownership changed')
        require(type(edit['name']) is str and re.fullmatch('[A-Za-z_][A-Za-z0-9_]*',edit['name']) and (doc.getObject(edit['name']) is None or doc.getObject(edit['name'])==preview_object),'Fresh feature name required')
        identity=identity_of(source)
        require(edit['boundary']=={'geometry_ids':identity['geometry_ids'],'mapped_edges':identity['mapped_edges']},'Incoming mapped boundary identity is stale')
    else:
        obj=native_ref(doc,edit['target'])
        require(obj.TypeId!='PartDesign::Pad','Committed native Pad conversion is unsupported in v1')
        source,body,identity=live(obj)
    _,_,_,W,H=rectangle(source)
    L,A,R=quantities(edit,W,H)
    return source,body,identity,L,A,R

def apply(doc,edit):
    source,body,identity,L,A,R=prepare(doc,edit)
    if edit['op']==OPS[0]:
        obj=body.newObject('PartDesign::FeaturePython',edit['name'])
        for prop,typ in SCHEMA.items():obj.addProperty(typ,prop,GROUP)
        obj.Profile=(source,[]);obj.DraftBody=body;obj.DraftVersion=1
        obj.DraftIdentity=json.dumps(identity,sort_keys=True,separators=(',',':'))
        for p in ('Profile','DraftBody','DraftVersion','DraftIdentity'):obj.setEditorMode(p,1)
        obj.Proxy=RectangularDraftV1()
        body.Tip=obj
    else:obj=native_ref(doc,edit['target'])
    obj.Length=L;obj.TaperAngle=A;obj.Reversed=R
    return {'object':obj.Name,'native_id':obj.ID,'semantic':'RectangularDraftV1'}

def planned_tip(edits,index,name,body):
    additions=[(i,e) for i,e in enumerate(edits) if e.get('op')=='add_object' and e.get('name')==name]
    return (len(additions)==1 and additions[0][0]<index
            and additions[0][1].get('body')==body
            and additions[0][1].get('type') in DOWNSTREAM_SCHEMAS)


def history_tip_name(edit):
    value=edit.get('tip') if edit.get('op')=='body_tip' else edit.get('value')
    if edit.get('op')=='set' and value is not None:
        require(type(value) is dict and set(value)=={'link'},'Tip set requires a native Link value')
        value=value['link']
    require(value is None or type(value) is str,'Tip requires an owned native name or null')
    return value


def preflight(doc,edits,placement_decoder=None,preview_object=None):
    # No geometry-changing batches may hide dependency edits behind a create.
    semantic=[e for e in edits if e.get('op') in OPS]
    if semantic:
        require(len(edits)==1,'Rectangular draft semantic edits must be isolated')
        prepare(doc,semantic[0],preview_object)
    # Ordinary Pad/Add/Pocket edits stay on their established native paths.
    # Dedicated semantic update already rejects a committed ordinary Pad.
    features=[o for o in doc.Objects if feature_family(o)]
    for feature in features:
        source,body,identity=live(feature)
        for edit_index,edit in enumerate(edits):
            target=doc.getObject(edit.get('object',''))
            if edit.get('op') in OPS:continue
            if target is not None and edit.get('op')=='set' and edit.get('property')=='Placement' and (target==source or target==body or target in source.OutList or (target.TypeId=='App::Part' and body in target.Group)):
                prepare_placement(edit,placement_decoder,target)
            if target==feature:
                require(edit.get('op')=='remove_object','Use dedicated semantic update for rectangular draft properties')
            if target==source:
                allowed=edit.get('op')=='set' and edit.get('property')=='Placement'
                if edit.get('op')=='datum':
                    i=edit.get('index')
                    dimensions=dimension_authority(identity['constraints'])
                    require(type(i) is int and i in dimensions,'Only original width/height Distance edits supported')
                    require(len(edits)==1,'Dependent source dimension edits must be isolated')
                    import FreeCAD as App
                    q=App.Units.Quantity(edit['value'])
                    require(q.Unit==App.Units.Quantity('1 mm').Unit and math.isfinite(q.Value),'Native dimensional quantity required')
                    _,_,_,W,H=rectangle(source)
                    scalars(feature.Length.Value,feature.TaperAngle.Value,feature.Reversed,q.Value if dimensions[i]=='width' else W,q.Value if dimensions[i]=='height' else H)
                    allowed=True
                require(allowed,'Dependent profile topology/constraint/family/expression edit unsupported before history; original Distance and rigid Placement only')
            if target is not None and target.TypeId=='App::Part' and body in target.Group:
                require(edit.get('op')=='set' and edit.get('property')=='Placement','Dependent container edits unsupported in v1')
            # A native history rewrite can indirectly replace source ownership.
            if target==body and (edit.get('op')=='body_tip' or (edit.get('op')=='set' and edit.get('property')=='Tip')):
                value=history_tip_name(edit)
                tip=None if value is None else doc.getObject(value)
                if value is not None and tip is None:
                    require(planned_tip(edits,edit_index,value,body.Name),'Unresolved or unowned planned Tip')
                    # After edits, ordinary worker validation calls live/history
                    # against the actual constructor, ownership and dependencies.
                    continue
                require(value is None or tip is not None,'Unresolved Tip')
                history(feature,source,body,True,tip)
                continue
            if target is not None and (target==body or target in source.OutList):
                if edit.get('op')=='set' and edit.get('property')=='Visibility' and target.TypeId=='App::Plane':
                    require(set(edit)=={'op','object','property','value'} and type(edit['value']) is bool,'Native plane Visibility requires an actual bool')
                    require(target.Document==doc and target in body.Origin.OriginFeatures and target.Role in ('XY_Plane','XZ_Plane','YZ_Plane'),'Visibility target must be a current owned origin plane')
                    require(target.getTypeIdOfProperty('Visibility')=='App::PropertyBool','Native plane Visibility property type changed')
                else:
                    require(edit.get('op')=='set' and edit.get('property')=='Placement','Dependent ownership/support edits unsupported in v1')
            require(edit.get('op') not in ('reference_copy','detach_sketch') or target!=source,'Dependent source replacement unsupported')

# PropertyPythonObject.cpp: dumps result -> json.dumps -> base64 in Python.value.
def serialized_preflight(path):
    with zipfile.ZipFile(path) as archive:
        xml=ET.fromstring(archive.read('Document.xml'))
    types={x.get('name'):x.get('type') for x in xml.findall('./Objects/Object')}
    objects={x.get('name'):x for x in xml.findall('./ObjectData/Object')}
    for name,obj in objects.items():
        props={x.get('name'):x for x in obj.findall('./Properties/Property')}
        family=types.get(name)=='PartDesign::FeaturePython' or bool({'DraftVersion','DraftIdentity'} & props.keys())
        if not family:continue
        require(len(obj.findall('./Properties/Property'))==len(props),'Duplicate saved draft properties')
        require(types.get(name)=='PartDesign::FeaturePython','Saved draft native type mismatch')
        props=serialized_schema(obj,'PartDesign::FeaturePython',SCHEMA)
        require(all(p in props and props[p].get('type')==t for p,t in SCHEMA.items()),'Missing/wrong saved draft property schema')
        python=props.get('Proxy')
        require(python is not None and python.get('type')=='App::PropertyPythonObject','Missing saved proxy')
        nodes=python.findall('Python')
        require(len(nodes)==1 and len(python)==1,'One saved Python node required')
        node=nodes[0]
        require(set(node.attrib)=={'value','encoded','module','class'} and node.get('encoded')=='yes' and node.get('module')==__name__ and node.get('class')=='RectangularDraftV1','Wrong/file-backed saved proxy state')
        try:state(strict_json(base64.b64decode(node.get('value'),validate=True)))
        except (ValueError,TypeError) as e:raise DraftError('Malformed saved JSON proxy state') from e
        require(all(p=='Proxy' or x.get('type')!='App::PropertyPythonObject' for p,x in props.items()),'Extra saved Python state forbidden')
        version=props['DraftVersion'].find('Integer')
        require(version is not None and version.get('value')=='1','Unsupported saved draft version')
        identnode=props['DraftIdentity'].find('String')
        require(identnode is not None,'Missing saved geometry identities')
        ident=strict_json(identnode.get('value'))
        require(type(ident) is dict and set(ident)=={'version','geometry_ids','mapped_edges','constraints'} and ident['version']==1 and len(ident['geometry_ids'])==4 and len(set(ident['geometry_ids']))==4 and len(ident['mapped_edges'])==4 and len(set(ident['mapped_edges']))==4 and len(ident['constraints']) in (11,12),'Malformed saved original identity schema')
        if len(ident['constraints'])==11:dimension_authority(ident['constraints'])
        profile=props['Profile'].find('LinkSub');body=props['DraftBody'].find('Link')
        require(profile is not None and body is not None and profile.get('value') in objects and body.get('value') in objects,'Saved native source/Body links unresolved')
        source_name,body_name=profile.get('value'),body.get('value')
        require(types.get(source_name)=='Sketcher::SketchObject' and types.get(body_name)=='PartDesign::Body','Saved native source/Body family mismatch')
        require(SOURCE_SCHEMA is not None and BODY_SCHEMA is not None,'Trusted linked schemas unavailable before restore')
        require(len(xml.findall('./Objects/Object'))==len(types) and len(xml.findall('./ObjectData/Object'))==len(objects),'Duplicate saved native object identities')
        linked={}
        for linked_name,schema in ((source_name,SOURCE_SCHEMA),(body_name,BODY_SCHEMA)):
            rows=objects[linked_name].findall('./Properties/Property')
            linked_props={x.get('name'):x for x in rows}
            linked_props=serialized_schema(objects[linked_name],types[linked_name])
            require(all(x.get('type')!='App::PropertyPythonObject' for x in rows),'Linked Python state is forbidden before restore')
            linked[linked_name]=linked_props
        group=linked[body_name].get('Group')
        members=[] if group is None else [x.get('value') for x in group.findall('./LinkList/Link')]
        require(len(members)>=2 and members[:2]==[source_name,name],'Saved original source/feature prefix required')
        require(all(member in objects for member in members),'Saved Body member link unresolved')
        require(ORIGIN_SCHEMA is not None and ORIGIN_FEATURE_SCHEMAS is not None and MAP_MODES is not None,'Trusted origin admission unavailable')
        def bounded(saved_name,expected_type,schema):
            require(saved_name in objects and types.get(saved_name)==expected_type,'Saved native dependency type mismatch')
            rows=objects[saved_name].findall('./Properties/Property')
            props={p.get('name'):p for p in rows}
            props=serialized_schema(objects[saved_name],expected_type)
            require(all(p.get('type')!='App::PropertyPythonObject' for p in rows),'Origin Python state is forbidden before restore')
            require(not objects[saved_name].findall('./Properties/Property[@name="ExpressionEngine"]/ExpressionEngine/Expression'),'Origin dependency expressions are unsupported')
            return props
        body_props=linked[body_name]
        origin_node=body_props['Origin'].find('Link')
        require(origin_node is not None,'Missing saved Body Origin link')
        origin_name=origin_node.get('value')
        origin_props=bounded(origin_name,*ORIGIN_SCHEMA)
        owners=[n for n,typ in types.items() if typ=='PartDesign::Body' and objects[n].find('./Properties/Property[@name="Origin"]/Link') is not None and objects[n].find('./Properties/Property[@name="Origin"]/Link').get('value')==origin_name]
        require(owners==[body_name],'Saved Origin must belong exclusively to its Body')
        feature_names=[n.get('value') for n in origin_props['OriginFeatures'].findall('./LinkList/Link')]
        require(len(feature_names)==len(ORIGIN_FEATURE_SCHEMAS) and len(set(feature_names))==len(feature_names),'Exact saved native origin feature set required')
        roles={}
        for feature_name in feature_names:
            require(feature_name in objects,'Saved origin feature link unresolved')
            role_node=objects[feature_name].find('./Properties/Property[@name="Role"]/String')
            role=None if role_node is None else role_node.get('value')
            require(role in ORIGIN_FEATURE_SCHEMAS and role not in roles,'Unknown or duplicate saved origin role')
            bounded(feature_name,*ORIGIN_FEATURE_SCHEMAS[role]);roles[role]=feature_name
        require(set(roles)==set(ORIGIN_FEATURE_SCHEMAS),'Saved origin roles changed')
        serialized_history(objects,types,body_name,source_name,name,body_props,members,[origin_name,*feature_names])
        source_props=linked[source_name]
        mode_node=source_props['MapMode'].find('Integer')
        require(mode_node is not None,'Missing native attachment enumeration')
        mode_value=mode_node.get('value')
        require(mode_value is not None and mode_value.isdecimal() and int(mode_value)<len(MAP_MODES),'Invalid saved attachment enumeration')
        mode=MAP_MODES[int(mode_value)]
        support=source_props['AttachmentSupport'].findall('./LinkSubList/Link')
        require(mode in ('Deactivated','FlatFace'),'Unsupported saved source frame')
        if mode=='Deactivated':require(not support,'Inactive saved source retains attachment support')
        else:
            require(len(support)==1 and set(support[0].attrib)=={'obj','sub'} and support[0].get('sub')=='' and support[0].get('obj') in [roles[r] for r in ('XY_Plane','XZ_Plane','YZ_Plane')],'Saved source frame must be its Body origin plane')
        require(not objects[source_name].findall('./Properties/Property[@name="ExpressionEngine"]/ExpressionEngine/Expression') and not objects[body_name].findall('./Properties/Property[@name="ExpressionEngine"]/ExpressionEngine/Expression'),'Saved source/Body expressions are unsupported')
    return True
