//! Body history uses native insertion and Tip commands, never a second graph.
use super::{Draft, Editor, objects};
use crate::{body_commands::body_of, history_commands};
use eframe::egui;
use serde_json::{Value, json};
use std::time::Instant;

fn group_rows(state: &Value, body: &str) -> Vec<Value> {
    let all = objects(state);
    let mut rows = Vec::new();
    if let Some(owner) = all.iter().find(|o| o["name"] == body) {
        for member in owner["properties"]["Group"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(object) = all.iter().find(|o| o["name"] == member["link"]) {
                rows.push(object.clone());
            }
        }
    }
    rows
}

/// Preserve native Body.Group order, including sketches and reference objects.
pub(super) fn body_rows(state: &Value, body: &str) -> Vec<Value> {
    let mut rows = group_rows(state, body);
    for object in objects(state) {
        let name = object["name"].as_str().unwrap_or_default();
        if name != body
            && body_of(state, name).as_deref() == Some(body)
            && !rows.iter().any(|row| row["name"] == name)
        {
            rows.push(object.clone());
        }
    }
    rows
}

impl Editor {
    pub(super) fn set_body_tip(&mut self, body: &str, tip: Option<&str>) {
        if self.busy || self.draft.is_some() || self.sketch_ui.session.is_some() {
            return;
        }
        if objects(&self.state).iter().any(|owner| {
            owner["name"] == body
                && owner["body_state"].is_object()
                && owner["body_state"]["tip"] == json!(tip)
        }) {
            return;
        }
        match history_commands::build(
            &self.state,
            &json!({"op":"body_tip","object":body,"tip":tip}),
        ) {
            Ok(edits) => self.transaction("Set Body tip", edits),
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn start_reorder(&mut self, object: &str) {
        if self.busy || self.draft.is_some() || self.sketch_ui.session.is_some() {
            return;
        }
        let Some(body) = body_of(&self.state, object) else {
            return;
        };
        let rows = group_rows(&self.state, &body);
        let Some(owner) = objects(&self.state).iter().find(|o| o["name"] == body) else {
            return;
        };
        let target = rows
            .iter()
            .find(|o| o["name"] != object)
            .map(|o| o["name"].clone())
            .unwrap_or(Value::Null);
        let request = json!({"op":"body_reorder","object":object,"target":target,"after":true,
            "tip":owner["body_state"]["tip"]});
        if let Err(error) = history_commands::build(&self.state, &request) {
            self.error = Some(error);
            return;
        }
        self.sequence += 1;
        self.draft = Some(Draft {
            id: "native.history".into(),
            name: "Reorder feature".into(),
            token: format!("history-{}-{}", std::process::id(), self.sequence),
            parameters: json!({"state":self.state,"request":request}),
            changed: Some(Instant::now()),
            valid: false,
            all_parameters: true,
            // Native history changes are Apply-only. Opening or canceling the
            // form must not open a preview transaction or branch native redo.
            deferred: true,
            committing: false,
        });
        self.sketch = None;
        self.viewport.set_sketch(None);
        self.transform_ui = super::transforms::TransformUi::default();
    }

    pub(super) fn history_fields(&mut self, ui: &mut egui::Ui) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        let baseline = &draft.parameters["state"];
        let object = draft.parameters["request"]["object"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let body = body_of(baseline, &object).unwrap_or_default();
        let rows = group_rows(baseline, &body);
        let original_tip = objects(baseline)
            .iter()
            .find(|o| o["name"] == body)
            .map(|o| o["body_state"]["tip"].clone())
            .unwrap_or(Value::Null);
        let can_tip = objects(baseline)
            .iter()
            .find(|o| o["name"] == object)
            .is_some_and(|o| o["history_flags"]["can_tip"] == true);
        let request = &mut draft.parameters["request"];
        let before = request.clone();
        ui.add_enabled_ui(!self.busy && !draft.committing, |ui| {
            ui.label(&object);
            let mut after = request["after"] == true;
            ui.horizontal(|ui| {
                ui.radio_value(&mut after, false, "Before");
                ui.radio_value(&mut after, true, "After");
            });
            request["after"] = json!(after);
            let selected = request["target"].as_str().unwrap_or(if after {
                "Start of Body"
            } else {
                "End of Body"
            });
            egui::ComboBox::from_id_salt("history_target")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut request["target"],
                        Value::Null,
                        if after {
                            "Start of Body"
                        } else {
                            "End of Body"
                        },
                    );
                    for row in &rows {
                        if row["name"] != object {
                            ui.selectable_value(
                                &mut request["target"],
                                row["name"].clone(),
                                row["label"].as_str().unwrap_or_default(),
                            );
                        }
                    }
                });
            if can_tip && original_tip != object {
                let mut set_tip = request["tip"] == object;
                if ui
                    .checkbox(&mut set_tip, "Set tip to moved feature")
                    .changed()
                {
                    request["tip"] = if set_tip { json!(object) } else { original_tip };
                }
            }
        });
        if *request != before {
            draft.changed = Some(Instant::now());
            draft.valid = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_order_follows_native_group_after_reorder_and_includes_unlisted_owned_references() {
        let state = json!({"objects":[
            {"name":"Body","type":"PartDesign::Body","body":"Body","properties":{"Group":[{"link":"B"},{"link":"Sketch"},{"link":"A"}]}},
            {"name":"A","body":"Body"},{"name":"Sketch","body":"Body"},
            {"name":"B","body":"Body"},{"name":"Plane","body":"Body"},
            {"name":"Other","body":"OtherBody"}
        ]});
        assert_eq!(
            body_rows(&state, "Body")
                .iter()
                .map(|o| o["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["B", "Sketch", "A", "Plane"]
        );
        assert_eq!(group_rows(&state, "Body").len(), 3);
    }
}
