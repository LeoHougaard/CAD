//! Actual Editor Open correction and continued external assembly editing.
use super::*;

#[test]
#[ignore = "Requires owned deep/shared/cycle FCStd fixtures and pinned runtime"]
fn partial_source_open_correction_history_reopen() {
    let mut n = Native::new();
    let input = PathBuf::from(std::env::var_os("CAD_PARTIAL_INPUT").unwrap());
    let scene = input.join("check4-deep/Saved.FCStd");
    let forged = input.join("Shortened.FCStd");
    let side = input.join("deep/Side.FCStd");
    let side_bytes = fs::read(&side).unwrap();
    n.send("new", json!({"name":"KeepDirty"}));
    let keep_path = n.output.join("KeepDirty.FCStd");
    n.editor
        .browser_picked(file_browser::Purpose::Save, keep_path.clone());
    n.receive(true);
    n.send("transaction", json!({"label":"Keep content","edits":[{"op":"add_object","type":"Part::Box","name":"KeepContent"}]}));
    n.send("transaction", json!({"label":"Keep redo","edits":[{"op":"add_object","type":"Part::Box","name":"KeepRedo"}]}));
    n.send("undo", json!({}));
    let keep = n.editor.state.clone();
    assert_eq!(keep["redo_count"], 1);
    for missing in [false, true] {
        if missing {
            fs::remove_file(&side).unwrap();
        }
        for path in if missing {
            vec![scene.clone(), forged.clone()]
        } else {
            vec![forged.clone()]
        } {
            n.editor.browser_picked(file_browser::Purpose::Open, path);
            n.receive(false);
            assert_eq!(n.editor.state, keep);
            assert!(n.editor.dirty);
            assert_eq!(n.editor.path, keep_path.to_string_lossy());
            assert!(n.editor.error.as_ref().unwrap().contains(if missing {
                "source_missing"
            } else {
                "source_identity"
            }));
        }
        if missing {
            fs::write(&side, &side_bytes).unwrap();
        }
    }
    assert_eq!(fs::read(&side).unwrap(), side_bytes);
    // The same browser callback retries after correcting the missing source.
    // The actor replaces/closes the prior root only after successful Open.
    n.editor
        .browser_picked(file_browser::Purpose::Open, scene.clone());
    n.receive(true);
    assert!(!n.editor.dirty);
    let source = n.obj("Instance")["instance_state"]["external_source"].clone();
    assert_eq!(source["dependencies"].as_array().unwrap().len(), 3);
    assert!(source.get("dependency_count").is_none());
    assert_eq!(
        n.obj("Instance")["instance_state"]["source_status"],
        "current"
    );
    assert!((n.obj("Instance")["shape"]["volume"].as_f64().unwrap() - 1000.).abs() < 2e-7);
    let saved_content = n.obj("DirtyContent").clone();
    n.send("transaction", json!({"label":"Continue editing","edits":[{"op":"set","object":"DirtyContent","property":"Length","value":37}]}));
    let edited = n.obj("DirtyContent").clone();
    n.send("undo", json!({}));
    assert_eq!(
        n.obj("DirtyContent")["properties"],
        saved_content["properties"]
    );
    n.send("redo", json!({}));
    assert_eq!(n.obj("DirtyContent")["properties"], edited["properties"]);
    let saved = n.output.join("Continued.FCStd");
    n.editor
        .browser_picked(file_browser::Purpose::Save, saved.clone());
    n.receive(true);
    assert!(!n.editor.dirty);
    n.send("close", json!({}));
    n.editor.browser_picked(file_browser::Purpose::Open, saved);
    n.receive(true);
    assert_eq!(
        n.obj("Instance")["instance_state"]["external_source"],
        source
    );
    assert_eq!(n.obj("DirtyContent")["properties"], edited["properties"]);
    n.editor.browser_picked(
        file_browser::Purpose::Open,
        input.join("check4-cycle/Saved.FCStd"),
    );
    n.receive(true);
    assert_eq!(
        n.obj("Instance")["instance_state"]["external_source"]["dependencies"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        n.obj("Instance")["instance_state"]["source_status"],
        "current"
    );
    n.send("close", json!({}));
    fs::write(n.output.join("result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"actual_editor":true,"failed_open_preserves_dirty_root_and_redo":true,"same_callback_source_correction":true,"deep_cycle_reopen":true,"continued_edit_undo_redo_save_reopen":true,"physical_gui":false})).unwrap()).unwrap();
}
