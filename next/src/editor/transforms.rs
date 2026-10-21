//! Native MultiTransform child tasks share the ordinary property widgets and
//! preview transaction. This state contains form buffers, never document history.
use super::{objects, properties::property_widget};
use crate::{feature_commands, property_commands, transform_commands};
use eframe::egui;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct TransformUi {
    original: Vec<String>,
    allocated: BTreeSet<String>,
    schemas: BTreeMap<String, Value>,
    templates: BTreeMap<String, Value>,
    buffers: BTreeMap<String, BTreeMap<String, String>>,
    pending: Option<String>,
    failed: BTreeSet<String>,
    selected: usize,
    advanced: bool,
    error: Option<String>,
}

impl TransformUi {
    /// Call once when opening the task, before any preview replaces state.
    pub(super) fn begin(&mut self, parameters: &mut Value, state: &Value) {
        *self = Self::default();
        let parent = objects(state)
            .iter()
            .find(|o| o["name"] == parameters["name"]);
        if parameters["create"] != true {
            if parameters["body"].is_null() {
                parameters["body"] = parent.map(|o| o["body"].clone()).unwrap_or_default();
            }
            parameters["restore_tip"] = objects(state)
                .iter()
                .find(|o| o["name"] == parameters["body"])
                .map(|o| o["body_state"]["tip"].clone())
                .unwrap_or_default();
            parameters["restore_tip_visibility"] = objects(state)
                .iter()
                .find(|o| o["name"] == parameters["restore_tip"])
                .map(|o| o["properties"]["Visibility"].clone())
                .unwrap_or(Value::Bool(true));
        }
        let links = parent
            .and_then(|o| o["properties"]["Transformations"].as_array())
            .cloned()
            .unwrap_or_default();
        let mut steps = Vec::new();
        for link in links {
            let Some(name) = link["link"].as_str() else {
                continue;
            };
            let Some(object) = objects(state).iter().find(|o| o["name"] == name) else {
                continue;
            };
            self.original.push(name.to_owned());
            self.allocated.insert(name.to_owned());
            self.schemas.insert(name.to_owned(), object.clone());
            steps.push(json!({"type":object["type"],"name":name,"create":false,
                "properties":{},"expressions":{}}));
        }
        parameters["transformations"] = json!(steps);
        parameters["removed_transformations"] = json!([]);
        if let Some(properties) = parameters["properties"].as_object_mut() {
            properties.remove("Transformations");
        }
    }

    /// Refresh conditional native metadata after a successful preview. Keep
    /// creation flags and the original child list fixed for the whole task.
    pub(super) fn sync(&mut self, parameters: &mut Value, state: &Value) {
        if let Some(steps) = parameters["transformations"].as_array_mut() {
            for step in steps {
                if let Some(name) = step["name"].as_str().map(str::to_owned)
                    && let Some(object) = objects(state).iter().find(|o| o["name"] == name)
                {
                    if let Some(changes) = step["properties"].as_object_mut() {
                        changes.retain(|property, value| {
                            if !property_commands::writable(object, property) {
                                return false;
                            }
                            if let Some(native) = object["properties"].get(property) {
                                *value = native.clone();
                            }
                            true
                        });
                        if let Some(buffers) = self.buffers.get_mut(&name) {
                            buffers.retain(|property, text| {
                                if let Some(value) = changes.get(property)
                                    && object["expressions"].get(property).is_none()
                                {
                                    *text = value
                                        .as_str()
                                        .map(str::to_owned)
                                        .unwrap_or_else(|| value.to_string());
                                    true
                                } else {
                                    false
                                }
                            });
                        }
                    }
                    self.schemas.insert(name, object.clone());
                }
            }
        }
    }

    /// The editor sends one feature_schema request and routes its reply back
    /// here. Pending requests must block preview until their replies arrive.
    pub(super) fn schema_request(&mut self, parameters: &Value) -> Option<String> {
        if self.pending.is_some() {
            return None;
        }
        let type_id = parameters["transformations"]
            .as_array()?
            .iter()
            .find_map(|step| {
                let type_id = step["type"].as_str()?;
                let name = step["name"].as_str()?;
                (!self.schemas.contains_key(name)
                    && !self.templates.contains_key(type_id)
                    && !self.failed.contains(type_id))
                .then(|| type_id.to_owned())
            })?;
        self.pending = Some(type_id.clone());
        Some(type_id)
    }

    pub(super) fn awaiting_schema(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn schema_reply(&mut self, schema: Value, success: bool) {
        if let Some(type_id) = self.pending.take() {
            if success {
                self.templates.insert(type_id, schema);
                self.error = None;
            } else {
                self.failed.insert(type_id);
                self.error = Some("Native transformation properties could not be loaded".into());
            }
        }
    }

    /// Returns true only for changes that need a new native preview. Root owns
    /// debounce, revision guards, preview commit/cancel and document history.
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        parameters: &mut Value,
        state: &Value,
        selection: &[(String, String)],
        busy: bool,
    ) -> bool {
        let mut changed = false;
        let mut add = None;
        let mut remove = None;
        let mut reorder = None;
        ui.label("Transformation steps");
        ui.add_enabled_ui(!busy && !self.awaiting_schema(), |ui| {
            ui.menu_button("Add step", |ui| {
                for type_id in transform_commands::STEP_TYPES {
                    let definition = feature_commands::get(type_id).unwrap();
                    if ui.button(definition.label).clicked() {
                        add = Some(*type_id);
                        ui.close();
                    }
                }
            });
            if let Some(steps) = parameters["transformations"].as_array_mut() {
                let count = steps.len();
                egui::ScrollArea::vertical()
                    .id_salt("transform_steps")
                    .max_height(95.)
                    .show(ui, |ui| {
                        for (index, step) in steps.iter().enumerate() {
                            ui.push_id(index, |ui| {
                                ui.horizontal(|ui| {
                                    let label = feature_commands::get(
                                        step["type"].as_str().unwrap_or_default(),
                                    )
                                    .map(|definition| definition.label)
                                    .unwrap_or("Transformation");
                                    if ui
                                        .selectable_label(
                                            self.selected == index,
                                            format!("{}. {label}", index + 1),
                                        )
                                        .clicked()
                                    {
                                        self.selected = index;
                                    }
                                    if ui
                                        .add_enabled(index > 0, egui::Button::new("Up").small())
                                        .clicked()
                                    {
                                        reorder = Some((index, index - 1));
                                    }
                                    if ui
                                        .add_enabled(
                                            index + 1 < count,
                                            egui::Button::new("Down").small(),
                                        )
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
                    });
                if let Some(index) = remove {
                    steps.remove(index);
                    self.selected = self.selected.min(steps.len().saturating_sub(1));
                    changed = true;
                } else if let Some((from, to)) = reorder {
                    steps.swap(from, to);
                    self.selected = to;
                    changed = true;
                }
            }
            if let Some(type_id) = add {
                match self.add(parameters, state, type_id) {
                    Ok(()) => changed = true,
                    Err(error) => self.error = Some(error),
                }
            }
            let steps = parameters["transformations"].as_array_mut().unwrap();
            if steps.is_empty() {
                ui.weak("Add a step to transform the original features.");
            } else if let Some(step) = steps.get_mut(self.selected) {
                let name = step["name"].as_str().unwrap_or_default().to_owned();
                let type_id = step["type"].as_str().unwrap_or_default().to_owned();
                let schema = self
                    .schemas
                    .get(&name)
                    .or_else(|| self.templates.get(&type_id));
                if let Some(schema) = schema {
                    changed |= step_fields(
                        ui,
                        step,
                        schema,
                        state,
                        selection,
                        self.buffers.entry(name).or_default(),
                        &mut self.advanced,
                    );
                } else {
                    ui.spinner();
                    ui.weak("Loading native step properties");
                }
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::from_rgb(225, 142, 138), error);
            if !busy && ui.small_button("Retry step properties").clicked() {
                self.failed.clear();
                self.error = None;
            }
        }
        let retained: BTreeSet<_> = parameters["transformations"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|step| step["name"].as_str())
            .collect();
        parameters["removed_transformations"] = json!(
            self.original
                .iter()
                .filter(|name| !retained.contains(name.as_str()))
                .collect::<Vec<_>>()
        );
        changed
    }

    fn add(&mut self, parameters: &mut Value, state: &Value, type_id: &str) -> Result<(), String> {
        let base = type_id.split("::").last().unwrap_or("Step");
        let steps = parameters["transformations"].as_array().unwrap();
        let mut suffix = 1;
        let name = loop {
            let candidate = format!("{base}{suffix}");
            if parameters["name"] != candidate
                && !objects(state).iter().any(|o| o["name"] == candidate)
                && !steps.iter().any(|step| step["name"] == candidate)
                && !self.allocated.contains(&candidate)
            {
                break candidate;
            }
            suffix += 1;
        };
        let mut properties = feature_commands::seed(
            state,
            type_id,
            parameters["body"].as_str(),
            &name,
            None,
            &[],
        )?;
        properties.remove("Originals");
        properties.remove("OriginalSubs");
        self.allocated.insert(name.clone());
        let steps = parameters["transformations"].as_array_mut().unwrap();
        steps.push(json!({"type":type_id,"name":name,"create":true,
            "properties":properties,"expressions":{}}));
        self.selected = steps.len() - 1;
        self.error = None;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn step_fields(
    ui: &mut egui::Ui,
    step: &mut Value,
    schema: &Value,
    state: &Value,
    selection: &[(String, String)],
    buffers: &mut BTreeMap<String, String>,
    advanced: &mut bool,
) -> bool {
    let name = step["name"].as_str().unwrap_or_default().to_owned();
    let preferred = feature_commands::get(step["type"].as_str().unwrap_or_default())
        .map(|definition| definition.preferred_properties)
        .unwrap_or_default();
    let mut fields: Vec<_> = schema["property_types"]
        .as_object()
        .map(|types| types.keys().cloned().collect())
        .unwrap_or_default();
    fields.sort_by_key(|property| {
        (
            preferred
                .iter()
                .position(|p| *p == property)
                .unwrap_or(usize::MAX),
            property.clone(),
        )
    });
    let mut changed = false;
    ui.checkbox(advanced, "All step properties");
    egui::ScrollArea::vertical()
        .id_salt(("transform_fields", &name))
        .max_height(245.)
        .show(ui, |ui| {
            for property in fields {
                let metadata = &schema["property_metadata"][&property];
                if matches!(
                    property.as_str(),
                    "Originals" | "OriginalSubs" | "BaseFeature" | "Transformations"
                ) || !property_commands::writable(schema, &property)
                    || !*advanced && metadata["hidden"] == true
                {
                    continue;
                }
                ui.push_id((&name, &property), |ui| {
                    let mut value = step["properties"]
                        .get(&property)
                        .cloned()
                        .unwrap_or_else(|| schema["properties"][&property].clone());
                    let mut expression = step["expressions"]
                        .get(&property)
                        .cloned()
                        .unwrap_or_else(|| schema["expressions"][&property].clone());
                    let type_id = schema["property_types"][&property]
                        .as_str()
                        .unwrap_or_default();
                    ui.label(&property)
                        .on_hover_text(metadata["documentation"].as_str().unwrap_or_default());
                    ui.add_enabled_ui(!expression.is_string(), |ui| {
                        if property_widget(
                            ui,
                            &property,
                            type_id,
                            metadata,
                            &mut value,
                            buffers,
                            objects(state),
                            &name,
                            selection,
                        ) {
                            step["properties"][&property] = value.clone();
                            changed = true;
                        }
                    });
                    if matches!(
                        type_id.strip_prefix("App::Property"),
                        Some(
                            "Length"
                                | "Distance"
                                | "Angle"
                                | "Float"
                                | "FloatConstraint"
                                | "Integer"
                                | "IntegerConstraint"
                                | "Percent"
                        )
                    ) {
                        let mut active = expression.is_string();
                        if ui.checkbox(&mut active, "Expression").changed() {
                            expression = if active {
                                json!(
                                    value
                                        .as_str()
                                        .map(str::to_owned)
                                        .unwrap_or_else(|| value.to_string())
                                )
                            } else {
                                Value::Null
                            };
                            step["expressions"][&property] = expression.clone();
                            changed = true;
                        }
                        if let Some(text) = expression.as_str() {
                            let mut text = text.to_owned();
                            if ui.text_edit_singleline(&mut text).changed() {
                                step["expressions"][&property] = json!(text);
                                changed = true;
                            }
                        }
                    }
                    ui.separator();
                });
            }
        });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_snapshot_keeps_committed_child_identity_after_preview() {
        let state = json!({"objects":[
            {"name":"Body","type":"PartDesign::Body","body_state":{"tip":"Downstream"}},
            {"name":"Result","type":"PartDesign::MultiTransform","body":"Body",
                "properties":{"Transformations":[{"link":"Linear"}]}},
            {"name":"Linear","type":"PartDesign::LinearPattern","properties":{"Length":4}}
        ]});
        let mut p = json!({"name":"Result","type":"PartDesign::MultiTransform",
            "create":false,"properties":{},"expressions":{}});
        let mut form = TransformUi::default();
        form.begin(&mut p, &state);
        assert_eq!(p["body"], "Body");
        assert_eq!(p["restore_tip"], "Downstream");
        assert_eq!(p["transformations"][0]["create"], false);
        form.add(&mut p, &state, "PartDesign::Scaled").unwrap();
        p["transformations"][1]["properties"]["Factor"] = json!(2);
        assert_eq!(p["transformations"][1]["create"], true);
        assert!(
            p["transformations"][1]["properties"]
                .get("Originals")
                .is_none()
        );
        let mut preview = state.clone();
        preview["objects"].as_array_mut().unwrap().push(json!({
            "name":p["transformations"][1]["name"],"type":"PartDesign::Scaled",
            "properties":{"Factor":1.5},"property_types":{"Factor":"App::PropertyFloat"},
            "property_metadata":{"Factor":{"read_only":false}},"expressions":{}}));
        let name = p["transformations"][1]["name"].as_str().unwrap().to_owned();
        form.buffers.insert(
            name.clone(),
            BTreeMap::from([("Factor".into(), "2".into())]),
        );
        form.sync(&mut p, &preview);
        assert_eq!(p["transformations"][1]["create"], true);
        assert_eq!(p["restore_tip"], "Downstream");
        assert_eq!(form.original, ["Linear"]);
        assert_eq!(p["transformations"][1]["properties"]["Factor"], 1.5);
        assert_eq!(form.buffers[&name]["Factor"], "1.5");
    }

    #[test]
    fn failed_schema_does_not_request_forever() {
        let mut p = json!({"transformations":[{"type":"PartDesign::Scaled","name":"Step"}]});
        let mut form = TransformUi::default();
        assert_eq!(
            form.schema_request(&p).as_deref(),
            Some("PartDesign::Scaled")
        );
        assert!(form.awaiting_schema());
        assert!(form.schema_request(&p).is_none());
        form.schema_reply(Value::Null, false);
        assert!(!form.awaiting_schema());
        assert!(form.schema_request(&p).is_none());
        p["transformations"] = json!([]);
        assert!(form.schema_request(&p).is_none());
    }
}
