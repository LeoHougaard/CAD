"""Focused native reference cardinality and real headless Editor workflow.

Run with --tests TEST_BINARY --runtime RUNTIME_JSON --output NEW_DIRECTORY.
Native preparation, the explicit Editor test and independent direct-native
checks run sequentially. Uses synthetic data and never opens a GUI window.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True


def dump(path, value):
    path.write_text(json.dumps(value, indent=2)+'\n', encoding='utf-8')


def prepare(out):
    import FreeCAD as A
    import Part
    import PartDesign
    import Sketcher
    d=A.newDocument('ReferenceSelections')
    for index,name in enumerate(['Original','A','B']):
        box=d.addObject('Part::Box',name)
        box.Length,box.Width,box.Height=10,10,10
        box.Placement.Base.x=index*20
    holder=d.addObject('App::Part','Holder')
    for kind,name in [('Link','SingleObject'),('LinkSub','SingleSub'),('LinkList','ObjectList'),('LinkSubList','SubList')]:
        holder.addProperty('App::Property'+kind,name,'Reference test')
    original=d.getObject('Original')
    holder.SingleObject=original
    holder.SingleSub=(original,['Face3'])
    holder.ObjectList=[original]
    holder.SubList=[(original,['Face3'])]
    body=d.addObject('PartDesign::Body','Body')
    base=body.newObject('PartDesign::AdditiveBox','SolidBase')
    base.Length,base.Width,base.Height=10,10,10
    blend=body.newObject('PartDesign::Fillet','SolidBlend')
    blend.Base=(base,['Edge1'])
    blend.Radius=0.3
    body.Tip=blend
    profiles=d.addObject('PartDesign::Body','ProfileBody')
    for name,width,height in [('OriginalProfile',4,3),('ProfileA',2,2),('ProfileB',3,2)]:
        sketch=profiles.newObject('Sketcher::SketchObject',name)
        corners=[(0,0),(width,0),(width,height),(0,height)]
        for start,end in zip(corners,corners[1:]+corners[:1]):
            sketch.addGeometry(Part.LineSegment(A.Vector(*start,0),A.Vector(*end,0)),False)
        for index in range(4):
            sketch.addConstraint(Sketcher.Constraint('Coincident',index,2,(index+1)%4,1))
            sketch.addConstraint(Sketcher.Constraint('Horizontal' if index%2==0 else 'Vertical',index))
        sketch.Visibility=False
    pad=profiles.newObject('PartDesign::Pad','ProfilePad')
    pad.Profile=d.getObject('OriginalProfile')
    pad.Length=5
    profiles.Tip=pad
    for name,z in [('PlaneOne',8),('PlaneTwo',10)]:
        plane=d.addObject('Part::Feature',name)
        plane.Shape=Part.makePlane(10,10,A.Vector(-2,-2,z))
    d.recompute()
    assert all(o.isValid() for o in d.Objects)
    d.saveAs(str((out/'Fixture.FCStd').resolve()))
    A.closeDocument(d.Name)


def encode(value):
    if value is None:
        return None
    if isinstance(value,list):
        return [encode(v) for v in value]
    if isinstance(value,tuple):
        return dict(link=value[0].Name,subelements=list(value[1]))
    return dict(link=value.Name)


def check(out):
    import FreeCAD as A
    import PartDesign
    d=A.openDocument(str((out/'Fixture.FCStd').resolve()))
    a,b=d.getObject('A'),d.getObject('B')
    holder=d.getObject('Holder')
    # Independent assignments use actual native objects/tuples, including
    # separate whole-object and face picks. Native LinkSubList itself merges
    # repeated owners and preserves whole/face subname order.
    direct={
        'single-object':a,
        'single-sub-faces':(a,['Face2','Face1']),
        'single-sub-whole':(a,[]),
        'single-sub-whole-first':(a,['','Face2']),
        'single-sub-whole-last':(a,['Face2','']),
        'object-list':[a,b],
        'sub-list-faces':[(a,['Face2']),(a,['Face1']),(b,['Face3'])],
        'sub-list-whole':[(a,['']),(b,[''])],
        'sub-list-mixed':[(a,['']),(a,['Face2']),(b,['Face1']),(b,[''])],
    }
    results=[]
    for record in json.loads((out/'editor/reference-boundaries.json').read_text()):
        setattr(holder,record['field'],direct[record['case']])
        expected=encode(getattr(holder,record['field']))
        assert record['actual']==expected,(record,expected)
        results.append(dict(case=record['case'],actual=record['actual'],expected=expected,matched=True))
    # Native references can retain unresolved subnames. Do not invent a generic
    # face-name restriction: the consuming feature must validate geometry.
    holder.SingleSub=(a,['Face999'])
    assert holder.isValid() and holder.SingleSub[1]==['Face999']
    saved=A.openDocument(str((out/'editor/References.FCStd').resolve()))
    blend=saved.getObject('SolidBlend')
    assert blend.isValid() and blend.Base[0].Name=='SolidBase' and blend.Base[1]==['Edge1','Edge2']
    control=d.getObject('SolidBlend')
    control.Base=(d.getObject('SolidBase'),['Edge1','Edge2'])
    d.recompute()
    assert control.isValid()
    difference=blend.Shape.cut(control.Shape).Volume+control.Shape.cut(blend.Shape).Volume
    assert difference<1e-8,difference
    profile=saved.getObject('ProfilePad')
    profile_control=d.getObject('ProfilePad')
    profile_control.Profile=d.getObject('ProfileA')
    profile_control.Length=6
    d.recompute()
    assert profile.isValid() and profile.Profile[0].Name=='ProfileA' and profile.Length.Value==6
    profile_difference=profile.Shape.cut(profile_control.Shape).Volume+profile_control.Shape.cut(profile.Shape).Volume
    assert profile_difference<1e-8,profile_difference
    for name in ['OriginalProfile','ProfileA','ProfileB']:
        actual,expected=saved.getObject(name),d.getObject(name)
        assert actual.GeometryCount==expected.GeometryCount==4
        assert actual.ConstraintCount==expected.ConstraintCount==8
        def geometry(sketch):
            return [(g.TypeId,(g.StartPoint.x,g.StartPoint.y,g.StartPoint.z),(g.EndPoint.x,g.EndPoint.y,g.EndPoint.z)) for g in sketch.Geometry]
        assert geometry(actual)==geometry(expected)
        assert [(c.Type,c.First,c.FirstPos,c.Second,c.SecondPos) for c in actual.Constraints]==[(c.Type,c.First,c.FirstPos,c.Second,c.SecondPos) for c in expected.Constraints]
    dump(out/'native-comparison.json',dict(freecad=A.Version(),boundaries=results,
        unresolved_subname_stored_natively=True,saved_blend_valid=True,shape_symmetric_difference=difference,
        base=encode(blend.Base),radius=blend.Radius.Value,
        saved_profile=encode(profile.Profile),profile_shape_symmetric_difference=profile_difference,
        source_sketch_geometry_and_constraints_preserved=True))
    A.closeDocument(saved.Name)
    A.closeDocument(d.Name)


def main():
    if len(sys.argv)==3 and sys.argv[1] in ('--native-prepare','--native-check'):
        (prepare if sys.argv[1]=='--native-prepare' else check)(Path(sys.argv[2]))
        return
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tests',required=True,type=Path)
    parser.add_argument('--runtime',required=True,type=Path)
    parser.add_argument('--output',required=True,type=Path)
    args=parser.parse_args()
    out=args.output.resolve()
    out.mkdir(parents=True,exist_ok=False)
    (out/'editor').mkdir()
    runtime=json.loads(args.runtime.read_text(encoding='utf-8-sig'))
    script=Path(__file__).resolve()
    manifest=[]
    def run(name,command,env=None):
        with (out/(name+'.log')).open('x',encoding='utf-8') as log:
            run_env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1') if env is None else env
            result=subprocess.run([str(v) for v in command],env=run_env,stdout=log,stderr=subprocess.STDOUT,
                **({'creationflags':subprocess.BELOW_NORMAL_PRIORITY_CLASS} if os.name=='nt' else {}))
        # Native executable/args are deliberately opaque in retained manifests.
        manifest.append(dict(phase=name,exit_code=result.returncode))
        dump(out/'phases.json',manifest)
        if result.returncode:
            raise RuntimeError(f'{name} failed; see its retained log')
    run('prepare',[runtime['executable'],*runtime['args'],script,'--native-prepare',out])
    baseline_hash=hashlib.sha256((out/'Fixture.FCStd').read_bytes()).hexdigest()
    env=dict(os.environ,CAD_EDITOR_ACCEPTANCE_RUNTIME=str(args.runtime.resolve()),
             CAD_EDITOR_ACCEPTANCE_OUTPUT=str(out/'editor'),CAD_REFERENCE_SELECTION_FIXTURE=str(out/'Fixture.FCStd'),
             PYTHONDONTWRITEBYTECODE='1')
    run('editor',[args.tests.resolve(),'--ignored','--exact',
        'editor::properties::acceptance_probe::reference_selection::native_reference_selection_preserves_candidates_and_native_cardinality',
        '--nocapture','--test-threads=1'],env)
    run('independent',[runtime['executable'],*runtime['args'],script,'--native-check',out])
    assert hashlib.sha256((out/'Fixture.FCStd').read_bytes()).hexdigest()==baseline_hash
    dump(out/'result.json',dict(passed=True,fixture_sha256=baseline_hash,source_preserved=True,
        native_phases_sequential=True,gui=False,resource_benchmark=False))
    print('Focused native reference assignment, Editor lifecycle and saved-file comparison passed.')


if __name__=='__main__':
    main()
