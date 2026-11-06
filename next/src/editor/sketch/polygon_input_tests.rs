use super::*;

fn editor() -> (Editor, std::sync::mpsc::Receiver<super::super::Job>) {
    let (jobs, input) = std::sync::mpsc::sync_channel(1);
    let (_output, replies) = std::sync::mpsc::channel();
    let mut editor = Editor {
        jobs, replies, busy: false,
        registry: crate::packages::Registry::builtin().unwrap(),
        state: json!({"document":"OffsetStudy","revision":1,"available":true,"objects":[]}),
        viewport: crate::viewport::Viewport::default(),
        sketch: Some("Sketch".into()), selected_object: None,
        sketch_ui: SketchUi::default(), sketch_creation: None,
        body_ui: Default::default(), property_ui: Default::default(),
        measurement_ui: Default::default(), units_ui: Default::default(),
        transform_ui: Default::default(), exchange_ui: Default::default(),
        draft: None, sequence: 1, error: None, status: String::new(),
        path: String::new(), file_input: String::new(), file_dialog: None,
        file_browser: Default::default(), overwrite: false, dirty: false,
        pending_file: None, allow_close: false, fit_next: false,
    };
    editor.viewport.set_sketch(Some(json!({"name":"Sketch","sketch":{"geometry":[
        {"index":0,"kind":"line","start":[20,20,0],"end":[60,20,0]}
    ],"constraints":[]}})));
    editor.viewport.align_sketch();
    editor.state["objects"] = json!([{"name":"Sketch","label":"Sketch","type":"Sketcher::SketchObject",
        "global_placement":{"position":[20.,30.,40.],"rotation":[0.7071067811865476,0.,0.,0.7071067811865476]},
        "sketch":{"geometry_count":1,"constraint_count":0,"geometry":[
            {"index":0,"kind":"line","start":[20.,20.,0.],"end":[60.,20.,0.]}
        ],"constraints":[]}}]);
    editor.sketch_ui.session = Some("polygon-session".into());
    editor.sync_sketch();
    editor.viewport.align_sketch();
    (editor,input)
}


fn frame(e: &mut Editor, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1280.,850.))),events,..Default::default()},
        |ctx| eframe::App::update(e,ctx,&mut eframe::Frame::_new_kittest()))
}
fn pointer(e: &mut Editor, ctx: &egui::Context, pos: egui::Pos2) {
    for pressed in [true,false] {
        frame(e,ctx,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton {
            pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}]);
    }
}
fn key(e: &mut Editor, ctx: &egui::Context, key: Key) {
    for pressed in [true,false] {frame(e,ctx,vec![egui::Event::Key {
        key,physical_key:None,pressed,repeat:false,modifiers:egui::Modifiers::NONE}]);}
}
fn click(e: &mut Editor, ctx: &egui::Context, label: &str) {
    frame(e,ctx,vec![]);
    let out=frame(e,ctx,vec![]);
    let pos=out.shapes.iter().find_map(|s|match &s.shape {
        egui::Shape::Text(t) if t.galley.text()==label=>Some(t.pos+t.galley.size()*0.5),_=>None
    }).unwrap_or_else(||panic!("Missing {label}"));
    pointer(e,ctx,pos);
}
#[test]
fn polygon_full_app_menu_mode_count_preview_and_atomic_transport() {
    let (mut e,input)=editor(); let ctx=egui::Context::default();
    ctx.style_mut(|s|s.animation_time=0.);
    frame(&mut e,&ctx,vec![]);
    click(&mut e,&ctx,"Curves"); click(&mut e,&ctx,"Regular polygon (P)");
    assert_eq!(e.sketch_ui.tool,Tool::RegularPolygon{sides:6,circumscribed:false});
    key(&mut e,&ctx,Key::Equals);
    assert_eq!(e.sketch_ui.tool,Tool::RegularPolygon{sides:7,circumscribed:false});
    // Pick origin through the actual canvas. Transform must stay in the viewport.
    let center=e.viewport.sketch_screen_point([0.;3]);
    pointer(&mut e,&ctx,center);
    assert_eq!(e.sketch_ui.points,vec![[0.;3]]);
    key(&mut e,&ctx,Key::Minus);
    let rim=e.viewport.sketch_screen_point([10.,10.,0.]);
    let out=frame(&mut e,&ctx,vec![egui::Event::PointerMoved(rim)]);
    assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().contains("6 sides"))));
    assert!(input.try_recv().is_err(),"preview sends no model jobs");
    pointer(&mut e,&ctx,rim);
    assert_eq!(e.sketch_ui.points.len(),2);
    assert!(input.try_recv().is_err(),"size click must not commit");
    pointer(&mut e,&ctx,rim);
    let job=input.try_recv().expect("one polygon transaction");
    assert_eq!(job.op,"transaction");
    assert_eq!(job.parameters["expected_revision"],1);
    assert_eq!(job.parameters["sketch_token"],"polygon-session");
    let edits=job.parameters["edits"].as_array().unwrap();
    assert_eq!(edits.iter().filter(|v|v["kind"]=="line").count(),6);
    assert_eq!(edits[0]["center"],json!([0.,0.,0.]));
    assert_eq!(edits.last().unwrap()["args"],json!([1,3,-1,1]));
    assert!(input.try_recv().is_err());
    assert!(e.sketch_ui.points.is_empty());
}
#[test]
fn polygon_full_app_escape_and_changed_owner_never_commit() {
    for stale in [false,true] {
        let (mut e,input)=editor(); let ctx=egui::Context::default();
        frame(&mut e,&ctx,vec![]); key(&mut e,&ctx,Key::P);
        let center=e.viewport.sketch_screen_point([0.;3]); pointer(&mut e,&ctx,center);
        assert_eq!(e.sketch_ui.points.len(),1);
        if stale {
            e.state["revision"]=json!(2);
            let rim=e.viewport.sketch_screen_point([25.,-25.,0.]); pointer(&mut e,&ctx,rim);
            assert!(e.error.as_deref().is_some_and(|s|s.contains("changed")));
        } else {key(&mut e,&ctx,Key::Escape);assert_eq!(e.sketch_ui.tool,Tool::Select);}
        assert!(e.sketch_ui.points.is_empty()); assert!(input.try_recv().is_err());
    }
}
#[test]
fn polygon_modes_construction_and_reset_use_the_shared_native_builder() {
    for mode in [false,true] {
        let (mut e,input)=editor();
        e.activate_sketch_drawing_tool(Tool::RegularPolygon{sides:5,circumscribed:mode});
        e.sketch_ui.construction=true;
        e.draw_point([0.;3],Some(SketchElement{index:-1,point:1}),None,false);
        // A mode/count edit preserves the center and its ownership.
        e.sketch_ui.tool=Tool::RegularPolygon{sides:8,circumscribed:!mode};
        e.draw_point([20.,10.,0.],None,None,false);
        assert!(input.try_recv().is_err());
        e.draw_point([40.,30.,0.],None,None,false);
        let j=input.try_recv().unwrap();
        let edits=j.parameters["edits"].as_array().unwrap();
        assert_eq!(edits.iter().filter(|v|v["op"]=="geometry").count(),9);
        assert!(edits.iter().filter(|v|v["op"]=="geometry").all(|v|v["construction"]==true));
        e.busy=false; e.prepare_new_sketch();
        assert!(e.sketch_ui.polygon_owner.is_none()); assert_eq!(e.sketch_ui.tool,Tool::Select);
    }
}

#[test]
fn polygon_full_app_relevant_mode_controls_are_in_curves_menu() {
    let (mut e,input)=editor();let ctx=egui::Context::default();
    ctx.style_mut(|s|s.animation_time=0.);
    frame(&mut e,&ctx,vec![]);key(&mut e,&ctx,Key::P);
    click(&mut e,&ctx,"Curves");click(&mut e,&ctx,"Circumscribed");
    assert_eq!(e.sketch_ui.tool,Tool::RegularPolygon{sides:6,circumscribed:true});
    assert!(input.try_recv().is_err());
}

// MOCK services: in-memory snapshot and disconnected job/reply channels only.
// All input below traverses the real App/canvas; no native worker is run.
fn count(e: &Editor) -> u32 {
    let Tool::RegularPolygon{sides,..}=e.sketch_ui.tool else {panic!("polygon expected")}; sides
}
fn move_radius(e: &mut Editor,ctx: &egui::Context,center: egui::Pos2,radius: f32) -> egui::FullOutput {
    frame(e,ctx,vec![egui::Event::PointerMoved(center+egui::vec2(radius,0.))])
}
#[test]
fn polygon_pointer_full_app_frozen_geometry_jitter_reversal_and_commit() {
    for mode in [false,true] {
        let (mut e,input)=editor(); let ctx=egui::Context::default();
        frame(&mut e,&ctx,vec![]);key(&mut e,&ctx,Key::P);
        if mode {click(&mut e,&ctx,"Curves");click(&mut e,&ctx,"Circumscribed");}
        let center=e.viewport.sketch_screen_point([0.;3]);pointer(&mut e,&ctx,center);
        let rim=e.viewport.sketch_screen_point([30.,20.,0.]);pointer(&mut e,&ctx,rim);
        assert_eq!(e.sketch_ui.points.len(),2);assert!(input.try_recv().is_err());
        let fixed=e.sketch_ui.points.clone();let snaps=e.sketch_ui.snaps.clone();
        let center=e.viewport.sketch_screen_point(fixed[0]);
        let radius=e.sketch_ui.polygon_count_pointer.unwrap().0;
        assert_eq!(count(&e),6);
        for delta in [0.,3.,-4.,7.,-7.,0.] {move_radius(&mut e,&ctx,center,radius+delta);assert_eq!(count(&e),6,"mode={mode} delta={delta} pointer={:?} center={center:?} fixed={fixed:?}",e.sketch_ui.polygon_count_pointer);}
        move_radius(&mut e,&ctx,center,radius-15.);assert_eq!(count(&e),5);
        for delta in [-13.,-12.,-10.] {move_radius(&mut e,&ctx,center,radius+delta);assert_eq!(count(&e),5);}
        let out=move_radius(&mut e,&ctx,center,radius+15.);assert_eq!(count(&e),7);
        assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().contains("7 sides"))));
        move_radius(&mut e,&ctx,center,radius);assert_eq!(count(&e),6);
        assert_eq!(e.sketch_ui.points,fixed);
        let expected=draw_snapped_edits("Sketch",e.sketch_ui.tool,&fixed,1,false,&snaps).unwrap();
        let (vertices,_)=crate::sketch_commands::regular_polygon(fixed[0],fixed[1],6,mode).unwrap();
        let out=move_radius(&mut e,&ctx,center,radius);
        assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Path(p)
            if p.points.len()==7 && p.points.iter().take(6).zip(&vertices)
                .all(|(actual,v)|actual.distance(e.viewport.sketch_screen_point(*v))<0.01))));
        assert!(input.try_recv().is_err());
        pointer(&mut e,&ctx,center+egui::vec2(radius,0.));
        let job=input.try_recv().unwrap();assert_eq!(job.parameters["edits"],json!(expected));
        assert!(input.try_recv().is_err());assert!(e.sketch_ui.points.is_empty());
    }
}
#[test]
fn polygon_pointer_full_app_bounds_controls_and_cancel_or_stale_session() {
    for end in ["escape","session","native_id"] {
        let (mut e,input)=editor();let ctx=egui::Context::default();
        frame(&mut e,&ctx,vec![]);key(&mut e,&ctx,Key::P);
        let center=e.viewport.sketch_screen_point([0.;3]);pointer(&mut e,&ctx,center);
        let rim=e.viewport.sketch_screen_point([40.,0.,0.]);pointer(&mut e,&ctx,rim);
        let radius=rim.distance(center);
        move_radius(&mut e,&ctx,center,radius-60.);assert_eq!(count(&e),3);
        move_radius(&mut e,&ctx,center,radius-65.);assert_eq!(count(&e),3);
        // Keyboard and explicit count state changes rebase pointer adjustment.
        key(&mut e,&ctx,Key::Equals);assert_eq!(count(&e),4);
        move_radius(&mut e,&ctx,center,radius-65.);assert_eq!(count(&e),4);
        e.sketch_ui.tool=Tool::RegularPolygon{sides:63,circumscribed:false};
        move_radius(&mut e,&ctx,center,radius);assert_eq!(count(&e),63);
        move_radius(&mut e,&ctx,center,radius+30.);assert_eq!(count(&e),64);
        move_radius(&mut e,&ctx,center,radius+50.);assert_eq!(count(&e),64);
        assert!(input.try_recv().is_err());
        match end {
            "escape"=>key(&mut e,&ctx,Key::Escape),
            "session"=>{e.sketch_ui.session=Some("replacement-session".into());pointer(&mut e,&ctx,rim);},
            _=>{e.state["objects"][0]["native_id"]=json!(99);pointer(&mut e,&ctx,rim);},
        }
        assert!(input.try_recv().is_err());assert!(e.sketch_ui.points.is_empty());
        assert!(e.sketch_ui.polygon_count_pointer.is_none());
    }
}

#[test]
fn polygon_pointer_full_app_alt_size_pick_retains_unsnapped_geometry() {
    let (mut e,input)=editor();let ctx=egui::Context::default();
    frame(&mut e,&ctx,vec![]);key(&mut e,&ctx,Key::P);
    let center=e.viewport.sketch_screen_point([0.;3])+egui::vec2(3.,3.);
    let modifiers=egui::Modifiers{alt:true,..Default::default()};
    for pos in [center,center+egui::vec2(100.,50.)] {
        for pressed in [true,false] {
            let _ = ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1280.,850.))),
                modifiers,events:vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{
                    pos,button:egui::PointerButton::Primary,pressed,modifiers}],..Default::default()},
                |ctx|eframe::App::update(&mut e,ctx,&mut eframe::Frame::_new_kittest()));
        }
    }
    assert_eq!(e.sketch_ui.points.len(),2);assert!(e.sketch_ui.snaps.iter().all(Option::is_none));
    assert!(input.try_recv().is_err());let fixed=e.sketch_ui.points.clone();
    let center=e.viewport.sketch_screen_point(fixed[0]);
    let radius=e.sketch_ui.polygon_count_pointer.unwrap().0;
    move_radius(&mut e,&ctx,center,radius+15.);assert_eq!(count(&e),7);
    assert_eq!(e.sketch_ui.points,fixed);
    pointer(&mut e,&ctx,center+egui::vec2(radius+15.,0.));assert!(input.try_recv().is_ok());
}


fn point_pattern_editor() -> (Editor, std::sync::mpsc::Receiver<super::super::Job>) {
    let (mut e, input) = editor();
    e.state["objects"][0]["sketch"] = json!({"geometry_count":1,"constraint_count":0,
        "geometry":[{"index":0,"kind":"point","editable":true,"persistent_id":81,
          "position":[31.75,0.,0.],"points":[[31.75,0.,0.]],"construction":false,
          "handles":[{"index":0,"point":1,"position":[31.75,0.,0.],"draggable":true}]}],
        "constraints":[]});
    e.sketch_ui.tool = Tool::Select;
    e.sync_sketch();e.viewport.align_sketch();e.viewport.fit_sketch();
    (e,input)
}

#[test]
fn standalone_point_real_pointer_selection_opens_pattern_task() {
    let (mut e,_input) = point_pattern_editor();let ctx=egui::Context::default();
    ctx.style_mut(|s|s.animation_time=0.);
    for _ in 0..3 {frame(&mut e,&ctx,vec![]);}
    let pos=e.viewport.sketch_screen_point([31.75,0.,0.]);
    assert_eq!(e.viewport.sketch_hit(pos),Some(SketchElement{index:0,point:1}));
    pointer(&mut e,&ctx,pos);
    assert_eq!(e.viewport.sketch_selection,vec![SketchElement{index:0,point:1}]);
    assert!(e.mirror_selection_owned());
    e.begin_sketch_task("circular_pattern",None);
    assert!(e.sketch_ui.task.is_some(), "selected standalone point must open the requested pattern task");
}

#[test]
fn standalone_point_owner_rejects_changed_id_and_curve_endpoints() {
    let (mut e,_input)=point_pattern_editor();
    e.viewport.sketch_selection=vec![SketchElement{index:0,point:1}];
    e.bind_mirror_selection();assert!(e.mirror_selection_owned());
    e.state["objects"][0]["sketch"]["geometry"][0]["persistent_id"]=json!(82);
    assert!(!e.mirror_selection_owned());
    e.begin_sketch_task("circular_pattern",None);assert!(e.sketch_ui.task.is_none());
    e.error=None;e.state["objects"][0]["sketch"]["geometry"][0]["kind"]=json!("line");
    e.bind_mirror_selection();assert!(!e.mirror_selection_owned());
}
