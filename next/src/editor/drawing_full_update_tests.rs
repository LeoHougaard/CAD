use super::*;
fn fixture() -> (Editor, Receiver<Job>) {
    let state = json!({"document":"ActualDocument","revision":17,"available":true,"preview":null,"sketch_edit":null,
        "drawing":{"document_uid":"actual-uid","objects":[]},
        "drawing_capabilities":{"available":true,"version":3,"projection":"Third angle","templates":[{"id":"ANSI-A-ASME-landscape-v1","paper_mm":[279.4,215.9]}]},
        "objects":[{"name":"ActualAssembly","label":"Source assembly","type":"Assembly::AssemblyObject","native_id":901}]});
    let (jobs, input) = mpsc::sync_channel(1);
    let (_output, replies) = mpsc::channel();
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
    (editor, input)
}
fn frame(e: &mut Editor, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        ctx.run(egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1280.,850.))),events,..Default::default()},
            |ctx| eframe::App::update(e,ctx,&mut eframe::Frame::_new_kittest()))
    }
fn click(e: &mut Editor, ctx: &egui::Context, label: &str) {
    frame(e, ctx, vec![]);
    let out = frame(e, ctx, vec![]);
    let pos = out
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() * 0.5),
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing button {label}"));
    for pressed in [true, false] {
        frame(
            e,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}
fn begin(e: &mut Editor, ctx: &egui::Context) {
    e.selected_object = Some("ActualAssembly".into());
    click(e, ctx, "Drawing");
    click(e, ctx, "Create drawing...");
}
#[test]
fn drawing_buttons_confirm_current_identity_and_cancel_without_request() {
    let (mut e, input) = fixture();
    let ctx = egui::Context::default();
    begin(&mut e, &ctx);
    assert!(input.try_recv().is_err());
    click(&mut e, &ctx, "Cancel");
    assert!(e.draft.is_none());
    assert!(input.try_recv().is_err());
    begin(&mut e, &ctx);
    click(&mut e, &ctx, "Apply");
    let j = input.try_recv().unwrap();
    assert_eq!(j.op, "transaction");
    assert_eq!(j.parameters["document"], "ActualDocument");
    assert_eq!(j.parameters["expected_revision"], 17);
    let edits = j.parameters["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["op"], "drawing_create_page");
    let mut keys: Vec<_> = edits[0].as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["name", "op", "projection", "scale", "source", "template_id"]);
    assert_eq!(
        edits[0]["source"],
        json!({"object":"ActualAssembly","native_id":901,"document_uid":"actual-uid"})
    );
    assert!(edits[0].get("owner").is_none());
    assert!(edits[0].get("property").is_none());
}
#[test]
fn drawing_stale_revision_uid_native_id_and_foreign_lease_are_denied() {
    for field in ["revision", "uid", "id", "preview", "sketch"] {
        let (mut e, input) = fixture();
        let ctx = egui::Context::default();
        begin(&mut e, &ctx);
        match field {
            "revision" => e.state["revision"] = json!(18),
            "uid" => e.state["drawing"]["document_uid"] = json!("replacement"),
            "id" => e.state["objects"][0]["native_id"] = json!(902),
            "preview" => e.state["preview"] = json!({"token":"foreign"}),
            _ => e.state["sketch_edit"] = json!({"token":"foreign"}),
        };
        click(&mut e, &ctx, "Apply");
        assert!(input.try_recv().is_err(), "{field}");
        assert!(e.error.is_some());
        click(&mut e, &ctx, "Cancel");
        assert!(e.draft.is_none());
        assert!(input.try_recv().is_err());
    }
}
#[test]
fn drawing_insert_orientations_layout_limits_and_unavailable() {
    let (mut e, input) = fixture();
    let source = json!({"object":"ActualAssembly","native_id":901,"document_uid":"actual-uid"});
    let page = json!({"object":"Paper","native_id":902,"document_uid":"actual-uid"});
    let view = json!({"object":"LineView","native_id":903,"document_uid":"actual-uid"});
    e.state["drawing"]["objects"] = json!([{"identity":page,"type":"TechDraw::DrawPage","source":source,"scale":0.5,"views":[view]},
        {"identity":view,"type":"TechDraw::DrawViewPart","x":{"value":75.},"y":{"value":65.}}]);
    for orientation in ["Front", "Right", "Top", "Iso"] {
        e.selected_object = Some("Paper".into());
        let ctx = egui::Context::default();
        click(&mut e, &ctx, "Drawing");click(&mut e, &ctx, "Insert view...");
        e.draft.as_mut().unwrap().parameters["edit"]["orientation"] = json!(orientation);
        let ctx = egui::Context::default();
        click(&mut e, &ctx, "Apply");
        let j = input.try_recv().unwrap();
        assert_eq!(j.parameters["edits"][0]["op"], "drawing_insert_view");
        assert_eq!(j.parameters["edits"][0]["orientation"], orientation);
        assert_eq!(j.parameters["edits"][0]["page"], page);
        e.busy = false;
        e.draft = None;
    }
    let ctx = egui::Context::default();
    click(&mut e, &ctx, "Drawing");click(&mut e, &ctx, "Layout...");
    let p = e.draft.as_ref().unwrap().parameters.clone();
    assert_eq!(p["edit"]["views"][0]["view"], view);
    // Rendering clamps these DragValues before Apply. Verify the actual
    // bounded command; raw invalid-edit rejection stays in the Root tests.
    for (scale, x, expected_scale, expected_x) in [(0., 75., 1e-6, 75.), (1001., 75., 1000., 75.), (0.5, 280., 0.5, 279.4)] {
        if e.draft.is_none() {click(&mut e, &ctx, "Drawing");click(&mut e, &ctx, "Layout...");}
        // A newly opened task owns a new token; retain that task's owner and
        // token rather than restoring the first task's whole parameter object.
        let draft=e.draft.as_mut().unwrap();
        draft.parameters["edit"]["scale"] = json!(scale);
        draft.parameters["edit"]["views"][0]["position_mm"][0] = json!(x);
        assert_eq!(draft.parameters["task_token"].as_str(),Some(draft.token.as_str()));
        click(&mut e, &ctx, "Apply");
        let job=input.try_recv().unwrap_or_else(|error|panic!("Layout scale={scale} x={x}: {error:?}; editor error={:?}; draft={:?}",e.error,e.draft.as_ref().map(|d|&d.parameters)));
        assert_eq!(job.op,"transaction");
        let edit=&job.parameters["edits"][0];
        assert_eq!(edit["op"],"drawing_layout");assert_eq!(edit["page"],page);
        assert_eq!(edit["scale"].as_f64(),Some(expected_scale));
        assert_eq!(edit["views"][0]["position_mm"][0].as_f64(),Some(expected_x));
        e.busy=false;e.draft=None;
    }
    click(&mut e, &ctx, "Drawing");click(&mut e, &ctx, "Layout...");
    click(&mut e, &ctx, "Cancel");
    e.state["drawing_capabilities"]["available"] = json!(false);
    click(&mut e, &ctx, "Drawing");
    let out=frame(&mut e,&ctx,vec![]);
    assert!(out.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="Drawing tools are unavailable in this session.")));
    assert!(e.draft.is_none());
    assert!(input.try_recv().is_err());
}
