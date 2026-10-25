//! Focused native assignment and actual Editor input checks, never a GUI window.
use super::*;

fn select(h: &mut NativeEditor, field: &str, picks: &[(&str, &str)]) -> Form {
    select_button(h, field, picks, "Use selection")
}

fn select_button(h: &mut NativeEditor, field: &str, picks: &[(&str, &str)], button: &str) -> Form {
    h.editor.property_ui.filter = field.into();
    h.editor.viewport.selected = None;
    h.editor.viewport.selections = picks
        .iter()
        .map(|(object, subelement)| crate::viewport::Selection {
            object: (*object).into(),
            subelement: (*subelement).into(),
        })
        .collect();
    let mut form = Form::new(false);
    form.frame(&mut h.editor, vec![]);
    form.frame(&mut h.editor, vec![]);
    form.click(&mut h.editor, button);
    form
}

fn buttons(h: &mut NativeEditor, button: &str) -> Form {
    let mut form = Form::new(true);
    for _ in 0..12 {
        form.frame(&mut h.editor, vec![]);
    }
    form.click(&mut h.editor, button);
    form
}

fn profile_and_termination_guards(h: &mut NativeEditor) {
    let original = object(&h.editor.state, "ProfilePad").clone();
    let source = object(&h.editor.state, "ProfileA")["sketch"].clone();
    h.editor.edit_native_properties("ProfilePad");
    h.properties(json!({"Length":6}), json!({}));
    h.preview(true);
    let accepted = h.editor.state.clone();
    let params = h.params().clone();
    let count = h.captures.len();
    for picks in [
        vec![("ProfileA", ""), ("Gone", "")],
        vec![("Gone", "")],
        vec![("ProfilePad", "Face1"), ("Gone", "")],
        vec![("ProfileA", ""), ("ProfileB", "")],
    ] {
        let form = select(h, "Profile", &picks);
        assert!(form.last.iter().any(|t| t.text.contains("retry")));
        assert_eq!(h.params(), &params);
        assert_eq!(h.editor.state, accepted);
        assert_eq!(h.captures.len(), count);
        assert!(
            h.editor.draft.as_ref().unwrap().valid
                && h.editor.draft.as_ref().unwrap().changed.is_none()
        );
        assert!(h.raw.try_recv().is_err());
    }
    select(h, "Profile", &[("ProfilePad", "Face1"), ("ProfileA", "")]);
    h.preview(true);
    assert_eq!(
        object(&h.editor.state, "ProfilePad")["properties"]["Profile"]["link"],
        "ProfileA"
    );
    assert_eq!(object(&h.editor.state, "ProfileA")["sketch"], source);
    buttons(h, "Cancel");
    h.receive("preview_cancel", true);
    assert_eq!(
        object(&h.editor.state, "ProfilePad")["properties"],
        original["properties"]
    );

    h.editor.edit_native_properties("ProfilePad");
    h.properties(json!({"Length":6}), json!({}));
    select(h, "Profile", &[("ProfileA", "")]);
    h.preview(true);
    buttons(h, "Apply");
    h.receive("preview_commit", true);
    assert!(h.editor.draft.is_none());
    let committed = object(&h.editor.state, "ProfilePad").clone();
    h.send("undo", json!({}));
    assert_eq!(
        object(&h.editor.state, "ProfilePad")["properties"]["Profile"]["link"],
        "OriginalProfile"
    );
    h.send("redo", json!({}));
    assert_eq!(
        object(&h.editor.state, "ProfilePad")["properties"],
        committed["properties"]
    );

    for field in ["UpToFace", "UpToShape"] {
        h.editor.edit_native_properties("ProfilePad");
        let link = json!({"link":"PlaneOne","subelements":if field=="UpToFace" {vec!["Face1"]} else {vec![]}});
        h.properties(
            json!({"Type":field,field:if field=="UpToShape" {json!([link])} else {link}}),
            json!({}),
        );
        h.preview(true);
        let accepted = h.editor.state.clone();
        let params = h.params().clone();
        let count = h.captures.len();
        let picks = if field == "UpToFace" {
            vec![("PlaneTwo", "Face1"), ("Gone", "Face2")]
        } else {
            vec![("PlaneTwo", ""), ("Gone", "")]
        };
        let form = select_button(h, field, &picks, "Use native selection");
        assert!(
            form.last
                .iter()
                .any(|t| t.text.contains("Gone") && t.text.contains("stale"))
        );
        assert_eq!(h.params(), &params);
        assert_eq!(h.editor.state, accepted);
        assert_eq!(h.captures.len(), count);
        assert!(
            h.editor.draft.as_ref().unwrap().valid
                && h.editor.draft.as_ref().unwrap().changed.is_none()
        );
        assert!(h.raw.try_recv().is_err());
        let sub = if field == "UpToFace" { "Face1" } else { "" };
        select_button(
            h,
            field,
            &[("ProfilePad", "Face1"), ("PlaneTwo", sub)],
            "Use native selection",
        );
        h.preview(true);
        let target = &object(&h.editor.state, "ProfilePad")["properties"][field];
        assert_eq!(
            if field == "UpToShape" {
                &target[0]["link"]
            } else {
                &target["link"]
            },
            "PlaneTwo"
        );
        buttons(h, "Cancel");
        h.receive("preview_cancel", true);
        assert_eq!(
            object(&h.editor.state, "ProfilePad")["properties"],
            committed["properties"]
        );
    }
    assert_eq!(object(&h.editor.state, "ProfileA")["sketch"], source);
    write_json("profile-and-termination-guard-state.json", &h.editor.state);
}

#[test]
#[ignore = "requires pinned native runtime and generated reference fixture"]
fn native_reference_selection_preserves_candidates_and_native_cardinality() {
    let fixture = input_file("CAD_REFERENCE_SELECTION_FIXTURE");
    let saved = acceptance_dir().join("References.FCStd");
    {
        let mut h = NativeEditor::new("References");
        h.send("open", json!({"path":fixture}));
        profile_and_termination_guards(&mut h);
        let baseline = h.editor.state.clone();
        let holder = object(&baseline, "Holder").clone();
        let cases = [
            (
                "single-object",
                "SingleObject",
                vec![("A", "Face2"), ("A", "Face1"), ("A", "Face2")],
                json!({"link":"A"}),
            ),
            (
                "single-sub-faces",
                "SingleSub",
                vec![("A", "Face2"), ("A", "Face1"), ("A", "Face2")],
                json!({"link":"A","subelements":["Face2","Face1"]}),
            ),
            (
                "single-sub-whole",
                "SingleSub",
                vec![("A", "")],
                json!({"link":"A","subelements":[]}),
            ),
            (
                "single-sub-whole-first",
                "SingleSub",
                vec![("A", ""), ("A", "Face2")],
                json!({"link":"A","subelements":["","Face2"]}),
            ),
            (
                "single-sub-whole-last",
                "SingleSub",
                vec![("A", "Face2"), ("A", "")],
                json!({"link":"A","subelements":["Face2",""]}),
            ),
            (
                "object-list",
                "ObjectList",
                vec![("A", "Face2"), ("B", "Face1"), ("A", "")],
                json!([{"link":"A"},{"link":"B"}]),
            ),
            (
                "sub-list-faces",
                "SubList",
                vec![("A", "Face2"), ("A", "Face1"), ("B", "Face3")],
                json!([{"link":"A","subelements":["Face2","Face1"]},{"link":"B","subelements":["Face3"]}]),
            ),
            (
                "sub-list-whole",
                "SubList",
                vec![("A", ""), ("B", "")],
                json!([{"link":"A","subelements":[""]},{"link":"B","subelements":[""]}]),
            ),
            (
                "sub-list-mixed",
                "SubList",
                vec![
                    ("A", ""),
                    ("A", "Face2"),
                    ("B", "Face1"),
                    ("A", "Face2"),
                    ("B", ""),
                ],
                json!([{"link":"A","subelements":["","Face2"]},{"link":"B","subelements":["Face1",""]}]),
            ),
        ];
        let mut boundaries = vec![];
        for (case, field, picks, expected) in cases {
            h.editor.edit_native_properties("Holder");
            let form = select(&mut h, field, &picks);
            assert!(h.editor.draft.as_ref().unwrap().changed.is_some());
            h.preview(true);
            let actual = object(&h.editor.state, "Holder")["properties"][field].clone();
            assert_eq!(actual, expected, "native {case}");
            boundaries.push(json!({"case":case,"field":field,"picks":picks,
                "type":object(&h.editor.state,"Holder")["property_types"][field],"actual":actual}));
            form.save(&format!("{case}-form.json"));
            buttons(&mut h, "Cancel");
            h.receive("preview_cancel", true);
            assert!(h.editor.draft.is_none());
            assert_eq!(
                object(&h.editor.state, "Holder")["properties"],
                holder["properties"]
            );
        }
        write_json("reference-boundaries.json", &json!(boundaries));

        // Invalid input cannot alter an open task, dispatch a request, change
        // its accepted candidate or make the old field disappear.
        for field in ["SingleObject", "SingleSub", "ObjectList", "SubList"] {
            h.editor.edit_native_properties("Holder");
            h.properties(json!({"Label":"Accepted reference candidate"}), json!({}));
            h.preview(true);
            let candidate = h.editor.state.clone();
            let params = h.params().clone();
            let count = h.captures.len();
            let selections = if field.starts_with("Single") {
                vec![
                    vec![("A", "Face1"), ("B", "Face2")],
                    vec![("Gone", "Face1")],
                ]
            } else {
                vec![vec![("A", "Face1"), ("Gone", "Face2")]]
            };
            for picks in selections {
                let form = select(&mut h, field, &picks);
                assert!(
                    form.last.iter().any(|t| t.text.contains("retry")),
                    "no correction message"
                );
                assert_eq!(h.params(), &params);
                assert_eq!(h.editor.state, candidate);
                assert!(h.editor.draft.as_ref().unwrap().valid);
                assert!(h.editor.draft.as_ref().unwrap().changed.is_none());
                assert_eq!(h.captures.len(), count);
                assert!(h.raw.try_recv().is_err());
            }
            let corrected = select(
                &mut h,
                field,
                &[("A", "Face2"), ("A", "Face1"), ("A", "Face2")],
            );
            assert!(h.editor.draft.as_ref().unwrap().changed.is_some());
            h.preview(true);
            corrected.save(&format!("{field}-corrected-form.json"));
            buttons(&mut h, "Cancel");
            h.receive("preview_cancel", true);
            assert_eq!(
                object(&h.editor.state, "Holder")["properties"],
                holder["properties"]
            );
        }

        // A real consuming native feature, rather than a reference property
        // alone, decides whether an existing object's stale edge can be used.
        h.editor.edit_native_properties("SolidBlend");
        select(
            &mut h,
            "Base",
            &[
                ("SolidBase", "Edge1"),
                ("SolidBase", "Edge2"),
                ("SolidBase", "Edge1"),
            ],
        );
        h.preview(true);
        let good = object(&h.editor.state, "SolidBlend").clone();
        assert_eq!(
            good["properties"]["Base"]["subelements"],
            json!(["Edge1", "Edge2"])
        );
        select(&mut h, "Base", &[("SolidBase", "Edge999")]);
        h.preview(false);
        assert_eq!(
            object(&h.editor.state, "SolidBlend")["properties"],
            good["properties"]
        );
        assert_eq!(
            object(&h.editor.state, "SolidBlend")["shape"],
            good["shape"]
        );
        assert_eq!(
            h.params()["properties"]["Base"]["subelements"],
            json!(["Edge999"])
        );
        assert!(h.editor.draft.is_some() && !h.editor.draft.as_ref().unwrap().valid);
        select(
            &mut h,
            "Base",
            &[("SolidBase", "Edge1"), ("SolidBase", "Edge2")],
        );
        h.preview(true);
        buttons(&mut h, "Apply");
        h.receive("preview_commit", true);
        assert!(h.editor.draft.is_none());
        let committed = object(&h.editor.state, "SolidBlend").clone();
        h.send("undo", json!({}));
        assert_eq!(
            object(&h.editor.state, "SolidBlend")["properties"]["Base"]["subelements"],
            json!(["Edge1"])
        );
        let undone = h.editor.state.clone();
        assert!(undone["redo_count"].as_u64().unwrap() > 0);
        h.editor.edit_native_properties("SolidBlend");
        assert!(h.editor.draft.as_ref().unwrap().deferred);
        select(
            &mut h,
            "Base",
            &[("SolidBase", "Edge1"), ("SolidBase", "Edge2")],
        );
        h.editor.preview();
        assert!(h.editor.draft.as_ref().unwrap().valid && !h.editor.busy);
        assert_eq!(h.editor.state, undone);
        let deferred = h.params().clone();
        select(&mut h, "Base", &[("SolidBase", "Edge1"), ("A", "Edge1")]);
        assert_eq!(h.params(), &deferred);
        assert!(h.editor.draft.as_ref().unwrap().valid);
        assert_eq!(h.editor.state, undone);
        assert!(h.raw.try_recv().is_err());
        buttons(&mut h, "Cancel");
        assert!(h.editor.draft.is_none() && !h.editor.busy);
        assert_eq!(h.editor.state, undone);
        assert!(h.raw.try_recv().is_err());
        h.send("redo", json!({}));
        assert_eq!(
            object(&h.editor.state, "SolidBlend")["properties"],
            committed["properties"]
        );
        h.send("save", json!({"path":saved}));
        write_json("reference-committed-state.json", &h.editor.state);
    }
    {
        let mut reopened = NativeEditor::new("ReferencesReopened");
        reopened.send("open", json!({"path":saved}));
        assert_eq!(
            object(&reopened.editor.state, "SolidBlend")["properties"]["Base"]["subelements"],
            json!(["Edge1", "Edge2"])
        );
        reopened.editor.edit_native_properties("SolidBlend");
        select(&mut reopened, "Base", &[("SolidBase", "Edge1")]);
        reopened.preview(true);
        buttons(&mut reopened, "Cancel");
        reopened.receive("preview_cancel", true);
        assert_eq!(
            object(&reopened.editor.state, "SolidBlend")["properties"]["Base"]["subelements"],
            json!(["Edge1", "Edge2"])
        );
        write_json("reference-reopened-state.json", &reopened.editor.state);
    }
}
