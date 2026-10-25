//! Worker-wide gesture ownership and mutation admission. Native replay stays native.
use serde_json::{Value, json};
use std::collections::HashMap;

pub(crate) const CHANNEL: &str = "cad-next-leases";
type Failure = (&'static str, &'static str);
const SKETCH_ACTIVE: Failure = (
    "sketch_active",
    "Finish or cancel sketch editing before another document write",
);
const PREVIEW_ACTIVE: Failure = (
    "preview_active",
    "Finish or cancel the active preview before another document write",
);
const UNAVAILABLE: Failure = (
    "document_unavailable",
    "Rollback failed; close and reopen this document",
);

#[derive(Default)]
struct Lease {
    sketch: Option<Value>,
    preview: Option<Value>,
    // Some(None) means a first, unpublished candidate is being prepared.
    pending: Option<Option<Value>>,
    poisoned: bool,
}

#[derive(Default)]
pub(crate) struct Leases {
    documents: HashMap<String, Lease>,
}

fn token(request: &Value, sketch: bool) -> Result<&str, Failure> {
    request["token"]
        .as_str()
        .filter(|s| !s.is_empty() && s.chars().count() <= 256)
        .ok_or((
            "invalid_request",
            if sketch {
                "Sketch token requires a nonempty string of at most 256 characters"
            } else {
                "Preview token requires a nonempty string of at most 256 characters"
            },
        ))
}

fn writable(op: &str) -> bool {
    matches!(
        op,
        "new"
            | "open"
            | "transaction"
            | "undo"
            | "redo"
            | "save"
            | "close"
            | "preview"
            | "preview_commit"
            | "preview_cancel"
            | "sketch_begin"
            | "sketch_finish"
            | "sketch_cancel"
            | "export_exchange"
    )
}

impl Leases {
    pub(crate) fn respond(&mut self, frame: &Value) -> Value {
        let result = self.apply(frame);
        self.documents.retain(|_, s| {
            s.sketch.is_some() || s.preview.is_some() || s.pending.is_some() || s.poisoned
        });
        match result {
            Ok(result) => json!({"ok":true,"result":result}),
            Err((code, message)) => json!({"ok":false,"error":{"code":code,"message":message}}),
        }
    }

    fn apply(&mut self, frame: &Value) -> Result<Value, Failure> {
        let action = frame["action"]
            .as_str()
            .ok_or(("lease_protocol", "Missing lease action"))?;
        let request = &frame["request"];
        let op = request["op"].as_str().unwrap_or("");
        if action == "global" {
            if writable(op) {
                let any_sketch = self.documents.values().any(|s| s.sketch.is_some());
                let any_preview = self.documents.values().any(|s| s.preview.is_some());
                // Python dictionary membership used to reject unhashable identifiers
                // only when a lease was present. Preserve that public error exactly.
                if (any_sketch || any_preview) && !matches!(op, "new" | "open") {
                    match &request["document"] {
                        Value::Array(_) => return Err(("native_error", "unhashable type: 'list'")),
                        Value::Object(_) => {
                            return Err(("native_error", "unhashable type: 'dict'"));
                        }
                        _ => (),
                    }
                }
                let state = request["document"]
                    .as_str()
                    .and_then(|d| self.documents.get(d));
                if any_sketch
                    && (matches!(op, "new" | "open") || state.is_none_or(|s| s.sketch.is_none()))
                {
                    return Err(SKETCH_ACTIVE);
                }
                if any_preview
                    && (matches!(op, "new" | "open") || state.is_none_or(|s| s.preview.is_none()))
                {
                    return Err(PREVIEW_ACTIVE);
                }
            }
            return Ok(Value::Null);
        }
        let document = frame["document"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or(("lease_protocol", "Lease document must be a nonempty string"))?;
        let state = self.documents.entry(document.into()).or_default();
        match action {
            "metadata" => Ok(
                json!({"preview":state.preview,"sketch_edit":state.sketch,"available":!state.poisoned}),
            ),
            "pre_revision" | "read_guard" => {
                if state.poisoned && (action == "read_guard" || op != "close") {
                    return Err(UNAVAILABLE);
                }
                Ok(Value::Null)
            }
            "local" => {
                if state.poisoned && op == "close" {
                    return Ok(Value::Null);
                }
                if state.preview.is_some()
                    && !op.starts_with("preview")
                    && !(state.sketch.is_some() && op == "sketch_cancel")
                {
                    return Err((
                        "preview_active",
                        "Finish or cancel the active preview before another write",
                    ));
                }
                if let Some(sketch) = &state.sketch {
                    if !matches!(
                        op,
                        "transaction"
                            | "undo"
                            | "redo"
                            | "preview"
                            | "preview_commit"
                            | "preview_cancel"
                            | "sketch_finish"
                            | "sketch_cancel"
                    ) {
                        return Err(SKETCH_ACTIVE);
                    }
                    let key = if matches!(op, "sketch_finish" | "sketch_cancel") {
                        "token"
                    } else {
                        "sketch_token"
                    };
                    if request[key] != sketch["token"] {
                        return Err(("sketch_owner", "Sketch editing belongs to another session"));
                    }
                }
                Ok(Value::Null)
            }
            "sketch_validate" => {
                token(request, true)?;
                if op == "sketch_begin" {
                    if state.sketch.is_some() {
                        return Err((
                            "sketch_active",
                            "Document already has an active sketch editing session",
                        ));
                    }
                } else if state.sketch.is_none() {
                    return Err((
                        "no_sketch_edit",
                        "Document has no active sketch editing session",
                    ));
                }
                Ok(Value::Null)
            }
            "sketch_begin" => {
                let token = token(request, true)?;
                if state.sketch.is_some() || state.preview.is_some() || state.pending.is_some() {
                    return Err(("lease_state", "Sketch begin while a gesture is owned"));
                }
                let object = frame["object"]
                    .as_str()
                    .ok_or(("lease_protocol", "Missing native sketch name"))?;
                state.sketch = Some(json!({"token":token,"object":object}));
                Ok(Value::Null)
            }
            "sketch_end" => {
                if state.sketch.is_none() || state.pending.is_some() {
                    return Err(("lease_state", "No settled sketch owner to release"));
                }
                state.sketch = None;
                state.preview = None;
                Ok(Value::Null)
            }
            "sketch_edits_shape" | "sketch_edits" => {
                let edits = frame["edits"]
                    .as_array()
                    .filter(|edits| !edits.is_empty())
                    .ok_or((
                        "invalid_request",
                        "Sketch editing requires a nonempty edits list",
                    ))?;
                if action == "sketch_edits_shape" {
                    return Ok(Value::Null);
                }
                let sketch = state
                    .sketch
                    .as_ref()
                    .ok_or(("lease_state", "Sketch edit without an owner"))?;
                for edit in edits {
                    let denied = (
                        "sketch_active",
                        "Only edits to the active sketch are allowed during sketch editing",
                    );
                    if !edit.is_object() || edit["object"] != sketch["object"] {
                        return Err(denied);
                    }
                    match &edit["op"] {
                        Value::Array(_) => return Err(("native_error", "unhashable type: 'list'")),
                        Value::Object(_) => {
                            return Err(("native_error", "unhashable type: 'dict'"));
                        }
                        _ => (),
                    }
                    if !matches!(
                        edit["op"].as_str().unwrap_or(""),
                        "set"
                            | "expression"
                            | "geometry"
                            | "constraint"
                            | "refraction"
                            | "via_point"
                            | "datum"
                            | "move_geometry"
                            | "construction"
                            | "delete_geometry"
                            | "delete_constraint"
                            | "constraint_state"
                            | "trim_geometry"
                            | "expose_internal"
                            | "external_geometry"
                            | "external_state"
                            | "delete_external"
                            | "fillet_geometry"
                            | "split_geometry"
                            | "extend_geometry"
                            | "carbon_copy"
                            | "spline_degree"
                            | "spline_knot"
                            | "spline_pole"
                            | "spline_weight"
                            | "mirror_geometry"
                    ) {
                        return Err(denied);
                    }
                }
                Ok(Value::Null)
            }
            "preview_validate" => {
                let token = token(request, false)?;
                if state.preview.as_ref().is_some_and(|p| p["token"] != token) {
                    return Err(("preview_owner", "Preview belongs to another gesture"));
                }
                if op != "preview" && state.preview.is_none() {
                    return Err(("no_preview", "Document has no active preview"));
                }
                if state.sketch.is_none()
                    && op == "preview"
                    && request["edits"].as_array().is_some_and(|edits| {
                        edits.iter().any(|edit| {
                            edit.is_object()
                                && matches!(
                                    edit.get("op")
                                        .or_else(|| edit.get("kind"))
                                        .and_then(Value::as_str),
                                    Some("body_tip" | "body_reorder")
                                )
                        })
                    })
                {
                    return Err((
                        "history_busy",
                        "Body history edits require a committed transaction outside previews",
                    ));
                }
                Ok(Value::Null)
            }
            "preview_ready" => {
                let label = request.get("label").cloned().unwrap_or_else(|| {
                    state.preview.as_ref().map_or_else(
                        || {
                            json!(if state.sketch.is_some() {
                                "Edit sketch"
                            } else {
                                "Edit"
                            })
                        },
                        |p| p["label"].clone(),
                    )
                });
                if !label.is_string() {
                    return Err(("invalid_request", "Preview label must be a string"));
                }
                let needs_redo = if state.sketch.is_some() {
                    !frame["opened"]
                        .as_bool()
                        .ok_or(("lease_protocol", "Missing native transaction state"))?
                } else {
                    state.preview.is_none()
                };
                if needs_redo {
                    // Ask for the native getter only after label validation, and only
                    // on paths where the reference implementation observes it.
                    let Some(value) = frame.get("redo") else {
                        return Ok(json!({"label":label,"needs_redo":true}));
                    };
                    let redo = value
                        .as_u64()
                        .ok_or(("lease_protocol", "Missing native redo count"))?;
                    if redo > 0 {
                        return Err((
                            "preview_redo_conflict",
                            if state.sketch.is_some() {
                                "Native redo history is present; commit an edit to branch history"
                            } else {
                                "Native redo history is present; use a committed edit to branch history"
                            },
                        ));
                    }
                }
                Ok(json!({"label":label,"needs_redo":false}))
            }
            "preview_stage" => {
                let token = token(request, false)?;
                if state.pending.is_some() || !frame["label"].is_string() {
                    return Err(("lease_state", "Invalid preview staging"));
                }
                if state.preview.as_ref().is_some_and(|p| p["token"] != token) {
                    return Err(("lease_state", "Staging cannot replace another gesture"));
                }
                state.pending = Some(state.preview.take());
                state.preview = Some(json!({"token":token,"label":frame["label"]}));
                Ok(Value::Null)
            }
            "preview_resolve" => {
                let accepted = frame["accepted"]
                    .as_bool()
                    .ok_or(("lease_protocol", "Missing preview acceptance"))?;
                let old = state
                    .pending
                    .take()
                    .ok_or(("lease_state", "No preview staging to resolve"))?;
                if !accepted {
                    state.preview = old;
                }
                Ok(Value::Null)
            }
            "preview_end" => {
                if state.pending.is_some() {
                    return Err(("lease_state", "Cannot release a pending preview"));
                }
                state.preview = None;
                Ok(Value::Null)
            }
            "history_guard" => {
                if state.sketch.is_some() || state.preview.is_some() {
                    return Err((
                        "history_busy",
                        "Body history edits require a committed transaction outside sketch editing and previews",
                    ));
                }
                Ok(Value::Null)
            }
            "poison" => {
                state.poisoned = true;
                Ok(Value::Null)
            }
            "close" => {
                self.documents.remove(document);
                Ok(Value::Null)
            }
            _ => Err(("lease_protocol", "Unknown lease action")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(s: &mut Leases, action: &str, request: Value, extra: Value) -> Value {
        let mut frame = json!({"action":action,"document":"A","request":request});
        frame
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        s.respond(&frame)
    }
    fn ok(s: &mut Leases, action: &str, req: Value, extra: Value) -> Value {
        let r = call(s, action, req, extra);
        assert_eq!(r["ok"], true, "{r}");
        r["result"].clone()
    }
    fn error(s: &mut Leases, action: &str, req: Value, extra: Value, code: &str) {
        let r = call(s, action, req, extra);
        assert_eq!(r["error"]["code"], code, "{r}");
    }
    fn begin(s: &mut Leases) {
        ok(
            s,
            "sketch_begin",
            json!({"op":"sketch_begin","token":"session"}),
            json!({"object":"Sketch"}),
        );
    }
    fn stage(s: &mut Leases, label: &str) {
        ok(
            s,
            "preview_stage",
            json!({"token":"gesture"}),
            json!({"label":label}),
        );
    }
    #[test]
    fn global_guards_cover_every_write_and_sketch_precedes_nested_preview() {
        let mut s = Leases::default();
        begin(&mut s);
        stage(&mut s, "Drag");
        ok(
            &mut s,
            "preview_resolve",
            json!({}),
            json!({"accepted":true}),
        );
        for op in [
            "new",
            "open",
            "transaction",
            "undo",
            "redo",
            "save",
            "close",
            "preview",
            "preview_commit",
            "preview_cancel",
            "sketch_begin",
            "sketch_finish",
            "sketch_cancel",
            "export_exchange",
        ] {
            for doc in [json!("B"), Value::Null, json!(17)] {
                error(
                    &mut s,
                    "global",
                    json!({"op":op,"document":doc}),
                    json!({}),
                    "sketch_active",
                );
            }
            if !matches!(op, "new" | "open") {
                ok(&mut s, "global", json!({"op":op,"document":"A"}), json!({}));
            }
        }
        for op in ["inspect", "mesh", "topology", "unsupported"] {
            ok(&mut s, "global", json!({"op":op,"document":"B"}), json!({}));
        }
        error(
            &mut s,
            "global",
            json!({"op":"transaction","document":[]}),
            json!({}),
            "native_error",
        );
        ok(&mut s, "sketch_end", json!({}), json!({}));
        stage(&mut s, "Feature");
        ok(
            &mut s,
            "preview_resolve",
            json!({}),
            json!({"accepted":true}),
        );
        error(
            &mut s,
            "global",
            json!({"op":"save","document":"B"}),
            json!({}),
            "preview_active",
        );
    }
    #[test]
    fn local_preview_precedes_session_owner_and_poisoned_close_bypasses_both() {
        let mut s = Leases::default();
        begin(&mut s);
        stage(&mut s, "Drag");
        ok(
            &mut s,
            "preview_resolve",
            json!({}),
            json!({"accepted":true}),
        );
        error(
            &mut s,
            "local",
            json!({"op":"transaction"}),
            json!({}),
            "preview_active",
        );
        error(
            &mut s,
            "local",
            json!({"op":"sketch_finish","token":"session"}),
            json!({}),
            "preview_active",
        );
        error(
            &mut s,
            "local",
            json!({"op":"sketch_cancel","token":"wrong"}),
            json!({}),
            "sketch_owner",
        );
        ok(
            &mut s,
            "local",
            json!({"op":"sketch_cancel","token":"session"}),
            json!({}),
        );
        error(
            &mut s,
            "local",
            json!({"op":"preview","token":"gesture"}),
            json!({}),
            "sketch_owner",
        );
        ok(
            &mut s,
            "local",
            json!({"op":"preview","sketch_token":"session"}),
            json!({}),
        );
        ok(&mut s, "poison", json!({}), json!({}));
        error(
            &mut s,
            "pre_revision",
            json!({"op":"transaction"}),
            json!({}),
            "document_unavailable",
        );
        error(
            &mut s,
            "read_guard",
            json!({}),
            json!({}),
            "document_unavailable",
        );
        ok(&mut s, "pre_revision", json!({"op":"close"}), json!({}));
        ok(&mut s, "local", json!({"op":"close"}), json!({}));
        ok(&mut s, "close", json!({}), json!({}));
        assert!(s.documents.is_empty());
        assert_eq!(
            ok(&mut s, "metadata", json!({}), json!({})),
            json!({"preview":null,"sketch_edit":null,"available":true})
        );
        begin(&mut s);
    }
    #[test]
    fn unpublished_candidates_and_rejected_replacements_restore_exact_metadata() {
        let mut s = Leases::default();
        for _ in 0..3 {
            ok(&mut s, "metadata", json!({}), json!({}));
        }
        assert!(s.documents.is_empty());
        stage(&mut s, "First");
        assert_eq!(
            ok(&mut s, "metadata", json!({}), json!({}))["preview"],
            json!({"token":"gesture","label":"First"})
        );
        ok(
            &mut s,
            "preview_resolve",
            json!({}),
            json!({"accepted":false}),
        );
        assert!(s.documents.is_empty());
        stage(&mut s, "First");
        ok(
            &mut s,
            "preview_resolve",
            json!({}),
            json!({"accepted":true}),
        );
        stage(&mut s, "Replacement");
        error(
            &mut s,
            "preview_stage",
            json!({"token":"gesture"}),
            json!({"label":"Nested"}),
            "lease_state",
        );
        error(
            &mut s,
            "preview_resolve",
            json!({}),
            json!({"accepted":null}),
            "lease_protocol",
        );
        ok(
            &mut s,
            "preview_resolve",
            json!({}),
            json!({"accepted":false}),
        );
        assert_eq!(
            ok(&mut s, "metadata", json!({}), json!({}))["preview"]["label"],
            "First"
        );
        ok(&mut s, "preview_end", json!({}), json!({}));
        error(
            &mut s,
            "preview_validate",
            json!({"op":"preview_commit","token":"gesture"}),
            json!({}),
            "no_preview",
        );
        begin(&mut s);
        stage(&mut s, "Pending");
        error(&mut s, "sketch_end", json!({}), json!({}), "lease_state");
        ok(
            &mut s,
            "preview_resolve",
            json!({}),
            json!({"accepted":false}),
        );
        assert!(ok(&mut s, "metadata", json!({}), json!({}))["sketch_edit"].is_object());
        ok(&mut s, "sketch_end", json!({}), json!({}));
        assert!(s.documents.is_empty());
    }
    #[test]
    fn tokens_labels_and_redo_are_typed_and_native_opened_state_controls_sketch_redo() {
        let mut s = Leases::default();
        for t in [
            Value::Null,
            json!(true),
            json!(1),
            json!(1.0),
            json!([]),
            json!({}),
            json!(""),
            json!("x".repeat(257)),
        ] {
            for action in ["sketch_validate", "preview_validate"] {
                error(
                    &mut s,
                    action,
                    json!({"op":"sketch_begin","token":t}),
                    json!({}),
                    "invalid_request",
                );
            }
        }
        let unicode = "\u{1f980}".repeat(256);
        ok(
            &mut s,
            "preview_validate",
            json!({"op":"preview","token":unicode}),
            json!({}),
        );
        error(
            &mut s,
            "preview_ready",
            json!({"label":null}),
            json!({"redo":1,"opened":false}),
            "invalid_request",
        );
        error(
            &mut s,
            "preview_ready",
            json!({}),
            json!({"redo":1,"opened":false}),
            "preview_redo_conflict",
        );
        assert_eq!(
            ok(
                &mut s,
                "preview_ready",
                json!({}),
                json!({"redo":0,"opened":false})
            ),
            json!({"label":"Edit","needs_redo":false})
        );
        begin(&mut s);
        error(
            &mut s,
            "preview_ready",
            json!({}),
            json!({"redo":1,"opened":false}),
            "preview_redo_conflict",
        );
        assert_eq!(
            ok(
                &mut s,
                "preview_ready",
                json!({}),
                json!({"redo":1,"opened":true})
            ),
            json!({"label":"Edit sketch","needs_redo":false})
        );
        stage(&mut s, "Owned label");
        ok(
            &mut s,
            "preview_resolve",
            json!({}),
            json!({"accepted":true}),
        );
        assert_eq!(
            ok(
                &mut s,
                "preview_ready",
                json!({}),
                json!({"redo":0,"opened":true})
            ),
            json!({"label":"Owned label","needs_redo":false})
        );
        error(
            &mut s,
            "preview_validate",
            json!({"op":"preview","token":"other"}),
            json!({}),
            "preview_owner",
        );
        error(
            &mut s,
            "history_guard",
            json!({}),
            json!({}),
            "history_busy",
        );
    }
    #[test]
    fn sketch_edit_admission_uses_rust_owned_object_and_preserves_malformed_op_errors() {
        let mut s = Leases::default();
        begin(&mut s);
        let before = ok(&mut s, "metadata", json!({}), json!({}));
        for edits in [Value::Null, json!([]), json!({})] {
            error(
                &mut s,
                "sketch_edits_shape",
                json!({}),
                json!({"edits":edits}),
                "invalid_request",
            );
        }
        for edit in [
            Value::Null,
            json!({"object":"Foreign","op":"datum"}),
            json!({"object":"Sketch","op":"add_object"}),
            json!({"object":"Sketch","op":"unknown"}),
        ] {
            error(
                &mut s,
                "sketch_edits",
                json!({}),
                json!({"edits":[edit]}),
                "sketch_active",
            );
        }
        for op in [json!([]), json!({})] {
            error(
                &mut s,
                "sketch_edits",
                json!({}),
                json!({"edits":[{"object":"Sketch","op":op}]}),
                "native_error",
            );
        }
        for op in [
            "set",
            "expression",
            "geometry",
            "constraint",
            "datum",
            "move_geometry",
            "construction",
            "delete_geometry",
            "delete_constraint",
            "constraint_state",
            "trim_geometry",
            "expose_internal",
            "external_geometry",
            "external_state",
            "delete_external",
            "fillet_geometry",
            "split_geometry",
            "extend_geometry",
            "carbon_copy",
            "spline_degree",
            "spline_knot",
            "spline_pole",
            "spline_weight",
            "mirror_geometry",
        ] {
            ok(
                &mut s,
                "sketch_edits",
                json!({}),
                json!({"edits":[{"object":"Sketch","op":op}]}),
            );
        }
        assert_eq!(ok(&mut s, "metadata", json!({}), json!({})), before);
    }
    #[test]
    fn feature_history_preview_preflight_keeps_alias_and_owner_precedence() {
        let mut s = Leases::default();
        for edit in [json!({"op":"body_tip"}), json!({"kind":"body_reorder"})] {
            error(
                &mut s,
                "preview_validate",
                json!({"op":"preview","token":"gesture","edits":[edit]}),
                json!({}),
                "history_busy",
            );
        }
        ok(
            &mut s,
            "preview_validate",
            json!({"op":"preview","token":"gesture","edits":[{"op":null,"kind":"body_tip"}]}),
            json!({}),
        );
        stage(&mut s, "Feature");
        ok(
            &mut s,
            "preview_resolve",
            json!({}),
            json!({"accepted":true}),
        );
        error(
            &mut s,
            "preview_validate",
            json!({"op":"preview","token":"other","edits":[{"op":"body_tip"}]}),
            json!({}),
            "preview_owner",
        );
        ok(&mut s, "preview_end", json!({}), json!({}));
        begin(&mut s);
        ok(
            &mut s,
            "preview_validate",
            json!({"op":"preview","token":"gesture","edits":[{"op":"body_tip"}]}),
            json!({}),
        );
        error(
            &mut s,
            "sketch_edits",
            json!({}),
            json!({"edits":[{"op":"body_tip","object":"Sketch"}]}),
            "sketch_active",
        );
    }
    #[test]
    fn preview_fact_demand_validates_label_before_requesting_only_needed_redo() {
        let mut s = Leases::default();
        error(
            &mut s,
            "preview_ready",
            json!({"label":null}),
            json!({}),
            "invalid_request",
        );
        assert_eq!(
            ok(&mut s, "preview_ready", json!({}), json!({})),
            json!({"label":"Edit","needs_redo":true})
        );
        error(
            &mut s,
            "preview_ready",
            json!({}),
            json!({"redo":null}),
            "lease_protocol",
        );
        assert_eq!(
            ok(&mut s, "preview_ready", json!({}), json!({"redo":0})),
            json!({"label":"Edit","needs_redo":false})
        );
        stage(&mut s, "Feature");
        ok(
            &mut s,
            "preview_resolve",
            json!({}),
            json!({"accepted":true}),
        );
        assert_eq!(
            ok(&mut s, "preview_ready", json!({}), json!({})),
            json!({"label":"Feature","needs_redo":false})
        );
        ok(&mut s, "preview_end", json!({}), json!({}));
        begin(&mut s);
        error(
            &mut s,
            "preview_ready",
            json!({"label":null}),
            json!({}),
            "invalid_request",
        );
        assert_eq!(
            ok(&mut s, "preview_ready", json!({}), json!({"opened":false})),
            json!({"label":"Edit sketch","needs_redo":true})
        );
        assert_eq!(
            ok(&mut s, "preview_ready", json!({}), json!({"opened":true})),
            json!({"label":"Edit sketch","needs_redo":false})
        );
        // An unneeded native count is deliberately unobserved, even if supplied.
        assert_eq!(
            ok(
                &mut s,
                "preview_ready",
                json!({}),
                json!({"opened":true,"redo":null})
            ),
            json!({"label":"Edit sketch","needs_redo":false})
        );
        error(
            &mut s,
            "preview_ready",
            json!({}),
            json!({"opened":false,"redo":1}),
            "preview_redo_conflict",
        );
        assert_eq!(
            ok(&mut s, "metadata", json!({}), json!({}))["preview"],
            Value::Null
        );
    }
}
