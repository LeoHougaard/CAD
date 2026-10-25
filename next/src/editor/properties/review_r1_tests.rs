#[test]
#[ignore = "independent review probe"]
fn review_inactive_cut_values_before_first_successful_preview() {
    let mut h = open_hole("review-inactive-cut");
    let initial = volume(&h);
    let mut form = Form::new(false);
    choose(&mut h, &mut form, "HoleCutType", "None", "Counterbore");
    toggle(&mut h, &mut form, "HoleCutCustomValues");
    quantity(&mut h, &mut form, "HoleCutDepth", "0.25 mm");
    quantity(&mut h, &mut form, "HoleCutDiameter", "5.5 mm");
    choose(&mut h, &mut form, "HoleCutType", "Counterbore", "None");
    write_json("review-inactive-cut-candidate.json", &json!({"draft":h.params(),
        "projected":crate::hole_commands::active_parameters(&h.editor.property_ui.schema,h.params())}));
    h.preview(true);
    assert_eq!(volume(&h),initial);
    h.editor.finish_preview(false);
    h.receive("preview_cancel", true);
}

#[test]
#[ignore = "independent review probe"]
fn review_open_must_reject_invalid_depth_cached_shape() {
    let root=input_file("CAD_HOLE_REVIEW_FIXTURES");
    let mut h=NativeEditor::new("review-invalid-cached-shape");
    h.editor.send("open",json!({"path":root.join("invalid-depth-cached-shape.FCStd")}));
    h.receive("open",false);
    assert!(h.editor.error.as_ref().unwrap().contains("invalid_model"));
}

#[test]
#[ignore = "independent review probe"]
fn review_open_must_regenerate_stale_depth_shape() {
    let root=input_file("CAD_HOLE_REVIEW_FIXTURES");
    let mut h=NativeEditor::new("review-stale-cached-shape");
    h.send("open",json!({"path":root.join("stale-depth.FCStd")}));
    assert_eq!(object(&h.editor.state,"Hole")["properties"]["Depth"],7.0);
    assert!((volume(&h)-31881.502603908782).abs()<1e-7,
        "Open published cached volume {} rather than touched native volume 31881.502603908782",volume(&h));
}
