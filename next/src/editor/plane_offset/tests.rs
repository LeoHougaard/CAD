use super::*;
use eframe::egui;
fn prepared() -> (Editor, std::sync::mpsc::Receiver<Job>, std::sync::mpsc::Sender<Reply>) {
    let (mut e, input, output) = super::super::rectangular_extrude_tests::fixture();
    let obj = json!({"name":"Plane","label":"Plane","body":"Body","type":"PartDesign::Plane","native_id":101,
        "plane_offset":{"version":1,"feature":"Plane","feature_id":101,"support":"Source","support_id":102,
        "revision":17,"origin":[0,0,0],"axis":[1,0,0],"length":10.,"flipped":false,"frame":[1,0,0,0]},
        "property_types":{"AttachmentOffset":"App::PropertyPlacement","MapReversed":"App::PropertyBool",
            "AttachmentSupport":"App::PropertyLinkSubList","MapMode":"App::PropertyEnumeration"},
        "property_metadata":{"AttachmentOffset":{"quantity":{"position":[{},{},{"value":10.,"input":"10 mm","default_unit":"mm"}]}}}});
    e.state["document_uid"] = json!("mock-plane-document");
    e.state["preview"] = json!({"token":"plane-test"});
    e.state["objects"] = json!([obj.clone(),{"name":"Source","body":"Body","native_id":102,"type":"PartDesign::Plane","effective_visibility":true},
        {"name":"Body","native_id":103,"type":"PartDesign::Body"}]);
    e.property_ui.schema = obj;
    e.draft = Some(Draft { id:"native.properties".into(), name:"Offset plane".into(), token:"plane-test".into(),
        parameters:json!({"create":false,"name":"Plane","body":"Body","type":"PartDesign::Plane","properties":{},"expressions":{},
        "offset_plane":{"document_uid":"mock-plane-document","support":"Source","support_id":102,"body_id":103,"feature_id":101,"magnitude":10.,"flipped":false}}),
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
fn actual_app_plane_offset_pointer_preview_release_cancel() {
    let (mut e,input,_output)=prepared(); let ctx=egui::Context::default();
    frame(&mut e,&ctx,vec![]);frame(&mut e,&ctx,vec![]);
    let end=e.viewport.depth_endpoint().expect("plane arrow on actual canvas");
    let state=e.state.clone();let selection=e.viewport.selected.clone();
    let origin=e.viewport.test_world_screen_point([0.,0.,0.]);
    let hit=(origin+end.to_vec2())*0.5;
    frame(&mut e,&ctx,vec![egui::Event::PointerMoved(hit)]);
    frame(&mut e,&ctx,vec![button(hit,true)]);
    let pos=hit+egui::vec2(65.,0.);
    frame(&mut e,&ctx,vec![egui::Event::PointerMoved(pos)]);
    let length=e.draft.as_ref().unwrap().parameters["offset_plane"]["magnitude"].as_f64().unwrap();
    assert_ne!(length,10.);
    frame(&mut e,&ctx,vec![button(pos,false)]);
    frame(&mut e,&ctx,vec![egui::Event::PointerMoved(pos+egui::vec2(30.,0.))]);
    assert_eq!(e.draft.as_ref().unwrap().parameters["offset_plane"]["magnitude"],length);
    assert_eq!(e.viewport.selected,selection); assert_eq!(e.state,state);
    e.draft.as_mut().unwrap().changed=Some(Instant::now()-Duration::from_millis(200));
    frame(&mut e,&ctx,vec![]);
    let preview=input.try_recv().unwrap();assert_eq!(preview.op,"preview");
    let edit=preview.parameters["edits"].as_array().unwrap().iter().find(|e|e["op"]=="plane_offset").unwrap();
    assert_eq!(edit["magnitude"],length);assert_eq!(edit["support_id"],102);
    e.busy=false;e.finish_preview(false);assert_eq!(input.try_recv().unwrap().op,"preview_cancel");
}
#[test]
fn plane_offset_stale_pending_expression_readonly_and_redo_guards() {
    let (mut e,_,_)=prepared();let owner=e.offset_plane_descriptor(true).unwrap().owner;
    let before=e.draft.as_ref().unwrap().parameters.clone();
    e.offset_plane_edit("stale",20.);e.offset_plane_edit(&owner,f64::NAN);
    assert_eq!(e.draft.as_ref().unwrap().parameters,before);
    e.state["revision"]=json!(18);assert!(e.offset_plane_descriptor(true).is_none());
    e.state["revision"]=json!(17);e.state["objects"][1]["native_id"]=json!(999);assert!(e.offset_plane_descriptor(true).is_none());
    e.state["objects"][1]["native_id"]=json!(102);
    e.state["objects"][0]["expressions"]=json!({"AttachmentOffset.Base.z":"10 mm"});assert!(e.offset_plane_descriptor(true).is_none());
    e.state["objects"][0]["expressions"]=json!({});
    e.state["objects"][0]["property_metadata"]["MapReversed"]["read_only"]=json!(true);assert!(e.offset_plane_descriptor(true).is_none());
    e.state["objects"][0]["property_metadata"]["MapReversed"]["read_only"]=json!(false);
    e.draft.as_mut().unwrap().parameters["offset_plane"]["magnitude"]=json!({"quantity_input":"1.5 in","default_unit":"mm"});
    assert!(e.offset_plane_descriptor(true).is_none());
    let edits=crate::property_commands::build(&e.draft.as_ref().unwrap().parameters).unwrap();
    assert_eq!(edits.last().unwrap()["magnitude"]["quantity_input"],"1.5 in");
    e.draft.as_mut().unwrap().parameters["offset_plane"]["magnitude"]=json!(-1.);
    assert!(crate::property_commands::build(&e.draft.as_ref().unwrap().parameters).unwrap_err().contains("Flip"));
    e.draft.as_mut().unwrap().parameters["offset_plane"]["magnitude"]=json!(10.);
    e.draft.as_mut().unwrap().deferred=true;assert!(e.offset_plane_descriptor(true).is_none());
}
#[test]
fn selected_plane_authoring_and_reopen_keep_generic_properties_available() {
    let (mut e,input,_)=prepared();e.draft=None;
    e.start_offset_plane("Source");
    let d=e.draft.as_ref().unwrap(); assert_eq!(d.parameters["body"],"Body");
    assert_eq!(d.parameters["properties"]["MapMode"],"FlatFace");
    assert_eq!(d.parameters["offset_plane"]["support_id"],102);
    assert_eq!(input.try_recv().unwrap().op,"feature_schema");
    e.busy=false;e.draft=None;e.edit_offset_plane("Plane");assert!(e.draft.as_ref().unwrap().parameters["offset_plane"].is_object());
    e.draft=None;e.edit_native_properties("Plane");assert!(e.draft.as_ref().unwrap().parameters.get("offset_plane").is_none());
}

fn text_position(out: &egui::FullOutput, needle: &str) -> egui::Pos2 {
    fn find(shape: &egui::epaint::Shape, needle: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == needle => Some(t.pos+egui::vec2(3.,5.)),
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s|find(s,needle)),
            _ => None,
        }
    }
    out.shapes.iter().find_map(|s|find(&s.shape,needle)).unwrap_or_else(||panic!("Missing text {needle}"))
}
#[test]
fn actual_app_plane_typed_literal_and_flip_follow_preview_path() {
    let (mut e,input,_)=prepared();let ctx=egui::Context::default();
    frame(&mut e,&ctx,vec![]);let out=frame(&mut e,&ctx,vec![]);
    let field=text_position(&out,"10 mm");
    frame(&mut e,&ctx,vec![egui::Event::PointerMoved(field),button(field,true)]);
    frame(&mut e,&ctx,vec![button(field,false)]);
    frame(&mut e,&ctx,vec![egui::Event::Key {key:egui::Key::A,physical_key:None,pressed:true,repeat:false,
        modifiers:egui::Modifiers {ctrl:true,command:true,..Default::default()}},egui::Event::Text("1.5 in".into())]);
    assert_eq!(e.draft.as_ref().unwrap().parameters["offset_plane"]["magnitude"]["quantity_input"],"1.5 in");
    assert!(e.offset_plane_descriptor(true).is_none(),"pending text must not be overwritten by arrow");
    let out=frame(&mut e,&ctx,vec![]);let flip=text_position(&out,"Flip direction");
    frame(&mut e,&ctx,vec![egui::Event::PointerMoved(flip),button(flip,true)]);
    frame(&mut e,&ctx,vec![button(flip,false)]);
    assert_eq!(e.draft.as_ref().unwrap().parameters["offset_plane"]["flipped"],true);
    e.draft.as_mut().unwrap().changed=Some(Instant::now()-Duration::from_millis(200));
    frame(&mut e,&ctx,vec![]);
    let job=input.try_recv().unwrap();assert_eq!(job.op,"preview");
    let edit=job.parameters["edits"].as_array().unwrap().iter().find(|v|v["op"]=="plane_offset").unwrap();
    assert_eq!(edit["magnitude"]["quantity_input"],"1.5 in");assert_eq!(edit["flipped"],true);
}

#[test]
fn older_plane_reply_preserves_newer_typed_input_and_apply_uses_owned_preview() {
    let (mut e,input,output)=prepared();
    let owner=e.offset_plane_descriptor(true).unwrap().owner;
    e.offset_plane_edit(&owner,20.);e.preview();assert_eq!(input.try_recv().unwrap().op,"preview");
    e.draft.as_mut().unwrap().parameters["offset_plane"]["magnitude"]=json!({"quantity_input":"1.5 in","default_unit":"mm"});
    e.draft.as_mut().unwrap().changed=Some(Instant::now());
    output.send(Reply {op:"preview".into(),accepted:true,state:e.state.clone(),scenes:vec![],
        error:None,elapsed:Duration::ZERO,data:Value::Null}).unwrap();e.poll();
    assert_eq!(e.draft.as_ref().unwrap().parameters["offset_plane"]["magnitude"]["quantity_input"],"1.5 in");
    assert!(!e.draft.as_ref().unwrap().valid);
    // A subsequently accepted solved preview enables the ordinary commit.
    e.draft.as_mut().unwrap().changed=None;
    output.send(Reply {op:"preview".into(),accepted:true,state:e.state.clone(),scenes:vec![],
        error:None,elapsed:Duration::ZERO,data:Value::Null}).unwrap();e.poll();
    assert!(e.draft.as_ref().unwrap().valid);e.finish_preview(true);
    let commit=input.try_recv().unwrap();assert_eq!(commit.op,"preview_commit");assert_eq!(commit.parameters["token"],"plane-test");
}
