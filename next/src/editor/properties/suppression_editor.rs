//! Actual feature-tree context menu, native actor, and suppression history.
use super::*;

fn seed(h: &mut NativeEditor) {
    h.send("new", json!({"name":"SuppressionEditor"}));
    let mut edits = vec![json!({"op":"add_object","type":"PartDesign::Body","name":"Body"})];
    for (name, size, position) in [
        ("Base", [10., 10., 10.], [0., 0., 0.]),
        ("A", [2., 2., 4.], [1., 1., 10.]),
        ("B", [3., 3., 3.], [6., 6., 10.]),
    ] {
        edits.push(
            json!({"op":"add_object","type":"PartDesign::AdditiveBox","name":name,"body":"Body"}),
        );
        for (property, value) in ["Length", "Width", "Height"].iter().zip(size) {
            edits.push(json!({"op":"set","object":name,"property":property,"value":value}));
        }
        edits.push(json!({"op":"set","object":name,"property":"Placement","value":{"position":position,"rotation":[0,0,0,1]}}));
    }
    h.editor.transaction("Native suppression baseline", edits);
    h.receive("transaction", true);
}

fn open_menu(form: &mut Form, h: &mut NativeEditor, name: &str) {
    for _ in 0..6 {
        form.frame(&mut h.editor, vec![]);
    }
    let pos = form.find(name);
    form.frame(
        &mut h.editor,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    form.frame(
        &mut h.editor,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    form.frame(&mut h.editor, vec![]);
}

fn volume(h: &NativeEditor, expected: f64) {
    let actual = object(&h.editor.state, "Body")["shape"]["volume"]
        .as_f64()
        .unwrap();
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
#[ignore = "requires CAD_EDITOR_ACCEPTANCE_RUNTIME/OUTPUT; native Editor menu, not physical desktop"]
fn native_tree_suppression_history_and_reopen() {
    let mut h = NativeEditor::new("suppression-tree");
    seed(&mut h);
    let baseline = h.editor.state.clone();
    let before = baseline["undo_count"].as_u64().unwrap();
    let placement = object(&baseline, "A")["properties"]["Placement"].clone();
    let mut form = Form::new(true);
    open_menu(&mut form, &mut h, "A");
    assert!(form.last.iter().any(|text| text.text == "Suppress"));
    form.click(&mut h.editor, "Suppress");
    assert!(h.editor.busy, "actual menu must queue native transaction");
    h.receive("transaction", true);
    volume(&h, 1027.);
    assert_eq!(h.editor.state["undo_count"], before + 1);
    assert_eq!(
        object(&h.editor.state, "A")["properties"]["_CadSuppressionPlacement"],
        placement
    );
    h.send("undo", json!({}));
    volume(&h, 1043.);
    assert!(
        object(&h.editor.state, "A")["properties"]
            .get("_CadSuppressionPlacement")
            .is_none()
    );
    h.send("redo", json!({}));
    volume(&h, 1027.);
    let path = acceptance_dir().join("tree-suppressed.FCStd");
    h.editor.file_input = path.to_string_lossy().into_owned();
    h.send("save", json!({"path":path}));
    assert_eq!(h.editor.path, path.to_string_lossy());
    assert!(!h.editor.dirty);
    // Actual application file switching closes its old document, not an API
    // close reply whose state is intentionally absent from the actor pipeline.
    h.send("new", json!({"name":"IntermediateSavedDocument"}));
    h.send("open", json!({"path":path}));
    open_menu(&mut form, &mut h, "A");
    assert!(form.last.iter().any(|text| text.text == "Unsuppress"));
    form.click(&mut h.editor, "Unsuppress");
    h.receive("transaction", true);
    volume(&h, 1043.);
    assert_eq!(
        object(&h.editor.state, "A")["properties"]["Placement"],
        placement
    );
    h.send("undo", json!({}));
    volume(&h, 1027.);
    h.send("redo", json!({}));
    volume(&h, 1043.);
    form.save("suppression-tree-frames.json");
}

#[test]
#[ignore = "requires CAD_EDITOR_ACCEPTANCE_RUNTIME/OUTPUT; native Editor availability control"]
fn native_tree_suppression_expression_is_disabled() {
    let mut h = NativeEditor::new("suppression-expression-menu");
    seed(&mut h);
    h.editor.transaction("Native suppression expression",vec![json!({"op":"expression","object":"A","property":"Suppressed","expression":"Base.Height > 9 mm"})]);
    h.receive("transaction", true);
    volume(&h, 1027.);
    assert_eq!(
        object(&h.editor.state, "A")["suppression"]["can_toggle"],
        false
    );
    let before = h.editor.state.clone();
    let mut form = Form::new(true);
    open_menu(&mut form, &mut h, "A");
    form.click(&mut h.editor, "Unsuppress");
    assert!(!h.editor.busy);
    assert!(h.raw.try_recv().is_err());
    assert_eq!(h.editor.state, before);
    assert!(h.editor.error.is_none());
    form.save("suppression-expression-disabled-frames.json");
}

#[test]
#[ignore = "requires CAD_EDITOR_ACCEPTANCE_RUNTIME/OUTPUT and CAD_SUPPRESSION_LEGACY"]
fn native_tree_suppression_legacy_is_disabled() {
    let mut h = NativeEditor::new("suppression-legacy-menu");
    h.send("open", json!({"path":input_file("CAD_SUPPRESSION_LEGACY")}));
    assert_eq!(
        object(&h.editor.state, "A")["suppression"]["can_toggle"],
        false
    );
    assert!(
        object(&h.editor.state, "A")["suppression"]["reason"]
            .as_str()
            .unwrap()
            .contains("not saved")
    );
    let before = h.editor.state.clone();
    let mut form = Form::new(true);
    open_menu(&mut form, &mut h, "A");
    form.click(&mut h.editor, "Unsuppress");
    assert!(!h.editor.busy && h.raw.try_recv().is_err());
    assert_eq!(h.editor.state, before);
    assert!(h.editor.error.is_none());
    form.save("suppression-legacy-disabled-frames.json");
}

#[test]
#[ignore = "requires CAD_EDITOR_ACCEPTANCE_RUNTIME/OUTPUT; native failure and owner guards"]
fn native_tree_suppression_failure_and_owner_recovery() {
    let mut h = NativeEditor::new("suppression-recovery-menu");
    seed(&mut h);
    h.editor.send("inspect", json!({}));
    assert!(h.editor.busy);
    h.editor.toggle_feature_suppression("A");
    h.receive("inspect", true);
    assert!(h.raw.try_recv().is_err());
    volume(&h, 1043.);
    h.editor.edit_native_properties("A");
    assert!(h.editor.draft.is_some());
    h.editor.toggle_feature_suppression("A");
    assert!(!h.editor.busy && h.raw.try_recv().is_err());
    h.editor.finish_preview(false);
    assert!(h.editor.draft.is_none());
    let mut form = Form::new(true);
    open_menu(&mut form, &mut h, "B | tip");
    form.click(&mut h.editor, "Suppress");
    h.receive("transaction", true);
    volume(&h, 1016.);
    open_menu(&mut form, &mut h, "A");
    form.click(&mut h.editor, "Suppress");
    h.receive("transaction", true);
    volume(&h, 1000.);
    let before = h.editor.state.clone();
    open_menu(&mut form, &mut h, "Base");
    form.click(&mut h.editor, "Suppress");
    h.receive("transaction", false);
    assert!(h.editor.error.as_ref().unwrap().contains("invalid_model"));
    assert_eq!(h.editor.state["available"], true);
    volume(&h, 1000.);
    assert!(h.editor.state["revision"].as_u64().unwrap() > before["revision"].as_u64().unwrap());
    assert_eq!(h.editor.state["undo_count"], before["undo_count"]);
    for name in ["Base", "A", "B"] {
        for field in ["properties", "expressions", "suppression"] {
            assert_eq!(
                object(&h.editor.state, name)[field],
                object(&before, name)[field]
            );
        }
    }
    open_menu(&mut form, &mut h, "A");
    form.click(&mut h.editor, "Unsuppress");
    h.receive("transaction", true);
    assert!(h.editor.error.is_none());
    volume(&h, 1016.);
    form.save("suppression-native-failure-recovery-frames.json");
}
