//! Full App / MOCK transport only. Native geometry is explicitly unrun.
use super::*;
fn mirror_fixture() -> (Editor,Receiver<Job>,Sender<Reply>) {
    let (mut e,input,output)=fixture();
    e.state["document_uid"]=json!("mock");
    e.state["objects"]=json!([
        {"name":"Body","type":"PartDesign::Body","label":"Body","native_id":10,"effective_visibility":true,"properties":{"Tip":{"link":"Pocket"}},
         "mirror_source":{"link":"Body","native_id":10,"document_uid":"mock","body":null,"body_id":null,"frame":[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]}},
        {"name":"Datum","type":"PartDesign::Plane","label":"Datum","native_id":20,"body":"Body","effective_visibility":true,
         "mirror_plane_source":{"link":"Datum","native_id":20,"document_uid":"mock","body":"Body","body_id":10,"frame":[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]}},
        {"name":"Sketch","type":"Sketcher::SketchObject","native_id":30}]);
    e.selected_object=Some("Body".into());
    e.viewport.selected=Some(crate::viewport::Selection{object:"Body".into(),subelement:"".into()});
    e.start_native_feature("Part::Mirroring");
    assert_eq!(input.try_recv().unwrap().op,"feature_schema");
    e.busy=false;e.native_schema_reply(json!({"properties":{}}),true);
    e.draft.as_mut().unwrap().changed=None;
    (e,input,output)
}
#[test]
fn part_mirror_full_app_selected_body_plane_preview_cancel() {
    let (mut e,input,output)=mirror_fixture();let ctx=egui::Context::default();
    let original=e.state["objects"].clone();
    let p=&e.draft.as_ref().unwrap().parameters;
    assert_eq!(p["part_mirror"]["source"]["link"],"Body");
    let edits=crate::property_commands::build(p).unwrap();
    assert_eq!(edits[0]["type"],"Part::Mirroring");assert!(edits[0].get("body").is_none());
    pointer(&mut e,&ctx,"Select entities");
    e.viewport.selections=vec![crate::viewport::Selection{object:"Datum".into(),subelement:"".into()}];
    e.collect_reference_field();assert!(e.error.is_none(),"{:?}",e.error);settle(&mut e,&ctx);
    let job=input.try_recv().unwrap();assert_eq!(job.op,"preview");
    assert_eq!(job.parameters["edits"].as_array().unwrap().last().unwrap()["task"]["plane"]["native_id"],20);
    let mut state=e.state.clone();state["preview"]=json!({"token":e.draft.as_ref().unwrap().token});
    state["objects"].as_array_mut().unwrap().push(json!({"name":"Mirroring","type":"Part::Mirroring"}));
    reply(&output,&mut e,&ctx,job,true,Value::Null,state);
    assert_eq!(e.state["objects"][0],original[0]); // MOCK source/Tip preservation only
    pointer(&mut e,&ctx,"Cancel");let cancel=input.try_recv().unwrap();assert_eq!(cancel.op,"preview_cancel");
    let mut state=e.state.clone();state["objects"]=original.clone();state["preview"]=Value::Null;
    reply(&output,&mut e,&ctx,cancel,true,Value::Null,state);assert!(e.draft.is_none());assert_eq!(e.state["objects"],original);
}
#[test]
fn part_mirror_full_app_apply_and_existing_edit_payload() {
    let (mut e,input,_)=mirror_fixture();let ctx=egui::Context::default();
    e.draft.as_mut().unwrap().valid=true;
    e.state["preview"]=json!({"token":e.draft.as_ref().unwrap().token});
    pointer(&mut e,&ctx,"Apply");assert_eq!(input.try_recv().unwrap().op,"preview_commit");
    let mut p=e.draft.as_ref().unwrap().parameters.clone();p["create"]=json!(false);
    p["part_mirror"]["feature_id"]=json!(41);
    let edits=crate::property_commands::build(&p).unwrap();assert_eq!(edits.len(),1);
    assert_eq!(edits[0]["task"]["feature_id"],41);assert_eq!(edits[0]["object"],p["name"]);
}
#[test]
fn part_mirror_full_app_bad_selection_stale_reply_and_origin_controls() {
    let (mut e,input,output)=mirror_fixture();let ctx=egui::Context::default();
    for (n,t) in [("Sketch",""),("Datum",""),("Gone",""),("Body","Face1")] {
        assert!(super::super::part_mirror::reference(&e.state,n,t,false).is_err());
    }
    assert!(super::super::part_mirror::initial(&e.state,&[]).is_err());
    // Native origin axes remain an explicitly developer-only payload capability.
    e.draft.as_mut().unwrap().parameters["part_mirror"]["axis"]=json!("YZ");
    e.draft.as_mut().unwrap().changed=Some(Instant::now());
    settle(&mut e,&ctx);let job=input.try_recv().unwrap();
    assert_eq!(job.parameters["edits"].as_array().unwrap().last().unwrap()["task"]["axis"],"YZ");
    let state=e.state.clone();reply(&output,&mut e,&ctx,job,false,Value::Null,state);
    assert!(e.draft.is_some());assert!(!e.draft.as_ref().unwrap().valid);
    assert_eq!(e.state["objects"][0]["properties"]["Tip"]["link"],"Pocket");
}

#[test]
fn part_mirror_full_app_open_existing_task_keeps_identity() {
    let (mut e,input,_)=mirror_fixture();let ctx=egui::Context::default();
    let mut task=e.draft.as_ref().unwrap().parameters["part_mirror"].clone();task["feature_id"]=json!(90);
    e.draft=None;
    e.state["objects"].as_array_mut().unwrap().push(json!({"name":"MirrorSaved","label":"MirrorSaved",
        "type":"Part::Mirroring","native_id":90,"part_mirror":task,"properties":{}}));
    e.edit_native_properties("MirrorSaved");full_frame(&mut e,&ctx,vec![]);
    pointer(&mut e,&ctx,"Select entities");
    e.viewport.selections=vec![crate::viewport::Selection{object:"Datum".into(),subelement:"".into()}];
    e.collect_reference_field();settle(&mut e,&ctx);
    let job=input.try_recv().unwrap();assert_eq!(job.op,"preview");
    let edits=job.parameters["edits"].as_array().unwrap();assert_eq!(edits.len(),1);
    assert_eq!(edits[0]["object"],"MirrorSaved");assert_eq!(edits[0]["task"]["feature_id"],90);
}
