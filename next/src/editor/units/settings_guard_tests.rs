use super::*;

#[test]
#[ignore = "review-only populated document faults, actual Editor/native"]
fn review_r1_settings_failure_restores_populated_model_and_redo() {
    for name in [
        "ReviewSettingPrepare",
        "ReviewSettingPublish",
        "ReviewSettingRestore",
    ] {
        let mut n = Native::case(name);
        n.fixture(name);
        n.send("transaction",json!({"edits":[{"op":"expression","object":"Box","property":"Length","expression":"1 in"}]}));
        n.send(
            "transaction",
            json!({"edits":[{"op":"set","object":"Box","property":"Width","value":50.8}]}),
        );
        n.send("undo", json!({}));
        n.save("before-failure.FCStd");
        let baseline = n.editor.state.clone();
        let path = n.editor.path.clone();
        assert!(!n.editor.dirty);
        n.editor.send("document_settings", json!({"unit_system":3}));
        n.receive("document_settings", false);
        let after = n.editor.state.clone();
        assert_eq!(after["units"], baseline["units"]);
        assert_eq!(after["revision"], baseline["revision"]);
        for key in ["undo_count", "redo_count", "preview", "sketch_edit"] {
            assert_eq!(after[key], baseline[key]);
        }
        assert_eq!(path, n.editor.path);
        assert!(!n.editor.dirty);
        for old in baseline["objects"].as_array().unwrap() {
            let new = objects(&after)
                .iter()
                .find(|o| o["name"] == old["name"])
                .unwrap();
            for key in [
                "name",
                "type",
                "label",
                "properties",
                "expressions",
                "shape",
                "sketch",
                "body_state",
            ] {
                assert_eq!(new[key], old[key], "{name} {key}");
            }
        }
        assert_eq!(after["available"], name != "ReviewSettingRestore");
        let error = n.editor.error.clone();
        fs::write(n.output.join("observation.json"),serde_json::to_vec_pretty(&json!({"before":baseline,"after":after,"path_preserved":path==n.editor.path,"dirty":n.editor.dirty,"error":error,"actual_model_history_preserved":true})).unwrap()).unwrap();
        n.key(Key::Escape, egui::Modifiers::NONE);
        if name != "ReviewSettingRestore" {
            n.send("redo", json!({}));
            assert_eq!(n.object("Box")["properties"]["Width"], 50.8);
            assert_eq!(n.object("Box")["expressions"]["Length"], "1 in");
            assert_eq!(n.editor.state["units"]["index"], baseline["units"]["index"]);
        } else {
            n.editor.send("document_settings", json!({"unit_system":0}));
            n.receive("document_settings", false);
            assert!(
                n.editor
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("close and reopen")
            );
        }
    }
}
