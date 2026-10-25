"""Persistent native suppression through public revision/preview boundaries."""
import argparse
import json
from pathlib import Path

from history_workflows import seed, compile_edits, fingerprint, same
from native_workflows import objects, set_value, transaction, near
from sketch_editor_workflows import SketchSession

CAPTURE = '_CadSuppressionPlacement'


def suppression_edits(compiler, state, name, value):
    return compile_edits(compiler, '--native-properties', {
        'type': objects(state)[name]['type'], 'name': name,
        'create': False, 'properties': {'Suppressed': value}})


def model(state):
    return {'geometry': fingerprint(state), 'suppression': {
        obj['name']: {p: obj['properties'][p] for p in ['Suppressed', CAPTURE]
                      if p in obj['properties']} for obj in state['objects']}}


def direct(session, compiler, output):
    doc, base = seed(session, 'SuppressionDirect', True)
    initial = model(base)
    edits = suppression_edits(compiler, base, 'A', True)
    assert compile_edits(compiler, '--native-history', dict(state=base,
        request=dict(op='feature_suppression',object='A',suppressed=True))) == edits
    state = session.call('preview', document=doc, token='suppress', edits=edits)['state']
    props = objects(state)['A']
    assert props['properties'][CAPTURE] == objects(base)['A']['properties']['Placement']
    assert props['property_metadata'][CAPTURE]['read_only']
    assert props['property_metadata'][CAPTURE]['hidden']
    near(objects(state)['Body']['shape']['volume'], 1027)
    session.call('preview', document=doc, token='suppress',
                 edits=edits + [set_value('A', 'Missing', 1)], expect_error='unsupported_property')
    same(model(session.call('inspect', document=doc)), model(state))
    restored = session.call('preview_cancel', document=doc, token='suppress')
    same(model(restored), initial)
    session.call('transaction', document=doc, label='Failed capture',
                 edits=edits + [set_value('A', 'Missing', 1)], expect_error='unsupported_property')
    restored = session.call('inspect', document=doc)
    same(model(restored), initial)
    session.call('preview', document=doc, token='suppress', edits=edits)
    committed = session.call('preview_commit', document=doc, token='suppress')
    same(model(session.call('undo', document=doc)), initial)
    same(model(session.call('redo', document=doc)), model(committed))
    session.call('transaction', document=doc,
                 edits=[set_value('A', CAPTURE, {'position':[0,0,0]})],
                 expect_error='read_only_property')
    session.call('inspect', document=doc)
    for field in ['_CadSuppressionShape', 'Shape']:
        session.call('transaction', document=doc,
                     edits=[set_value('A', field, 7)], expect_error='read_only_property')
        session.call('inspect', document=doc)
    session.call('transaction', document=doc,
                 edits=[dict(op='expression', object='A', property='_CadSuppressionShape', expression='Base.Shape')],
                 expect_error='read_only_property')
    session.call('inspect', document=doc)
    saved = output/'direct-suppressed.FCStd'
    session.call('save', document=doc, path=str(saved))
    session.call('close', document=doc)
    state = session.call('open', path=str(saved)); doc = state['document']
    reopened = model(state)
    state = transaction(session, doc, 'Unsuppress restored native feature',
                        suppression_edits(compiler, state, 'A', False))
    near(objects(state)['Body']['shape']['volume'],1043)
    assert objects(state)['A']['properties']['Placement'] == objects(base)['A']['properties']['Placement']
    same(model(session.call('undo', document=doc)), reopened)
    same(model(session.call('redo', document=doc)), model(state))
    session.call('close', document=doc)
    return {'public_compiler':True, 'preview_failure_cancel':True,
            'failed_registration_atomic':True,'native_history':True,'reopen_placement':True}


def expression(session, compiler, output):
    doc, base = seed(session, 'SuppressionExpression')
    state = transaction(session, doc, 'Native suppression formula', [
        dict(op='expression', object='A', property='Suppressed', expression='Base.Height > 11 mm')])
    baseline = model(state)
    suppressed = transaction(session, doc, 'Parent suppresses feature', [set_value('Base','Height',12)])
    assert objects(suppressed)['A']['properties']['Suppressed']
    assert objects(suppressed)['A']['properties'][CAPTURE] == objects(base)['A']['properties']['Placement']
    same(model(session.call('undo',document=doc)),baseline)
    same(model(session.call('redo',document=doc)),model(suppressed))
    path=output/'expression-suppressed.FCStd'
    session.call('save',document=doc,path=str(path));session.call('close',document=doc)
    state=session.call('open',path=str(path));doc=state['document']
    suppressed=model(state)
    session.call('transaction',document=doc,edits=[set_value('Base','Height',10),
                 set_value('A','Missing',1)],expect_error='unsupported_property')
    same(model(session.call('inspect',document=doc)),suppressed)
    state=transaction(session,doc,'Parent restores feature',[set_value('Base','Height',10)])
    near(objects(state)['Body']['shape']['volume'],1043)
    assert objects(state)['A']['properties']['Placement'] == objects(base)['A']['properties']['Placement']
    same(model(session.call('undo',document=doc)),suppressed)
    same(model(session.call('redo',document=doc)),model(state))
    session.call('close',document=doc)
    return {'expression_native_capture':True,'parent_history':True,
            'expression_reopen':True,'failed_parent_change_atomic':True}


def legacy(session, path):
    state=session.call('open',path=str(path.resolve()));doc=state['document']
    before=model(state)
    session.call('transaction',document=doc,edits=[set_value('A','Suppressed',False)],
                 expect_error='suppression_state_unavailable')
    restored=session.call('inspect',document=doc)
    same(model(restored),before)
    assert restored['available']
    session.call('close',document=doc)
    return {'missing_legacy_placement_not_invented':True,'failure_atomic':True}


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--host',type=Path,required=True);p.add_argument('--compiler',type=Path,required=True)
    p.add_argument('--runtime',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    p.add_argument('--legacy',type=Path)
    a=p.parse_args();a.output.mkdir(parents=True)
    session=SketchSession(a.host,json.loads(a.runtime.read_text()))
    try:
        report={'direct':direct(session,a.compiler,a.output),'expression':expression(session,a.compiler,a.output)}
        if a.legacy:report['legacy']=legacy(session,a.legacy)
        (a.output/'report.json').write_text(json.dumps(report,indent=2))
        print(json.dumps(report,indent=2))
    finally:session.close()


if __name__=='__main__':main()
