use crate::editor::*;
use eframe::egui;
fn prepared() -> (Editor, Receiver<Job>, Sender<Reply>) {
    let (mut e,input,output)=crate::editor::rectangular_extrude_tests::fixture();
    e.state["objects"].as_array_mut().unwrap().extend([
        json!({"name":"A","type":"App::Link","assembly":"ActualAssembly","native_id":902}),
        json!({"name":"B","type":"App::Link","assembly":"ActualAssembly","native_id":903}),
        json!({"name":"Mate","type":"App::FeaturePython","assembly":"ActualAssembly","native_id":904,
            "properties":{"JointType":"Fixed","Reference1":{"link":"A","subelements":["Face3","Face3"]},
            "Reference2":{"link":"B","subelements":["Face4","Face4"]},"Offset1":{"position":[4,5,6],"rotation":[0,0,0,1]}}})]);
    assert!(e.edit_assembly_object("Mate"));
    (e,input,output)
}
fn frame(e:&mut Editor,ctx:&egui::Context,events:Vec<egui::Event>)->egui::FullOutput {
    ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1500.,1000.))),events,..Default::default()},
        |ctx|eframe::App::update(e,ctx,&mut eframe::Frame::_new_kittest()))
}
fn click(e:&mut Editor,ctx:&egui::Context,text:&str) {
    frame(e,ctx,vec![]);let out=frame(e,ctx,vec![]);
    fn find(s:&egui::Shape,text:&str)->Option<egui::Pos2>{match s {
        egui::Shape::Text(t) if t.galley.text()==text=>Some(t.pos+t.galley.size()*0.5),
        egui::Shape::Vec(v)=>v.iter().find_map(|s|find(s,text)),_=>None}}
    let pos=out.shapes.iter().find_map(|s|find(&s.shape,text)).unwrap_or_else(||panic!("Missing visible {text}"));
    for pressed in [true,false] {frame(e,ctx,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{
        pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}]);}
}
#[test]
fn primary_flip_actual_app_edit_both_sides_cancel_apply_reply_reopen() {
    let (mut e,input,output)=prepared();let ctx=egui::Context::default();let baseline=e.state.clone();
    click(&mut e,&ctx,"Flip primary axis 1");click(&mut e,&ctx,"Flip primary axis 2");
    assert_eq!(e.draft.as_ref().unwrap().parameters["request"]["flip_primary"],json!([true,true]));
    click(&mut e,&ctx,"Flip primary axis 1");click(&mut e,&ctx,"Flip primary axis 2");
    assert_eq!(e.draft.as_ref().unwrap().parameters["request"]["flip_primary"],json!([false,false]));
    click(&mut e,&ctx,"Cancel");assert!(e.draft.is_none());assert_eq!(e.state,baseline);assert!(input.try_recv().is_err());
    assert!(e.edit_assembly_object("Mate"));click(&mut e,&ctx,"Flip primary axis 2");
    e.draft.as_mut().unwrap().changed=Some(Instant::now()-Duration::from_secs(2));frame(&mut e,&ctx,vec![]);click(&mut e,&ctx,"Apply");let job=input.try_recv().unwrap();assert_eq!(job.op,"transaction");
    let edit=&job.parameters["edits"][0];assert_eq!(edit["flip_primary"],json!([false,true]));
    assert_eq!(edit["reference1"]["subelements"],json!(["Face3","Face3"]));assert_eq!(edit["properties"],json!({}));
    let mut state=baseline.clone();state["revision"]=json!(18);state["undo_count"]=json!(1);
    state["objects"][3]["properties"]["Offset2"]=json!({"position":[0,0,0],"rotation":[1,0,0,0]});
    output.send(Reply{op:"transaction".into(),accepted:true,state:state.clone(),scenes:vec![],error:None,elapsed:Duration::ZERO,data:Value::Null}).unwrap();
    e.poll();assert!(e.draft.is_none());assert_eq!(e.state,state);
    assert!(e.edit_assembly_object("Mate"));assert!(e.draft.as_ref().unwrap().parameters["request"].get("flip_primary").is_none());
    click(&mut e,&ctx,"Flip primary axis 2");assert_eq!(e.draft.as_ref().unwrap().parameters["request"]["flip_primary"],json!([false,true]));
}
#[test]
fn primary_flip_create_and_expression_readonly_compile_denials() {
    let (mut e,input,_)=prepared();let ctx=egui::Context::default();
    for locked in ["read_only","expression"] {
        let mut state=e.state.clone();
        if locked=="read_only" {state["objects"][3]["property_metadata"]["Offset1"]["read_only"]=json!(true);}
        else {state["objects"][3]["expressions"]=json!({"Offset1.Rotation.Angle":"45 deg"});}
        let mut request=e.draft.as_ref().unwrap().parameters["request"].clone();request["flip_primary"]=json!([true,false]);
        assert!(crate::assembly_commands::build(&state,&request).is_err());
    }
    e.draft=None;
    e.start_assembly_task(json!({"op":"assembly_joint","assembly":"ActualAssembly","name":"NewMate","create":true,"joint_type":"Fixed",
        "reference1":{"link":"A","subelements":["Face3","Face3"]},"reference2":{"link":"B","subelements":["Face4","Face4"]},"properties":{},"expressions":{}}),"New native joint");
    click(&mut e,&ctx,"Flip primary axis 1");e.draft.as_mut().unwrap().changed=Some(Instant::now()-Duration::from_secs(2));frame(&mut e,&ctx,vec![]);click(&mut e,&ctx,"Apply");let job=input.try_recv().unwrap();
    assert_eq!(job.parameters["edits"][0]["create"],true);assert_eq!(job.parameters["edits"][0]["flip_primary"],json!([true,false]));
}
