fn r2_same_holes(h:&NativeEditor,c:&Value) {
    let expected=c["expected"].as_object().unwrap();
    assert_eq!(objects(&h.editor.state).iter().filter(|o|o["type"]=="PartDesign::Hole").count(),expected.len());
    for (name,goal) in expected {
        let actual=object(&h.editor.state,name);
        assert_eq!(actual["label"],goal["label"]);
        for (field,value) in goal["properties"].as_object().unwrap() {assert_eq!(actual["properties"][field],*value,"{name}.{field}");}
        assert_eq!(actual["expressions"],goal["expressions"]);
        assert_eq!(actual["properties"]["Profile"]["link"],goal["profile"]);
        assert_eq!(actual["properties"]["BaseFeature"]["link"],goal["base"]);
        for field in ["volume","area"] {assert!((actual["shape"][field].as_f64().unwrap()-goal[field].as_f64().unwrap()).abs()<1e-7,"{name}.{field}");}
    }
    assert_eq!(object(&h.editor.state,"Body")["body_state"]["tip"],c["body_tip"]);
}
#[test]
#[ignore="independent r2 serialized and cleanup challenges"]
fn r2_serialized_native_contract_and_failed_open_history() {
    let root=input_file("CAD_R2_FIXTURES");
    let data:Value=serde_json::from_str(&fs::read_to_string(root.join("cases.json")).unwrap()).unwrap();
    let mut h=open_hole("r2-new-serialized");let mut f=Form::new(false);
    quantity(&mut h,&mut f,"Depth","2 mm");h.preview(true);fixed_click_finish(&mut h,true);
    let accepted=h.editor.state.clone();
    let pre=acceptance_dir().join("r2-before-history.FCStd");h.send("save",json!({"path":pre}));
    h.send("undo",json!({}));let before=h.editor.state.clone();
    h.editor.path="prior-path.FCStd".into();h.editor.file_input="prior-path.FCStd".into();
    let dirty=h.editor.dirty;
    for c in data["cases"].as_array().unwrap().iter().filter(|c|c["accepted"]==false) {
        for _ in 0..2 {
            h.editor.send("open",json!({"path":c["input"]}));h.receive("open",false);
            assert!(!h.editor.error.as_ref().unwrap().contains("already_open"));
            assert_eq!(h.editor.state,before,"failed Open {}",c["name"]);
            assert_eq!(h.editor.dirty,dirty);assert_eq!(h.editor.path,"prior-path.FCStd");assert_eq!(h.editor.file_input,"prior-path.FCStd");
        }
    }
    h.send("redo",json!({}));
    for name in ["Hole","Profile","Base"] {for field in ["properties","expressions"] {assert_eq!(object(&h.editor.state,name)[field],object(&accepted,name)[field]);}}
    let a=&object(&h.editor.state,"Hole")["shape"];let b=&object(&accepted,"Hole")["shape"];
    let mut normalized=a.clone();
    // Only the measured native Redo min-z roundoff is admitted here.
    assert!((a["bounds"]["min"][2].as_f64().unwrap()-b["bounds"]["min"][2].as_f64().unwrap()).abs()<=3e-16);
    normalized["bounds"]["min"][2]=b["bounds"]["min"][2].clone();assert_eq!(normalized,*b);
    write_json("r2-history-shapes.json",&json!({"accepted":b,"redo":a}));
    h.send("save",json!({"path":acceptance_dir().join("r2-after-history.FCStd")}));
    drop(h);
    let mut results=vec![];
    for c in data["cases"].as_array().unwrap().iter().filter(|c|c["accepted"]==true) {
        // A new actor for each Open prevents inventory/history from concealing identity drift.
        let mut h=NativeEditor::new(c["name"].as_str().unwrap());h.send("open",json!({"path":c["input"]}));r2_same_holes(&h,c);
        let saved=acceptance_dir().join(format!("r2-{}.FCStd",c["name"].as_str().unwrap()));h.send("save",json!({"path":saved}));
        results.push(json!({"saved":saved,"reference":c["control"]}));
    }
    write_json("r2-saved-pairs.json",&json!(results));
}
#[test]
#[ignore="independent r2 actual enabled widgets and invalid dormant unit text"]
fn r2_dormant_invalid_quantity_and_unit_buffer_switchback() {
    let mut h=open_hole("r2-dormant-units");let mut f=Form::new(false);
    choose(&mut h,&mut f,"HoleCutType","None","Counterbore");toggle(&mut h,&mut f,"HoleCutCustomValues");
    quantity(&mut h,&mut f,"HoleCutDepth","not-a-length");quantity(&mut h,&mut f,"HoleCutDiameter","0.55 cm");
    assert_eq!(h.params()["hole_task_literals"]["fields"]["HoleCutDepth"],true);
    choose(&mut h,&mut f,"HoleCutType","Counterbore","None");h.preview(true);
    assert_eq!(h.params()["properties"]["HoleCutDepth"]["quantity_input"],"not-a-length");
    assert_eq!(h.params()["properties"]["HoleCutDepth"]["quantity_input"],"not-a-length");
    assert_eq!(h.params()["properties"]["HoleCutDiameter"]["quantity_input"],"0.55 cm");
    choose(&mut h,&mut f,"HoleCutType","None","Counterbore");h.preview(false);
    assert_eq!(h.params()["properties"]["HoleCutDepth"]["quantity_input"],"not-a-length");
    quantity(&mut h,&mut f,"HoleCutDepth","0.025 cm");
    assert_eq!(h.params()["properties"]["HoleCutDepth"]["quantity_input"],"0.025 cm");
    h.preview(true);
    assert_eq!(object(&h.editor.state,"Hole")["properties"]["HoleCutDepth"],0.25);
    // Units replaces an accepted active input with its native value. Its next
    // frame uses native full-precision text; dormant inputs above keep raw text.
    let native_input=h.editor.property_ui.schema["property_metadata"]["HoleCutDepth"]["quantity"]["input"].as_str().unwrap().to_owned();
    assert_eq!(h.params()["properties"]["HoleCutDepth"],0.25);
    row(&mut h,&mut f,"HoleCutDepth");
    assert!(f.last.iter().any(|t|t.text==native_input));
    assert!(!serde_json::to_string(&h.editor.state).unwrap().contains("hole_task_literals"));
    let edits=crate::property_commands::build(&crate::hole_commands::active_parameters(&h.editor.property_ui.schema,h.params())).unwrap();
    assert!(!serde_json::to_string(&edits).unwrap().contains("hole_task_literals"));
    fixed_click_finish(&mut h,true);h.send("save",json!({"path":acceptance_dir().join("r2-widget-units.FCStd")}));
    f.save("r2-unit-widgets.json");
}
#[test]
#[ignore="independent r2 formula and readonly admission"]
fn r2_dormant_expression_is_forwarded_and_retained() {
    let mut h=open_hole("r2-expression");let mut f=Form::new(false);
    choose(&mut h,&mut f,"HoleCutType","None","Counterbore");toggle(&mut h,&mut f,"HoleCutCustomValues");
    quantity(&mut h,&mut f,"HoleCutDiameter","5.5 mm");
    expression(&mut h,&mut f,"HoleCutDepth","Base.Height / 12");
    choose(&mut h,&mut f,"HoleCutType","Counterbore","None");
    let projected=crate::hole_commands::active_parameters(&h.editor.property_ui.schema,h.params());
    assert_eq!(projected["expressions"]["HoleCutDepth"],"Base.Height / 12");
    h.preview(false);assert!(h.editor.error.as_ref().unwrap().contains("read_only_property"));
    assert_eq!(h.params()["expressions"]["HoleCutDepth"],"Base.Height / 12");
    choose(&mut h,&mut f,"HoleCutType","None","Counterbore");h.preview(true);
    assert_eq!(object(&h.editor.state,"Hole")["expressions"]["HoleCutDepth"],"Base.Height / 12");
    fixed_click_finish(&mut h,false);f.save("r2-expression-widgets.json");
}
#[test]
#[ignore="independent r2 computed protection with real and forged provenance"]
fn r2_provenance_does_not_hide_computed_writes() {
    for field in ["Shape","ThreadDiameter","Diameter"] {
        let mut h=open_hole(&format!("r2-protected-{field}"));let mut f=Form::new(false);
        choose(&mut h,&mut f,"HoleCutType","None","Counterbore");toggle(&mut h,&mut f,"HoleCutCustomValues");
        quantity(&mut h,&mut f,"HoleCutDepth","0.25 mm");quantity(&mut h,&mut f,"HoleCutDiameter","5.5 mm");
        assert!(h.params().get("hole_task_literals").is_some());
        h.editor.draft.as_mut().unwrap().parameters["properties"][field]=json!(7);
        if field!="Diameter" {h.editor.draft.as_mut().unwrap().parameters["hole_task_literals"]["fields"][field]=json!(true);}
        let projected=crate::hole_commands::active_parameters(&h.editor.property_ui.schema,h.params());assert_eq!(projected["properties"][field],7);
        let before=h.editor.state.clone();h.preview(false);assert!(h.editor.error.as_ref().unwrap().contains("read_only_property"));
        for name in ["Hole","Profile","Base"] {for key in ["properties","expressions","shape"] {assert_eq!(object(&h.editor.state,name)[key],object(&before,name)[key]);}}
    }
}
