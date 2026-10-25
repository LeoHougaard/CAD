//! Full native property forms. Only user changes become transaction edits.
use super::{Draft, Editor, objects};
use crate::{feature_commands, property_commands};
use eframe::egui;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct PropertyUi {
    pub(super) schema: Value,
    loading: bool,
    filter: String,
    advanced: bool,
    buffers: BTreeMap<String, String>,
    reference: super::profile_references::ProfileReferenceUi,
}

impl Editor {
    fn native_link_selection(&self) -> Vec<(String, String)> {
        if !self.viewport.selections.is_empty() {
            self.viewport
                .selections
                .iter()
                .map(|s| (s.object.clone(), s.subelement.clone()))
                .collect()
        } else if let Some(selection) = &self.viewport.selected {
            vec![(selection.object.clone(), selection.subelement.clone())]
        } else {
            self.tree_selections()
                .iter()
                .map(|name| (name.clone(), String::new()))
                .collect()
        }
    }
    pub(super) fn sync_native_schema(&mut self) {
        if let Some(draft) = &mut self.draft
            && let Some(object) = objects(&self.state)
                .iter()
                .find(|o| o["name"] == draft.parameters["name"])
        {
            let reference_profile = draft.parameters.get("reference_choice").is_some();
            let inactive_literals =
                crate::hole_commands::inactive_literals(object, &draft.parameters);
            if let Some(changes) = draft.parameters["properties"].as_object_mut() {
                // Native options can lock dependent fields or normalize their
                // values. Replay the accepted state, not stale form inputs.
                changes.retain(|property, value| {
                    if inactive_literals.contains(property) {
                        return true;
                    }
                    if !property_commands::writable(object, property) {
                        return false;
                    }
                    if reference_profile && property == "Profile" {
                        return true;
                    }
                    if let Some(native) = object["properties"].get(property) {
                        *value = native.clone();
                    }
                    true
                });
                self.property_ui.buffers.retain(|property, _| {
                    property.starts_with('@')
                        || changes.contains_key(property)
                            && object["expressions"].get(property).is_none()
                });
            }
            self.property_ui.schema = object.clone();
        }
    }
    pub(super) fn native_feature_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Features", |ui| {
            let mut groups = Vec::new();
            for definition in feature_commands::definitions() {
                if !groups.contains(&definition.group) {
                    groups.push(definition.group);
                }
            }
            for group in groups {
                ui.menu_button(group, |ui| {
                    for definition in feature_commands::definitions()
                        .iter()
                        .filter(|d| d.group == group)
                    {
                        if ui.button(definition.label).clicked() {
                            self.start_native_feature(definition.type_id);
                            ui.close();
                        }
                    }
                });
            }
        });
        if ui
            .add_enabled(
                self.selected_object.is_some() || self.viewport.selected.is_some(),
                egui::Button::new("Properties"),
            )
            .clicked()
            && let Some(name) = self
                .viewport
                .selected
                .as_ref()
                .map(|s| s.object.clone())
                .or(self.selected_object.clone())
        {
            self.edit_native_properties(&name);
        }
    }

    pub(super) fn start_native_feature(&mut self, type_id: &str) {
        let Some(definition) = feature_commands::get(type_id) else {
            self.error = Some(format!("Unknown native feature {type_id}"));
            return;
        };
        let name = self.unique(definition.type_id.split("::").last().unwrap_or("Feature"));
        let selections = self.native_link_selection();
        let selected = self
            .tree_selections()
            .first()
            .cloned()
            .or(self.sketch.clone())
            .or(self.selected_object.clone());
        let mut body = self.body().or_else(|| {
            selections
                .first()
                .and_then(|(n, _)| crate::body_commands::body_of(&self.state, n))
        });
        if !definition.type_id.starts_with("PartDesign::") {
            body = None;
        }
        if definition.type_id.starts_with("PartDesign::")
            && definition.type_id != "PartDesign::Body"
            && body.is_none()
        {
            self.error = Some("Create a Body before adding a Part Design feature".into());
            return;
        }
        let context = feature_commands::FeatureContext {
            body: body.as_deref(),
            name: &name,
            selected_object: selected.as_deref(),
            selections: &selections,
        };
        let mut properties = match feature_commands::defaults(definition, &self.state, &context) {
            Ok(p) => p,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        if definition.preferred_properties.contains(&"Profile")
            && let Some(source) = selected.as_deref().and_then(|name| {
                objects(&self.state).iter().find(|object| {
                    object["name"] == name && object["type"] == "Sketcher::SketchObject"
                })
            })
        {
            properties.insert("Profile".into(), json!({"link":source["name"]}));
        }
        let reference_name = self.unique("Reference");
        self.property_ui = PropertyUi {
            reference: super::profile_references::ProfileReferenceUi::new(
                &self.state,
                reference_name,
            ),
            loading: true,
            ..PropertyUi::default()
        };
        self.draft = Some(Draft {
            id: "native.properties".into(),
            name: definition.label.into(),
            token: format!("feature-{}-{}", std::process::id(), self.sequence),
            parameters: json!({"create":true,"type":type_id,"name":name,"body":body,
                "properties":properties,"expressions":{}}),
            changed: None,
            valid: false,
            all_parameters: true,
            deferred: self.state["redo_count"].as_u64().unwrap_or(0) > 0,
            committing: false,
        });
        if let Some(draft) = &mut self.draft {
            let profile = draft.parameters["properties"]["Profile"].clone();
            self.property_ui
                .reference
                .prepare(&mut draft.parameters, &profile);
        }
        self.sketch = None;
        self.viewport.set_sketch(None);
        self.transform_ui = super::transforms::TransformUi::default();
        if type_id == "PartDesign::MultiTransform"
            && let Some(draft) = &mut self.draft
        {
            self.transform_ui.begin(&mut draft.parameters, &self.state);
        }
        self.send("feature_schema", json!({"type":type_id,"values":self.draft.as_ref().map(|d| d.parameters["properties"].clone()).unwrap_or_default()}));
    }

    pub(super) fn native_schema_reply(&mut self, schema: Value, success: bool) {
        self.property_ui.loading = false;
        if success {
            self.property_ui.schema = schema;
            if let Some(draft) = &mut self.draft {
                draft.changed = Some(Instant::now());
            }
        }
    }

    pub(super) fn edit_native_properties(&mut self, name: &str) {
        let Some(object) = objects(&self.state)
            .iter()
            .find(|o| o["name"] == name)
            .cloned()
        else {
            return;
        };
        self.sequence += 1;
        let body = crate::body_commands::body_of(&self.state, name);
        let tip = body
            .as_deref()
            .and_then(|body| objects(&self.state).iter().find(|o| o["name"] == body))
            .map(|o| o["body_state"]["tip"].clone())
            .unwrap_or(Value::Null);
        let tip_visibility = tip
            .as_str()
            .and_then(|tip| objects(&self.state).iter().find(|o| o["name"] == tip))
            .map(|o| o["properties"]["Visibility"].clone())
            .unwrap_or(Value::Null);
        let reference_name = self.unique("Reference");
        self.property_ui = PropertyUi {
            reference: super::profile_references::ProfileReferenceUi::new(
                &self.state,
                reference_name,
            ),
            schema: object.clone(),
            ..PropertyUi::default()
        };
        self.draft = Some(Draft {
            id: "native.properties".into(),
            name: format!("Edit {}", object["label"].as_str().unwrap_or(name)),
            token: format!("properties-{}-{}", std::process::id(), self.sequence),
            parameters: json!({"create":false,"name":name,"type":object["type"],"body":body,"tip":tip,"tip_visibility":tip_visibility,"properties":{},"expressions":{}}),
            changed: None,
            valid: false,
            all_parameters: true,
            deferred: self.state["redo_count"].as_u64().unwrap_or(0) > 0,
            committing: false,
        });
        self.transform_ui = super::transforms::TransformUi::default();
        if object["type"] == "PartDesign::MultiTransform"
            && let Some(draft) = &mut self.draft
        {
            self.transform_ui.begin(&mut draft.parameters, &self.state);
        }
        self.sketch = None;
        self.viewport.set_sketch(None);
    }

    pub(super) fn native_property_fields(&mut self, ui: &mut egui::Ui) {
        let selected = self.native_link_selection();
        let draft = self.draft.as_mut().unwrap();
        ui.label(&draft.name);
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::from_rgb(225, 142, 138), error);
        }
        if self.property_ui.loading {
            ui.spinner();
            return;
        }
        if self.property_ui.schema.is_null() {
            ui.weak("Native property schema unavailable");
            return;
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.property_ui.filter)
                    .hint_text("Find property")
                    .desired_width(180.),
            );
            ui.checkbox(&mut self.property_ui.advanced, "All properties");
        });
        let task_schema = crate::hole_commands::task_schema(
            &crate::extrusion_commands::task_schema(&self.property_ui.schema, &draft.parameters),
            &draft.parameters,
        );
        let schema = &task_schema;
        let extrusion = crate::extrusion_commands::supported(
            draft.parameters["type"].as_str().unwrap_or_default(),
        );
        let filter = self.property_ui.filter.to_lowercase();
        let definition = draft.parameters["type"]
            .as_str()
            .and_then(feature_commands::get);
        let preferred = definition.map(|d| d.preferred_properties).unwrap_or(&[]);
        let mut fields = schema["property_types"]
            .as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        fields.sort_by_key(|p| {
            (
                preferred.iter().position(|f| *f == p).unwrap_or(usize::MAX),
                p.clone(),
            )
        });
        let native_objects = objects(&self.state);
        let object_name = draft.parameters["name"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let mut changed = false;
        let nested = draft.parameters["transformations"].is_array();
        if nested {
            changed |= self.transform_ui.show(
                ui,
                &mut draft.parameters,
                &self.state,
                &selected,
                self.busy,
            );
        }
        egui::ScrollArea::vertical()
            .id_salt("native_properties")
            .max_height(410.)
            .show(ui, |ui| {
                ui.set_min_width(340.);
                ui.add_enabled_ui(!self.busy, |ui| {
                    for property in fields {
                        if nested && property == "Transformations" {
                            continue;
                        }
                        let metadata = &schema["property_metadata"][&property];
                        let type_id = schema["property_types"][&property]
                            .as_str()
                            .unwrap_or_default();
                        let writable = property_commands::writable(schema, &property);
                        let hidden = metadata["hidden"].as_bool().unwrap_or(false);
                        if !filter.is_empty()
                            && !property.to_lowercase().contains(&filter)
                            && !metadata["group"]
                                .as_str()
                                .unwrap_or_default()
                                .to_lowercase()
                                .contains(&filter)
                        {
                            continue;
                        }
                        if filter.is_empty() && !self.property_ui.advanced && (hidden || !writable)
                        {
                            continue;
                        }
                        let mut value = draft.parameters["properties"]
                            .get(&property)
                            .cloned()
                            .unwrap_or_else(|| schema["properties"][&property].clone());
                        let mut expression = draft.parameters["expressions"]
                            .get(&property)
                            .cloned()
                            .unwrap_or_else(|| schema["expressions"][&property].clone());
                        let mut expression_changed = false;
                        let mut value_changed = false;
                        let edge_base = (type_id == "Part::PropertyFilletEdges").then(|| {
                            draft.parameters["properties"]
                                .get("Base")
                                .unwrap_or(&schema["properties"]["Base"])
                                .clone()
                        });
                        ui.push_id(&property, |ui| {
                            let title = ui.label(&property);
                            title.on_hover_text(format!(
                                "{}\n{}\n{}",
                                metadata["group"].as_str().unwrap_or_default(),
                                type_id,
                                metadata["documentation"].as_str().unwrap_or_default()
                            ));
                            if !writable {
                                ui.weak(display(&value));
                                return;
                            }
                            ui.horizontal(|ui| {
                                ui.add_enabled_ui(!expression.is_string(), |ui| {
                                    if let Some(base) = &edge_base {
                                        let before = value.clone();
                                        fillet_edges_widget(
                                            ui,
                                            &mut value,
                                            &selected,
                                            base,
                                            native_objects,
                                            &metadata["quantity"],
                                            &metadata["quantity_default"],
                                        );
                                        value_changed = value != before;
                                    } else {
                                        let link_objects = if property == "Profile"
                                            || extrusion
                                                && crate::extrusion_commands::reference(&property)
                                        {
                                            self.property_ui.reference.objects()
                                        } else {
                                            native_objects
                                        };
                                        let profile_selection = if property == "Profile"
                                            || extrusion
                                                && crate::extrusion_commands::reference(&property)
                                        {
                                            Some(
                                                self.property_ui
                                                    .reference
                                                    .selections(&object_name, &selected),
                                            )
                                        } else {
                                            None
                                        };
                                        let link_selection = match &profile_selection {
                                            Some(result) => {
                                                result.as_deref().map_err(String::as_str)
                                            }
                                            None => Ok(selected.as_slice()),
                                        };
                                        let target = extrusion
                                            && crate::extrusion_commands::reference(&property);
                                        value_changed = property_widget_with_selection(
                                            ui,
                                            &property,
                                            type_id,
                                            metadata,
                                            &mut value,
                                            &mut self.property_ui.buffers,
                                            link_objects,
                                            &object_name,
                                            if target { Ok(&[]) } else { link_selection },
                                        );
                                        if target {
                                            let selection = link_selection
                                                .map_err(str::to_owned)
                                                .and_then(|picks| {
                                                    crate::extrusion_commands::selection(
                                                        &property, picks,
                                                    )
                                                });
                                            let reason = selection
                                                .as_ref()
                                                .err()
                                                .cloned()
                                                .unwrap_or_default();
                                            if ui
                                                .add_enabled(
                                                    selection.is_ok(),
                                                    egui::Button::new("Use native selection"),
                                                )
                                                .on_disabled_hover_text(&reason)
                                                .clicked()
                                            {
                                                value = selection.unwrap();
                                                value_changed = true;
                                            }
                                            if !selected.is_empty() && !reason.is_empty() {
                                                ui.colored_label(
                                                    egui::Color32::from_rgb(225, 142, 138),
                                                    reason,
                                                );
                                            }
                                        }
                                    }
                                });
                                if expression_capable(type_id) {
                                    let mut active = expression.is_string();
                                    if ui
                                        .checkbox(&mut active, "fx")
                                        .on_hover_text("Native expression")
                                        .changed()
                                    {
                                        expression = if active {
                                            json!(display(&value))
                                        } else {
                                            Value::Null
                                        };
                                        expression_changed = true;
                                    }
                                }
                            });
                            if let Some(expr) = expression.as_str() {
                                let mut text = expr.to_owned();
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut text)
                                            .hint_text("Native expression")
                                            .desired_width(310.),
                                    )
                                    .changed()
                                {
                                    expression = json!(text);
                                    expression_changed = true;
                                }
                            }
                            if property == "Profile"
                                && definition.is_some_and(|definition| {
                                    definition.preferred_properties.contains(&"Profile")
                                })
                            {
                                changed |= self.property_ui.reference.show(
                                    ui,
                                    &mut draft.parameters,
                                    &value,
                                    value_changed,
                                );
                            }
                            ui.separator();
                        });
                        if value_changed {
                            crate::hole_commands::remember_literal(
                                schema,
                                &mut draft.parameters,
                                &property,
                            );
                            draft.parameters["properties"][&property] = value;
                            changed = true;
                        }
                        if expression_changed {
                            draft.parameters["expressions"][&property] = expression;
                            changed = true;
                        }
                    }
                    // Component expression paths are native paths, including Placement.Base.x.
                    egui::CollapsingHeader::new("Expression paths").show(ui, |ui| {
                        let path = self.property_ui.buffers.entry("@path".into()).or_default();
                        ui.add(
                            egui::TextEdit::singleline(path)
                                .hint_text("Property or component path"),
                        );
                        let path = path.clone();
                        let expr = self
                            .property_ui
                            .buffers
                            .entry("@expression".into())
                            .or_default();
                        ui.add(egui::TextEdit::singleline(expr).hint_text("Expression"));
                        let expr = expr.clone();
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    !path.is_empty() && !expr.is_empty(),
                                    egui::Button::new("Set expression"),
                                )
                                .clicked()
                            {
                                draft.parameters["expressions"][&path] = json!(expr);
                                changed = true;
                            }
                            if ui
                                .add_enabled(
                                    !path.is_empty(),
                                    egui::Button::new("Clear expression"),
                                )
                                .clicked()
                            {
                                draft.parameters["expressions"][&path] = Value::Null;
                                changed = true;
                            }
                        });
                        if let Some(expressions) = schema["expressions"].as_object() {
                            for (path, expr) in expressions {
                                ui.weak(format!("{path} = {}", display(expr)));
                            }
                        }
                    });
                });
            });
        if changed {
            draft.changed = Some(Instant::now());
            draft.valid = false;
        }
        if ui
            .add_enabled(!self.busy, egui::Button::new("Recompute preview"))
            .clicked()
        {
            draft.changed = Some(Instant::now() - Duration::from_secs(1));
            draft.valid = false;
        }
    }
}

pub(super) fn display(value: &Value) -> String {
    value["quantity_input"]
        .as_str()
        .or_else(|| value.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
fn expression_capable(type_id: &str) -> bool {
    matches!(
        type_id.strip_prefix("App::Property").unwrap_or_default(),
        "Length"
            | "Distance"
            | "Angle"
            | "Area"
            | "Volume"
            | "Quantity"
            | "QuantityConstraint"
            | "Float"
            | "FloatConstraint"
            | "Integer"
            | "IntegerConstraint"
            | "Percent"
            | "Speed"
            | "Acceleration"
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn property_widget(
    ui: &mut egui::Ui,
    property: &str,
    type_id: &str,
    metadata: &Value,
    value: &mut Value,
    buffers: &mut BTreeMap<String, String>,
    objects: &[Value],
    own: &str,
    selected: &[(String, String)],
) -> bool {
    property_widget_with_selection(
        ui,
        property,
        type_id,
        metadata,
        value,
        buffers,
        objects,
        own,
        Ok(selected),
    )
}

#[allow(clippy::too_many_arguments)]
fn property_widget_with_selection(
    ui: &mut egui::Ui,
    property: &str,
    type_id: &str,
    metadata: &Value,
    value: &mut Value,
    buffers: &mut BTreeMap<String, String>,
    objects: &[Value],
    own: &str,
    selections: Result<&[(String, String)], &str>,
) -> bool {
    let selected = selections.unwrap_or_default();
    let before = value.clone();
    let kind = type_id.strip_prefix("App::Property").unwrap_or_default();
    if metadata["quantity"].get("signature").is_some() {
        super::units::quantity_field(ui, value, &metadata["quantity"]);
    } else if type_id == "Part::PropertyFilletEdges" {
        let base = objects
            .iter()
            .find(|object| object["name"] == own)
            .map(|object| &object["properties"]["Base"])
            .unwrap_or(&Value::Null);
        fillet_edges_widget(
            ui,
            value,
            selected,
            base,
            objects,
            &metadata["quantity"],
            &metadata["quantity_default"],
        );
    } else if kind == "Map" {
        map_widget(ui, value);
    } else if kind == "Color" {
        vector_widget(ui, value, 4);
    } else if kind == "Enumeration" {
        egui::ComboBox::from_id_salt("enum")
            .selected_text(display(value))
            .show_ui(ui, |ui| {
                if let Some(values) = metadata["enumerations"].as_array() {
                    for item in values {
                        ui.selectable_value(value, item.clone(), display(item));
                    }
                }
            });
    } else if kind == "Bool" {
        let mut v = value.as_bool().unwrap_or(false);
        if ui.checkbox(&mut v, "").changed() {
            *value = json!(v);
        }
    } else if kind.starts_with("Link") || kind.starts_with("XLink") {
        let multiple = kind.contains("List");
        let sub = kind.contains("Sub");
        let selection = selections
            .map_err(str::to_owned)
            .and_then(|picks| property_commands::selected_reference(type_id, objects, picks));
        let reason = selection.as_ref().err().cloned().unwrap_or_default();
        if multiple {
            let mut links = value.as_array().cloned().unwrap_or_default();
            let previous = links.clone();
            let mut remove = None;
            let mut reorder = None;
            let count = links.len();
            ui.vertical(|ui| {
                for (index, link) in links.iter_mut().enumerate() {
                    ui.push_id(index, |ui| {
                        ui.horizontal(|ui| {
                            link_widget(ui, link, sub, objects, own);
                            if ui
                                .add_enabled(index > 0, egui::Button::new("Up").small())
                                .clicked()
                            {
                                reorder = Some((index, index - 1));
                            }
                            if ui
                                .add_enabled(index + 1 < count, egui::Button::new("Down").small())
                                .clicked()
                            {
                                reorder = Some((index, index + 1));
                            }
                            if ui.small_button("Remove").clicked() {
                                remove = Some(index);
                            }
                        });
                    });
                }
                if let Some(index) = remove {
                    links.remove(index);
                } else if let Some((from, to)) = reorder {
                    links.swap(from, to);
                }
                ui.horizontal(|ui| {
                    ui.menu_button("Add object", |ui| {
                        for object in objects.iter().filter(|o| o["name"] != own) {
                            if ui
                                .button(object["label"].as_str().unwrap_or_default())
                                .clicked()
                            {
                                let mut link = json!({"link":object["name"]});
                                if sub {
                                    link["subelements"] = json!([]);
                                }
                                links.push(link);
                                ui.close();
                            }
                        }
                    });
                    if ui
                        .add_enabled(selection.is_ok(), egui::Button::new("Use selection"))
                        .on_disabled_hover_text(&reason)
                        .clicked()
                    {
                        links = selection.as_ref().unwrap().as_array().unwrap().clone();
                    }
                });
            });
            if links != previous {
                *value = json!(links);
            }
        } else {
            ui.vertical(|ui| {
                link_widget(ui, value, sub, objects, own);
                if ui
                    .add_enabled(selection.is_ok(), egui::Button::new("Use selection"))
                    .on_disabled_hover_text(&reason)
                    .clicked()
                {
                    *value = selection.as_ref().unwrap().clone();
                }
            });
        }
        if (selections.is_err() || !selected.is_empty()) && selection.is_err() {
            ui.colored_label(egui::Color32::from_rgb(225, 142, 138), reason);
        }
    } else if matches!(kind, "VectorDistance" | "Position") {
        physical_vector(ui, value, &metadata["quantity"]);
    } else if kind == "Vector" {
        vector_widget(ui, value, 3);
    } else if kind == "Placement" {
        ui.vertical(|ui| {
            ui.label("Position");
            physical_vector(
                ui,
                &mut value["position"],
                &metadata["quantity"]["position"],
            );
            ui.label("Rotation quaternion x, y, z, w");
            vector_widget(ui, &mut value["rotation"], 4);
        });
    } else if kind.ends_with("List") {
        list_widget(ui, value, kind);
    } else if matches!(kind, "Float" | "FloatConstraint" | "Percent" | "Precision") {
        let mut number = value.as_f64().unwrap_or_default();
        if ui
            .add(egui::DragValue::new(&mut number).speed(0.05))
            .changed()
        {
            *value = json!(number);
        }
    } else if matches!(kind, "Integer" | "IntegerConstraint") {
        let mut v = value.as_i64().unwrap_or_default();
        if ui.add(egui::DragValue::new(&mut v)).changed() {
            *value = json!(v);
        }
    } else {
        let text = buffers
            .entry(property.into())
            .or_insert_with(|| display(value));
        if ui
            .add(egui::TextEdit::singleline(text).desired_width(220.))
            .changed()
        {
            *value = json!(text);
        }
    }
    before != *value
}

fn link_widget(ui: &mut egui::Ui, value: &mut Value, sub: bool, objects: &[Value], own: &str) {
    let name = value["link"].as_str().unwrap_or_default();
    let label = objects
        .iter()
        .find(|o| o["name"] == name)
        .and_then(|o| o["label"].as_str())
        .unwrap_or(name);
    egui::ComboBox::from_id_salt("link")
        .selected_text(if label.is_empty() { "None" } else { label })
        .width(150.)
        .show_ui(ui, |ui| {
            if ui.selectable_label(value.is_null(), "None").clicked() {
                *value = Value::Null;
            }
            for object in objects.iter().filter(|o| o["name"] != own) {
                if ui
                    .selectable_label(
                        value["link"] == object["name"],
                        object["label"].as_str().unwrap_or_default(),
                    )
                    .clicked()
                {
                    *value = json!({"link":object["name"]});
                    if sub {
                        value["subelements"] = json!([]);
                    }
                }
            }
        });
    if sub && !value.is_null() {
        let id = ui.make_persistent_id("subelement_text");
        let canonical = value["subelements"]
            .as_array()
            .map(|v| {
                v.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let mut text = ui
            .data_mut(|data| data.get_temp::<String>(id))
            .unwrap_or(canonical);
        let response = ui.add(
            egui::TextEdit::singleline(&mut text)
                .hint_text("Face1, Edge2, H_Axis...")
                .desired_width(180.),
        );
        if response.changed() {
            value["subelements"] = json!(
                text.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            );
        }
        if response.has_focus() {
            ui.data_mut(|data| data.insert_temp(id, text));
        } else {
            ui.data_mut(|data| data.remove::<String>(id));
        }
    }
}
fn physical_vector(ui: &mut egui::Ui, value: &mut Value, metadata: &Value) {
    ui.horizontal(|ui| {
        for i in 0..3 {
            ui.push_id(i, |ui| {
                super::units::quantity_field(ui, &mut value[i], &metadata[i]);
            });
        }
    });
}

fn vector_widget(ui: &mut egui::Ui, value: &mut Value, count: usize) {
    let mut v = (0..count)
        .map(|i| value[i].as_f64().unwrap_or(if i == 3 { 1. } else { 0. }))
        .collect::<Vec<_>>();
    let mut changed = false;
    ui.horizontal(|ui| {
        for component in &mut v {
            changed |= ui.add(egui::DragValue::new(component).speed(0.1)).changed();
        }
    });
    if changed {
        *value = json!(v);
    }
}

fn list_widget(ui: &mut egui::Ui, value: &mut Value, kind: &str) {
    let mut items = value.as_array().cloned().unwrap_or_default();
    let mut changed = false;
    let mut remove = None;
    ui.vertical(|ui| {
        for (index, item) in items.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                ui.horizontal(|ui| {
                    match kind {
                        "VectorList" => {
                            let before = item.clone();
                            vector_widget(ui, item, 3);
                            changed |= *item != before;
                        }
                        "BoolList" => {
                            let mut v = item.as_bool().unwrap_or_default();
                            if ui.checkbox(&mut v, "").changed() {
                                *item = json!(v);
                                changed = true;
                            }
                        }
                        "IntegerList" => {
                            let mut v = item.as_i64().unwrap_or_default();
                            if ui.add(egui::DragValue::new(&mut v)).changed() {
                                *item = json!(v);
                                changed = true;
                            }
                        }
                        "FloatList" => {
                            let mut v = item.as_f64().unwrap_or_default();
                            if ui.add(egui::DragValue::new(&mut v).speed(0.1)).changed() {
                                *item = json!(v);
                                changed = true;
                            }
                        }
                        _ => {
                            let mut v = item.as_str().unwrap_or_default().to_owned();
                            if ui.text_edit_singleline(&mut v).changed() {
                                *item = json!(v);
                                changed = true;
                            }
                        }
                    }
                    if ui.small_button("Remove").clicked() {
                        remove = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove {
            items.remove(index);
            changed = true;
        }
        if ui.small_button("Add value").clicked() {
            items.push(match kind {
                "VectorList" => json!([0, 0, 0]),
                "BoolList" => json!(false),
                "IntegerList" => json!(0),
                "FloatList" => json!(0.),
                _ => json!(""),
            });
            changed = true;
        }
    });
    if changed {
        *value = json!(items);
    }
}

fn map_widget(ui: &mut egui::Ui, value: &mut Value) {
    let mut entries = value
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), display(v)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut changed = false;
    let mut remove = None;
    ui.vertical(|ui| {
        for (index, (key, text)) in entries.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                ui.horizontal(|ui| {
                    changed |= ui
                        .add(egui::TextEdit::singleline(key).desired_width(90.))
                        .changed();
                    changed |= ui
                        .add(egui::TextEdit::singleline(text).desired_width(150.))
                        .changed();
                    if ui.small_button("Remove").clicked() {
                        remove = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove {
            entries.remove(index);
            changed = true;
        }
        if ui.small_button("Add entry").clicked() {
            let mut n = 1;
            while entries.iter().any(|(key, _)| key == &format!("Key{n}")) {
                n += 1;
            }
            entries.push((format!("Key{n}"), String::new()));
            changed = true;
        }
    });
    if changed {
        *value = Value::Object(entries.into_iter().map(|(k, v)| (k, json!(v))).collect());
    }
}

fn selected_fillet_edges(
    base: &Value,
    objects: &[Value],
    selected: &[(String, String)],
) -> Result<Vec<Value>, String> {
    let source = base["link"]
        .as_str()
        .filter(|name| !name.is_empty())
        .ok_or("Choose a Base before selecting its edges")?;
    let body_tip = objects
        .iter()
        .find(|object| object["name"] == source && object["type"] == "PartDesign::Body")
        .and_then(|body| body["body_state"]["tip"].as_str());
    let mut edges = Vec::new();
    for (owner, subelement) in selected {
        let Some(number) = subelement.strip_prefix("Edge") else {
            continue;
        };
        let index = number
            .parse::<i32>()
            .ok()
            .filter(|index| *index > 0)
            .ok_or("Select native edges with positive indices")?;
        // A Body is displayed through its current native Tip. Its historical
        // features and unrelated containers do not share this edge identity.
        let displayed_tip = body_tip == Some(owner.as_str())
            && objects
                .iter()
                .any(|object| object["name"] == *owner && object["body"] == source);
        if owner != source && !displayed_tip {
            return Err(format!(
                "Select edges of Base {source}; {owner}.{subelement} belongs to another object"
            ));
        }
        let edge = json!([index, 1., 1.]);
        if !edges.contains(&edge) {
            edges.push(edge);
        }
    }
    if edges.is_empty() {
        return Err(format!("Select native edges of Base {source}"));
    }
    Ok(edges)
}

fn fillet_edges_widget(
    ui: &mut egui::Ui,
    value: &mut Value,
    selected: &[(String, String)],
    base: &Value,
    objects: &[Value],
    metadata: &Value,
    default: &Value,
) {
    let mut edges = value.as_array().cloned().unwrap_or_default();
    let mut changed = false;
    let mut remove = None;
    ui.vertical(|ui| {
        ui.weak("Native edge index, start radius, end radius");
        for (index, edge) in edges.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                ui.horizontal(|ui| {
                    let mut number = edge[0].as_i64().unwrap_or(1);
                    if ui
                        .add(egui::DragValue::new(&mut number).range(1..=i32::MAX))
                        .changed()
                    {
                        edge[0] = json!(number);
                        changed = true;
                    }
                    for i in 1..3 {
                        ui.push_id(i, |ui| {
                            // Add/selection/removal can replace or move a row
                            // before native preview refreshes its metadata.
                            // Match unchanged numbers to native quantities by
                            // value; new rows use the native default quantity.
                            let quantity = if edge[i].is_number() {
                                metadata
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(Value::as_array)
                                    .flatten()
                                    .find(|q| q["value"] == edge[i])
                                    .unwrap_or(default)
                            } else {
                                metadata
                                    .get(index)
                                    .and_then(|row| row.get(i))
                                    .unwrap_or(default)
                            };
                            changed |= super::units::quantity_field(ui, &mut edge[i], quantity);
                        });
                    }
                    if ui.small_button("Remove").clicked() {
                        remove = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove {
            edges.remove(index);
            changed = true;
        }
        if ui.small_button("Add edge").clicked() {
            edges.push(json!([1, 1., 1.]));
            changed = true;
        }
        let picked = selected_fillet_edges(base, objects, selected);
        if selected.iter().any(|(_, name)| name.starts_with("Edge"))
            && let Err(error) = &picked
        {
            ui.colored_label(egui::Color32::from_rgb(225, 142, 138), error);
        }
        if ui
            .add_enabled(picked.is_ok(), egui::Button::new("Use selected edges"))
            .clicked()
        {
            edges = picked.unwrap();
            changed = true;
        }
    });
    if changed {
        *value = json!(edges);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picks(values: &[(&str, &str)]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|(object, edge)| ((*object).into(), (*edge).into()))
            .collect()
    }

    #[test]
    fn fillet_edge_picks_reject_foreign_owners_even_when_edge_numbers_match() {
        let base = json!({"link":"Base"});
        let selected = picks(&[("Base", "Edge4"), ("Other", "Edge4")]);
        let error = selected_fillet_edges(&base, &[], &selected).unwrap_err();
        assert!(error.contains("Base") && error.contains("Other.Edge4"));
        assert_eq!(base, json!({"link":"Base"}));
        assert_eq!(
            selected_fillet_edges(
                &base,
                &[],
                &picks(&[
                    ("Base", "Edge4"),
                    ("Base", "Face1"),
                    ("Base", "Edge4"),
                    ("Base", "Edge2")
                ])
            )
            .unwrap(),
            json!([[4, 1., 1.], [2, 1., 1.]])
                .as_array()
                .unwrap()
                .clone()
        );
        let linked = json!({"link":"Other","subelements":[]});
        assert!(selected_fillet_edges(&linked, &[], &picks(&[("Other", "Edge4")])).is_ok());
        for invalid in [Value::Null, json!("Base"), json!({"link":""})] {
            assert!(selected_fillet_edges(&invalid, &[], &selected).is_err());
        }
    }

    #[test]
    fn body_base_accepts_only_its_current_native_display_tip() {
        let objects = vec![
            json!({"name":"Body","type":"PartDesign::Body","body_state":{"tip":"Finish"}}),
            json!({"name":"Finish","body":"Body"}),
            json!({"name":"OldPad","body":"Body"}),
            json!({"name":"OtherPad","body":"OtherBody"}),
            json!({"name":"Container","type":"App::Part","body_state":{"tip":"Finish"}}),
        ];
        let base = json!({"link":"Body"});
        assert!(selected_fillet_edges(&base, &objects, &picks(&[("Finish", "Edge2")])).is_ok());
        assert!(selected_fillet_edges(&base, &objects, &picks(&[("Body", "Edge2")])).is_ok());
        for owner in ["OldPad", "OtherPad"] {
            assert!(selected_fillet_edges(&base, &objects, &picks(&[(owner, "Edge2")])).is_err());
        }
        assert!(
            selected_fillet_edges(
                &json!({"link":"Container"}),
                &objects,
                &picks(&[("Finish", "Edge2")])
            )
            .is_err()
        );
        for edge in ["Edge0", "Edge-1", "Edge4294967296", "Face1"] {
            assert!(selected_fillet_edges(&base, &objects, &picks(&[("Body", edge)])).is_err());
        }
    }

    #[test]
    fn mismatched_edge_widget_shows_error_and_preserves_native_values() {
        let context = egui::Context::default();
        let base = json!({"link":"Base"});
        let original = json!([[7, 2.5, 3.]]);
        let mut value = original.clone();
        let selected = picks(&[("Other", "Edge7")]);
        let output = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                fillet_edges_widget(
                    ui,
                    &mut value,
                    &selected,
                    &base,
                    &[],
                    &Value::Null,
                    &Value::Null,
                );
            });
        });
        assert_eq!(value, original);
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains("Other.Edge7 belongs to another object"))
        }));
    }
}

#[cfg(test)]
#[path = "properties/extrusion_acceptance.rs"]
mod acceptance_probe;

#[cfg(test)]
#[path = "properties/reference_selection_tests.rs"]
mod reference_selection_tests;
