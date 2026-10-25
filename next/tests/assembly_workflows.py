"""Local native assembly authoring through the public Rust compiler and host."""
import argparse,json,subprocess
from pathlib import Path
from preview_workflows import PreviewSession
from native_workflows import add,set_value,objects,near

IDENTITY={"position":[0,0,0],"rotation":[0,0,0,1]}


def compile_command(compiler,state,request):
    result=subprocess.run([str(compiler),'--native-assembly','-'],input=json.dumps(dict(state=state,request=request)),
                          capture_output=True,text=True,encoding='utf-8')
    assert result.returncode==0,result.stderr
    return json.loads(result.stdout)


def run(host,compiler,runtime,output):
    output.mkdir()
    session=PreviewSession(host,runtime)
    transcript=[]
    try:
        state=session.call('new',name='LocalAssembly')
        doc=state['document']
        def call(op,**params):
            nonlocal state
            result=session.call(op,document=doc,**params)
            transcript.append(dict(op=op,parameters=params,result=result))
            (output/'transcript.json').write_text(json.dumps(transcript,indent=2))
            if 'state' in result:state=result['state']
            elif 'objects' in result:state=result
            return result
        def command(op,**params):
            request=dict(op=op,**params)
            edits=compile_command(compiler,state,request)
            return call('transaction',label='Native assembly '+op,edits=edits)['state']
        def joint(kind='Fixed',create=True,**params):
            return command('assembly_joint',assembly='Assembly',name='Joint',create=create,joint_type=kind,
                reference1=dict(link='PlateInstance',subelements=['','']),
                reference2=dict(link='PinInstance',subelements=['','']),properties=params)
        def reject_native(edits,code):
            before=objects(state)
            call('transaction',label='Rejected assembly edit',edits=edits,expect_error=code)
            call('inspect')
            assert objects(state)==before,(code,'native state changed after rejected edit')
        call('transaction',label='Editable plate and pin sources',edits=[add('Part::Box','Plate'),
            set_value('Plate','Length',40),set_value('Plate','Width',30),set_value('Plate','Height',5),
            add('Part::Cylinder','Pin'),set_value('Pin','Radius',2),set_value('Pin','Height',12)])
        command('assembly_create',name='Assembly',joints='Joints')
        assert objects(state)['Assembly']['type']=='Assembly::AssemblyObject'
        assert objects(state)['Joints']['type']=='Assembly::JointGroup'
        for name,source,x in [('PlateInstance','Plate',0),('PinInstance','Pin',30),('RepeatedPin','PinInstance',60)]:
            command('assembly_insert',assembly='Assembly',name=name,source=source,
                    placement=dict(IDENTITY,position=[x,0,0]))
        assert objects(state)['RepeatedPin']['properties']['LinkedObject']=={'link':'Pin'}
        command('assembly_ground',assembly='Assembly',object='PlateInstance',name='Ground',grounded=True)
        assert objects(state)['PlateInstance']['instance_state']['grounded']
        before=state['undo_count']
        joint()
        assert state['undo_count']==before+1
        assert objects(state)['Joint']['properties']['Reference1']['link']=='PlateInstance'
        near(objects(state)['PinInstance']['properties']['LinkPlacement']['position'][0],0)
        call('undo');assert 'Joint' not in objects(state)
        near(objects(state)['PinInstance']['properties']['LinkPlacement']['position'][0],30)
        call('redo');assert 'Joint' in objects(state)
        # Native connector identities persist; no transform is guessed in Rust.
        request=dict(op='assembly_joint',assembly='Assembly',name='Joint',create=False,joint_type='Fixed',
            reference1=dict(link='PlateInstance',subelements=['Face6','Face6']),
            reference2=dict(link='PinInstance',subelements=['Face3','Face3']),properties={})
        command(**request)
        refs=[objects(state)['Joint']['properties'][p] for p in ['Reference1','Reference2']]
        placement_before=objects(state)['PinInstance']['properties']['LinkPlacement']
        call('transaction',label='Parent height update',edits=[set_value('Plate','Height',8),set_value('Pin','Radius',3)])
        assert objects(state)['PinInstance']['properties']['LinkPlacement']!=placement_before
        assert [objects(state)['Joint']['properties'][p] for p in ['Reference1','Reference2']]==refs
        near(objects(state)['PinInstance']['shape']['volume'],objects(state)['RepeatedPin']['shape']['volume'])
        command('assembly_placement',assembly='Assembly',object='RepeatedPin',placement=dict(IDENTITY,position=[60,20,0]))
        near(objects(state)['RepeatedPin']['properties']['LinkPlacement']['position'][1],20)
        # Preview uses the same edit and lease guards even though Editor tasks are Apply-only.
        base=objects(state)
        edits=compile_command(compiler,state,dict(op='assembly_placement',assembly='Assembly',object='RepeatedPin',placement=dict(IDENTITY,position=[90,20,0])))
        call('preview',token='assembly-preview',label='Place component preview',edits=edits)
        call('transaction',edits=edits,expect_error='preview_active')
        call('preview_cancel',token='assembly-preview');assert objects(state)==base
        old_revision=state['revision']
        call('transaction',expected_revision=old_revision-1,edits=edits,expect_error='stale_revision')
        assert state['revision']==old_revision
        reject_native([dict(op='assembly_joint',assembly='Assembly',name='Broken',create=True,joint_type='Fixed',
            reference1=dict(link='PlateInstance',subelements=['Face999','Face999']),
            reference2=dict(link='PinInstance',subelements=['','']),properties={})], 'invalid_assembly')
        reject_native([dict(op='assembly_remove',assembly='Assembly',object='PinInstance')],'dependent_objects')
        # Follow this shipped runtime's accepted unground behavior. The native
        # solve docstring advertises -6, but the independent control returns 0.
        command('assembly_ground',assembly='Assembly',object='PlateInstance',name='Unused',grounded=False)
        assert not objects(state)['PlateInstance']['instance_state']['grounded']
        call('undo'); assert objects(state)['PlateInstance']['instance_state']['grounded']
        for kind in ['Revolute','Slider','Cylindrical','Ball']:
            joint(kind,False)
            assert objects(state)['Joint']['properties']['JointType']==kind
            call('undo');call('redo')
        joint('Fixed',False,Offset1=dict(IDENTITY,position=[0,0,5]))
        call('transaction',label='Hide source definitions',edits=[set_value('Plate','Visibility',False),set_value('Pin','Visibility',False)])
        mesh=call('mesh',object='PinInstance');assert len(mesh['positions'])>0
        topology=call('topology',object='PinInstance');assert topology['faces']
        # App::Link lacks getGlobalPlacement. Display extraction must traverse
        # the enclosing native assembly instead of treating every link as root.
        pose=dict(position=[100,50,20],rotation=[0,0,2**-.5,2**-.5])
        call('transaction',label='Move assembly container',edits=[set_value('Assembly','Placement',pose)])
        placed=call('mesh',object='RepeatedPin')
        for a,b in zip(placed['world_from_shape']['position'],pose['position']):near(a,b)
        for a,b in zip(placed['world_from_shape']['rotation'],pose['rotation']):near(a,b)
        assert objects(state)['RepeatedPin']['global_placement']['position']!=[60,20,0]
        call('undo')
        # Read requests cannot solve or mutate the accepted document.
        snapshot_before=state
        call('inspect')
        assert state['revision']==snapshot_before['revision']
        assert [(o['name'],o['properties'],o.get('global_placement')) for o in state['objects']]==[(o['name'],o['properties'],o.get('global_placement')) for o in snapshot_before['objects']]
        saved=output/'Assembly.FCStd'
        call('save',path=str(saved))
        final=objects(state)
        call('close')
        opened=session.call('open',path=str(saved));state=opened;doc=state['document']
        assert set(objects(state))==set(final)
        for name in ['Joint','Ground','PlateInstance','PinInstance','RepeatedPin']:
            assert objects(state)[name]['properties']==final[name]['properties'],name
        call('transaction',label='Reopened parent update',edits=[set_value('Pin','Height',16)])
        command('assembly_remove',assembly='Assembly',object='Joint')
        command('assembly_remove',assembly='Assembly',object='PinInstance')
        call('undo');assert 'PinInstance' in objects(state)
        call('undo');assert 'Joint' in objects(state)
        call('redo');call('redo')
        (output/'result.json').write_text(json.dumps(dict(passed=True,local_links=True,native_solver=True,
            gui_verified=False,cross_document_authoring=False,joint_types_tested=['Fixed','Revolute','Slider','Cylindrical','Ball'],
            saved=str(saved),native_capabilities=session.capabilities),indent=2))
        print('assembly workflows passed')
    finally:
        session.close()


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--host',type=Path,required=True);parser.add_argument('--compiler',type=Path,required=True)
    parser.add_argument('--runtime',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();run(args.host,args.compiler,json.loads(args.runtime.read_text()),args.output)
