//! Matched R1 regressions. Native conversion is queried, never reconstructed here.
use super::*;

fn texts(output: &egui::FullOutput) -> Vec<String> {
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}
fn visible(n: &mut Native, text: &str) {
    n.frame(vec![]);
    n.frame(vec![]);
    let rendered = texts(&n.frame(vec![]));
    assert!(
        rendered.iter().any(|s| s == text),
        "Missing {text:?}: {rendered:?}"
    );
}
fn meta(n: &Native, name: &str) -> Value {
    n.object(name)["property_metadata"]["Edges"].clone()
}
fn radius_seed(n: &Native, name: &str) -> String {
    meta(n, name)["quantity"][0][1]["input"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn quantity(n: &mut Native, unit: &str, text: &str) -> Value {
    n.send("quantities", json!({"items":[{"unit":unit,"text":text}]}));
    n.events.last().unwrap()["data"]["items"][0]["base_value"].clone()
}
fn pick(n: &mut Native, owner: &str, edge: &str) {
    n.editor.viewport.selections = vec![crate::viewport::Selection {
        object: owner.into(),
        subelement: edge.into(),
    }];
}
fn cancel(n: &mut Native) {
    n.editor.finish_preview(false);
    if n.editor.busy {
        n.receive("preview_cancel", true);
    }
}

#[test]
#[ignore = "pinned native; actual main Editor fillet/chamfer forms"]
fn fixed_r1_01_main_radius_forms_keep_native_context_and_ownership() {
    for (kind, name) in [("Part::Fillet", "Fillet"), ("Part::Chamfer", "Chamfer")] {
        let mut n = Native::case(&format!("r1-01-{name}"));
        n.send(
            "open",
            json!({"path":std::env::var("CAD_UNITS_R1_FIXTURE").unwrap()}),
        );
        if name == "Chamfer" {
            n.send(
                "transaction",
                json!({"edits":[{"op":"add_object","type":kind,"name":name},
                {"op":"set","object":name,"property":"Base","value":{"link":"Box"}},
                {"op":"set","object":name,"property":"Edges","value":[[1,2.54,2.54]]}]}),
            );
        }
        n.save("baseline.FCStd");
        let baseline = n.object(name)["properties"]["Edges"].clone();
        let history = (
            n.editor.state["undo_count"].clone(),
            n.editor.state["redo_count"].clone(),
        );
        let native_meta = meta(&n, name);
        let seed = radius_seed(&n, name);
        assert_eq!(native_meta["quantity"][0][1]["default_unit"], "in");
        n.editor.edit_native_properties(name);
        visible(&mut n, &seed);
        assert!(
            n.editor.draft.as_ref().unwrap().parameters["properties"]
                .as_object()
                .unwrap()
                .is_empty()
        );
        n.text(&seed, "0.12");
        assert_eq!(
            n.editor.draft.as_ref().unwrap().parameters["properties"]["Edges"][0][1]["default_unit"],
            "in"
        );
        n.editor.preview();
        n.receive("preview", true);
        let expected = quantity(&mut n, "mm", "0.12 in");
        assert_eq!(n.object(name)["properties"]["Edges"][0][1], expected);
        assert_eq!(n.object(name)["properties"]["Edges"][0][2], baseline[0][2]);
        let seed = radius_seed(&n, name);
        n.text(&seed, "0.13 in");
        n.editor.preview();
        n.receive("preview", true);
        let accepted = n.object(name)["properties"]["Edges"].clone();
        let seed = radius_seed(&n, name);
        n.text(&seed, "2 s");
        n.editor.preview();
        n.receive("preview", false);
        assert_eq!(n.object(name)["properties"]["Edges"], accepted);
        n.text("2 s", "0.1 in");
        n.editor.preview();
        n.receive("preview", true);
        cancel(&mut n);
        assert_eq!(n.object(name)["properties"]["Edges"], baseline);
        assert_eq!(
            (
                n.editor.state["undo_count"].clone(),
                n.editor.state["redo_count"].clone()
            ),
            history
        );
        assert!(!n.editor.dirty);

        // New rows use the native default, with no round trip of untouched numbers.
        n.editor.edit_native_properties(name);
        n.click("Add edge");
        let default = native_meta["quantity_default"]["input"].as_str().unwrap();
        visible(&mut n, default);
        n.editor.draft.as_mut().unwrap().parameters["properties"]["Edges"][1][0] = json!(2);
        n.text(default, "0.04");
        let rows = &n.editor.draft.as_ref().unwrap().parameters["properties"]["Edges"];
        assert_eq!(rows[0], baseline[0]);
        assert_eq!(rows[1][1]["default_unit"], "in");
        assert_eq!(rows[1][2], 1.);
        n.editor.preview();
        n.receive("preview", true);
        assert_eq!(
            n.object(name)["properties"]["Edges"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let expected = quantity(&mut n, "mm", "0.04 in");
        assert_eq!(n.object(name)["properties"]["Edges"][1][1], expected);
        cancel(&mut n);
        assert_eq!(n.object(name)["properties"]["Edges"], baseline);

        // Replacing row zero by selection must not reuse its old 2.54 mm metadata.
        pick(&mut n, "Box", "Edge2");
        n.editor.edit_native_properties(name);
        n.click("Use selected edges");
        visible(&mut n, default);
        assert_eq!(
            n.editor.draft.as_ref().unwrap().parameters["properties"]["Edges"],
            json!([[2, 1., 1.]])
        );
        n.text(default, "0.05");
        n.editor.preview();
        n.receive("preview", true);
        let expected = quantity(&mut n, "mm", "0.05 in");
        assert_eq!(n.object(name)["properties"]["Edges"][0][1], expected);
        pick(&mut n, "Fillet", "Edge1");
        let rendered = texts(&n.frame(vec![]));
        assert!(
            rendered
                .iter()
                .any(|s| s.contains("belongs to another object")),
            "{rendered:?}"
        );
        cancel(&mut n);
        assert_eq!(n.object(name)["properties"]["Edges"], baseline);

        // The main creation form keeps its draft Base and selected-edge identity.
        pick(&mut n, "Box", "Edge3");
        n.editor.selected_object = Some("Box".into());
        n.editor.body_ui.selected = vec!["Box".into()];
        n.editor.start_native_feature(kind);
        n.receive("feature_schema", true);
        let created = n.editor.draft.as_ref().unwrap().parameters["name"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            n.editor.draft.as_ref().unwrap().parameters["properties"]["Base"]["link"],
            "Box"
        );
        n.click("Use selected edges");
        visible(&mut n, default);
        n.text(default, "0.06");
        n.editor.preview();
        n.receive("preview", true);
        let expected = quantity(&mut n, "mm", "0.06 in");
        assert_eq!(n.object(&created)["properties"]["Edges"][0][1], expected);
        assert_eq!(n.object(&created)["properties"]["Edges"][0][0], 3);
        assert_eq!(n.object(&created)["properties"]["Base"]["link"], "Box");
        n.click("Apply");
        n.receive("preview_commit", true);
        assert!(n.object(&created)["shape"]["valid"].as_bool().unwrap());

        n.send("transaction",json!({"edits":[{"op":"set","object":"Box","property":"Label","value":"Redo control"}]}));
        n.send("undo", json!({}));
        n.save("before-deferred.FCStd");
        let before = n.editor.state.clone();
        assert_eq!(before["redo_count"], 1);
        n.editor.edit_native_properties(&created);
        assert!(n.editor.draft.as_ref().unwrap().deferred);
        let seed = radius_seed(&n, &created);
        n.text(&seed, "0.07");
        n.editor.preview();
        assert!(!n.editor.busy);
        assert_eq!(n.editor.state, before);
        n.click("Cancel");
        assert_eq!(n.editor.state, before);
        assert!(!n.editor.dirty);
        n.send("redo", json!({}));
        assert_eq!(n.object("Box")["label"], "Redo control");
        n.save("accepted.FCStd");
        fs::write(n.output.join("finding-response.json"),serde_json::to_vec_pretty(&json!({"finding":"DU-R1-01","type":kind,"main_form":true,"existing_new_added_selected_rows":true,"foreign_owner_rejected":true,"deferred_cancel_redo":true,"native_metadata":native_meta})).unwrap()).unwrap();
    }
}

#[test]
#[ignore = "pinned native; annotated radian package and native contract"]
fn fixed_r1_02_radian_package_preserves_display_and_numeric_contract() {
    let mut n = Native::case("r1-02-radians");
    n.fixture("FixedRadianPackage");
    n.save("baseline.FCStd");
    let before = n.object("Sketch")["sketch"].clone();
    let history = n.editor.state["undo_count"].clone();
    let packages = n.output.join("packages");
    fs::create_dir_all(&packages).unwrap();
    fs::write(
        packages.join("angle.json"),
        include_str!("../../../tests/fixtures/units_angle.json"),
    )
    .unwrap();
    n.editor.registry = Registry::load(&packages).unwrap();
    n.editor.sketch = Some("Sketch".into());
    n.editor.start_package("review.angle");
    n.receive("quantities", true);
    for text in ["90", "180", "1 rad"] {
        let seed = n.editor.units_ui.fields["angle"]["input"]
            .as_str()
            .unwrap()
            .to_owned();
        n.text(&seed, text);
        n.editor.preview();
        n.receive("quantities", true);
        n.receive("preview", true);
        let meta = n.editor.units_ui.fields["angle"].clone();
        let parameter = n.editor.draft.as_ref().unwrap().parameters["angle"].clone();
        assert_eq!(parameter, meta["base_value"]);
        assert_eq!(
            n.object("Sketch")["sketch"]["constraints"][4]["value"],
            parameter
        );
        visible(&mut n, meta["input"].as_str().unwrap());
        assert!(
            !texts(&n.frame(vec![]))
                .iter()
                .any(|s| s == &parameter.to_string())
        );
    }
    let seed = n.editor.units_ui.fields["angle"]["input"]
        .as_str()
        .unwrap()
        .to_owned();
    n.text(&seed, "1 in");
    n.editor.preview();
    n.receive("quantities", false);
    assert_eq!(n.object("Sketch")["sketch"]["constraints"][4]["value"], 1.);
    n.text("1 in", "45 deg");
    n.editor.preview();
    n.receive("quantities", true);
    n.receive("preview", true);
    visible(&mut n, "45 \u{b0}");
    cancel(&mut n);
    assert_eq!(n.object("Sketch")["sketch"], before);
    assert_eq!(n.editor.state["undo_count"], history);
    assert!(!n.editor.dirty);
    let edits = n
        .editor
        .registry
        .build(
            "review.angle",
            &json!({"object":"Sketch","index":4,"angle":1.0}),
        )
        .unwrap();
    assert_eq!(edits[1]["args"][1], 1.0);
    n.send("transaction", json!({"edits":edits}));
    assert_eq!(n.object("Sketch")["sketch"]["constraints"][4]["value"], 1.0);
    n.send("transaction",json!({"edits":[{"op":"expression","object":"Sketch","property":"Constraints[4]","expression":"1 rad"}]}));
    assert!(
        n.object("Sketch")["expressions"]
            .as_object()
            .unwrap()
            .values()
            .any(|v| v == "1 rad")
    );
    assert_eq!(n.object("Sketch")["sketch"]["constraints"][4]["value"], 1.0);
    n.save("radian-legacy.FCStd");
    fs::write(n.output.join("finding-response.json"),serde_json::to_vec_pretty(&json!({"finding":"DU-R1-02","successive_bare_explicit_edits":true,"invalid_recovery":true,"cancel_preserves_baseline":true,"legacy_numeric_radians":1.0,"native_expression":"1 rad"})).unwrap()).unwrap();
}

#[test]
#[ignore = "pinned native; actual Help and Units Escape ownership"]
fn fixed_r1_03_help_owns_first_escape_and_retains_units_selection() {
    let mut n = Native::case("r1-03-help");
    n.fixture("FixedHelpUnits");
    n.save("baseline.FCStd");
    let before = n.editor.state.clone();
    let dirty = n.editor.dirty;
    n.click("Units");
    n.click("Document units...");
    let choice = n.editor.state["units"]["choices"][3]["label"]
        .as_str()
        .unwrap()
        .to_owned();
    n.click(&choice);
    assert_eq!(n.editor.units_ui.selection, Some(3));
    n.key(Key::F1, egui::Modifiers::NONE);
    assert!(workflow_guide::active(&n.ctx));
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert!(!workflow_guide::active(&n.ctx));
    assert!(n.editor.units_ui.active());
    assert_eq!(n.editor.units_ui.selection, Some(3));
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.dirty, dirty);
    assert!(!n.editor.busy);
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert!(!n.editor.units_ui.active());
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.dirty, dirty);
    assert!(!n.editor.busy);
    fs::write(n.output.join("finding-response.json"),serde_json::to_vec_pretty(&json!({"finding":"DU-R1-03","first_escape_help_only":true,"unsent_selection_preserved":3,"second_escape_units_only":true,"state_revision_dirty_history_unchanged":true})).unwrap()).unwrap();
}

#[test]
#[allow(clippy::approx_constant)] // Exact native query fixture values, without conversion arithmetic.
fn shared_quantity_callers_keep_native_internal_base_source_and_unitless_values() {
    let context = egui::Context::default();
    let cases = [
        (
            "package requested radians",
            json!(1.5707963267948966),
            json!({"base_value":1.5707963267948966,"value":90.,"source":"90","default_unit":"\u{b0}","input":"90 \u{b0}"}),
        ),
        (
            "native angle property / MultiTransform",
            json!(90.),
            json!({"value":90.,"default_unit":"\u{b0}","input":"90 \u{b0}"}),
        ),
        (
            "sketch angular increment source",
            json!("10 deg"),
            json!({"source":"10 deg","base_value":0.17453292519943295,"value":10.,"default_unit":"\u{b0}","input":"10 \u{b0}"}),
        ),
        (
            "physical position / assembly / attachment / pole",
            json!(25.4),
            json!({"value":25.4,"base_value":25.4,"default_unit":"in","input":"1 in"}),
        ),
        (
            "unitless weight and ratio",
            json!(1.5),
            json!({"value":1.5,"base_value":1.5,"default_unit":"","input":"1.5"}),
        ),
    ];
    for (caller, mut value, metadata) in cases {
        let untouched = value.clone();
        let mut changed = false;
        let output = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.push_id(caller, |ui| {
                    changed = crate::editor::units::quantity_field(ui, &mut value, &metadata);
                });
            });
        });
        assert!(!changed);
        assert_eq!(value, untouched);
        assert!(
            texts(&output)
                .iter()
                .any(|t| t == metadata["input"].as_str().unwrap()),
            "{caller}: {:?}",
            texts(&output)
        );
    }
}

#[test]
fn shared_quantity_callers_generic_edge_widget_matches_native_values_after_row_changes() {
    let native: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/units_fillet_metadata.json"
    ))
    .unwrap();
    let mut moved = native.clone();
    moved["quantity"].as_array_mut().unwrap().push(json!([
        null,
        native["quantity_default"],
        native["quantity_default"]
    ]));
    let objects = vec![
        json!({"name":"Fillet","properties":{"Base":{"link":"Box"}}}),
        json!({"name":"Box","label":"Box"}),
    ];
    for (label, mut edges, metadata, expected) in [
        (
            "existing",
            json!([[1, 2.54, 2.54]]),
            native.clone(),
            vec![
                native["quantity"][0][1]["input"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            ],
        ),
        (
            "added",
            json!([[1, 2.54, 2.54], [2, 1., 1.]]),
            native.clone(),
            vec![
                native["quantity"][0][1]["input"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                native["quantity_default"]["input"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            ],
        ),
        (
            "selected",
            json!([[2, 1., 1.]]),
            native.clone(),
            vec![
                native["quantity_default"]["input"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            ],
        ),
        (
            "moved or removed",
            json!([[2, 1., 1.], [1, 2.54, 2.54]]),
            moved,
            vec![
                native["quantity"][0][1]["input"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                native["quantity_default"]["input"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            ],
        ),
    ] {
        let untouched = edges.clone();
        let mut buffers = std::collections::BTreeMap::new();
        let mut changed = false;
        let context = egui::Context::default();
        let output = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                changed = crate::editor::properties::property_widget(
                    ui,
                    "Edges",
                    "Part::PropertyFilletEdges",
                    &metadata,
                    &mut edges,
                    &mut buffers,
                    &objects,
                    "Fillet",
                    &[],
                );
            });
        });
        assert!(!changed);
        assert_eq!(edges, untouched);
        for seed in expected {
            assert!(
                texts(&output).contains(&seed),
                "{label}: {:?}",
                texts(&output)
            );
        }
    }
}
