//! Actual egui widgets/menu, with native execution kept in the explicit probe.
use super::super::*;
use super::*;

struct Ui {
    context: egui::Context,
    labels: Vec<(String, egui::Rect)>,
}

impl Ui {
    fn new() -> Self {
        Self {
            context: egui::Context::default(),
            labels: vec![],
        }
    }
    fn frame(&mut self, events: Vec<egui::Event>, mut draw: impl FnMut(&mut egui::Ui)) {
        let output = self.context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400., 1200.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| draw(ui));
            },
        );
        fn collect(shape: &egui::Shape, labels: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::Shape::Text(t) => labels.push((
                    t.galley.job.text.clone(),
                    egui::Rect::from_min_size(t.pos, t.galley.size()),
                )),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, labels);
                    }
                }
                _ => (),
            }
        }
        self.labels.clear();
        for shape in output.shapes {
            collect(&shape.shape, &mut self.labels);
        }
    }
    fn click(&mut self, label: &str, mut draw: impl FnMut(&mut egui::Ui)) {
        let pos = self
            .labels
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("missing {label}; {:?}", self.labels))
            .1
            .center();
        self.frame(
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            &mut draw,
        );
        self.frame(
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            draw,
        );
    }
}

fn picks(values: &[(&str, &str)]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|(o, s)| (o.to_string(), s.to_string()))
        .collect()
}

fn selection_widget(
    kind: &str,
    selected: &[(String, String)],
    value: &mut Value,
) -> (bool, Vec<String>) {
    let mut ui = Ui::new();
    let objects = vec![
        json!({"name":"Original","label":"Original"}),
        json!({"name":"A","label":"A"}),
        json!({"name":"B","label":"B"}),
    ];
    let mut buffers = BTreeMap::new();
    let mut changed = false;
    let mut draw = |ui: &mut egui::Ui| {
        changed |= property_widget(
            ui,
            "Reference",
            kind,
            &json!({}),
            value,
            &mut buffers,
            &objects,
            "Owner",
            selected,
        );
    };
    ui.frame(vec![], &mut draw);
    ui.frame(vec![], &mut draw);
    ui.click("Use selection", &mut draw);
    (
        changed,
        ui.labels.into_iter().map(|(text, _)| text).collect(),
    )
}

#[test]
fn ambiguous_singular_selection_keeps_existing_field_and_explains_correction() {
    for kind in ["App::PropertyLink", "App::PropertyLinkSub"] {
        let original = if kind.ends_with("Sub") {
            json!({"link":"Original","subelements":["Face3"]})
        } else {
            json!({"link":"Original"})
        };
        let mut value = original.clone();
        let (changed, labels) =
            selection_widget(kind, &picks(&[("A", "Face1"), ("B", "Face2")]), &mut value);
        assert_eq!(
            value, original,
            "{kind} silently replaced the field with the first selected object"
        );
        assert!(!changed);
        assert!(
            labels
                .iter()
                .any(|text| text.contains("one native object") && text.contains("retry")),
            "missing correction: {labels:?}"
        );
    }
}

#[test]
fn duplicate_same_object_subelements_are_valid_and_keep_native_cardinality() {
    let selected = picks(&[("A", "Face2"), ("A", "Face1"), ("A", "Face2")]);
    for (kind, expected) in [
        ("App::PropertyLink", json!({"link":"A"})),
        (
            "App::PropertyLinkSub",
            json!({"link":"A","subelements":["Face2","Face1"]}),
        ),
        (
            "App::PropertyXLinkSub",
            json!({"link":"A","subelements":["Face2","Face1"]}),
        ),
        ("App::PropertyLinkList", json!([{"link":"A"}])),
        (
            "App::PropertyLinkSubList",
            json!([{"link":"A","subelements":["Face2","Face1"]}]),
        ),
    ] {
        let mut value = Value::Null;
        assert!(selection_widget(kind, &selected, &mut value).0);
        assert_eq!(value, expected, "{kind}");
    }
}

#[test]
fn list_fields_retain_mixed_objects_whole_targets_and_subelements() {
    let selected = picks(&[
        ("A", ""),
        ("A", "Face2"),
        ("B", "Face1"),
        ("A", "Face2"),
        ("B", ""),
    ]);
    for (kind, expected) in [
        ("App::PropertyLinkList", json!([{"link":"A"},{"link":"B"}])),
        (
            "App::PropertyLinkSubList",
            json!([{"link":"A","subelements":["","Face2"]},{"link":"B","subelements":["Face1",""]}]),
        ),
    ] {
        let mut value = Value::Null;
        assert!(selection_widget(kind, &selected, &mut value).0);
        assert_eq!(value, expected, "{kind} dropped a valid native selection");
    }
    let mut value = Value::Null;
    selection_widget("App::PropertyLinkSub", &picks(&[("A", "")]), &mut value);
    assert_eq!(value, json!({"link":"A","subelements":[]}));
}

#[test]
fn stale_object_selection_keeps_field_then_corrected_selection_can_retry() {
    let mut value = json!({"link":"Original","subelements":["Face3"]});
    let original = value.clone();
    let (changed, labels) = selection_widget(
        "App::PropertyLinkSub",
        &picks(&[("Gone", "Face1")]),
        &mut value,
    );
    assert_eq!(
        value, original,
        "stale native object replaced a valid field"
    );
    assert!(!changed);
    assert!(
        labels
            .iter()
            .any(|text| text.contains("Gone") && text.contains("stale")),
        "{labels:?}"
    );
    assert!(
        selection_widget(
            "App::PropertyLinkSub",
            &picks(&[("A", "Face1"), ("A", "Face2")]),
            &mut value
        )
        .0
    );
    assert_eq!(value, json!({"link":"A","subelements":["Face1","Face2"]}));
}

#[test]
fn rejected_or_empty_list_selection_preserves_null_and_existing_values() {
    for kind in ["App::PropertyLinkList", "App::PropertyLinkSubList"] {
        for original in [
            Value::Null,
            json!([]),
            json!([{"link":"Original","subelements":["Face3"]}]),
        ] {
            for selected in [vec![], picks(&[("A", "Face1"), ("Gone", "Face2")])] {
                let mut value = original.clone();
                assert!(!selection_widget(kind, &selected, &mut value).0);
                assert_eq!(value, original, "{kind}");
            }
        }
    }
}

fn editor() -> (Editor, Receiver<Job>) {
    let (jobs, input) = mpsc::sync_channel(1);
    let (_, replies) = mpsc::channel();
    (
        Editor {
            jobs,
            replies,
            busy: false,
            registry: Registry::builtin().unwrap(),
            state: json!({"document":"Menu","revision":1,"objects":[
                {"name":"A","type":"Part::Box","label":"A","shape":{"solids":1}},
                {"name":"B","type":"Part::Box","label":"B","shape":{"solids":1}}
            ]}),
            viewport: Viewport::default(),
            sketch: None,
            selected_object: None,
            sketch_ui: SketchUi::default(),
            sketch_creation: None,
            body_ui: BodyUi::default(),
            property_ui: PropertyUi::default(),
            measurement_ui: super::super::measurements::MeasurementUi::default(),
            transform_ui: super::super::transforms::TransformUi::default(),
            exchange_ui: super::super::exchange::ExchangeUi::default(),
            draft: None,
            sequence: 0,
            error: None,
            status: String::new(),
            path: String::new(),
            file_input: String::new(),
            file_dialog: None,
            file_browser: super::super::file_browser::FileBrowser::default(),
            overwrite: false,
            dirty: false,
            pending_file: None,
            allow_close: false,
            fit_next: false,
        },
        input,
    )
}

#[test]
fn feature_menu_labels_uniquely_identify_and_open_the_two_union_operations() {
    let mut labels = std::collections::BTreeSet::new();
    for definition in feature_commands::definitions() {
        assert!(
            labels.insert((definition.group, definition.label)),
            "indistinguishable menu route: {} > {}",
            definition.group,
            definition.label
        );
    }
    for (label, native) in [
        ("Union of two objects", "Part::Fuse"),
        ("Union of multiple objects", "Part::MultiFuse"),
    ] {
        let (mut editor, jobs) = editor();
        let mut ui = Ui::new();
        ui.frame(vec![], |ui| editor.native_feature_menu(ui));
        ui.click("Features", |ui| editor.native_feature_menu(ui));
        ui.frame(vec![], |ui| editor.native_feature_menu(ui));
        ui.click("Part operations", |ui| editor.native_feature_menu(ui));
        ui.frame(vec![], |ui| editor.native_feature_menu(ui));
        ui.click(label, |ui| editor.native_feature_menu(ui));
        assert_eq!(editor.draft.as_ref().unwrap().parameters["type"], native);
        let request = jobs.try_recv().unwrap();
        assert_eq!(request.op, "feature_schema");
        assert_eq!(request.parameters["type"], native);
    }
}

fn profile_editor() -> (Editor, Receiver<Job>) {
    let (mut e, jobs) = editor();
    e.state = json!({"document":"ProfileScope","revision":1,"preview":{"token":"accepted","label":"accepted"},"objects":[
        {"name":"Body","type":"PartDesign::Body","body_state":{"tip":"Pad"}},
        {"name":"Original","label":"Original","type":"Sketcher::SketchObject","body":"Body"},
        {"name":"A","label":"A","type":"Sketcher::SketchObject","body":"Body"},
        {"name":"B","label":"B","type":"Sketcher::SketchObject","body":"Body"},
        {"name":"Pad","label":"Pad","type":"PartDesign::Pad","body":"Body",
         "properties":{"Profile":{"link":"Original","subelements":[]}},
         "property_types":{"Profile":"App::PropertyLinkSub"},
         "property_metadata":{"Profile":{"read_only":false}},"expressions":{}}
    ]});
    e.edit_native_properties("Pad");
    e.property_ui.filter = "Profile".into();
    e.draft.as_mut().unwrap().valid = true;
    (e, jobs)
}

fn pick_editor(e: &mut Editor, selected: &[(&str, &str)]) {
    e.viewport.selected = None;
    e.viewport.selections = selected
        .iter()
        .map(|(o, s)| crate::viewport::Selection {
            object: (*o).into(),
            subelement: (*s).into(),
        })
        .collect();
}

fn click_reference(e: &mut Editor, button: &str) -> Ui {
    let mut ui = Ui::new();
    ui.frame(vec![], |ui| e.native_property_fields(ui));
    ui.frame(vec![], |ui| e.native_property_fields(ui));
    ui.click(button, |ui| e.native_property_fields(ui));
    ui
}

#[test]
fn actual_editor_profile_rejects_stale_picks_before_filtering_and_allows_correction() {
    let (mut e, jobs) = profile_editor();
    let params = e.draft.as_ref().unwrap().parameters.clone();
    let state = e.state.clone();
    for picks in [
        vec![("A", ""), ("Gone", "")],
        vec![("Gone", "")],
        vec![("Pad", "Face1"), ("Gone", "")],
        vec![("A", ""), ("B", "")],
        vec![("Pad", "Face1")],
    ] {
        pick_editor(&mut e, &picks);
        let ui = click_reference(&mut e, "Use selection");
        assert_eq!(e.draft.as_ref().unwrap().parameters, params);
        assert_eq!(e.state, state);
        assert!(e.draft.as_ref().unwrap().valid && e.draft.as_ref().unwrap().changed.is_none());
        if picks.iter().any(|(o, _)| *o == "Gone") {
            assert!(
                ui.labels
                    .iter()
                    .any(|(text, _)| text.contains("Gone") && text.contains("stale"))
            );
        } else {
            assert!(!ui.labels.iter().any(|(text, _)| text.contains("stale")));
        }
        assert!(jobs.try_recv().is_err());
    }
    pick_editor(&mut e, &[("Pad", "Face1"), ("A", "")]);
    click_reference(&mut e, "Use selection");
    assert_eq!(
        e.draft.as_ref().unwrap().parameters["properties"]["Profile"],
        json!({"link":"A","subelements":[]})
    );
    assert!(e.draft.as_ref().unwrap().changed.is_some() && !e.draft.as_ref().unwrap().valid);
    assert_eq!(e.state, state);
    assert!(jobs.try_recv().is_err());
}

#[test]
fn actual_editor_termination_buttons_reject_missing_picks_before_scope_filtering() {
    for field in [
        "UpToFace",
        "UpToFace2",
        "UpToShape",
        "UpToShape2",
        "ReferenceAxis",
    ] {
        let (mut e, jobs) = profile_editor();
        let list = field.starts_with("UpToShape");
        let pad = e.state["objects"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|o| o["name"] == "Pad")
            .unwrap();
        pad["properties"][field] = if list {
            json!([{"link":"Original","subelements":["Face3"]}])
        } else {
            json!({"link":"Original","subelements":["Face3"]})
        };
        pad["property_types"][field] = json!(if list {
            "App::PropertyLinkSubList"
        } else {
            "App::PropertyLinkSub"
        });
        pad["property_metadata"][field] = json!({"read_only":false});
        e.edit_native_properties("Pad");
        e.property_ui.filter = field.into();
        let d = e.draft.as_mut().unwrap();
        let extent = if list { "UpToShape" } else { "UpToFace" };
        d.parameters["properties"] = if field.ends_with('2') {
            json!({"SideType":"Two sides","Type2":extent})
        } else if field == "ReferenceAxis" {
            json!({"UseCustomVector":false})
        } else {
            json!({"Type":extent})
        };
        d.valid = true;
        d.changed = None;
        let params = d.parameters.clone();
        let state = e.state.clone();
        for picks in [
            vec![("A", "Face2"), ("Gone", "Face1")],
            vec![("Pad", "Face1"), ("Gone", "Face2")],
            vec![("Gone", "")],
        ] {
            pick_editor(&mut e, &picks);
            let ui = click_reference(&mut e, "Use native selection");
            assert!(
                ui.labels
                    .iter()
                    .any(|(text, _)| text.contains("Gone") && text.contains("retry")),
                "{field}"
            );
            assert_eq!(e.draft.as_ref().unwrap().parameters, params);
            assert_eq!(e.state, state);
            assert!(e.draft.as_ref().unwrap().valid && e.draft.as_ref().unwrap().changed.is_none());
            assert!(jobs.try_recv().is_err());
        }
        if !list {
            pick_editor(&mut e, &[("A", "Face2"), ("A", "Face1")]);
            click_reference(&mut e, "Use native selection");
            assert_eq!(
                e.draft.as_ref().unwrap().parameters,
                params,
                "operation-specific singular target rule"
            );
        }
        pick_editor(&mut e, &[("Pad", "Face1"), ("A", "Face2")]);
        click_reference(&mut e, "Use native selection");
        let link = json!({"link":"A","subelements":["Face2"]});
        assert_eq!(
            e.draft.as_ref().unwrap().parameters["properties"][field],
            if list { json!([link]) } else { link }
        );
        assert!(e.draft.as_ref().unwrap().changed.is_some());
        assert!(jobs.try_recv().is_err());
    }
}

#[test]
fn actual_editor_invalid_profile_pick_preserves_foreign_copy_choice() {
    let (mut e, jobs) = profile_editor();
    e.state["objects"][1]["body"] = json!("OtherBody");
    e.edit_native_properties("Pad");
    let original = json!({"link":"Original","subelements":[]});
    let params = &mut e.draft.as_mut().unwrap().parameters;
    params["properties"]["Profile"] = original.clone();
    e.property_ui.reference.prepare(params, &original);
    params["reference_choice"]["request"]["mode"] = json!("dependent");
    e.draft.as_mut().unwrap().valid = true;
    e.property_ui.filter = "Profile".into();
    let before = e.draft.as_ref().unwrap().parameters.clone();
    pick_editor(&mut e, &[("A", ""), ("Gone", "")]);
    let ui = click_reference(&mut e, "Use selection");
    assert!(ui.labels.iter().any(|(text, _)| text == "Dependent copy"));
    assert_eq!(e.draft.as_ref().unwrap().parameters, before);
    assert!(e.draft.as_ref().unwrap().valid);
    assert!(jobs.try_recv().is_err());
}
