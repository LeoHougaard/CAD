fn combined_native_lengths(h: &mut NativeEditor) -> Value {
    h.editor.send("quantities", json!({"items":[
        {"unit":"mm","text":"0.01 in"}, {"unit":"mm","text":"0.25 in"}
    ]}));
    let reply = h.raw.recv_timeout(Duration::from_secs(25)).unwrap();
    assert_eq!(reply.op, "quantities");
    assert!(reply.accepted, "{:?}", reply.error);
    let data = reply.data.clone();
    h.forward.send(reply).unwrap();
    h.editor.poll();
    write_json("hole-imperial-native-quantities.json", &data);
    data
}

#[test]
#[ignore = "requires native Hole/units integration fixtures"]
fn combined_hole_imperial_wrappers_controller_switchback_and_owned_settings() {
    let mut h = NativeEditor::new("hole-imperial");
    h.send("open", json!({"path":input_file("CAD_HOLE_INPUT")}));
    h.send("document_settings", json!({"unit_system":3}));
    let expected = combined_native_lengths(&mut h);
    h.editor.edit_native_properties("Hole");
    let mut f = Form::new(false);
    choose(&mut h, &mut f, "HoleCutType", "None", "Counterbore");
    toggle(&mut h, &mut f, "HoleCutCustomValues");
    quantity(&mut h, &mut f, "HoleCutDepth", "0.01");
    quantity(&mut h, &mut f, "HoleCutDiameter", "0.25");
    assert_eq!(h.params()["properties"]["HoleCutDepth"]["default_unit"], "in");
    choose(&mut h, &mut f, "HoleCutType", "Counterbore", "None");
    h.preview(true);
    assert_eq!(h.params()["properties"]["HoleCutDepth"]["quantity_input"], "0.01");
    choose(&mut h, &mut f, "HoleCutType", "None", "Counterbore");
    h.preview(true);
    assert_eq!(object(&h.editor.state,"Hole")["properties"]["HoleCutDepth"], expected["items"][0]["base_value"]);
    assert_eq!(object(&h.editor.state,"Hole")["properties"]["HoleCutDiameter"], expected["items"][1]["base_value"]);
    quantity(&mut h, &mut f, "Depth", "0.25");
    choose(&mut h, &mut f, "DepthType", "Dimension", "ThroughAll");
    h.preview(true);
    assert_eq!(h.params()["properties"]["Depth"]["quantity_input"], "0.25");
    choose(&mut h, &mut f, "DepthType", "ThroughAll", "Dimension");
    h.preview(true);
    assert_eq!(object(&h.editor.state,"Hole")["properties"]["Depth"], expected["items"][1]["base_value"]);
    assert_eq!(object(&h.editor.state,"Hole")["property_metadata"]["Depth"]["quantity"]["default_unit"], "in");
    // Compare two native inspection snapshots after display meshing, as in the
    // public settings guard tests. Acceptance snapshots precede lazy meshing.
    h.send("inspect", json!({}));
    let before = h.editor.state.clone();
    h.editor.send("document_settings", json!({"unit_system":3}));
    h.receive("document_settings", false);
    assert!(h.editor.error.as_ref().unwrap().contains("preview_active"));
    assert_eq!(h.editor.state, before);
    fixed_click_finish(&mut h, true);
    h.send("save", json!({"path":acceptance_dir().join("hole-imperial.FCStd")}));
    h.send("sketch_begin", json!({"object":"Profile","token":"hole-units-sketch"}));
    h.send("inspect", json!({}));
    let before = h.editor.state.clone();
    h.editor.send("document_settings", json!({"unit_system":0}));
    h.receive("document_settings", false);
    assert!(h.editor.error.as_ref().unwrap().contains("sketch_active"));
    assert_eq!(h.editor.state, before);
    h.send("sketch_cancel", json!({"token":"hole-units-sketch"}));
    assert_eq!(h.editor.state["units"]["index"], 3);
    f.save("hole-imperial-widgets.json");
}
