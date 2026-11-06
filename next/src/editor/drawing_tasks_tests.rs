use super::*;
use crate::editor::*;
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
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 850.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::TopBottomPanel::top("test_toolbar").show(ctx, |ui| e.drawing_controls(ui));
            if e.draft.is_some() {
                e.drawing_panel(ctx);
            }
        },
    )
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
    let source = reference(&e.state, "ActualAssembly", "Assembly::AssemblyObject").unwrap();
    let page = json!({"object":"Paper","native_id":902,"document_uid":"actual-uid"});
    let view = json!({"object":"LineView","native_id":903,"document_uid":"actual-uid"});
    e.state["drawing"]["objects"] = json!([{"identity":page,"type":"TechDraw::DrawPage","source":source,"scale":0.5,"views":[view]},
        {"identity":view,"type":"TechDraw::DrawViewPart","x":{"value":75.},"y":{"value":65.}}]);
    for orientation in ["Front", "Right", "Top", "Iso"] {
        e.selected_object = Some("Paper".into());
        e.start_drawing("drawing_insert_view");
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
    e.start_drawing("drawing_layout");
    let p = e.draft.as_ref().unwrap().parameters.clone();
    assert_eq!(edit(&e.state, &p).unwrap()["views"][0]["view"], view);
    for (scale, x) in [(0., 75.), (1001., 75.), (0.5, 280.)] {
        let mut bad = p.clone();
        bad["edit"]["scale"] = json!(scale);
        bad["edit"]["views"][0]["position_mm"][0] = json!(x);
        assert!(edit(&e.state, &bad).is_err());
    }
    e.finish_drawing(false);
    e.state["drawing_capabilities"]["available"] = json!(false);
    e.start_drawing("drawing_layout");
    assert!(e.draft.is_none());
    assert!(input.try_recv().is_err());
}
