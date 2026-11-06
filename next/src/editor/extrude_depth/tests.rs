use super::*;
use eframe::egui;

fn prepared() -> (Editor, std::sync::mpsc::Receiver<Job>, std::sync::mpsc::Sender<Reply>) {
    let (mut e, input, output) = super::super::rectangular_extrude_tests::fixture();
    let mut obj: Value = serde_json::from_str(include_str!("../../../tests/fixtures/offline-pad-schema.json")).unwrap();
    obj["name"] = json!("Pad"); obj["native_id"] = json!(101);
    obj["extrude_handle"] = json!({"feature":"Pad","feature_id":101,"profile":"Profile","profile_id":102,
        "revision":17,"origin":[0,0,0],"axis":[1,0,0],"signed_length":10.,"factor":1.});
    e.state["document_uid"] = json!("mock-depth-document");
    e.state["preview"] = json!({"token":"depth-test"});
    e.state["objects"] = json!([obj.clone(),{"name":"Profile","native_id":102}]);
    e.property_ui.schema = obj;
    e.draft = Some(Draft { id:"native.properties".into(), name:"Extrude".into(), token:"depth-test".into(),
        parameters:json!({"create":false,"name":"Pad","type":"PartDesign::Pad","properties":{},"expressions":{}}),
        changed:None,valid:true,all_parameters:false,deferred:false,committing:false });
    (e,input,output)
}
fn frame(e: &mut Editor, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(egui::RawInput { screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,
        egui::vec2(1280.,850.))),events,..Default::default() },
        |ctx| eframe::App::update(e,ctx,&mut eframe::Frame::_new_kittest()))
}
fn button(pos:egui::Pos2,pressed:bool)->egui::Event {
    egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}
}

#[test]
fn actual_app_depth_pointer_preview_field_release_and_cancel() {
    let (mut e,input,_output)=prepared(); let ctx=egui::Context::default();
    frame(&mut e,&ctx,vec![]); let painted=frame(&mut e,&ctx,vec![]);
    let end=e.viewport.depth_endpoint().expect("visible handle from actual canvas");
    assert!(!painted.shapes.is_empty());
    let state=e.state.clone(); let selection=e.viewport.selected.clone();
    // Human-friendly offset hit inside the existing logical point allowance.
    let axis=(e.viewport.test_world_screen_point([1.,0.,0.])-e.viewport.test_world_screen_point([0.,0.,0.])).normalized();
    let hit=end+egui::vec2(-axis.y,axis.x)*9.;
    frame(&mut e,&ctx,vec![egui::Event::PointerMoved(hit)]);
    frame(&mut e,&ctx,vec![button(hit,true)]);
    frame(&mut e,&ctx,vec![egui::Event::PointerMoved(hit+egui::vec2(65.,0.))]);
    let length=e.draft.as_ref().unwrap().parameters["properties"]["Length"].as_f64().unwrap();
    assert_ne!(length,10.);
    assert_eq!(e.property_ui.schema["property_metadata"]["Length"]["quantity"]["value"],json!(length));
    frame(&mut e,&ctx,vec![button(hit+egui::vec2(65.,0.),false)]);
    frame(&mut e,&ctx,vec![egui::Event::PointerMoved(hit+egui::vec2(90.,0.))]);
    assert_eq!(e.draft.as_ref().unwrap().parameters["properties"]["Length"],length,
        "pointer movement after release must not extend the depth");
    assert_eq!(e.viewport.selected,selection);
    assert_eq!(e.state,state,"input alone does not mutate native state/history");
    assert!(input.try_recv().is_err());
    e.draft.as_mut().unwrap().changed=Some(Instant::now()-Duration::from_millis(200));
    frame(&mut e,&ctx,vec![]);
    let preview=input.try_recv().expect("ordinary App debounce must send preview");
    assert_eq!(preview.op,"preview");assert_eq!(preview.parameters["token"],"depth-test");
    assert!(preview.parameters["edits"].as_array().unwrap().iter().any(|edit|
        edit["property"]=="Length" && edit["value"]==length));
    assert!(input.try_recv().is_err());
    // Transport is a mock, so release busy explicitly before testing Cancel.
    e.busy=false; e.finish_preview(false);
    let cancel=input.try_recv().unwrap(); assert_eq!(cancel.op,"preview_cancel");
    assert_eq!(cancel.parameters["token"],"depth-test");
}

#[test]
fn actual_app_offset_miss_does_not_edit_or_send() {
    let (mut e,input,_output)=prepared();let ctx=egui::Context::default();
    frame(&mut e,&ctx,vec![]);frame(&mut e,&ctx,vec![]);
    let axis=(e.viewport.test_world_screen_point([1.,0.,0.])-e.viewport.test_world_screen_point([0.,0.,0.])).normalized();
    let miss=e.viewport.depth_endpoint().unwrap()+egui::vec2(-axis.y,axis.x)*11.;
    frame(&mut e,&ctx,vec![egui::Event::PointerMoved(miss)]);
    frame(&mut e,&ctx,vec![button(miss,true)]);
    frame(&mut e,&ctx,vec![egui::Event::PointerMoved(miss+egui::vec2(55.,0.))]);
    frame(&mut e,&ctx,vec![button(miss+egui::vec2(55.,0.),false)]);
    assert_eq!(e.draft.as_ref().unwrap().parameters["properties"],json!({}));
    assert!(input.try_recv().is_err());
}

#[test]
fn invalid_stale_redo_and_signed_zero_edits_are_guarded() {
    let (mut e,input,_output)=prepared();let owner=e.extrude_depth_descriptor(true).unwrap().owner;
    let before=e.draft.as_ref().unwrap().parameters.clone();
    e.extrude_depth_edit("stale",22.); e.extrude_depth_edit(&owner,f64::NAN);
    assert_eq!(e.draft.as_ref().unwrap().parameters,before);
    e.extrude_depth_edit(&owner,-8.);
    assert_eq!(e.draft.as_ref().unwrap().parameters["properties"]["Reversed"],true);
    e.extrude_depth_edit(&owner,0.);
    assert_eq!(e.draft.as_ref().unwrap().parameters["properties"]["Length"],0.);
    e.draft.as_mut().unwrap().parameters["properties"]["Length"]=json!({"quantity_input":"bad in"});
    assert!(e.extrude_depth_descriptor(true).is_none());
    e.draft.as_mut().unwrap().parameters["properties"]=json!({});
    e.state["objects"][1]["native_id"]=json!(999);
    assert!(e.extrude_depth_descriptor(true).is_none());
    e.state["objects"][1]["native_id"]=json!(102);
    e.draft.as_mut().unwrap().deferred=true;
    assert!(e.extrude_depth_descriptor(true).is_none());
    assert!(input.try_recv().is_err());
}

#[test]
fn older_preview_reply_preserves_newer_drag_quantity() {
    let (mut e,input,output)=prepared();
    let owner=e.extrude_depth_descriptor(true).unwrap().owner;
    e.extrude_depth_edit(&owner,20.);e.preview();
    assert_eq!(input.try_recv().unwrap().op,"preview");
    e.extrude_depth_edit(&owner,25.4);
    output.send(Reply {op:"preview".into(),accepted:true,state:e.state.clone(),
        scenes:vec![],error:None,elapsed:Duration::ZERO,data:Value::Null}).unwrap();
    e.poll();
    let d=e.draft.as_ref().unwrap();
    assert_eq!(d.parameters["properties"]["Length"],25.4);
    assert!(d.changed.is_some());assert!(!d.valid);
    assert_eq!(e.property_ui.schema["property_metadata"]["Length"]["quantity"]["input"],"25.4 mm");
    assert!(input.try_recv().is_err());
}
