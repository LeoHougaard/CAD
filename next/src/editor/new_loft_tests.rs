//! Authored host input controls. Not compiled or executed by R361.
use super::*;
fn loft_fixture() -> (Editor, Receiver<Job>, Sender<Reply>) {
    let (mut e,input,output)=fixture();
    e.state["document_uid"]=json!("uid");
    e.state["objects"]=json!((0..3).map(|i|json!({"name":format!("S{i}"),"label":format!("S{i}"),
        "type":"Sketcher::SketchObject","native_id":100+i,"effective_visibility":true,"body":"Body",
        "global_placement":{"position":[0,0,i*20],"rotation":[0,0,0,1]},
        "loft_source":{"link":format!("S{i}"),"target":"","source_id":100+i,"body":"Body","body_id":90,
            "document_uid":"uid","frame":[1,0,0,0,0,1,0,0,0,0,1,i*20,0,0,0,1]},
        "loft_vertices":{"Vertex1":"mapped-1"}})).collect::<Vec<_>>());
    e.start_native_feature("Part::Loft");
    let schema=input.try_recv().unwrap();assert_eq!(schema.op,"feature_schema");
    e.busy=false;
    e.native_schema_reply(json!({"properties":{"Solid":true,"Ruled":false,"Closed":false}}),true);
    e.draft.as_mut().unwrap().changed=None;
    (e,input,output)
}
#[test]
fn new_loft_full_app_origin_order_correction_remove_and_source_unchanged() {
    let (mut e,input,_)=loft_fixture();let ctx=egui::Context::default();let original=e.state.clone();
    full_frame(&mut e,&ctx,vec![]);
    for (name,target) in [("S0","Origin"),("S1",""),("S2","Vertex1")] {
        e.viewport.selected=Some(crate::viewport::Selection{object:name.into(),subelement:target.into()});
        e.viewport.selections.clear();pointer(&mut e,&ctx,"Append selection");
        e.draft.as_mut().unwrap().changed=None;
    }
    let p=&e.draft.as_ref().unwrap().parameters;
    assert_eq!(p["loft_sections"][0]["target"],"Origin");
    assert_eq!(p["loft_sections"][2]["mapped_name"],"mapped-1");
    let edits=crate::property_commands::build(p).unwrap();
    assert_eq!(edits[0]["type"],"Part::Loft");assert!(edits[0].get("body").is_none());
    assert_eq!(edits.last().unwrap()["sections"],p["loft_sections"]);
    e.viewport.selected=Some(crate::viewport::Selection{object:"S1".into(),subelement:"Face1".into()});
    pointer(&mut e,&ctx,"Correct");
    e.draft.as_mut().unwrap().changed=None;
    assert_eq!(e.draft.as_ref().unwrap().parameters["loft_sections"][0]["link"],"S1");
    pointer(&mut e,&ctx,"Remove");
    assert_eq!(e.draft.as_ref().unwrap().parameters["loft_sections"].as_array().unwrap().len(),2);
    assert_eq!(e.state,original);assert!(input.try_recv().is_err());
}
#[test]
fn new_loft_full_app_origin_collector_stale_owner_and_apply_cancel_dispatch() {
    let (mut e,input,_)=loft_fixture();let ctx=egui::Context::default();
    full_frame(&mut e,&ctx,vec![]);pointer(&mut e,&ctx,"Show sketch origins");
    // The canvas precedes the task panel; consume its toggle on the next frame.
    full_frame(&mut e,&ctx,vec![]);
    assert_eq!(e.viewport.loft_origins.len(),3);
    let pos=e.viewport.test_world_screen_point([0.,0.,0.]);
    assert_eq!(e.viewport.test_scene_pick(pos).unwrap().subelement,"Origin");
    assert!(super::super::new_loft::section_source(&e.state,"Gone","Origin").is_err());
    e.draft.as_mut().unwrap().parameters["loft_sections"]=json!([
        super::super::new_loft::section_source(&e.state,"S0","Origin").unwrap(),
        super::super::new_loft::section_source(&e.state,"S1","").unwrap()]);
    e.draft.as_mut().unwrap().changed=None;e.draft.as_mut().unwrap().valid=true;
    // Synthetic accepted-preview receipt; production button and request path.
    e.state["preview"]=json!({"token":e.draft.as_ref().unwrap().token});
    pointer(&mut e,&ctx,"Cancel");assert_eq!(input.try_recv().unwrap().op,"preview_cancel");
}

#[test]
fn new_loft_full_app_apply_uses_existing_preview_commit() {
    let (mut e,input,_)=loft_fixture();let ctx=egui::Context::default();
    full_frame(&mut e,&ctx,vec![]);
    let token=e.draft.as_ref().unwrap().token.clone();
    e.draft.as_mut().unwrap().valid=true;e.draft.as_mut().unwrap().changed=None;
    e.state["preview"]=json!({"token":token});
    pointer(&mut e,&ctx,"Apply");
    let job=input.try_recv().unwrap();assert_eq!(job.op,"preview_commit");
    assert_eq!(job.parameters["token"],token);
}
#[test]
fn new_loft_full_app_rejected_preview_callback_retains_sources_and_allows_retry() {
    let (mut e,input,output)=loft_fixture();let ctx=egui::Context::default();
    full_frame(&mut e,&ctx,vec![]);
    let original=e.state.clone();
    e.busy=true;
    output.send(Reply{op:"preview".into(),accepted:false,state:original.clone(),scenes:vec![],
        error:Some("stale_reference: source identity changed".into()),elapsed:Duration::ZERO,data:Value::Null}).unwrap();
    full_frame(&mut e,&ctx,vec![]);
    assert!(!e.busy);assert!(e.draft.is_some());assert_eq!(e.state["objects"],original["objects"]);
    assert!(e.error.as_deref().unwrap().contains("stale_reference"));
    e.viewport.selected=Some(crate::viewport::Selection{object:"S0".into(),subelement:"Origin".into()});
    pointer(&mut e,&ctx,"Append selection");
    assert_eq!(e.draft.as_ref().unwrap().parameters["loft_sections"][0]["source_id"],100);
    assert!(input.try_recv().is_err());
}
