"""Current native Fillet and unit-wrapped Hole inputs after suppressed reopen."""
import argparse,json
from pathlib import Path
from native_workflows import set_value,transaction,objects
from history_workflows import same
from suppression_workflows import model
from suppression_checkpoint_workflows import RecordedSession

def inputs(state):
    h=objects(state)['Hole']
    return {k:h['properties'][k] for k in ['Depth','Diameter','ThreadType','ThreadSize','HoleCutType','HoleCutCustomValues','HoleCutDepth','HoleCutDiameter','HoleCutCountersinkAngle']}

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for k in ['host','runtime','fillet','hole','output']:p.add_argument('--'+k,type=Path,required=True)
    a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
    s=RecordedSession(a.host.resolve(),json.loads(a.runtime.read_text()),a.output);results={}
    try:
        opened=s.call('open',path=str(a.fillet.resolve()));doc=opened['document']
        transaction(s,doc,'Native-invalid radius while suppressed',[set_value('Fillet','Radius',100)])
        path=a.output/'invalid-suppressed.FCStd';s.call('save',document=doc,path=str(path));s.call('close',document=doc)
        opened=s.call('open',path=str(path));doc=opened['document']
        s.call('transaction',document=doc,edits=[set_value('Fillet','Suppressed',False)],expect_error='invalid_model')
        restored=s.call('inspect',document=doc);same(model(restored),model(opened))
        assert restored['available'] and restored['undo_count']==opened['undo_count']
        assert '_CadSuppressionShape' in objects(restored)['Fillet']['property_types']
        fixed=transaction(s,doc,'Correct current radius',[set_value('Fillet','Radius',.75),set_value('Fillet','Suppressed',False)])
        assert '_CadSuppressionShape' not in objects(fixed)['Fillet']['property_types']
        same(model(s.call('undo',document=doc)),model(opened));same(model(s.call('redo',document=doc)),model(fixed))
        s.call('close',document=doc);results['fillet_current_radius_and_native_history']=True
        opened=s.call('open',path=str(a.hole.resolve()));doc=opened['document']
        s.call('document_settings',document=doc,expected_revision=s.revisions[doc],unit_system=3)
        def quantity(text):return dict(quantity_input=text,default_unit='in')
        configured=transaction(s,doc,'Native inch Hole controller inputs',[
            set_value('Hole','HoleCutType','Counterbore'),set_value('Hole','HoleCutCustomValues',True),
            set_value('Hole','HoleCutDepth',quantity('0.01')),set_value('Hole','HoleCutDiameter',quantity('0.25'))])
        base=inputs(configured);path=a.output/'hole-configured.FCStd';s.call('save',document=doc,path=str(path))
        suppressed=s.call('preview',document=doc,token='hole-suppression',edits=[set_value('Hole','Suppressed',True)])['state']
        assert '_CadSuppressionShape' in objects(suppressed)['Hole']['property_types']
        s.call('document_settings',document=doc,expected_revision=s.revisions[doc],unit_system=3,expect_error='preview_active')
        s.call('preview',document=doc,token='hole-suppression',edits=[set_value('Hole','Suppressed',True),set_value('Hole','Shape',7)],expect_error='read_only_property')
        recovered=s.call('inspect',document=doc);assert inputs(recovered)==base and recovered['preview']
        cancelled=s.call('preview_cancel',document=doc,token='hole-suppression')
        assert inputs(cancelled)==base and cancelled['units']['index']==3
        assert '_CadSuppressionShape' not in objects(cancelled)['Hole']['property_types']
        transaction(s,doc,'Save suppressed Hole',[set_value('Hole','Suppressed',True)])
        path=a.output/'hole-suppressed.FCStd';s.call('save',document=doc,path=str(path));s.call('close',document=doc)
        opened=s.call('open',path=str(path));doc=opened['document'];assert inputs(opened)==base and opened['units']['index']==3
        assert objects(opened)['Hole']['property_metadata']['HoleCutDepth']['quantity']['default_unit']=='in'
        history=(opened['undo_count'],opened['redo_count'])
        settings=s.call('document_settings',document=doc,expected_revision=s.revisions[doc],unit_system=0)
        assert (settings['undo_count'],settings['redo_count'])==history and inputs(settings)==base
        changed=transaction(s,doc,'Current native wrapped cut depth after restore',[
            set_value('Hole','HoleCutDepth',quantity('0.02')),set_value('Hole','Suppressed',False)])
        expected=s.call('quantities',document=doc,expected_revision=s.revisions[doc],items=[dict(unit='mm',text='0.02 in')])['items'][0]['base_value']
        assert objects(changed)['Hole']['properties']['HoleCutDepth']==expected
        for key in ['_CadSuppressionShape','_CadSuppressionPlacement']:assert key not in objects(changed)['Hole']['property_types']
        path=a.output/'hole-current.FCStd';s.call('save',document=doc,path=str(path));s.call('close',document=doc)
        reopened=s.call('open',path=str(path));assert inputs(reopened)==inputs(changed) and reopened['units']['index']==0
        s.call('close',document=reopened['document'])
        results['hole_units_current_inputs_preview_failure_cancel_and_two_reopens']=True
        (a.output/'results.json').write_text(json.dumps(results,indent=2)+'\n')
    finally:s.close()

if __name__=='__main__':main()
