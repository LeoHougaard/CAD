"""Suppression across same-document checkpoints, consumed Finish and recovery."""
import argparse,json
from pathlib import Path
from native_workflows import add,objects,set_value,transaction,link,constraint
from sketch_editor_workflows import SketchSession

class RecordedSession(SketchSession):
    def __init__(self,host,runtime,folder):
        self.events=[];self.output=folder/'events.json'
        super().__init__(host,runtime)
    def call(self,op,expect_error=None,**kw):
        result=super().call(op,expect_error=expect_error,**kw)
        self.events.append(dict(op=op,parameters=kw,expected_error=expect_error,result=result))
        self.output.write_text(json.dumps(self.events,indent=2)+'\n')
        return result

def native(state,name):return objects(state)[name]['sketch']['native_xml']
def feature(state,name):
    o=objects(state)[name]
    # Uncaptured downstream aliases can change on native abort/recompute even
    # on 1a0 without Q07. The persisted native seed map remains exact.
    return {k:o.get(k) for k in ['properties','expressions','suppression','native_capture_map','shape']}
def owned(state,doc):return state['native_inventory'][doc]

def seed(s,name):
    doc=s.call('new',name=name)['document']
    edits=[add('PartDesign::Body','Body'),add('PartDesign::AdditiveBox','Base','Body')]
    edits += [set_value('Base',k,v) for k,v in [('Length',20),('Width',20),('Height',10)]]
    edits += [add('Sketcher::SketchObject','Profile','Body'),
        set_value('Profile','Placement',dict(position=[5,5,10],rotation=[0,0,0,1])),
        dict(op='geometry',object='Profile',kind='circle',center=[0,0,0],radius=2),
        constraint('Profile','Radius',0,2),dict(op='constraint_state',object='Profile',index=0,name='Size'),
        add('PartDesign::Pad','Pad','Body'),set_value('Pad','Profile',link('Profile')),set_value('Pad','Length',4),
        add('PartDesign::AdditiveBox','A','Body')]
    edits += [set_value('A',k,v) for k,v in [('Length',2),('Width',2),('Height',3)]]
    edits += [set_value('A','Placement',dict(position=[12,12,10],rotation=[0,0,0,1])),
        dict(op='expression',object='A',property='Suppressed',expression='Profile.Constraints.Size > 2.5 mm')]
    transaction(s,doc,'Native consumed profile and suppression expression',edits)
    # The source's native Shape must exist before addExternal resolves Edge1.
    return doc,transaction(s,doc,'Native projected follower',[
        add('Sketcher::SketchObject','Follower'),dict(op='external_geometry',object='Follower',source='Profile',subelement='Edge1')])

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for k in ['host','runtime','output']:p.add_argument('--'+k,type=Path,required=True)
    a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
    s=RecordedSession(a.host.resolve(),json.loads(a.runtime.read_text()),a.output);records=[]
    try:
        doc,base=seed(s,'SuppressionCheckpoint');other=s.call('new',name='SuppressionOther')['document']
        transaction(s,other,'Keeper with native Redo',[add('Part::Box','Box')]);transaction(s,other,'Keeper edit',[set_value('Box','Length',12)])
        other_before=s.call('undo',document=other);other_facts=owned(other_before,other)
        s.call('document_settings',document=doc,expected_revision=s.revisions[doc],unit_system=3)
        begin=s.call('sketch_begin',document=doc,object='Profile',token='owned')
        original=native(begin,'Profile');a_before=feature(begin,'A');uuid=owned(begin,doc)['uuid']
        point=dict(op='geometry',object='Profile',kind='point',position=[1,2,0],construction=True)
        accepted=transaction(s,doc,'Native accepted checkpoint',[dict(op='datum',object='Profile',index=0,value='3 mm'),point])
        exact=native(accepted,'Profile');follow=native(accepted,'Follower')
        assert feature(accepted,'A')==a_before
        s.call('document_settings',document=doc,expected_revision=s.revisions[doc],unit_system=3,expect_error='sketch_active')
        s.call('transaction',document=doc,edits=[set_value('A','Suppressed',True)],expect_error='sketch_active')
        s.call('transaction',document=other,edits=[set_value('Box','Length',20)],expect_error='sketch_active')
        candidate=s.call('preview',document=doc,token='drag',edits=[dict(point,position=[3,4,0])])['state']
        s.call('preview',document=doc,token='drag',edits=[set_value('Profile','Missing',1)],expect_error='unsupported_property')
        recovered=s.call('inspect',document=doc);assert native(recovered,'Profile')==native(candidate,'Profile')
        cancelled=s.call('preview_cancel',document=doc,token='drag');assert native(cancelled,'Profile')==exact
        assert feature(cancelled,'A')==a_before and native(cancelled,'Follower')==follow
        s.call('undo',document=doc);redone=s.call('redo',document=doc);assert native(redone,'Profile')==exact
        s.call('preview',document=doc,token='restore-fault',edits=[point],suppression_restore_failures=1,expect_error='native_error')
        recovered=s.call('inspect',document=doc);assert recovered['available'] and native(recovered,'Profile')==exact
        s.call('sketch_cancel',document=doc,token='owned',suppression_validate_failures=1,expect_error='native_error')
        recovered=s.call('inspect',document=doc);assert recovered['sketch_edit'] and native(recovered,'Profile')==exact
        assert feature(recovered,'A')==a_before
        s.call('sketch_finish',document=doc,token='owned',suppression_validate_failures=1,expect_error='native_error')
        recovered=s.call('inspect',document=doc);assert recovered['available'] and native(recovered,'Profile')==exact
        assert feature(recovered,'A')==a_before
        finished=s.call('sketch_finish',document=doc,token='owned');assert finished['units']['index']==3
        assert objects(finished)['A']['properties']['Suppressed'] and '_CadSuppressionShape' in objects(finished)['A']['property_types']
        assert owned(finished,doc)['uuid']==uuid and owned(finished,other)==other_facts
        s.call('undo',document=doc);redone=s.call('redo',document=doc)
        assert native(redone,'Profile')==exact and feature(redone,'A')==feature(finished,'A')
        baseline=redone
        s.call('sketch_begin',document=doc,object='Profile',token='later')
        transaction(s,doc,'Later native action',[dict(point,position=[8,9,0])])
        cancelled=s.call('sketch_cancel',document=doc,token='later')
        assert native(cancelled,'Profile')==exact and feature(cancelled,'A')==feature(baseline,'A')
        assert cancelled['units']==baseline['units'] and owned(cancelled,other)==other_facts
        path=a.output/'combined-suppressed.FCStd';s.call('save',document=doc,path=str(path));s.call('close',document=doc)
        reopened=s.call('open',path=str(path));doc=reopened['document']
        assert native(reopened,'Profile')==exact and feature(reopened,'A')==feature(baseline,'A')
        assert reopened['units']==baseline['units']
        s.call('close',document=doc);s.call('close',document=other)
        records.append(dict(checkpoint_ids_xml_elementmap_exact=True,candidate_and_accepted_restore=True,failed_restore_recovered=True,
            cancel_and_finish_recovery=True,consumed_finish_native_suppression=True,units_outside_checkpoint=True,other_document_exact=True,fresh_open=True))
        doc,base=seed(s,'SuppressionPoison')
        saved=a.output/'before-poison.FCStd';s.call('save',document=doc,path=str(saved))
        s.call('sketch_begin',document=doc,object='Profile',token='poison')
        transaction(s,doc,'Checkpoint before poison',[point])
        s.call('preview',document=doc,token='fault',edits=[point],suppression_restore_failures=2,expect_error='native_error')
        poisoned=s.call('inspect',document=doc);assert not poisoned['available']
        s.call('close',document=doc)
        reopened=s.call('open',path=str(saved));assert reopened['available']
        assert native(reopened,'Profile')==native(base,'Profile')
        assert feature(reopened,'A')==feature(base,'A')
        s.call('close',document=reopened['document'])
        records.append(dict(failed_checkpoint_recovery_poison=True,close_fresh_open_recovery=True))
        doc,base=seed(s,'SuppressionFailedClose')
        transaction(s,doc,'Literal native suppression input',[dict(op='expression',object='A',property='Suppressed',expression=None)])
        s.call('close',document=doc,suppression_close_failures=1,expect_error='native_error')
        before=s.call('inspect',document=doc)
        captured=transaction(s,doc,'Suppression after rejected Close',[set_value('A','Suppressed',True)])
        assert '_CadSuppressionShape' in objects(captured)['A']['property_types']
        assert objects(captured)['A']['suppression']['can_toggle']
        assert owned(captured,doc)['uuid']==owned(before,doc)['uuid']
        restored=transaction(s,doc,'Restore after rejected Close',[set_value('A','Suppressed',False)])
        assert '_CadSuppressionShape' not in objects(restored)['A']['property_types']
        assert objects(restored)['A']['properties']==objects(before)['A']['properties']
        s.call('close',document=doc)
        records.append(dict(failed_native_close_retains_observer_and_native_identity=True))
        keeper=s.call('new',name='SuppressionOpenKeeper')['document']
        before=transaction(s,keeper,'Keeper before failed Open',[add('Part::Box','Box')])
        s.call('open',path=str(saved),suppression_validate_failures=1,expect_error='native_error')
        restored=s.call('inspect',document=keeper)
        assert restored==before
        assert restored['suppression_managed_documents']==[keeper]
        assert set(restored['native_inventory'])=={keeper}
        s.call('close',document=keeper)
        records.append(dict(failed_open_cleanup_observer_inventory_and_keeper_exact=True))
        (a.output/'results.json').write_text(json.dumps(records,indent=2)+'\n')
        print(json.dumps(records,indent=2))
    finally:s.close()

if __name__=='__main__':main()
