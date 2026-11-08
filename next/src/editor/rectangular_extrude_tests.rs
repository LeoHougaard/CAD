use super::*;

#[test]
fn passive_rectangle_projection_uses_actual_release_and_preview_points() {
    rectangle_canvas_input([0.,0.,0.],9);
}
#[test]
fn center_rectangle_axis_curve_click_stays_freehand() {
    rectangle_canvas_input([10.,0.,0.],8);
}
fn rectangle_canvas_input(center_model:[f64;3],expected_constraints:usize) {
    let (mut editor,input,_output)=fixture();
    // Explicit MOCK native state. All interactions below use the real canvas.
    editor.state["document_uid"]=json!("mock-projection-uid");
    editor.state["objects"]=json!([{"name":"Profile","label":"Profile","native_id":8801,
        "type":"Sketcher::SketchObject","global_placement":{"position":[0,0,0],"rotation":[0,0,0,1]},
        "sketch":{"geometry_count":0,"constraint_count":0,"geometry":[],"constraints":[],"axes":[
            {"index":-1,"kind":"line","start":[0,0,0],"end":[1,0,0]},
            {"index":-2,"kind":"line","start":[0,0,0],"end":[0,1,0]}]}}]);
    editor.state["units"]=json!({"sketch_frame":{"size_mm":[152.4,152.4],"purpose":"editing-guide"},
        "preview_length":{"scale":1.0,"default_unit":"mm","decimals":3}});
    editor.sketch=Some("Profile".into());
    editor.sketch_ui.session=Some("mock-edit-session".into());
    editor.sketch_ui.tool=crate::sketch_commands::Tool::CenterRectangle;
    editor.sync_sketch();editor.viewport.set_references(&editor.state);
    let state=editor.state.clone();
    let ctx=egui::Context::default();let mut time=0.;
    let mut frame=|editor:&mut Editor,events:Vec<egui::Event>| {
        time+=0.1;
        ctx.data_mut(|d| {
            d.remove::<Value>(egui::Id::new("convergence_input_projection"));
            d.remove::<Value>(egui::Id::new("convergence_feedback_projection"));
        });
        let _=ctx.run(egui::RawInput{time:Some(time),events,screen_rect:Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,egui::vec2(1280.,850.))),..Default::default()},|ctx| {
            egui::CentralPanel::default().show(ctx,|ui|editor.canvas(ui,false,false));
        });
    };
    frame(&mut editor,vec![]);frame(&mut editor,vec![]);
    let center=editor.viewport.sketch_screen_point(center_model);
    let free=editor.viewport.sketch_point(center).unwrap();
    if center_model[0]!=0. {
        assert_eq!(editor.viewport.sketch_hit(center),Some(crate::sketch_commands::SketchElement{index:-1,point:0}));
    }
    let button=|pos,pressed|egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,
        pressed,modifiers:egui::Modifiers::NONE};
    frame(&mut editor,vec![egui::Event::PointerMoved(center)]);
    frame(&mut editor,vec![button(center,true)]);
    assert!(ctx.data(|d|d.get_temp::<Value>(egui::Id::new("convergence_input_projection"))).is_none());
    frame(&mut editor,vec![button(center,false)]);
    let first=ctx.data(|d|d.get_temp::<Value>(egui::Id::new("convergence_input_projection"))).unwrap();
    assert_eq!(first["point"],if center_model[0]==0. {json!([0.,0.,0.])} else {json!(free)});
    assert_eq!(first["live"],json!([center.x,center.y]));
    assert_eq!(first["document_uid"],state["document_uid"]);
    assert!(input.try_recv().is_err());
    let corner=editor.viewport.sketch_screen_point([20.,17.,0.]);
    frame(&mut editor,vec![egui::Event::PointerMoved(corner)]);
    let preview=ctx.data(|d|d.get_temp::<Value>(egui::Id::new("convergence_feedback_projection"))).unwrap();
    assert_eq!(preview["center_point"],first["point"]);
    frame(&mut editor,vec![button(corner,true)]);
    assert!(ctx.data(|d|d.get_temp::<Value>(egui::Id::new("convergence_input_projection"))).is_none());
    frame(&mut editor,vec![button(corner,false)]);
    let accepted=ctx.data(|d|d.get_temp::<Value>(egui::Id::new("convergence_input_projection"))).unwrap();
    assert_eq!(accepted["point"],preview["point"]);
    assert_eq!(accepted["live"],json!([corner.x,corner.y]));
    let job=input.try_recv().expect("actual canvas rectangle transaction");
    assert_eq!(job.op,"transaction");
    assert_eq!(job.parameters["edits"].as_array().unwrap().iter().filter(|e|e["op"]=="geometry").count(),4);
    assert!(editor.error.is_none(),"supported freehand center must not report an edge constraint error");
    assert_eq!(job.parameters["edits"].as_array().unwrap().iter().filter(|e|e["op"]=="constraint").count(),expected_constraints);
    assert!(input.try_recv().is_err(),"passive receipts add no model requests");
    assert_eq!(editor.state,state,"passive receipt capture does not adopt or mutate model state");
}
#[test]
fn active_sketch_frame_is_not_a_finished_revolution_axis_overlay() {
    let (mut editor,input,_output)=fixture();
    let profile=json!({"name":"Profile","label":"Profile","native_id":8801,
        "type":"Sketcher::SketchObject",
        "global_placement":{"position":[0,0,0],"rotation":[0,0,0,1]},
        "sketch":{"geometry_count":0,"geometry":[],"axes":[
            {"index":-1,"kind":"line","start":[0,0,0],"end":[1,0,0]},
            {"index":-2,"kind":"line","start":[0,0,0],"end":[0,1,0]}]}});
    // Explicit MOCK state only; production canvas decides the editing context.
    editor.state["objects"]=json!([profile]);
    editor.state["units"]=json!({"sketch_frame":{"size_mm":[152.4,152.4],"purpose":"editing-guide"}});
    editor.sketch=Some("Profile".into());
    editor.sketch_ui.session=Some("mock-edit-session".into());
    editor.sync_sketch();
    editor.viewport.set_references(&editor.state);
    let ctx=egui::Context::default();
    let frame=|editor:&mut Editor| {
        let _=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,egui::vec2(1280.,850.))),..Default::default()},|ctx| {
            egui::CentralPanel::default().show(ctx,|ui|editor.canvas(ui,false,true));
        });
    };
    let state=editor.state.clone();
    frame(&mut editor);
    assert!(editor.viewport.sketch_plane_corners().is_some());
    assert_eq!(editor.state,state);
    // Same finished profile remains available for picking, with no edit lease.
    editor.sketch_ui.session=None;
    editor.sketch=None;
    editor.property_ui.axis_pick=true;
    editor.property_ui.axis_profile=Some("Profile".into());
    editor.draft=Some(Draft{id:"native.properties".into(),name:"Revolution".into(),
        token:"mock-revolution".into(),parameters:json!({"type":"PartDesign::Revolution",
            "properties":{"Profile":{"link":"Profile"}}}),changed:None,valid:true,
        committing:false,all_parameters:false,deferred:false});
    frame(&mut editor);
    assert!(editor.viewport.sketch_matches(&Some(profile)));
    assert!(editor.viewport.sketch_plane_corners().is_none());
    let axis=editor.viewport.sketch_axis_hit(editor.viewport.sketch_screen_point([5.,0.,0.]));
    assert!(axis.is_some(),"finished profile axes remain available to the actual picker");
    assert_eq!(editor.state,state);
    assert!(input.try_recv().is_err(),"no model request from read-only context rendering");
}

pub(super) fn fixture() -> (Editor, Receiver<Job>, Sender<Reply>) {
    let state = json!({"document":"ActualDocument","revision":17,"available":true,"preview":null,"sketch_edit":null,
        "drawing":{"document_uid":"actual-uid","objects":[]},
        "drawing_capabilities":{"available":true,"version":3,"projection":"Third angle","templates":[{"id":"ANSI-A-ASME-landscape-v1","paper_mm":[279.4,215.9]}]},
        "objects":[{"name":"ActualAssembly","label":"Source assembly","type":"Assembly::AssemblyObject","native_id":901}]});
    let (jobs, input) = mpsc::sync_channel(1);
    let (output, replies) = mpsc::channel();
    let editor = Editor {
        jobs,
        replies,
        busy: false,
        registry: Registry::builtin().unwrap(),
        state: state.clone(),
        viewport: Viewport::default(),
        sketch: None,
        selected_object: None,
        sketch_ui: SketchUi::default(),
        sketch_creation: None,
        body_ui: BodyUi::default(),
        property_ui: properties::PropertyUi::default(),
        measurement_ui: measurements::MeasurementUi::default(),
        units_ui: Default::default(),
        transform_ui: transforms::TransformUi::default(),
        exchange_ui: exchange::ExchangeUi::default(),
        draft: None,
        sequence: 17,
        error: None,
        status: String::new(),
        path: "retained-prefix.FCStd".into(),
        file_input: String::new(),
        file_dialog: None,
        file_browser: file_browser::FileBrowser::default(),
        overwrite: false,
        dirty: true,
        pending_file: None,
        allow_close: false,
        fit_next: false,
    };
    (editor, input, output)
}
// Fixture construction is generated from the existing Editor test fixture.
fn full_frame(e: &mut Editor, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    let modifiers = events.iter().find_map(|event| match event {
        egui::Event::Key { modifiers, .. } | egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
        _ => None,
    }).unwrap_or_default();
    ctx.run(egui::RawInput {modifiers,screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1280.,850.))),events,..Default::default()},
        |ctx| eframe::App::update(e,ctx,&mut eframe::Frame::_new_kittest()))
}
fn pointer(e:&mut Editor,ctx:&egui::Context,text:&str) {
    full_frame(e,ctx,vec![]);
    let out=full_frame(e,ctx,vec![]);
    let pos=out.shapes.iter().find_map(|s|match &s.shape {
        egui::Shape::Text(t) if t.galley.text()==text=>Some(t.pos+t.galley.size()*0.5),_=>None
    }).unwrap_or_else(||panic!("Missing actual rendered target {text}"));
    for pressed in [true,false] { full_frame(e,ctx,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}]); }
}
fn replace_text(e:&mut Editor,ctx:&egui::Context,old:&str,new:&str) {
    pointer(e,ctx,old);
    full_frame(e,ctx,vec![egui::Event::Key{key:Key::A,physical_key:None,pressed:true,repeat:false,modifiers:egui::Modifiers::CTRL | egui::Modifiers::COMMAND},egui::Event::Text(new.into())]);
    full_frame(e,ctx,vec![egui::Event::Key{key:Key::A,physical_key:None,pressed:false,repeat:false,modifiers:egui::Modifiers::NONE}]);
}
fn settle(e:&mut Editor,ctx:&egui::Context) {
    // Advance only debounce time; values still enter through egui pointer/text events.
    if let Some(d)=&mut e.draft { if d.changed.is_some(){d.changed=Some(Instant::now()-Duration::from_secs(1));} }
    full_frame(e,ctx,vec![]);
}
fn reply(output:&Sender<Reply>,e:&mut Editor,ctx:&egui::Context,job:Job,accepted:bool,data:Value,state:Value) {
    output.send(Reply{op:job.op,accepted,state,scenes:vec![],error:if accepted{None}else{Some("injected rejection".into())},elapsed:Duration::ZERO,data}).unwrap();
    full_frame(e,ctx,vec![]);
}
fn rectangular_fixture()->(Editor,Receiver<Job>,Sender<Reply>) {
    let (mut e,input,output)=fixture();
    let b=json!({"eligible":true,"version":1,"document":"Live","document_uid":"current-test-owner","revision":1,
        "source":{"name":"Profile","native_id":401},"body":{"name":"Body","native_id":402},
        "boundary":{"geometry_ids":[11,22,33,44],"mapped_edges":["g11;","g22;","g33;","g44;"]}});
    e.state=json!({"document":"Live","document_uid":"current-test-owner","revision":1,"available":true,"redo_count":0,"preview":null,
        "rectangular_capabilities":{"version":1},"objects":[
        {"name":"Body","native_id":402,"type":"PartDesign::Body","body":"Body","body_state":{"tip":"Profile"}},
        {"name":"Profile","native_id":401,"type":"Sketcher::SketchObject","body":"Body","rectangular_profile":b}]});
    e.selected_object=Some("Profile".into());
    e.start_native_feature_for_profile("PartDesign::Pad",Some("Profile"));
    (e,input,output)
}
fn quantity(state:&Value,source:&str,value:f64)->Value {
    json!({"document":state["document"],"revision":state["revision"],"items":[{"source":source,"input":"native formatted quantity","base_value":value}]})
}
fn start(e:&mut Editor,input:&Receiver<Job>,output:&Sender<Reply>,ctx:&egui::Context) {
    let job=input.try_recv().unwrap();assert_eq!(job.op,"feature_schema");
    // Native schema response, then actual fields render in full update.
    let state=e.state.clone();
    reply(output,e,ctx,job,true,json!({"properties":{"Length":10.0},"property_meta":{}}),state);
    settle(e,ctx);
    let query=input.try_recv().unwrap();assert_eq!(query.op,"quantities");
    let state=e.state.clone();reply(output,e,ctx,query,true,quantity(&state,"0 deg",0.0),state);
    let initial=input.try_recv().unwrap();assert_eq!(initial.op,"preview");
    accept_preview(e,input,output,ctx,initial,500);
    replace_text(e,ctx,"10 mm","2 in");settle(e,ctx);
    let j=input.try_recv().unwrap();assert_eq!(j.parameters["edits"][0]["length"],"2 in");accept_preview(e,input,output,ctx,j,501);
}
fn accept_preview(e:&mut Editor,_input:&Receiver<Job>,output:&Sender<Reply>,ctx:&egui::Context,job:Job,id:u64) {
    assert_eq!(job.op,"preview");assert_eq!(job.parameters["edits"].as_array().unwrap().len(),1);
    let d=e.draft.as_ref().unwrap();let mut m=d.parameters["binding"].clone();
    m["target"]=json!({"name":d.parameters["name"],"native_id":id});
    let mut state=e.state.clone();state["revision"]=json!(state["revision"].as_u64().unwrap()+1);
    m["revision"]=state["revision"].clone();
    let feature=json!({"name":d.parameters["name"],"native_id":id,"type":"PartDesign::FeaturePython","rectangular_draft":m});
    state["objects"].as_array_mut().unwrap().truncate(2);state["objects"].as_array_mut().unwrap().push(feature);
    state["preview"]=json!({"token":d.token});
    reply(output,e,ctx,job,true,Value::Null,state);
    assert!(e.draft.as_ref().unwrap().valid);
}
fn magnitude(e:&mut Editor,input:&Receiver<Job>,output:&Sender<Reply>,ctx:&egui::Context,old:&str,new:&str,parsed:f64) {
    replace_text(e,ctx,old,new);settle(e,ctx);
    let q=input.try_recv().unwrap();assert_eq!(q.op,"quantities");assert_eq!(q.parameters["items"][0]["text"],new);
    let state=e.state.clone();reply(output,e,ctx,q,true,quantity(&state,new,parsed),state);
}
#[test]
fn full_update_four_transitions_and_stale_apply() {
    let(mut e,input,output)=rectangular_fixture();let ctx=egui::Context::default();start(&mut e,&input,&output,&ctx);
    let token=e.draft.as_ref().unwrap().token.clone();
    magnitude(&mut e,&input,&output,&ctx,"0 deg","3 deg",3.);
    let j=input.try_recv().unwrap();assert_eq!(j.parameters["token"],token);assert_eq!(j.parameters["edits"][0]["angle"],"3 deg");assert_eq!(j.parameters["edits"][0]["length"],"2 in");
    accept_preview(&mut e,&input,&output,&ctx,j,502);
    pointer(&mut e,&ctx,"Inward");settle(&mut e,&ctx);
    let j=input.try_recv().unwrap();assert_eq!(j.parameters["edits"][0]["angle"],"-3 deg");accept_preview(&mut e,&input,&output,&ctx,j,503);
    magnitude(&mut e,&input,&output,&ctx,"3 deg","30 deg",30.);
    let j=input.try_recv().unwrap();assert_eq!(j.parameters["edits"][0]["angle"],"-30 deg");assert_eq!(j.parameters["token"],token);accept_preview(&mut e,&input,&output,&ctx,j,504);
    replace_text(&mut e,&ctx,"30 deg","31 deg");pointer(&mut e,&ctx,"Apply");assert!(input.try_recv().is_err());assert!(!e.draft.as_ref().unwrap().valid);
    pointer(&mut e,&ctx,"Cancel");let j=input.try_recv().unwrap();assert_eq!(j.op,"preview_cancel");assert!(e.draft.is_some());
    let mut state=e.state.clone();state["preview"]=Value::Null;reply(&output,&mut e,&ctx,j,true,Value::Null,state);assert!(e.draft.is_none());
}
#[test]
fn full_update_source_negative_magnitude_branch_and_rejection() {
    let(mut e,input,output)=rectangular_fixture();let ctx=egui::Context::default();start(&mut e,&input,&output,&ctx);
    magnitude(&mut e,&input,&output,&ctx,"0 deg","3 deg",3.);let j=input.try_recv().unwrap();accept_preview(&mut e,&input,&output,&ctx,j,602);
    let before=e.state.clone();magnitude(&mut e,&input,&output,&ctx,"3 deg","-3 deg",-3.);
    assert!(input.try_recv().is_err());assert_eq!(e.state,before);assert!(!e.draft.as_ref().unwrap().valid);assert!(e.error.as_deref().unwrap().contains("nonnegative"));
    magnitude(&mut e,&input,&output,&ctx,"-3 deg","30 deg",30.);let j=input.try_recv().unwrap();assert_eq!(j.parameters["edits"][0]["angle"],"30 deg");accept_preview(&mut e,&input,&output,&ctx,j,603);
    pointer(&mut e,&ctx,"Inward");settle(&mut e,&ctx);let j=input.try_recv().unwrap();assert_eq!(j.parameters["edits"][0]["angle"],"-30 deg");
    let mut state=e.state.clone();state["revision"]=json!(99);reply(&output,&mut e,&ctx,j,false,Value::Null,state);
    assert!(!e.draft.as_ref().unwrap().valid);assert!(!e.draft.as_ref().unwrap().committing);
    e.finish_preview(true);assert!(input.try_recv().is_err());pointer(&mut e,&ctx,"Cancel");assert_eq!(input.try_recv().unwrap().op,"preview_cancel");
}
#[test]
fn semantic_encoding_and_configuration_are_exact() {
    let mut p=json!({"operation":"New","termination":"Blind","side":"one-side","solid":true,"expression":"","direction":"Normal","profile_input":"Profile","source":{"name":"Profile"},
        "length":"2 in","magnitude":"0.1 rad","parsed_input":"0.1 rad","parsed_magnitude":5.729577951308233,
        "inward":true,"reversed":false,"target":{"name":"Current","native_id":7}});
    let e=rectangular_extrude::compile(&p).unwrap().remove(0);
    assert_eq!(e["angle"],"-5.729577951308233 deg");assert_eq!(e["length"],"2 in");
    let keys:std::collections::BTreeSet<_>=e.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys,["op","operation","termination","side","target","length","angle","reversed"].into_iter().collect());
    for (key,value) in [("operation",json!("Pocket")),("side",json!("Two sides")),("expression",json!("Other.Angle")),("parsed_magnitude",json!(-3))] {
        let old=p[key].clone();p[key]=value;assert!(rectangular_extrude::compile(&p).is_err());p[key]=old;
    }
}

#[test]
fn deferred_redo_apply_and_current_semantic_edit_are_isolated() {
    let(mut e,input,output)=rectangular_fixture();let ctx=egui::Context::default();
    e.state["redo_count"]=json!(1);e.draft.as_mut().unwrap().deferred=true;
    let schema=input.try_recv().unwrap();let state=e.state.clone();reply(&output,&mut e,&ctx,schema,true,json!({"properties":{"Length":10.0}}),state);
    settle(&mut e,&ctx);let q=input.try_recv().unwrap();let state=e.state.clone();reply(&output,&mut e,&ctx,q,true,quantity(&state,"0 deg",0.0),state);
    assert!(input.try_recv().is_err());assert!(e.draft.as_ref().unwrap().valid);
    pointer(&mut e,&ctx,"Apply");let j=input.try_recv().unwrap();assert_eq!(j.op,"transaction");assert_eq!(j.parameters["edits"][0]["op"],"create_rectangular_draft");
    let state=e.state.clone();reply(&output,&mut e,&ctx,j,false,Value::Null,state);assert!(!e.draft.as_ref().unwrap().committing);
    pointer(&mut e,&ctx,"Cancel");assert!(e.draft.is_none());assert!(input.try_recv().is_err());
    // Metadata supplied by a fresh Open. The old pre-close ID is never used.
    let mut m=e.state["objects"][1]["rectangular_profile"].clone();
    m["target"]=json!({"name":"Restored","native_id":9001});m["length"]=json!(50.8);m["angle"]=json!(-30.);m["reversed"]=json!(false);
    e.state["objects"].as_array_mut().unwrap().push(json!({"name":"Restored","native_id":9001,"type":"PartDesign::FeaturePython","rectangular_draft":m}));
    e.edit_native_properties("Restored");assert_eq!(e.draft.as_ref().unwrap().id,rectangular_extrude::ID);
    settle(&mut e,&ctx);let q=input.try_recv().unwrap();let state=e.state.clone();reply(&output,&mut e,&ctx,q,true,quantity(&state,"30 deg",30.0),state);
    pointer(&mut e,&ctx,"Apply");let j=input.try_recv().unwrap();assert_eq!(j.parameters["edits"][0]["op"],"update_rectangular_draft");assert_eq!(j.parameters["edits"][0]["target"]["native_id"],9001);
}

#[test]
fn delayed_quantity_and_stopped_queue_never_enable_apply() {
    let(mut e,input,output)=rectangular_fixture();let ctx=egui::Context::default();start(&mut e,&input,&output,&ctx);
    replace_text(&mut e,&ctx,"0 deg","3 deg");settle(&mut e,&ctx);let q=input.try_recv().unwrap();
    // Typing while a quantity is pending must invalidate the reply binding.
    replace_text(&mut e,&ctx,"3 deg","4 deg");let state=e.state.clone();
    reply(&output,&mut e,&ctx,q,true,quantity(&state,"3 deg",3.0),state);
    assert!(input.try_recv().is_err());assert!(!e.draft.as_ref().unwrap().valid);
    let(jobs,receiver)=mpsc::sync_channel(1);drop(receiver);e.jobs=jobs;
    settle(&mut e,&ctx);assert_eq!(e.error.as_deref(),Some("Model thread stopped"));
    assert!(!e.draft.as_ref().unwrap().valid);assert!(!e.draft.as_ref().unwrap().committing);
}

#[test]
fn cross_body_and_reference_modes_stay_stock_before_modeling() {
 for mode in [None,Some("independent"),Some("dependent"),Some("cross_reference")] {
  let(mut e,input,_output)=rectangular_fixture();let _schema=input.try_recv().unwrap();
  let original=e.draft.as_ref().unwrap().parameters.clone();
  e.draft.as_mut().unwrap().parameters["body"]=json!("TargetBody");
  if let Some(mode)=mode {e.draft.as_mut().unwrap().parameters["reference_choice"]=json!({"request":{"source":"Profile","body":"TargetBody","mode":mode}});}
  let before=e.draft.as_ref().unwrap().parameters.clone();
  assert!(!e.select_rectangular_route());assert_eq!(e.draft.as_ref().unwrap().id,"native.properties");
  let after=&e.draft.as_ref().unwrap().parameters;assert_eq!(after["body"],"TargetBody");assert_eq!(after["reference_choice"],before["reference_choice"]);assert_eq!(after["properties"],original["properties"]);assert_eq!(after["rectangular_route"],"stock");assert!(input.try_recv().is_err());
 }
}
#[test]
fn rectangular_quantity_receipts_reject_foreign_or_incomplete_data() {
 for bad in ["document","revision","source","count","value"] {
  let(mut e,input,output)=rectangular_fixture();let ctx=egui::Context::default();start(&mut e,&input,&output,&ctx);
  e.draft.as_mut().unwrap().parameters["magnitude"]=json!("3 deg");assert!(e.rectangular_quantity_request());
  let job=input.try_recv().unwrap();assert_eq!(job.op,"quantities");let state=e.state.clone();let mut data=quantity(&state,"3 deg",3.0);
  match bad {"document"=>data["document"]=json!("Other"),"revision"=>data["revision"]=json!(999),"source"=>data["items"][0]["source"]=json!("4 deg"),"count"=>data["items"]=json!([]),_=>data["items"][0]["base_value"]=Value::Null};
  reply(&output,&mut e,&ctx,job,true,data,state);assert!(!e.draft.as_ref().unwrap().valid);assert!(input.try_recv().is_err());
 }
}

#[path = "convergence_native_driver.rs"]
mod convergence_native_driver;

#[test]
fn passive_test_read_preserves_model_scene_and_invalid_draft() {
    // Declared offline model: exercises actual send and reply, not native geometry.
    let (mut e, input, output) = rectangular_fixture();
    let ctx=egui::Context::default();
    start(&mut e,&input,&output,&ctx);
    magnitude(&mut e,&input,&output,&ctx,"0 deg","-3 deg",-3.);
    assert!(!e.busy && !e.draft.as_ref().unwrap().valid);
    assert!(e.error.as_deref().unwrap().contains("nonnegative"));
    assert!(input.try_recv().is_err());
    e.measurement_ui=measurements::MeasurementUi::test_retained_receipt(&e.state);
    e.viewport.set_scene(json!({"object":"Profile","revision":1}),
        json!({"vertices":[{"subelement":"Vertex1","point":[3.,4.,5.]}]}));
    let snapshot = |e:&Editor| json!({
        "state":e.state,"path":e.path,"file_input":e.file_input,
        "dirty":e.dirty,"fit":e.fit_next,"error":e.error,"status":e.status,
        "measurement":e.measurement_ui.test_snapshot(),
        "selected":e.selected_object,"selection":format!("{:?}",e.viewport.selected),
        "selections":format!("{:?}",e.viewport.selections),
        "draft":e.draft.as_ref().map(|d|json!({"id":d.id,"token":d.token,"parameters":d.parameters,"valid":d.valid,"committing":d.committing,"changed":d.changed.is_some()})),
        "vertex":e.viewport.test_scene_vertex_world("Profile","Vertex1"),
        "camera":{ "origin":format!("{:?}",e.viewport.test_world_screen_point([0.,0.,0.])),"offset":format!("{:?}",e.viewport.test_world_screen_point([10.,20.,30.]))}
    });
    assert_eq!(e.viewport.test_scene_vertex_world("Profile","Vertex1"),Some([3.,4.,5.]));
    let before=snapshot(&e);
    for accepted in [true,false] {
        e.send("test_read",json!({"tag":"retained receipt"}));assert!(e.busy);
        let job=input.try_recv().expect("one actual passive observation job");
        assert_eq!(job.op,"test_read");
        assert_eq!(job.parameters,json!({"tag":"retained receipt","document":e.state["document"],"expected_revision":e.state["revision"]}));
        assert_eq!(snapshot(&e),before,"send must preserve validation and measurement before reply");
        assert!(input.try_recv().is_err());
        output.send(Reply{op:"test_read".into(),accepted,state:Value::Null,scenes:vec![],
            error:(!accepted).then(||"test observer rejected".into()),elapsed:Duration::ZERO,
            data:json!({"test_read":"actual data belongs in raw driver journal"})}).unwrap();
        e.poll();assert!(!e.busy);
        let mut expected=before.clone();
        if !accepted { expected["error"]=json!("test observer rejected"); }
        assert_eq!(snapshot(&e),expected);
        assert!(input.try_recv().is_err(),"passive reply must not queue modeling work");
    }
    drop(input);
    e.send("test_read",json!({"tag":"disconnected"}));
    assert!(!e.busy);assert_eq!(e.error.as_deref(),Some("Model thread stopped"));
    let mut expected=before;expected["error"]=json!("Model thread stopped");
    assert_eq!(snapshot(&e),expected,"enqueue failure stays visible without destroying receipt");
}

#[test]
fn owned_sketch_cancel_dispatches_current_revision_and_tokens() {
    let (mut editor,input,_output)=fixture();
    editor.state["sketch_edit"]=json!({"object":"Profile","token":"owned-sketch"});
    editor.sketch_ui.session=Some("owned-sketch".into());
    editor.send("sketch_cancel",json!({"token":"owned-sketch"}));
    let job=input.try_recv().expect("actual cancel dispatch");
    assert_eq!(job.op,"sketch_cancel");
    assert_eq!(job.parameters,json!({"document":"ActualDocument","expected_revision":17,
        "token":"owned-sketch","sketch_token":"owned-sketch"}));
    assert!(input.try_recv().is_err());
}

#[test]
fn diagnostic_cancel_adopts_without_creation_undo() {
    let (mut editor,input,output)=fixture();
    editor.prepare_new_sketch(); // Production new_sketch=true precondition.
    editor.sketch=Some("Profile".into());
    editor.sketch_ui.session=Some("owned-sketch".into());
    editor.state["sketch_edit"]=json!({"object":"Profile","token":"owned-sketch"});
    editor.diagnostic_cancel_sketch_for_close();
    let job=input.try_recv().expect("actual diagnostic Cancel");
    assert_eq!(job.op,"sketch_cancel");
    assert_eq!(job.parameters["token"],"owned-sketch");
    assert_eq!(job.parameters["sketch_token"],"owned-sketch");
    let mut state=editor.state.clone();state["revision"]=json!(18);state["sketch_edit"]=Value::Null;
    let ctx=egui::Context::default();
    reply(&output,&mut editor,&ctx,job,true,Value::Null,state);
    assert!(!editor.busy);assert!(editor.sketch_ui.session.is_none());assert!(editor.sketch.is_none());
    assert!(input.try_recv().is_err(),"cleanup Cancel must not queue creation Undo");
    editor.send("close",json!({}));
    let close=input.try_recv().expect("normal current-revision Close");
    assert_eq!(close.op,"close");
    assert_eq!(close.parameters,json!({"document":"ActualDocument","expected_revision":18}));
    assert!(input.try_recv().is_err());
}
#[test]
fn ordinary_new_sketch_cancel_keeps_creation_undo() {
    let (mut editor,input,output)=fixture();editor.prepare_new_sketch();
    editor.sketch=Some("Profile".into());editor.sketch_ui.session=Some("owned-sketch".into());
    editor.state["sketch_edit"]=json!({"object":"Profile","token":"owned-sketch"});
    editor.send("sketch_cancel",json!({"token":"owned-sketch"}));
    let job=input.try_recv().expect("ordinary Cancel");
    let mut state=editor.state.clone();state["revision"]=json!(18);state["sketch_edit"]=Value::Null;
    reply(&output,&mut editor,&egui::Context::default(),job,true,Value::Null,state);
    let undo=input.try_recv().expect("ordinary new-sketch Cancel retains creation Undo");
    assert_eq!(undo.op,"undo");assert_eq!(undo.parameters["expected_revision"],18);
    assert!(undo.parameters.get("sketch_token").is_none());assert!(input.try_recv().is_err());
}

fn dimension_escape_fixture() -> (Editor,Receiver<Job>,Sender<Reply>) {
    let (mut editor,input,output)=fixture();
    editor.state["document_uid"]=json!("mock-dimension-escape");
    editor.state["sketch_edit"]=json!({"object":"Profile","token":"owned-sketch"});
    editor.state["objects"]=json!([{"name":"Profile","label":"Profile","native_id":8801,
        "type":"Sketcher::SketchObject","global_placement":{"position":[0,0,0],"rotation":[0,0,0,1]},
        "sketch":{"geometry_count":4,"constraint_count":0,"constraints":[],"geometry":[
            {"index":0,"kind":"line","start":[-20,-25,0],"end":[20,-25,0]},
            {"index":1,"kind":"line","start":[20,-25,0],"end":[20,25,0]},
            {"index":2,"kind":"line","start":[20,25,0],"end":[-20,25,0]},
            {"index":3,"kind":"line","start":[-20,25,0],"end":[-20,-25,0]}],"axes":[]}}]);
    editor.sketch=Some("Profile".into());editor.sketch_ui.session=Some("owned-sketch".into());
    editor.sync_sketch();(editor,input,output)
}
fn dimension_key(editor:&mut Editor,ctx:&egui::Context,key:Key) {
    for pressed in [true,false] {
        full_frame(editor,ctx,vec![egui::Event::Key{key,physical_key:None,pressed,repeat:false,modifiers:egui::Modifiers::NONE}]);
    }
}
#[test]
fn dimension_escape_clears_tool_picks_before_next_d_shortcut() {
    let (mut editor,input,_output)=dimension_escape_fixture();
    let ctx=egui::Context::default();full_frame(&mut editor,&ctx,vec![]);
    dimension_key(&mut editor,&ctx,Key::D);assert!(editor.sketch_ui.dimension_tool.is_some());
    // Declared precondition: final real S22 selection was whole right line.
    editor.viewport.sketch_selection=vec![crate::sketch_commands::SketchElement{index:1,point:0}];
    let state=editor.state.clone();
    dimension_key(&mut editor,&ctx,Key::Escape);
    assert!(editor.sketch_ui.dimension_tool.is_none());assert!(editor.viewport.sketch_selection.is_empty());
    assert!(editor.sketch_ui.dimension.is_none());assert!(!editor.busy);assert_eq!(editor.state,state);
    dimension_key(&mut editor,&ctx,Key::D);
    assert!(editor.sketch_ui.dimension_tool.is_some());assert!(editor.sketch_ui.dimension.is_none());
    assert!(!editor.busy);assert!(input.try_recv().is_err(),"fresh D must not measure a stale selected line");
    assert_eq!(editor.state,state);
}
#[test]
fn escape_outside_dimension_preserves_ordinary_sketch_selection() {
    let (mut editor,input,_output)=dimension_escape_fixture();
    let ctx=egui::Context::default();full_frame(&mut editor,&ctx,vec![]);
    editor.viewport.sketch_selection=vec![crate::sketch_commands::SketchElement{index:1,point:0}];
    let selection=editor.viewport.sketch_selection.clone();let state=editor.state.clone();
    dimension_key(&mut editor,&ctx,Key::Escape);
    assert_eq!(editor.viewport.sketch_selection,selection);assert_eq!(editor.state,state);
    assert!(input.try_recv().is_err());
}

// R141: synthetic native replies, real full App/TextEdit/Apply paths. No native credit.
// Scope same-label controls to the current Dimension window, using its actual
// TextEdit response layer and current paint. No dialog state is changed here.
fn r154_dimension_pointer(e:&mut Editor,ctx:&egui::Context,text:&str) {
    full_frame(e,ctx,vec![]);
    let frame_before=ctx.cumulative_frame_nr();
    let out=full_frame(e,ctx,vec![]);
    let (field,_,frame)=ctx.data(|d|d.get_temp::<(egui::Response,egui::Rect,u64)>(egui::Id::new("convergence_dimension_response"))).unwrap();
    assert!((frame_before..=ctx.cumulative_frame_nr()).contains(&frame),"current Dimension response");
    let mut matches=Vec::new();let mut all=0;
    for shape in &out.shapes {
        if let egui::Shape::Text(t)=&shape.shape {
            if t.galley.text()!=text {continue;}
            all+=1;
            let rect=t.galley.rect.translate(t.pos.to_vec2());let pos=rect.center();
            if shape.clip_rect.contains_rect(rect) && ctx.layer_id_at(pos)==Some(field.layer_id) {
                matches.push(pos);
            }
        }
    }
    assert_eq!(matches.len(),1,"one current Dimension target {text}; all labels={all}");
    let pos=matches[0];
    println!("owned Dimension target {text}: labels={all}, layer={:?}, point={pos:?}",field.layer_id);
    for pressed in [true,false] {
        assert_eq!(ctx.layer_id_at(pos),Some(field.layer_id),"Dimension owns input");
        full_frame(e,ctx,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{
            pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}]);
    }
}

fn compact_dimension_fixture(compact:Option<&str>, lossless:&str) -> (Editor,Receiver<Job>,Sender<Reply>) {
    let (mut e,input,output)=dimension_escape_fixture();
    e.state["units"]=json!({"defaults":{"mm":{"default_unit":"in"}}});
    let mut q=json!({"input":lossless,"display":compact.unwrap_or(lossless),"default_unit":"in"});
    if let Some(text)=compact {q["edit_display"]=json!(text);}
    e.state["objects"][0]["sketch"]["constraints"]=json!([{"index":0,"type":"Distance","first":0,"first_pos":0,
        "second":-2000,"second_pos":0,"third":-2000,"third_pos":0,"value":62.712040119522754,"driving":true,"quantity":q}]);
    e.state["objects"][0]["sketch"]["constraint_count"]=json!(1);
    e.sync_sketch();e.dimension_at(0);(e,input,output)
}
#[test]
fn r141_full_app_compact_paint_and_untouched_apply_are_lossless() {
    let full="2.4689779574615258 in";
    let (mut e,input,_output)=compact_dimension_fixture(Some("2.469 in"),full);
    let ctx=egui::Context::default();full_frame(&mut e,&ctx,vec![]);
    let out=full_frame(&mut e,&ctx,vec![]);
    let id=ctx.memory(|m|m.focused()).unwrap();let response=ctx.read_response(id).unwrap();
    assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text=="2.469 in"
        && response.rect.contains_rect(t.galley.rect.translate(t.pos.to_vec2())))));
    r154_dimension_pointer(&mut e,&ctx,"Apply");let job=input.try_recv().unwrap();
    assert_eq!(job.op,"transaction");assert_eq!(job.parameters["edits"][0]["value"],full);
    assert_eq!(job.parameters["edits"][0]["input_default_unit"],"in");
}
#[test]
fn r141_full_app_explicit_text_failure_retry_and_cancel() {
    let (mut e,input,output)=compact_dimension_fixture(Some("2.469 in"),"2.4689779574615258 in");
    let ctx=egui::Context::default();full_frame(&mut e,&ctx,vec![]);
    r141_replace_dimension(&mut e,&ctx,"2 in");r154_dimension_pointer(&mut e,&ctx,"Apply");
    let job=input.try_recv().unwrap();assert_eq!(job.parameters["edits"][0]["value"],"2 in");
    let state=e.state.clone();reply(&output,&mut e,&ctx,job,false,Value::Null,state);
    assert_eq!(e.sketch_ui.dimension.as_ref().unwrap().value,"2 in");
    r154_dimension_pointer(&mut e,&ctx,"Apply");let retry=input.try_recv().unwrap();assert_eq!(retry.parameters["edits"][0]["value"],"2 in");
    let state=e.state.clone();reply(&output,&mut e,&ctx,retry,false,Value::Null,state);
    r154_dimension_pointer(&mut e,&ctx,"Cancel");assert!(e.sketch_ui.dimension.is_none());assert!(input.try_recv().is_err());
}
#[test]
fn r141_full_app_expression_reference_and_stale_target() {
    let (mut e,input,_output)=compact_dimension_fixture(Some("2.469 in"),"2.4689779574615258 in");
    let ctx=egui::Context::default();full_frame(&mut e,&ctx,vec![]);
    r154_dimension_pointer(&mut e,&ctx,"Expression");r141_replace_dimension(&mut e,&ctx,"Constraints.Other / 2");
    r154_dimension_pointer(&mut e,&ctx,"Apply");let job=input.try_recv().unwrap();
    assert_eq!(job.parameters["edits"][0]["op"],"expression");assert_eq!(job.parameters["edits"][0]["expression"],"Constraints.Other / 2");
    let (mut e,input,_)=compact_dimension_fixture(Some("2.469 in"),"2.4689779574615258 in");
    let ctx=egui::Context::default();full_frame(&mut e,&ctx,vec![]);
    e.state["objects"][0]["sketch"]["constraints"][0]["value"]=json!(63.0);
    r154_dimension_pointer(&mut e,&ctx,"Apply");assert!(input.try_recv().is_err());assert!(e.error.as_deref().unwrap().contains("changed"));
    r154_dimension_pointer(&mut e,&ctx,"Cancel");e.state["objects"][0]["sketch"]["constraints"][0]["driving"]=json!(false);
    e.dimension_at(0);assert!(e.sketch_ui.dimension.is_none());assert!(input.try_recv().is_err());
}
#[test]
fn r141_full_app_legacy_tiny_and_large_seeds_remain_exact() {
    for (compact,full) in [(None,"2.4689779574615258 in"),(Some("1e-12 in"),"1e-12 in"),
        (Some("1e+30 in"),"1e+30 in"),(Some("3/8 in"),"3/8 in")] {
        let (mut e,input,_)=compact_dimension_fixture(compact,full);let ctx=egui::Context::default();
        full_frame(&mut e,&ctx,vec![]);assert_eq!(e.sketch_ui.dimension.as_ref().unwrap().value,compact.unwrap_or(full));
        // Unsolicited quantity replies cannot overwrite an existing edit.
        e.quantity_reply(&json!({"items":[{"input":"99 mm","edit_display":"99 mm"}]}));
        r154_dimension_pointer(&mut e,&ctx,"Apply");let job=input.try_recv().unwrap();assert_eq!(job.parameters["edits"][0]["value"],full);
    }
}

#[test]
fn r141_full_app_new_dimension_quantity_reply_preserves_seed_and_typed_same_compact_intent() {
    for typed in [false,true] {
        let (mut e,input,output)=dimension_escape_fixture();let ctx=egui::Context::default();
        e.state["units"]=json!({"defaults":{"mm":{"default_unit":"in"}}});
        e.viewport.sketch_selection=vec![crate::sketch_commands::SketchElement{index:0,point:0}];
        e.new_dimension("Distance");
        let query=input.try_recv().unwrap();assert_eq!(query.op,"quantities");let state=e.state.clone();
        reply(&output,&mut e,&ctx,query,true,json!({"items":[{"input":"2.4689779574615258 in","edit_display":"2.469 in","default_unit":"in"}]}),state);
        full_frame(&mut e,&ctx,vec![]);assert_eq!(e.sketch_ui.dimension.as_ref().unwrap().value,"2.469 in");
        if typed {r141_replace_dimension(&mut e,&ctx,"2.469 in");}
        r154_dimension_pointer(&mut e,&ctx,"Apply");let job=input.try_recv().unwrap();
        let datum=job.parameters["edits"].as_array().unwrap().iter().find(|v|v["op"]=="datum").unwrap();
        assert_eq!(datum["value"],if typed {"2.469 in"} else {"2.4689779574615258 in"});
        assert_eq!(datum["input_default_unit"],"in");
    }
}

fn r141_replace_dimension(e:&mut Editor,ctx:&egui::Context,text:&str) {
    full_frame(e,ctx,vec![]);
    let (r,_,_)=ctx.data(|d|d.get_temp::<(egui::Response,egui::Rect,u64)>(egui::Id::new("convergence_dimension_response"))).unwrap();
    let pos=r.rect.center();
    for pressed in [true,false] {full_frame(e,ctx,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{
        pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}]);}
    full_frame(e,ctx,vec![egui::Event::Key{key:Key::A,physical_key:None,pressed:true,repeat:false,
        modifiers:egui::Modifiers::CTRL|egui::Modifiers::COMMAND},egui::Event::Text(text.into())]);
    full_frame(e,ctx,vec![egui::Event::Key{key:Key::A,physical_key:None,pressed:false,repeat:false,modifiers:egui::Modifiers::NONE}]);
}

#[test]
fn region_canvas_input_composes_profile_and_ordinary_axis_without_mutating_source() {
    let (mut editor,input,_output)=fixture();
    editor.state["objects"]=json!([
        {"name":"Body","type":"PartDesign::Body","native_id":101},
        {"name":"Sketch","type":"Sketcher::SketchObject","native_id":102,"body":"Body",
         "effective_visibility":true,"global_placement":{"position":[0,0,0],"rotation":[0,0,0,1]},
         "sketch":{"geometry":[{"kind":"line","persistent_id":44,"construction":false,"start":[-10,-10,0],"end":[10,-10,0]}]},
         "planar_regions":{"revision":17,"source_token":"mock-native-token","regions":[{"key":"stable-boundary-key",
         "triangles":[[[-10,-10,0],[10,-10,0],[0,10,0]]]}]}}
    ]);
    let original=editor.state.clone();
    editor.draft=Some(Draft{id:"native.properties".into(),name:"Revolution".into(),token:"mock-region".into(),
        parameters:json!({"create":true,"name":"Revolution","type":"PartDesign::Revolution","body":"Body",
        "properties":{"Profile":{"link":"Sketch"},"Angle":360},"expressions":{}}),
        changed:None,valid:false,committing:false,all_parameters:false,deferred:false});
    editor.property_ui.region_pick=true;
    let ctx=egui::Context::default(); let mut time=0.;
    let mut frame=|editor:&mut Editor,events| {
        time+=0.1;
        let _=ctx.run(egui::RawInput{time:Some(time),events,screen_rect:Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,egui::vec2(1280.,850.))),..Default::default()},|ctx| {
                egui::CentralPanel::default().show(ctx,|ui|editor.canvas(ui,false,false));
            });
    };
    editor.viewport.set_sketch(Some(editor.state["objects"][1].clone()));editor.viewport.align_sketch();editor.viewport.set_sketch(None);
    frame(&mut editor,vec![]);
    frame(&mut editor,vec![]);
    // The feature window covers the initial region. Pan it into unobscured
    // canvas using real input; clicking through the dialog must stay blocked.
    let start=egui::pos2(1000.,650.);let end=egui::pos2(1250.,650.);
    frame(&mut editor,vec![egui::Event::PointerMoved(start)]);
    frame(&mut editor,vec![egui::Event::PointerButton{pos:start,pressed:true,button:egui::PointerButton::Middle,modifiers:egui::Modifiers::NONE}]);
    frame(&mut editor,vec![egui::Event::PointerMoved(end)]);
    frame(&mut editor,vec![egui::Event::PointerButton{pos:end,pressed:false,button:egui::PointerButton::Middle,modifiers:egui::Modifiers::NONE}]);
    let pos=editor.viewport.sketch_screen_point([0.,0.,0.]);
    frame(&mut editor,vec![egui::Event::PointerMoved(pos)]);
    for pressed in [true,false] {
        frame(&mut editor,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,pressed,
            button:egui::PointerButton::Primary,modifiers:egui::Modifiers::NONE}]);
    }
    let region=editor.draft.as_ref().unwrap().parameters["region_profile"].clone();
    assert_eq!(region["regions"],json!(["stable-boundary-key"]));
    assert_eq!(region["source_id"],102);
    assert_eq!(editor.state,original);
    assert!(input.try_recv().is_err());
    editor.property_ui.region_pick=false;editor.property_ui.axis_pick=true;
    frame(&mut editor,vec![]);
    let pos=editor.viewport.sketch_screen_point([0.,-10.,0.]);
    for pressed in [true,false] {
        frame(&mut editor,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,pressed,
            button:egui::PointerButton::Primary,modifiers:egui::Modifiers::NONE}]);
    }
    let parameters=&editor.draft.as_ref().unwrap().parameters;
    assert_eq!(parameters["construction_axis"]["geometry_id"],44);
    let edits=crate::property_commands::build(parameters).unwrap();
    assert_eq!(edits[0]["op"],"create_planar_region_profile");
    assert_eq!(edits[1]["op"],"create_revolve_axis");
    assert_eq!(edits[1]["master"],"Sketch");
    assert_eq!(edits[1]["profile"],region["name"]);
    assert!(edits.iter().any(|e|e["property"]=="Profile" && e["value"]["link"]==region["name"]));
    assert_eq!(editor.state,original);
}
#[test]
fn region_picker_rejects_stale_revision_foreign_body_and_is_disabled_outside_collector() {
    let mut v=Viewport::default();
    let state=json!({"revision":7,"objects":[{"name":"S","body":"B","effective_visibility":true,
        "planar_regions":{"revision":6,"regions":[{"key":"r","triangles":[[[0,0,0],[1,0,0],[0,1,0]]]}]}}]});
    v.set_regions(&state,Some("B"),true);assert!(v.region_scene.is_empty());
    let mut current=state.clone();current["objects"][0]["planar_regions"]["revision"]=json!(7);
    v.set_regions(&current,Some("Foreign"),true);assert!(v.region_scene.is_empty());
    v.set_regions(&current,Some("B"),false);assert!(v.region_scene.is_empty());
    v.set_regions(&current,Some("B"),true);assert_eq!(v.region_scene.len(),1);
}

// Full App with real pointer input and synthetic native state; no native credit.
#[test]
fn region_full_app_press_release_dispatches_identity_bound_profile() {
    let (mut editor,input,_output)=fixture();
    editor.state["objects"]=json!([
        {"name":"Body","type":"PartDesign::Body","native_id":101},
        {"name":"Sketch","type":"Sketcher::SketchObject","native_id":102,"body":"Body",
         "effective_visibility":true,"global_placement":{"position":[0,0,0],"rotation":[0,0,0,1]},
         "sketch":{"geometry":[{"kind":"line","persistent_id":44,"construction":false,"start":[-10,-10,0],"end":[10,-10,0]}]},
         "planar_regions":{"revision":17,"source_token":"mock-native-token","regions":[{"key":"stable-boundary-key",
         "triangles":[[[-10,-10,0],[10,-10,0],[0,10,0]]]}]}}
    ]);
    let original=editor.state.clone();
    editor.draft=Some(Draft{id:"native.properties".into(),name:"Revolution".into(),token:"mock-region".into(),
        parameters:json!({"create":true,"name":"Revolution","type":"PartDesign::Revolution","body":"Body",
        "properties":{"Profile":{"link":"Sketch"},"Angle":360},"expressions":{}}),
        changed:None,valid:false,committing:false,all_parameters:false,deferred:false});
    editor.property_ui.region_pick=true;
    let ctx=egui::Context::default(); let mut time=0.;
    let mut frame=|editor:&mut Editor,events| {
        time+=0.1;
        let _=ctx.run(egui::RawInput{time:Some(time),events,screen_rect:Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,egui::vec2(1280.,850.))),..Default::default()},|ctx| {
                eframe::App::update(editor,ctx,&mut eframe::Frame::_new_kittest());
            });
    };
    editor.viewport.set_sketch(Some(editor.state["objects"][1].clone()));editor.viewport.align_sketch();editor.viewport.set_sketch(None);
    frame(&mut editor,vec![]);
    // The first frame sizes the Feature area with egui's provisional 600x400
    // bounds. A geometry hit and matching layer_id_at alone do not prove input
    // ownership: the Feature move widget owns this press (R284's failed case).
    let initial=editor.viewport.sketch_screen_point([0.,0.,0.]);
    assert!(editor.viewport.region_hit(initial).is_some());
    frame(&mut editor,vec![egui::Event::PointerMoved(initial),egui::Event::PointerButton{
        pos:initial,pressed:true,button:egui::PointerButton::Primary,modifiers:egui::Modifiers::NONE}]);
    assert_eq!(ctx.interaction_snapshot(|i|i.dragged),Some(egui::Id::new("Revolve").with("move")));
    assert!(!editor.viewport.response.as_ref().unwrap().is_pointer_button_down_on());
    frame(&mut editor,vec![egui::Event::PointerButton{pos:initial,pressed:false,
        button:egui::PointerButton::Primary,modifiers:egui::Modifiers::NONE}]);
    assert!(editor.draft.as_ref().unwrap().parameters["region_profile"].is_null());
    // Settle the actual App, then navigate via the visible viewport. Never
    // bypass the floating window or assign a private camera/model position.

    frame(&mut editor,vec![]);
    let start=egui::pos2(1000.,650.);let end=egui::pos2(1250.,650.);
    frame(&mut editor,vec![egui::Event::PointerMoved(start)]);
    frame(&mut editor,vec![egui::Event::PointerButton{pos:start,pressed:true,button:egui::PointerButton::Middle,modifiers:egui::Modifiers::NONE}]);
    frame(&mut editor,vec![egui::Event::PointerMoved(end)]);
    frame(&mut editor,vec![egui::Event::PointerButton{pos:end,pressed:false,button:egui::PointerButton::Middle,modifiers:egui::Modifiers::NONE}]);
    let pos=editor.viewport.sketch_screen_point([0.,0.,0.]);
    frame(&mut editor,vec![egui::Event::PointerMoved(pos)]);
    assert!(editor.viewport.region_hit(pos).is_some());
    assert!(editor.viewport.response.as_ref().unwrap().hovered());
    let viewport_id=editor.viewport.response.as_ref().unwrap().id;
    assert_eq!(ctx.layer_id_at(pos),Some(editor.viewport.response.as_ref().unwrap().layer_id));
    for pressed in [true,false] {
        frame(&mut editor,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,pressed,
            button:egui::PointerButton::Primary,modifiers:egui::Modifiers::NONE}]);
        let response=editor.viewport.response.as_ref().unwrap();
        assert_eq!(response.id,viewport_id);
        if pressed {assert!(response.is_pointer_button_down_on());}
        else {assert!(response.clicked());}

    }
    let region=editor.draft.as_ref().unwrap().parameters["region_profile"].clone();
    assert_eq!(region["regions"],json!(["stable-boundary-key"]));
    assert_eq!(region["source_id"],102);
    assert_eq!(region["source_token"],"mock-native-token");
    assert_eq!(region["body_id"],101);
    assert_eq!(region["source"],"Sketch");
    editor.property_ui.schema=json!({"properties":{"Profile":{"link":"Sketch"},"ReferenceAxis":null},
        "property_types":{"Profile":"App::PropertyLinkSub","ReferenceAxis":"App::PropertyLinkSub"},
        "property_metadata":{"Profile":{"read_only":false},"ReferenceAxis":{"read_only":false}}});
    frame(&mut editor,vec![]);frame(&mut editor,vec![]);
    let control=ctx.data(|d|d.get_temp::<egui::Response>(egui::Id::new("reference_field").with("ReferenceAxis"))).expect("rendered Revolve axis field");
    assert!(control.enabled());let axis_field=control.rect.center();
    for pressed in [true,false] {frame(&mut editor,vec![egui::Event::PointerMoved(axis_field),egui::Event::PointerButton {
        pos:axis_field,pressed,button:egui::PointerButton::Primary,modifiers:egui::Modifiers::NONE}]);}
    assert_eq!(editor.property_ui.active_reference.as_deref(),Some("ReferenceAxis"));
    assert!(editor.property_ui.axis_pick);assert!(!editor.property_ui.region_pick);

    frame(&mut editor,vec![]);
    let pos=editor.viewport.sketch_screen_point([0.,-10.,0.]);
    frame(&mut editor,vec![egui::Event::PointerMoved(pos)]);
    assert!(editor.viewport.response.as_ref().unwrap().hovered());
    for pressed in [true,false] {
        frame(&mut editor,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,pressed,
            button:egui::PointerButton::Primary,modifiers:egui::Modifiers::NONE}]);
    }
    let parameters=&editor.draft.as_ref().unwrap().parameters;
    assert_eq!(parameters["construction_axis"]["geometry_id"],44);
    let edits=crate::property_commands::build(parameters).unwrap();
    assert_eq!(edits[0]["op"],"create_planar_region_profile");
    assert_eq!(edits[1]["op"],"create_revolve_axis");
    assert_eq!(edits[1]["master"],"Sketch");
    assert_eq!(edits[1]["profile"],region["name"]);
    assert!(edits.iter().any(|e|e["property"]=="Profile" && e["value"]["link"]==region["name"]));
    assert_eq!(editor.state,original);
    assert!(input.try_recv().is_err());
}

#[path = "new_loft_tests.rs"]
mod new_loft_tests;


// Real Editor reply adoption and pointer/text input; native data are explicit doubles.
#[test]
fn accepted_region_preview_keeps_source_across_quantity_edit_and_rejection() {
    let (mut e,input,output)=fixture();
    let region=json!({"name":"RegionProfile","source":"Sketch","source_id":102,
        "body":"Body","body_id":101,"source_token":"native-source-token","regions":["outside-key"]});
    e.state["objects"]=json!([
        {"name":"Body","type":"PartDesign::Body","native_id":101},
        {"name":"Sketch","label":"Sketch","type":"Sketcher::SketchObject","native_id":102,"body":"Body",
            "rectangular_profile":{"eligible":false}}
    ]);
    e.draft=Some(Draft{id:"native.properties".into(),name:"Extrude".into(),token:"region-task".into(),
        parameters:json!({"create":true,"type":"PartDesign::Pad","name":"Pad","body":"Body",
            "region_profile":region,"properties":{"Profile":{"link":"Sketch"},"Length":10.},"expressions":{}}),
        changed:None,valid:false,committing:false,all_parameters:true,deferred:false});
    let native=json!({"name":"Pad","label":"Pad","type":"PartDesign::Pad","native_id":104,"body":"Body",
        "properties":{"Profile":{"link":"RegionProfile"},"Length":10.},
        "property_types":{"Profile":"App::PropertyLinkSub","Length":"App::PropertyLength"},
        "property_metadata":{"Length":{"quantity":{"value":10.,"input":"10 mm","default_unit":"mm",
            "signature":[1,0,0,0,0,0,0,0]}}}});
    e.property_ui.schema=native.clone();
    e.preview();
    let first=input.try_recv().expect("initial region preview");
    assert_eq!(first.op,"preview");
    assert_eq!(first.parameters["edits"][0]["source"],"Sketch");
    let mut state=e.state.clone();state["revision"]=json!(18);state["preview"]=json!({"token":"region-task"});
    state["objects"].as_array_mut().unwrap().push(native);
    let ctx=egui::Context::default();
    reply(&output,&mut e,&ctx,first,true,Value::Null,state.clone());
    assert_eq!(e.draft.as_ref().unwrap().parameters["properties"]["Profile"]["link"],"Sketch",
        "adopting a successful preview must not replace the task's semantic source with its helper");
    assert_eq!(e.property_ui.schema["properties"]["Profile"]["link"],"RegionProfile");
    replace_text(&mut e,&ctx,"10 mm","1 in");settle(&mut e,&ctx);
    let second=input.try_recv().expect("length edit must queue another region preview");
    assert_eq!(second.op,"preview");
    assert_eq!(second.parameters["edits"][0]["source"],"Sketch");
    assert_eq!(second.parameters["edits"][0]["regions"],json!(["outside-key"]));
    assert!(second.parameters["edits"].as_array().unwrap().iter().any(|edit|
        edit["property"]=="Length" && edit["value"]["quantity_input"]=="1 in"));
    reply(&output,&mut e,&ctx,second,false,Value::Null,state.clone());
    replace_text(&mut e,&ctx,"1 in","2 in");settle(&mut e,&ctx);
    let retry=input.try_recv().expect("rejected replacement remains editable");
    assert_eq!(retry.op,"preview");
    assert_eq!(retry.parameters["edits"][0]["source"],"Sketch");
    reply(&output,&mut e,&ctx,retry,true,Value::Null,state.clone());
    pointer(&mut e,&ctx,"Cancel");
    let cancel=input.try_recv().expect("cancel after replacement preview");
    assert_eq!(cancel.op,"preview_cancel");
    state["preview"]=Value::Null;
    reply(&output,&mut e,&ctx,cancel,true,Value::Null,state);
    assert!(e.draft.is_none());
}

#[path = "part_mirror_tests.rs"]
mod part_mirror_tests;
