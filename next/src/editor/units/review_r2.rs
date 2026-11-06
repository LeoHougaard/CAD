use super::*;

fn rendered(n: &mut Native) -> Vec<String> {
    n.frame(vec![]);
    n.frame(vec![]);
    text_shapes(&n.frame(vec![]))
}
fn text_shapes(o: &egui::FullOutput) -> Vec<String> {
    o.shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}
fn cancel(n: &mut Native) {
    n.editor.finish_preview(false);
    if n.editor.busy {
        n.receive("preview_cancel", true);
    }
}
fn select(n: &mut Native, owner: &str, edge: &str) {
    n.editor.viewport.selections = vec![crate::viewport::Selection {
        object: owner.into(),
        subelement: edge.into(),
    }];
}
fn help_escape(n: &mut Native) {
    let events = n.events.len();
    n.key(Key::F1, egui::Modifiers::NONE);
    assert!(workflow_guide::active(&n.ctx));
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert!(!workflow_guide::active(&n.ctx));
    assert!(!n.editor.busy);
    assert_eq!(n.events.len(), events);
    assert!(matches!(n.raw.try_recv(), Err(mpsc::TryRecvError::Empty)));
}

#[test]
#[ignore = "independent r2 actual Editor edge Base and default contexts"]
fn review_r2_main_edges_use_draft_base_and_reject_invalid_default_contexts() {
    for name in ["Fillet", "Chamfer"] {
        let mut n = Native::case(&format!("r2-base-{name}"));
        n.send(
            "open",
            json!({"path":std::env::var("CAD_UNITS_R1_FIXTURE").unwrap()}),
        );
        let mut edits = vec![json!({"op":"add_object","type":"Part::Box","name":"Other"})];
        if name == "Chamfer" {
            edits.extend([
                json!({"op":"add_object","type":"Part::Chamfer","name":name}),
                json!({"op":"set","object":name,"property":"Base","value":{"link":"Box"}}),
                json!({"op":"set","object":name,"property":"Edges","value":[[1,2.54,2.54]]}),
            ]);
        }
        n.send("transaction", json!({"edits":edits}));
        n.save("baseline.FCStd");
        let baseline = n.object(name)["properties"].clone();
        let seed = n.object(name)["property_metadata"]["Edges"]["quantity"][0][1]["input"]
            .as_str()
            .unwrap()
            .to_owned();
        n.editor.edit_native_properties(name);
        assert!(rendered(&mut n).contains(&seed));
        n.text(&seed, "0.08");
        for bad in ["", "s"] {
            n.editor.draft.as_mut().unwrap().parameters["properties"]["Edges"][0][1]["default_unit"] =
                json!(bad);
            n.editor.preview();
            n.receive("preview", false);
            assert_eq!(n.object(name)["properties"]["Edges"], baseline["Edges"]);
        }
        n.editor.draft.as_mut().unwrap().parameters["properties"]["Edges"][0][1]["default_unit"] =
            json!("in");
        n.editor.preview();
        n.receive("preview", true);
        n.send(
            "quantities",
            json!({"items":[{"unit":"mm","text":"0.08 in"}]}),
        );
        assert_eq!(
            n.object(name)["properties"]["Edges"][0][1],
            n.events.last().unwrap()["data"]["items"][0]["base_value"]
        );
        cancel(&mut n);
        assert_eq!(n.object(name)["properties"], baseline);

        // The special main route must take Base from this draft, before preview.
        n.editor.edit_native_properties(name);
        n.editor.draft.as_mut().unwrap().parameters["properties"]["Base"] = json!({"link":"Other"});
        select(&mut n, "Box", "Edge2");
        assert!(
            rendered(&mut n)
                .iter()
                .any(|s| s.contains("Base Other; Box.Edge2 belongs to another object"))
        );
        select(&mut n, "Other", "Edge2");
        n.click("Use selected edges");
        assert_eq!(
            n.editor.draft.as_ref().unwrap().parameters["properties"]["Edges"],
            json!([[2, 1., 1.]])
        );
        let default = n.object(name)["property_metadata"]["Edges"]["quantity_default"]["input"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(rendered(&mut n).contains(&default));
        n.editor.preview();
        n.receive("preview", true);
        assert_eq!(n.object(name)["properties"]["Base"]["link"], "Other");
        assert_eq!(n.object(name)["properties"]["Edges"], json!([[2, 1., 1.]]));
        cancel(&mut n);
        assert_eq!(n.object(name)["properties"], baseline);
        assert!(!n.editor.dirty);
    }
}

#[test]
#[ignore = "independent r2 native query and real shared quantity widget"]
fn review_r2_native_seeds_diverse_requested_units_keep_exact_values_on_repaint() {
    let mut n = Native::case("r2-native-seeds");
    n.fixture("R2NativeSeeds");
    n.units(3);
    let before = n.editor.state.clone();
    let items = json!([
        {"unit":"rad","value":0.12345678912345678},
        {"unit":"deg","value":91.23456789123457},
        {"unit":"cm","value":0.12345678912345678},
        {"unit":"m^2","value":0.12345678912345678},
        {"unit":"cm^3","value":0.12345678912345678},
        {"unit":"m/s","value":0.12345678912345678},
        {"unit":"","value":0.12345678912345678},
        {"unit":"rad","text":"0.12345678912345678 rad"},
        {"unit":"cm","text":"1/2 in"}
    ]);
    n.send("quantities", json!({"items":items}));
    assert_eq!(n.editor.state, before);
    let metadata = n.events.last().unwrap()["data"]["items"]
        .as_array()
        .unwrap()
        .clone();
    let context = egui::Context::default();
    for (index, q) in metadata.iter().enumerate() {
        let seed = q["input"].as_str().unwrap();
        for key in ["base_value", "value", "source"] {
            let mut value = q[key].clone();
            let untouched = value.clone();
            for _ in 0..3 {
                let mut changed = false;
                let o = context.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.push_id((index, key), |ui| {
                            changed = super::super::quantity_field(ui, &mut value, q)
                        });
                    });
                });
                assert!(!changed);
                assert_eq!(value, untouched);
                assert!(
                    text_shapes(&o).iter().any(|s| s == seed),
                    "{index} {key} {q}"
                );
            }
        }
    }
    fs::write(
        n.output.join("native-seed-metadata.json"),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();
    assert_eq!(n.editor.state, before);
}

#[test]
#[ignore = "independent r2 actual Help, stale Units, preview and sketch owners"]
fn review_r2_help_preserves_stale_units_preview_and_sketch_owners_without_queries() {
    let mut n = Native::case("r2-help-owners");
    n.fixture("R2HelpOwners");
    n.save("baseline.FCStd");
    n.click("Units");
    n.click("Document units...");
    let choice = n.editor.state["units"]["choices"][3]["label"]
        .as_str()
        .unwrap()
        .to_owned();
    n.click(&choice);
    n.editor.jobs.try_send(Job{op:"transaction".into(),parameters:json!({
        "document":n.editor.state["document"],"expected_revision":n.editor.state["revision"],
        "edits":[{"op":"set","object":"Box","property":"Length","value":30}]
    })}).unwrap();
    n.receive("transaction", true);
    let stale = n.editor.state.clone();
    help_escape(&mut n);
    assert_eq!(n.editor.units_ui.selection, Some(3));
    n.click("Apply units");
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, stale);
    assert!(
        n.editor
            .error
            .as_ref()
            .unwrap()
            .contains("Document changed")
    );
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert!(!n.editor.units_ui.active());

    // A foreign caller changes document identity while Help hides the dialog.
    n.click("Units");
    n.click("Document units...");
    n.key(Key::F1, egui::Modifiers::NONE);
    n.editor
        .jobs
        .try_send(Job {
            op: "new".into(),
            parameters: json!({"name":"R2OtherDocument"}),
        })
        .unwrap();
    n.receive("new", true);
    let switched = n.editor.state.clone();
    n.key(Key::Escape, egui::Modifiers::NONE);
    rendered(&mut n);
    assert!(!n.editor.units_ui.active());
    assert_eq!(n.editor.state, switched);
    assert!(!n.editor.busy);

    n.fixture("R2OwnersActive");
    n.save("active-baseline.FCStd");
    n.editor.edit_native_properties("Box");
    let seed = n.object("Box")["property_metadata"]["Length"]["quantity"]["input"]
        .as_str()
        .unwrap()
        .to_owned();
    n.text(&seed, "31 mm");
    n.editor.preview();
    n.receive("preview", true);
    let preview = n.editor.state.clone();
    let token = n.editor.draft.as_ref().unwrap().token.clone();
    help_escape(&mut n);
    assert_eq!(n.editor.draft.as_ref().unwrap().token, token);
    assert_eq!(n.editor.state, preview);
    n.key(Key::Escape, egui::Modifiers::NONE);
    n.receive("preview_cancel", true);
    assert!(n.editor.draft.is_none());
    assert_eq!(n.object("Box")["properties"]["Length"], 25.4);

    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    let session = n.editor.sketch_ui.session.clone();
    let before = n.editor.state.clone();
    help_escape(&mut n);
    assert_eq!(n.editor.sketch_ui.session, session);
    assert_eq!(n.editor.state, before);
    n.send("sketch_finish", json!({"token":session.unwrap()}));
}
