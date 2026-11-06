"""TechDraw persistence metadata, learned only from trusted native prototypes."""
import io
import zipfile
import xml.etree.ElementTree as ET

CONTRACTS = {}
# FreeCAD 4fd3bf320 Property.h. Touched is the sole permitted status variation.
BITS = {'Touched':0,'Immutable':1,'ReadOnly':2,'Hidden':3,'Transient':4,
        'MaterialEdit':5,'NoMaterialListEdit':6,'Output':7,'LockDynamic':8,
        'NoModify':9,'PartialTrigger':10,'NoRecompute':11,'CopyOnChange':16,
        'UserEdit':17,'Single':12,'Ordered':13,'EvalOnRestore':14,'Busy':15,'DisableNotify':18}
KNOWN = set(range(19)) | set(range(21,32))
NO_VALUE = (1<<4)|(1<<22)|(1<<25)


def require(ok,message):
    if not ok:raise ValueError(message)


def uint(value,limit):
    require(isinstance(value,str) and value.isascii() and value.isdecimal() and len(value)<=10,'Invalid persistence count/status')
    number=int(value);require(number<=limit,'Persistence count/status out of range');return number


def status(obj,prop):
    value=0
    for flag in obj.getPropertyStatus(prop):
        if flag=='AllowPartial':continue  # Native link flag, not Property::Status.
        bit=BITS.get(flag) if isinstance(flag,str) else flag
        require(type(bit) is int and bit in KNOWN,'Unknown native property status')
        value |= 1<<bit
    return value & ~1


def rows(obj):
    containers=obj.findall('Properties')
    require(len(containers)==1,'Drawing requires exactly one Properties container')
    parent=containers[0]
    require(set(parent.attrib)=={'Count','TransientCount'},'Unexpected property-container metadata')
    normal=parent.findall('Property');transient=parent.findall('_Property')
    require(all(n.tag in ('Property','_Property') for n in parent),'Unexpected property-container child')
    require(uint(parent.get('Count'),256)==len(normal) and uint(parent.get('TransientCount'),256)==len(transient),'Inconsistent property-container counts')
    # Native Save emits metadata first. Restore consumes those declared rows in that order.
    require([n.tag for n in parent]==['_Property']*len(transient)+['Property']*len(normal),'Unexpected persistence row ordering')
    names=[n.get('name') for n in parent]
    require(all(isinstance(n,str) and n for n in names) and len(names)==len(set(names)),'Duplicate persistence property identity')
    return {n.get('name'):n for n in normal},{n.get('name'):n for n in transient}


def learn(obj):
    """dumpContent is already used by qualified native-owner content capture.

    The XML layout of a TechDraw prototype dump remains a future native gate:
    missing/ambiguous containers fail closed, with no fabricated fallback.
    """
    expected={'normal':{},'transient':{},'omitted':{}}
    for name in obj.PropertiesList:
        typ=obj.getTypeIdOfProperty(name);flags=status(obj,name)
        group='omitted' if flags&(1<<22) else ('transient' if not flags&(1<<21) and flags&((1<<4)|(1<<25)) else 'normal')
        expected[group][name]={'type':typ,'status':flags}
    containers=[]
    with zipfile.ZipFile(io.BytesIO(obj.dumpContent())) as z:
        for entry in z.infolist():
            if not entry.filename.lower().endswith('.xml'):continue
            require(entry.file_size<=1024*1024,'Oversized trusted prototype XML')
            data=z.read(entry)
            require(b'<!DOCTYPE' not in data.upper() and b'<!ENTITY' not in data.upper(),'Prototype XML DTD forbidden')
            root=ET.fromstring(data);containers.extend(root.iter('Properties'))
    containers=[c for c in containers if {n.get('name') for n in c.findall('Property')}==set(expected['normal']) and {n.get('name') for n in c.findall('_Property')}==set(expected['transient'])]
    require(len(containers)==1,'Trusted TechDraw dump must have one matching object Properties container')
    wrapper=ET.Element('Prototype');wrapper.append(containers[0]);normal,transient=rows(wrapper)
    require(set(normal)==set(expected['normal']) and set(transient)==set(expected['transient']),'Native Save partition differs from PropertyContainer contract')
    for group,items in [('normal',normal),('transient',transient)]:
        for name,node in items.items():
            flags=uint(node.get('status','0'),2**32-1)&~1
            require(node.get('type')==expected[group][name]['type'] and flags==expected[group][name]['status'],'Trusted native persistence metadata differs')
            require(group!='normal' or not flags&NO_VALUE,'Drawing authority cannot be persisted as transient/no-persist')
            if group=='transient':require(set(node.attrib)=={'name','type','status'} and len(node)==0,'Invalid trusted transient metadata')
            # Dynamic metadata is not guessed: save exactly the constructor's attributes.
            expected[group][name]['attributes']={k:v for k,v in node.attrib.items() if k!='status'}
    CONTRACTS[obj.TypeId]=expected


def properties(obj,native_type):
    require(native_type in CONTRACTS,'Trusted drawing persistence contract unavailable')
    contract=CONTRACTS[native_type];normal,transient=rows(obj)
    require(set(normal)==set(contract['normal']) and set(transient)==set(contract['transient']),'Unqualified persisted/transient property rows')
    for group,items in [('normal',normal),('transient',transient)]:
        for name,node in items.items():
            expected=contract[group][name]
            require({k:v for k,v in node.attrib.items() if k!='status'}==expected['attributes'],'Changed native persistence property metadata')
            value=uint(node.get('status','0'),2**32-1)
            require((value&~1)==expected['status'],'Changed native persistence property status')
            if group=='normal':require(not value&NO_VALUE,'Persisted authority marked transient/no-persist')
            else:require(set(node.attrib)=={'name','type','status'} and len(node)==0,'Malformed transient metadata')
    return normal
