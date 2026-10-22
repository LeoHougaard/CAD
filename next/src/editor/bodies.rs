//! Body focus is UI state. Ownership, tips and Boolean links come from the document.
use super::{Draft, Editor, objects};
use crate::body_commands::body_of;
use eframe::egui;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct BodyUi {
    pub(super) active: Option<String>,
    pub(super) selected: Vec<String>,
    pending: Option<String>,
}

impl BodyUi {
    fn activate_tree_body(&mut self, state: &Value, name: &str) {
        if objects(state)
            .iter()
            .any(|object| object["name"] == name && object["type"] == "PartDesign::Body")
        {
            self.active = Some(name.to_owned());
        }
    }
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

fn reference_object(object: &Value) -> bool {
    matches!(
        object["type"].as_str().unwrap_or_default(),
        "App::Origin"
            | "App::Line"
            | "App::Plane"
            | "App::CoordinateSystem"
            | "PartDesign::Plane"
            | "PartDesign::Line"
            | "PartDesign::Point"
            | "PartDesign::CoordinateSystem"
            | "PartDesign::ShapeBinder"
            | "PartDesign::SubShapeBinder"
    )
}
fn select_tree_name(selected: &mut Vec<String>, name: &str, additive: bool) {
    if additive {
        if let Some(index) = selected.iter().position(|selected| selected == name) {
            selected.remove(index);
        } else {
            selected.push(name.to_owned());
        }
    } else {
        *selected = vec![name.to_owned()];
    }
}
fn deletion_edits(state: &Value, names: &[String]) -> Vec<Value> {
    // Dependents usually follow their inputs in document order. Native dependency
    // checks still own rejection and the complete operation remains atomic.
    objects(state)
        .iter()
        .rev()
        .filter_map(|object| {
            let name = object["name"].as_str()?;
            names
                .iter()
                .any(|selected| selected == name)
                .then(|| json!({"op":"remove_object","object":name}))
        })
        .collect()
}
fn tree_visibility_edits(state: &Value, name: &str, show: bool) -> Vec<Value> {
    let mut targets = vec![name.to_owned()];
    if show {
        if let Some(body) = body_of(state, name)
            && body != name
        {
            targets.push(body);
        }
        if let Some(object) = objects(state).iter().find(|object| object["name"] == name)
            && object["type"] == "PartDesign::Body"
            && let Some(tip) = object["body_state"]["tip"].as_str()
        {
            targets.push(tip.to_owned());
        }
    }
    targets.sort();
    targets.dedup();
    targets
        .into_iter()
        .filter_map(|name| {
            let current = objects(state)
                .iter()
                .find(|object| object["name"] == name)?["properties"]["Visibility"]
                .as_bool()?;
            (current != show)
                .then(|| json!({"op":"set","object":name,"property":"Visibility","value":show}))
        })
        .collect()
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
    pub(super) fn tree_selections(&self) -> &[String] {
        &self.body_ui.selected
    }
    pub(super) fn sync_bodies(&mut self, op: &str, success: bool) {
        if success && matches!(op, "new" | "open") {
            self.body_ui = BodyUi::default();
        }
        if op == "transaction"
            && let Some(pending) = self.body_ui.pending.take()
            && success
        {
            self.body_ui.selected = vec![pending.clone()];
            self.selected_object = Some(pending.clone());
            self.body_ui.active = Some(pending);
        }
        self.body_ui.selected.retain(|name| {
            objects(&self.state)
                .iter()
                .any(|object| object["name"] == *name)
        });
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
        let selected_objects: Vec<&str> = if self.viewport.selections.is_empty() {
            self.body_ui.selected.iter().map(String::as_str).collect()
        } else {
            self.viewport
                .selections
                .iter()
                .map(|selection| selection.object.as_str())
                .collect()
        };
        for object in selected_objects {
            if let Some(body) = body_of(&self.state, object)
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
    fn tree_selectable(&self) -> bool {
        !self.busy
            && self.sketch_creation.is_none()
            && self.sketch_ui.session.is_none()
            && self.sketch_ui.drag.is_none()
            && self.sketch_ui.dimension.is_none()
            && self.pending_file.is_none()
            && self.file_dialog.is_none()
            && !self.overwrite
    }
    fn tree_mutable(&self) -> bool {
        self.tree_selectable() && self.draft.is_none()
    }
    fn select_tree_object(&mut self, name: &str, additive: bool) {
        select_tree_name(&mut self.body_ui.selected, name, additive);
        self.selected_object = self.tree_selections().last().cloned();
        self.viewport.clear_selection();
        if self.draft.is_none() {
            self.sketch = None;
            self.viewport.set_sketch(None);
            self.sketch_ui.clear_points();
            self.body_ui.activate_tree_body(&self.state, name);
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
                            self.feature_row(ui, body);
                            let rows = super::history::body_rows(&self.state, name);
                            ui.indent(name, |ui| self.tree_rows(ui, &rows, name));
                        }
                        let unowned: Vec<Value> = objects(&self.state)
                            .iter()
                            .filter(|object| {
                                object["type"] != "PartDesign::Body"
                                    && body_of(
                                        &self.state,
                                        object["name"].as_str().unwrap_or_default(),
                                    )
                                    .is_none()
                            })
                            .cloned()
                            .collect();
                        self.tree_rows(ui, &unowned, "document");
                        if objects(&self.state).is_empty() {
                            ui.weak("No features");
                        }
                    });
            });
    }
    fn tree_rows(&mut self, ui: &mut egui::Ui, rows: &[Value], group: &str) {
        // Native Group members are history rows, including datum/binder objects.
        // Only additional owned references belong in the collapsed section.
        let members: std::collections::BTreeSet<&str> = objects(&self.state)
            .iter()
            .find(|object| object["name"] == group && object["type"] == "PartDesign::Body")
            .and_then(|body| body["properties"]["Group"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|member| member["link"].as_str())
            .collect();
        let collapsed: Vec<bool> = rows
            .iter()
            .map(|object| {
                reference_object(object)
                    && !members.contains(object["name"].as_str().unwrap_or_default())
            })
            .collect();
        for (object, collapsed) in rows.iter().zip(&collapsed) {
            if !collapsed {
                self.feature_row(ui, object);
            }
        }
        let references: Vec<_> = rows
            .iter()
            .zip(collapsed)
            .filter_map(|(object, collapsed)| collapsed.then_some(object))
            .collect();
        if !references.is_empty() {
            egui::CollapsingHeader::new("References")
                .id_salt(("references", group))
                .default_open(false)
                .show(ui, |ui| {
                    for object in references {
                        self.feature_row(ui, object);
                    }
                });
        }
    }
    fn feature_row(&mut self, ui: &mut egui::Ui, object: &Value) {
        let name = object["name"].as_str().unwrap_or_default();
        if name.is_empty() {
            return;
        }
        let is_body = object["type"] == "PartDesign::Body";
        let is_sketch = object["type"] == "Sketcher::SketchObject";
        let consumed = object["body_state"]["consumed_by"]
            .as_array()
            .is_some_and(|consumers| !consumers.is_empty());
        let text = if consumed {
            format!("{} | tool", label(object))
        } else {
            label(object).to_owned()
        };
        let tip = !is_body
            && body_of(&self.state, name).is_some_and(|body| {
                objects(&self.state)
                    .iter()
                    .any(|o| o["name"] == body && o["body_state"]["tip"] == name)
            });
        let text = if tip { format!("{text} | tip") } else { text };
        let text = if is_body {
            egui::RichText::new(text).strong()
        } else {
            egui::RichText::new(text)
        };
        let selected = self
            .tree_selections()
            .iter()
            .any(|selected| selected == name)
            || self.selected_object.as_deref() == Some(name);
        let response = ui.selectable_label(selected, text);
        if response.clicked() && self.tree_selectable() {
            let additive = ui.input(|input| {
                input.modifiers.ctrl || input.modifiers.shift || input.modifiers.command
            });
            self.select_tree_object(name, additive);
        }
        if response.secondary_clicked()
            && self.tree_selectable()
            && !self
                .body_ui
                .selected
                .iter()
                .any(|selected| selected == name)
        {
            self.select_tree_object(name, false);
        }
        if response.double_clicked() && self.tree_mutable() {
            if is_sketch {
                self.request_sketch(name);
            } else if object["type"] == "PartDesign::Boolean" {
                self.start_boolean(Some(name));
            } else {
                self.edit_feature(name);
            }
        }
        let mutable = self.tree_mutable();
        response.context_menu(|ui| {
            ui.add_enabled_ui(mutable, |ui| {
                if ui.button("Properties").clicked() {
                    self.edit_native_properties(name);
                    ui.close();
                }
                if is_sketch && ui.button("Edit sketch").clicked() {
                    self.request_sketch(name);
                    ui.close();
                }
                if object["type"] == "PartDesign::Boolean" && ui.button("Edit Boolean").clicked() {
                    self.start_boolean(Some(name));
                    ui.close();
                }
                if is_body && ui.button("Roll back to start").clicked() {
                    self.set_body_tip(name, None);
                    ui.close();
                }
                if object["history_flags"]["can_tip"] == true && ui.button("Set tip here").clicked()
                {
                    if let Some(body) = body_of(&self.state, name) {
                        self.set_body_tip(&body, Some(name));
                    }
                    ui.close();
                }
                if object["history_flags"]["can_reorder"] == true
                    && ui.button("Reorder...").clicked()
                {
                    self.start_reorder(name);
                    ui.close();
                }
                if object["properties"]["Visibility"].is_boolean() {
                    let show = object["properties"]["Visibility"] == false;
                    if ui.button(if show { "Show" } else { "Hide" }).clicked() {
                        let edits = tree_visibility_edits(&self.state, name, show);
                        if !edits.is_empty() {
                            self.transaction(
                                if show {
                                    "Show native object"
                                } else {
                                    "Hide native object"
                                },
                                edits,
                            );
                        }
                        ui.close();
                    }
                }
                if ui.button("Delete").clicked() {
                    let names = if self
                        .body_ui
                        .selected
                        .iter()
                        .any(|selected| selected == name)
                    {
                        self.body_ui.selected.clone()
                    } else {
                        vec![name.to_owned()]
                    };
                    let edits = deletion_edits(&self.state, &names);
                    if !edits.is_empty() {
                        self.transaction("Delete native objects", edits);
                    }
                    ui.close();
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreign_profile_selection_keeps_target_until_a_body_is_explicitly_selected() {
        let state = json!({"objects":[
            {"name":"Target", "type":"PartDesign::Body"},
            {"name":"Source", "type":"PartDesign::Body"},
            {"name":"ForeignSketch", "type":"Sketcher::SketchObject", "body":"Source"}
        ]});
        let mut model = BodyUi {
            active: Some("Target".into()),
            ..BodyUi::default()
        };
        select_tree_name(&mut model.selected, "ForeignSketch", false);
        model.activate_tree_body(&state, "ForeignSketch");
        assert_eq!(model.active.as_deref(), Some("Target"));
        select_tree_name(&mut model.selected, "Source", true);
        model.activate_tree_body(&state, "Source");
        assert_eq!(model.active.as_deref(), Some("Source"));
        assert_eq!(model.selected, ["ForeignSketch", "Source"]);
    }

    #[test]
    fn native_tree_selection_preserves_click_order_and_toggles_independent_objects() {
        let mut selected = vec![];
        select_tree_name(&mut selected, "SketchA", false);
        select_tree_name(&mut selected, "SketchB", true);
        select_tree_name(&mut selected, "DatumPlane", true);
        assert_eq!(selected, ["SketchA", "SketchB", "DatumPlane"]);
        select_tree_name(&mut selected, "SketchA", true);
        assert_eq!(selected, ["SketchB", "DatumPlane"]);
        select_tree_name(&mut selected, "Pad", false);
        assert_eq!(selected, ["Pad"]);
    }
    #[test]
    fn references_are_grouped_without_excluding_non_solid_native_features() {
        let native = [
            json!({"type":"App::Origin"}),
            json!({"type":"App::Line"}),
            json!({"type":"App::Plane"}),
            json!({"type":"PartDesign::Plane"}),
            json!({"type":"PartDesign::SubShapeBinder"}),
            json!({"type":"Sketcher::SketchObject","shape":{"solids":0}}),
            json!({"type":"PartDesign::AdditiveLoft","shape":{"solids":0}}),
            json!({"type":"Part::Feature","shape":{"solids":0}}),
        ];
        let (references, features): (Vec<_>, Vec<_>) =
            native.iter().partition(|object| reference_object(object));
        assert_eq!(references.len(), 5);
        assert_eq!(features.len(), 3);
        assert_eq!(references.len() + features.len(), native.len());
    }
    #[test]
    fn native_tree_delete_is_atomic_edit_batch_in_reverse_document_order() {
        let state = json!({"objects":[{"name":"Body"},{"name":"Sketch"},{"name":"Pad"}]});
        assert_eq!(
            deletion_edits(&state, &["Sketch".into(), "Pad".into(), "Sketch".into()]),
            vec![
                json!({"op":"remove_object","object":"Pad"}),
                json!({"op":"remove_object","object":"Sketch"})
            ]
        );
    }
    #[test]
    fn native_reference_visibility_and_body_tip_visibility_remain_separate() {
        let state = json!({"objects":[
            {"name":"Body","type":"PartDesign::Body","body":"Body","body_state":{"tip":"Pad"},"properties":{"Visibility":false}},
            {"name":"Datum","type":"PartDesign::Plane","body":"Body","properties":{"Visibility":true}},
            {"name":"OldPad","body":"Body","properties":{"Visibility":false}},
            {"name":"Pad","body":"Body","properties":{"Visibility":false}}]});
        assert_eq!(
            tree_visibility_edits(&state, "Datum", false),
            vec![json!({"op":"set","object":"Datum","property":"Visibility","value":false})]
        );
        assert_eq!(
            tree_visibility_edits(&state, "Body", true),
            vec![
                json!({"op":"set","object":"Body","property":"Visibility","value":true}),
                json!({"op":"set","object":"Pad","property":"Visibility","value":true})
            ]
        );
        assert_eq!(state["objects"][0]["properties"]["Visibility"], false);
    }
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
