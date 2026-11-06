//! Real App scheduling with explicit synthetic native replies; no geometry qualification.
use super::*;
#[test]
fn helix_callback_schema_debounce_preview_and_cancel() {
    let (mut e,input,output)=super::rectangular_extrude_tests::fixture();
    e.sketch=None;
    e.state["objects"]=json!([{"name":"Pad","type":"PartDesign::Pad","native_id":9007199254740993u64}]);
    e.viewport.set_scene(json!({"object":"Pad","revision":17}),json!({"faces":[{"subelement":"Face1","mapped_name":"stable-face"}]}));
    e.viewport.selected=Some(crate::viewport::Selection{object:"Pad".into(),subelement:"Face1".into()});
    e.start_native_feature("Part::Helix");
    assert_eq!(input.try_recv().unwrap().op,"feature_schema");
    assert_eq!(e.draft.as_ref().unwrap().parameters["helix_reference"]["source_id"],"9007199254740993");
    assert!(input.try_recv().is_err());
    e.busy=false;
    e.native_schema_reply(json!({"type":"Part::Helix","properties":{},"property_types":{},"property_metadata":{}}),true);
    e.draft.as_mut().unwrap().parameters["properties"]=json!({"Turns":15.,"StartAngle":{"quantity_input":"0 deg","default_unit":"deg"},"LocalCoord":"Left-handed"});
    e.draft.as_mut().unwrap().changed=Some(Instant::now()-Duration::from_millis(1000));
    let ctx=egui::Context::default();
    let _=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1280.,850.))),..Default::default()},|ctx|eframe::App::update(&mut e,ctx,&mut eframe::Frame::_new_kittest()));
    let preview=input.try_recv().unwrap();assert_eq!(preview.op,"preview");
    assert_eq!(preview.parameters["edits"][0]["op"],"create_standalone_helix");
    assert!(preview.parameters["edits"].as_array().unwrap().iter().any(|x|x["property"]=="Turns" && x["value"]==15.));
    let mut state=e.state.clone();
    state["preview"]=json!({"token":e.draft.as_ref().unwrap().token,"valid":true});
    output.send(Reply{op:"preview".into(),accepted:true,state,scenes:vec![],error:None,elapsed:Duration::ZERO,data:Value::Null}).unwrap();
    e.poll();assert!(e.draft.as_ref().unwrap().valid);
    e.finish_preview(false);assert_eq!(input.try_recv().unwrap().op,"preview_cancel");
    let mut state=e.state.clone();state["preview"]=Value::Null;
    output.send(Reply{op:"preview_cancel".into(),accepted:true,state,scenes:vec![],error:None,elapsed:Duration::ZERO,data:Value::Null}).unwrap();
    e.poll();assert!(e.draft.is_none());
}
#[test]
fn helix_missing_or_stale_selection_cannot_open_task() {
    let (mut e,input,_output)=super::rectangular_extrude_tests::fixture();
    e.viewport.clear_selection(); e.start_native_feature("Part::Helix");
    assert!(e.draft.is_none());assert!(input.try_recv().is_err());
    e.sketch=None;
    e.state["objects"]=json!([{"name":"Pad","type":"PartDesign::Pad","native_id":1}]);
    e.viewport.set_scene(json!({"object":"Pad","revision":16}),json!({"faces":[{"subelement":"Face1","mapped_name":"old"}]}));
    e.viewport.selected=Some(crate::viewport::Selection{object:"Pad".into(),subelement:"Face1".into()});
    e.start_native_feature("Part::Helix");
    assert!(e.draft.is_none());assert!(input.try_recv().is_err());
    assert!(e.error.as_deref().unwrap().contains("stale"));
}
