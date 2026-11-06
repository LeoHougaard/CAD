use super::*;

fn editor() -> Editor {
    let (jobs, _input) = std::sync::mpsc::sync_channel(1);
    let (_output, replies) = std::sync::mpsc::channel();
    let mut editor = Editor {
        jobs, replies, busy: false,
        registry: crate::packages::Registry::builtin().unwrap(),
        state: json!({"document":"OffsetStudy","revision":1,"objects":[]}),
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
    editor.sketch_ui.tool = Tool::Line;
    editor
}

#[test]
fn offset_endpoint_and_origin_snap_exactly_and_alt_keeps_freehand_input() {
    let editor = editor();
    let before = editor.state.clone();
    for (point, expected) in [
        ([60., 20., 0.], SketchElement { index: 0, point: 2 }),
        ([0., 0., 0.], SketchElement { index: -1, point: 1 }),
    ] {
        let cursor = editor.viewport.sketch_screen_point(point) + egui::vec2(9., 0.);
        assert_eq!(editor.snap(cursor, false), Some((point, Some(expected))));
        let (free, hit) = editor.snap(cursor, true).unwrap();
        assert_eq!(hit, None);
        assert_ne!(free, point);
    }
    assert_eq!(editor.state, before, "hover and snapping must not mutate the document");
}

#[test]
fn near_horizontal_intent_aligns_but_outside_click_and_alt_remain_free() {
    let mut editor = editor();
    editor.sketch_ui.points = vec![[20., 60., 0.]];
    let target = editor.viewport.sketch_screen_point([40., 60., 0.]);
    let near = target + egui::vec2(0., 7.);
    assert_eq!(editor.snap(near, false).unwrap().0[1], 60.);
    assert_ne!(editor.snap(near, true).unwrap().0[1], 60.);
    let outside = target + egui::vec2(0., 9.);
    assert_ne!(editor.snap(outside, false).unwrap().0[1], 60.);
}
