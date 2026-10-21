//! Body focus is UI state. Ownership, tips and Boolean links come from the document.
use super::{Draft, Editor, objects};
use crate::body_commands::body_of;
use eframe::egui;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct BodyUi {
    pub(super) active: Option<String>,
    pending: Option<String>,
}

fn bodies(state: &Value) -> Vec<Value> {
    objects(state)
        .iter()
        .filter(|o| o["type"] == "PartDesign::Body")
        .cloned()
        .collect()
}
fn solid(body: &Value) -> bool {
    body["shape"]["solids"].as_u64().unwrap_or(0) > 0
}
fn available(body: &Value, editing: Option<&str>) -> bool {
    solid(body)
        && body["body_state"]["consumed_by"]
            .as_array()
            .is_none_or(|consumers| consumers.iter().all(|c| c.as_str() == editing))
}
fn label(object: &Value) -> &str {
    object["label"]
        .as_str()
        .or_else(|| object["name"].as_str())
        .unwrap_or_default()
}

fn initial_target(
    state: &Value,
    active: Option<&str>,
    selected: &[String],
    editing: Option<&str>,
) -> Option<String> {
    if let Some(name) = editing {
        return body_of(state, name);
    }
    let all = bodies(state);
    let valid = |name: &str| all.iter().any(|b| b["name"] == name && available(b, None));
    let selected: Vec<_> = selected.iter().filter(|name| valid(name)).collect();
    if selected.len() >= 2 {
        return active
            .filter(|active| selected.iter().any(|name| name.as_str() == *active))
            .map(str::to_owned)
            .or_else(|| selected.first().map(|s| (*s).clone()));
    }
    active
        .filter(|name| valid(name))
        .map(str::to_owned)
        .or_else(|| {
            all.iter()
                .find(|b| available(b, None))
                .and_then(|b| b["name"].as_str())
                .map(str::to_owned)
        })
}

impl Editor {
    pub(super) fn sync_bodies(&mut self, op: &str, success: bool) {
        if success && matches!(op, "new" | "open") {
            self.body_ui = BodyUi::default();
        }
        if op == "transaction"
            && let Some(pending) = self.body_ui.pending.take()
            && success
        {
            self.body_ui.active = Some(pending);
        }
        if !self
            .body_ui
            .active
            .as_ref()
            .is_some_and(|name| bodies(&self.state).iter().any(|b| b["name"] == *name))
        {
            self.body_ui.active = bodies(&self.state)
                .first()
                .and_then(|b| b["name"].as_str())
                .map(str::to_owned);
        }
    }
    fn new_body(&mut self) {
        let name = self.unique("Body");
        self.body_ui.pending = Some(name.clone());
        self.viewport.clear_selection();
        self.selected_object = None;
        self.transaction(
            "Create body",
            vec![json!({"op":"add_object","type":"PartDesign::Body","name":name})],
        );
    }
    pub(super) fn body_toolbar(&mut self, ui: &mut egui::Ui) {
        let all = bodies(&self.state);
        if !all.is_empty() {
            let text = all
                .iter()
                .find(|b| {
                    Some(b["name"].as_str().unwrap_or_default()) == self.body_ui.active.as_deref()
                })
                .map(label)
                .unwrap_or("Body");
            egui::ComboBox::from_id_salt("active_body")
                .selected_text(text)
                .width(85.)
                .show_ui(ui, |ui| {
                    for body in &all {
                        let name = body["name"].as_str().unwrap_or_default();
                        if ui
                            .selectable_label(
                                self.body_ui.active.as_deref() == Some(name),
                                label(body),
                            )
                            .clicked()
                        {
                            self.body_ui.active = Some(name.into());
                        }
                    }
                })
                .response
                .on_hover_text("Body receiving new sketches and features");
        }
        if ui.button("New body").clicked() {
            self.new_body();
        }
        if ui
            .add_enabled(
                all.iter().filter(|b| available(b, None)).count() >= 2,
                egui::Button::new("Boolean"),
            )
            .on_disabled_hover_text("Create two solid bodies first")
            .clicked()
        {
            self.start_boolean(None);
        }
    }
    pub(super) fn start_boolean(&mut self, editing: Option<&str>) {
        let all = bodies(&self.state);
        let existing = editing
            .and_then(|name| objects(&self.state).iter().find(|o| o["name"] == name))
            .cloned();
        let mut selected = Vec::new();
        for selection in &self.viewport.selections {
            if let Some(body) = body_of(&self.state, &selection.object)
                && all
                    .iter()
                    .any(|b| b["name"] == body && available(b, editing))
                && !selected.contains(&body)
            {
                selected.push(body);
            }
        }
        let target = initial_target(
            &self.state,
            self.body_ui.active.as_deref(),
            &selected,
            editing,
        );
        let Some(target) = target else {
            self.error = Some("Choose a solid target body".into());
            return;
        };
        let mut tools = existing
            .as_ref()
            .and_then(|o| o["properties"]["Group"].as_array())
            .map(|links| {
                links
                    .iter()
                    .filter_map(|l| l["link"].as_str())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if existing.is_none() {
            tools = selected
                .into_iter()
                .filter(|name| *name != target)
                .collect();
            if tools.is_empty()
                && let Some(tool) = all
                    .iter()
                    .find(|b| b["name"] != target && available(b, None))
                    .and_then(|b| b["name"].as_str())
            {
                tools.push(tool.into());
            }
        }
        if tools.is_empty() {
            self.error = Some("Choose another solid body as the tool".into());
            return;
        }
        let operation = existing
            .as_ref()
            .and_then(|o| o["properties"]["Type"].as_str())
            .unwrap_or("Fuse")
            .to_owned();
        let name = editing
            .map(str::to_owned)
            .unwrap_or_else(|| self.unique("Boolean"));
        let parameters = json!({"name":name,"target":target,"tools":tools,"operation":operation});
        self.sequence += 1;
        self.draft = Some(Draft {
            id: if editing.is_some() {
                "native.boolean"
            } else {
                "body.boolean"
            }
            .into(),
            name: if editing.is_some() {
                format!("Edit {name}")
            } else {
                "Boolean".into()
            },
            token: format!("boolean-{}-{}", std::process::id(), self.sequence),
            parameters,
            changed: editing
                .is_none()
                .then(|| Instant::now() - Duration::from_secs(1)),
            valid: false,
            all_parameters: false,
            deferred: self.state["redo_count"].as_u64().unwrap_or(0) > 0,
            committing: false,
        });
        self.body_ui.active = Some(target);
        self.viewport.clear_selection();
        self.fit_next = true;
    }
    pub(super) fn boolean_fields(&mut self, ui: &mut egui::Ui) {
        let all = bodies(&self.state);
        let Some(draft) = &mut self.draft else { return };
        let edit = draft.id == "native.boolean";
        let own_name = draft.parameters["name"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let mut target = draft.parameters["target"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let mut tools = draft.parameters["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut operation = draft.parameters["operation"]
            .as_str()
            .unwrap_or("Fuse")
            .to_owned();
        let before = (target.clone(), tools.clone(), operation.clone());
        ui.add_enabled_ui(!self.busy, |ui| {
            ui.horizontal(|ui| {
                ui.label("Operation");
                egui::ComboBox::from_id_salt("boolean_operation")
                    .selected_text(match operation.as_str() {
                        "Cut" => "Subtract",
                        "Common" => "Intersect",
                        _ => "Union",
                    })
                    .show_ui(ui, |ui| {
                        for (native, text) in [
                            ("Fuse", "Union"),
                            ("Cut", "Subtract"),
                            ("Common", "Intersect"),
                        ] {
                            ui.selectable_value(&mut operation, native.into(), text);
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Target");
                ui.add_enabled_ui(!edit, |ui| {
                    let text = all
                        .iter()
                        .find(|b| b["name"] == target)
                        .map(label)
                        .unwrap_or(&target);
                    egui::ComboBox::from_id_salt("boolean_target")
                        .selected_text(text)
                        .show_ui(ui, |ui| {
                            for body in &all {
                                if !available(body, Some(&own_name)) {
                                    continue;
                                }
                                ui.selectable_value(
                                    &mut target,
                                    body["name"].as_str().unwrap_or_default().into(),
                                    label(body),
                                );
                            }
                        });
                });
            });
            tools.retain(|name| *name != target);
            ui.menu_button(format!("Tools ({})", tools.len()), |ui| {
                for body in &all {
                    let name = body["name"].as_str().unwrap_or_default();
                    if name == target || !available(body, Some(&own_name)) {
                        continue;
                    }
                    let mut selected = tools.iter().any(|tool| tool == name);
                    if ui.checkbox(&mut selected, label(body)).changed() {
                        if selected {
                            tools.push(name.into());
                        } else {
                            tools.retain(|tool| tool != name);
                        }
                    }
                }
            });
            if ui
                .add_enabled(
                    !edit && tools.len() == 1,
                    egui::Button::new("Swap target and tool").small(),
                )
                .clicked()
            {
                std::mem::swap(&mut target, &mut tools[0]);
            }
        });
        if before != (target.clone(), tools.clone(), operation.clone()) {
            draft.parameters["target"] = json!(target);
            draft.parameters["tools"] = json!(tools);
            draft.parameters["operation"] = json!(operation);
            draft.changed = Some(Instant::now());
            draft.valid = false;
        }
    }
    pub(super) fn body_tree(&mut self, ctx: &egui::Context) {
        let all = bodies(&self.state);
        egui::Window::new("Features")
            .id(egui::Id::new("features"))
            .default_pos([10., 82.])
            .default_width(145.)
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(450.)
                    .show(ui, |ui| {
                        for body in &all {
                            let name = body["name"].as_str().unwrap_or_default();
                            let consumed = body["body_state"]["consumed_by"]
                                .as_array()
                                .is_some_and(|c| !c.is_empty());
                            let text =
                                format!("{}{}", label(body), if consumed { " | tool" } else { "" });
                            if ui
                                .selectable_label(
                                    self.body_ui.active.as_deref() == Some(name),
                                    egui::RichText::new(text).strong(),
                                )
                                .clicked()
                                && !self.busy
                                && self.draft.is_none()
                                && self.sketch_ui.session.is_none()
                            {
                                self.body_ui.active = Some(name.into());
                                self.selected_object = Some(name.into());
                                self.viewport.clear_selection();
                            }
                            let rows: Vec<Value> = objects(&self.state)
                                .iter()
                                .filter(|o| {
                                    o["name"] != name
                                        && body_of(
                                            &self.state,
                                            o["name"].as_str().unwrap_or_default(),
                                        )
                                        .as_deref()
                                            == Some(name)
                                })
                                .cloned()
                                .collect();
                            ui.indent(name, |ui| {
                                for object in rows {
                                    self.feature_row(ui, &object);
                                }
                            });
                        }
                        let unowned: Vec<Value> = objects(&self.state)
                            .iter()
                            .filter(|o| {
                                o["type"] != "PartDesign::Body"
                                    && body_of(&self.state, o["name"].as_str().unwrap_or_default())
                                        .is_none()
                            })
                            .cloned()
                            .collect();
                        for object in unowned {
                            self.feature_row(ui, &object);
                        }
                        if objects(&self.state).is_empty() {
                            ui.weak("No features");
                        }
                    });
            });
    }
    fn feature_row(&mut self, ui: &mut egui::Ui, object: &Value) {
        let sketch = object["type"] == "Sketcher::SketchObject";
        if !sketch && !solid(object) && object["type"] != "PartDesign::Boolean" {
            return;
        }
        let name = object["name"].as_str().unwrap_or_default();
        let response =
            ui.selectable_label(self.selected_object.as_deref() == Some(name), label(object));
        if response.clicked()
            && !self.busy
            && self.draft.is_none()
            && (self.sketch_ui.session.is_none() || self.sketch.as_deref() == Some(name))
        {
            self.selected_object = Some(name.into());
            if let Some(body) = body_of(&self.state, name) {
                self.body_ui.active = Some(body);
            }
            if sketch {
                self.request_sketch(name);
            }
        }
        if !sketch
            && response.double_clicked()
            && !self.busy
            && self.draft.is_none()
            && self.sketch_ui.session.is_none()
        {
            if object["type"] == "PartDesign::Boolean" {
                self.start_boolean(Some(name));
            } else {
                self.edit_feature(name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_pair_overrides_an_unrelated_active_body() {
        let state = json!({"objects":[
            {"name":"A","type":"PartDesign::Body","shape":{"solids":1}},
            {"name":"B","type":"PartDesign::Body","shape":{"solids":1}},
            {"name":"C","type":"PartDesign::Body","shape":{"solids":1}},
            {"name":"EarlierBoolean","body":"C"}
        ]});
        let selection = ["A".into(), "B".into()];
        assert_eq!(
            initial_target(&state, Some("C"), &selection, None).as_deref(),
            Some("A")
        );
        assert_eq!(
            initial_target(&state, Some("B"), &selection, None).as_deref(),
            Some("B")
        );
        assert_eq!(
            initial_target(&state, Some("C"), &["A".into()], None).as_deref(),
            Some("C")
        );
        assert_eq!(
            initial_target(&state, Some("B"), &selection, Some("EarlierBoolean")).as_deref(),
            Some("C")
        );
    }
}
