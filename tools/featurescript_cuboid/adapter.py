from pathlib import Path
import argparse, json, math, re, hashlib

# Authored from official fCuboid and defineFeature APIs. Exact token grammar only.
SOURCE = '''FeatureScript 2500;
import(path : "onshape/std/geometry.fs", version : "2500.0");
annotation { "Feature Type Name" : "Bounded cuboid" }
export const boundedCuboid = defineFeature(function(context is Context, id is Id, definition is map)
    precondition
    {
        annotation { "Name" : "Length" }
        isLength(definition.length, NONNEGATIVE_LENGTH_BOUNDS);
        annotation { "Name" : "Width" }
        isLength(definition.width, NONNEGATIVE_LENGTH_BOUNDS);
        annotation { "Name" : "Height" }
        isLength(definition.height, NONNEGATIVE_LENGTH_BOUNDS);
    }
    {
        fCuboid(context, id + "cuboid", {
            "corner1" : vector(0, 0, 0) * millimeter,
            "corner2" : vector(definition.length, definition.width, definition.height)
        });
    });
'''
TOKEN = re.compile(r'[ \t\r\n]+|"[^"\r\n]*"|[A-Za-z_][A-Za-z_0-9]*|[0-9]+|[{}()\[\];:,.+*=]')
def tokens(source):
    if len(source) > 16384: raise ValueError('source too large')
    result, pos = [], 0
    while pos < len(source):
        match = TOKEN.match(source, pos)
        if not match: raise ValueError('unsupported source character at '+str(pos))
        token = match.group(); pos = match.end()
        if not token.isspace(): result.append(token)
    return result

def adapt(source, quantities, name='Cuboid'):
    if tokens(source) != tokens(SOURCE):
        raise ValueError('unsupported FeatureScript; only the exact boundedCuboid token grammar is accepted')
    if not isinstance(name, str) or not re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]*', name): raise ValueError('invalid native name')
    if not isinstance(quantities, dict) or set(quantities) != {'length','width','height'}: raise ValueError('exactly three dimensions required')
    values = {}
    for key, q in quantities.items():
        if not isinstance(q, dict) or set(q) != {'value','unit'}: raise ValueError('typed quantity required')
        value, unit = q['value'], q['unit']
        if type(value) not in (int, float) or not math.isfinite(value) or value <= 0: raise ValueError('positive finite dimension required')
        if not isinstance(unit,str) or unit not in {'mm','cm','m','in'}: raise ValueError('unsupported length unit')
        value *= {'mm':1,'cm':10,'m':1000,'in':25.4}[unit]
        if not math.isfinite(value) or not 1e-6 <= value <= 1e6: raise ValueError('dimension outside bounded native range')
        values[key] = value
    params = {'name': {'type':'identifier','default':name}}
    params.update({k:{'type':'number','unit':'mm','minimum':1e-6,'default':v} for k,v in values.items()})
    edits = [{'op':'add_object','type':'Part::Box','name':{'$param':'name'}}]
    edits += [{'op':'set','object':{'$param':'name'},'property':k.title(),'value':{'$param':k}} for k in ('length','width','height')]
    return {'version':1,'id':'ported.boundedCuboid','label':'Bounded FeatureScript cuboid','description':'Manual fCuboid port. Native Box properties regenerate; no FeatureScript runtime. Lengths in mm.','parameters':params,'edits':edits}

def main():
    p=argparse.ArgumentParser(description='Exact-template FeatureScript cuboid to native package. No code execution.')
    p.add_argument('source',type=Path); p.add_argument('quantities',type=Path); p.add_argument('output',type=Path); p.add_argument('--name',default='Cuboid')
    a=p.parse_args()
    package=adapt(a.source.read_text(encoding='utf8'),json.loads(a.quantities.read_text(encoding='utf8')),a.name)
    # Validate everything before touching the output. Replace only a complete JSON file.
    data=json.dumps(package,indent=2,allow_nan=False)+'\n'
    temporary=a.output.with_suffix('.tmp')
    with temporary.open('x',encoding='utf8') as f: f.write(data)
    temporary.replace(a.output)
if __name__=='__main__': main()
