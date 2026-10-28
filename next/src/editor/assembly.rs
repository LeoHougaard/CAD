//! Assembly tasks retain native Links, joint proxies and connector references.
use super::{Draft, Editor, objects};
use crate::assembly_commands::{self, identity_placement};
use eframe::egui;
use serde_json::{Value, json};
use std::time::Instant;

fn label(obj: &Value) -> &str {
    obj["label"]
        .as_str()
        .or_else(|| obj["name"].as_str())
        .unwrap_or_default()
}

fn discard_joint_override(request: &mut Value, property: &str) {
    for edits in ["properties", "expressions"] {
        if let Some(edits) = request[edits].as_object_mut() {
            edits.retain(|path, _| {
                let path = path.trim_start_matches('.');
                path != property
                    && !path
                        .strip_prefix(property)
                        .is_some_and(|suffix| suffix.starts_with('.') || suffix.starts_with('['))
            });
        }
    }
}

fn choose(ui: &mut egui::Ui, id: &str, value: &mut Value, choices: &[Value]) {
    let text = choices
        .iter()
        .find(|o| o["name"] == *value)
        .map(label)
        .unwrap_or(value.as_str().unwrap_or("Choose"))
        .to_owned();
    ui.horizontal(|ui| {
        ui.label(id);
        egui::ComboBox::from_id_salt(("assembly", id))
            .selected_text(text)
            .show_ui(ui, |ui| {
                for obj in choices {
                    ui.selectable_value(value, obj["name"].clone(), label(obj));
                }
            });
    });
}

fn placement_fields(
    ui: &mut egui::Ui,
    title: &str,
    value: &mut Value,
    metadata: &Value,
    defaults: &Value,
) {
    ui.push_id(title, |ui| {
        ui.label(title);
        for (field, names) in [
            ("position", &["X", "Y", "Z"][..]),
            ("rotation", &["Qx", "Qy", "Qz", "Qw"][..]),
        ] {
            ui.horizontal(|ui| {
                for (index, name) in names.iter().enumerate() {
                    ui.label(*name);
                    if field == "position" {
                        let quantity = metadata["position"].get(index).unwrap_or(&defaults["mm"]);
                        ui.push_id(index, |ui| {
                            super::units::quantity_field(ui, &mut value[field][index], quantity);
                        });
                        continue;
                    }
                    let mut number = value[field][index].as_f64().unwrap_or_default();
                    if ui
                        .add(egui::DragValue::new(&mut number).speed(0.1))
                        .changed()
                    {
                        value[field][index] = json!(number);
                    }
                }
            });
        }
    });
}

impl Editor {
    pub(super) fn start_assembly_task(&mut self, request: Value, title: &str) {
        if self.busy || self.draft.is_some() || self.sketch_ui.session.is_some() {
            return;
        }
        if self.state["recovery_only"] == true && request["op"] != "assembly_relink" {
            self.error = Some(
                "Recover native links through Save or Update / relink external source first".into(),
            );
            return;
        }
        self.sequence += 1;
        self.draft = Some(Draft {
            id: "native.assembly".into(),
            name: title.into(),
            token: format!("assembly-{}-{}", std::process::id(), self.sequence),
            parameters: json!({"state":self.state,"request":request}),
            changed: Some(Instant::now()),
            valid: false,
            all_parameters: true,
            deferred: true,
            committing: false,
        });
        self.sketch = None;
        self.viewport.set_sketch(None);
    }

    pub(super) fn edit_assembly_object(&mut self, name: &str) -> bool {
        let Some(obj) = objects(&self.state)
            .iter()
            .find(|o| o["name"] == name)
            .cloned()
        else {
            return false;
        };
        if obj["type"] == "App::Link" && obj["assembly"].is_string() {
            self.start_assembly_task(
                json!({"op":"assembly_placement","assembly":obj["assembly"],
                "object":name,"placement":obj["properties"]["LinkPlacement"]}),
                "Place component",
            );
        } else if obj["properties"]["JointType"].is_string() && obj["assembly"].is_string() {
            self.start_assembly_task(json!({"op":"assembly_joint","assembly":obj["assembly"],
                "name":name,"create":false,"joint_type":obj["properties"]["JointType"],
                "reference1":obj["properties"]["Reference1"],"reference2":obj["properties"]["Reference2"],
                "properties":{},"expressions":{}}), "Edit native joint");
        } else {
            return false;
        }
        true
    }

    pub(super) fn assembly_toolbar(&mut self, ui: &mut egui::Ui) {
        let all = objects(&self.state);
        let selected = self
            .selected_object
            .as_ref()
            .and_then(|n| all.iter().find(|o| o["name"] == *n))
            .filter(|o| {
                !self.source_recovery()
                    || self.state["save_recovery"]["pending_edges"]
                        .as_array()
                        .is_some_and(|edges| edges.iter().any(|edge| edge["name"] == o["name"]))
            })
            .or_else(|| {
                if self.source_recovery() {
                    all.iter().find(|o| {
                        self.state["save_recovery"]["pending_edges"]
                            .as_array()
                            .is_some_and(|edges| edges.iter().any(|edge| edge["name"] == o["name"]))
                    })
                } else {
                    None
                }
            })
            .cloned();
        let assembly = selected
            .as_ref()
            .and_then(|o| {
                if o["type"] == "Assembly::AssemblyObject" {
                    o["name"].as_str()
                } else {
                    o["assembly"].as_str()
                }
            })
            .or_else(|| {
                all.iter()
                    .find(|o| o["type"] == "Assembly::AssemblyObject")
                    .and_then(|o| o["name"].as_str())
            })
            .map(str::to_owned);
        let instances: Vec<Value> = all
            .iter()
            .filter(|o| o["type"] == "App::Link" && o["assembly"] == json!(assembly))
            .cloned()
            .collect();
        let sources: Vec<Value> = all
            .iter()
            .filter(|o| {
                o["type"] == "PartDesign::Body"
                    || o["type"].as_str().is_some_and(|t| t.starts_with("Part::"))
            })
            .cloned()
            .collect();
        ui.menu_button("Assembly", |ui| {
            ui.add_enabled_ui(!self.source_recovery(), |ui| {
            if ui.button("New assembly").clicked() {
                let name = self.unique("Assembly"); let joints = self.unique("Joints");
                self.start_assembly_task(json!({"op":"assembly_create","name":name,"joints":joints}), "New assembly"); ui.close();
            }
            if ui.add_enabled(assembly.is_some() && !sources.is_empty(), egui::Button::new("Insert component")).clicked() {
                let source = selected.as_ref().filter(|o| sources.iter().any(|s| s["name"] == o["name"]) || o["type"] == "App::Link")
                    .map(|o| o["name"].clone()).unwrap_or_else(|| sources[0]["name"].clone());
                let name = self.unique("Component");
                self.start_assembly_task(json!({"op":"assembly_insert","assembly":assembly,"name":name,
                    "source":source,"placement":identity_placement()}), "Insert component"); ui.close();
            }
            if ui.add_enabled(assembly.is_some(), egui::Button::new("Insert external component...")).clicked() {
                if self.state["path"].as_str().is_none_or(str::is_empty) {
                    self.file_dialog(true);
                    self.status = "Save the root FCStd, then choose Assembly > Insert external component again.".into();
                } else {
                    let name = self.unique("Component");
                    self.start_assembly_task(json!({"op":"assembly_insert","assembly":assembly,"name":name,
                        "external_source":null,"placement":identity_placement()}), "Insert external component");
                }
                ui.close();
            }
            if ui.add_enabled(instances.len() >= 2, egui::Button::new("New joint")).clicked() {
                let name = self.unique("Joint");
                self.start_assembly_task(json!({"op":"assembly_joint","assembly":assembly,"name":name,
                    "create":true,"joint_type":"Fixed","reference1":{"link":instances[0]["name"],"subelements":["",""]},
                    "reference2":{"link":instances[1]["name"],"subelements":["",""]},"properties":{},"expressions":{}}), "New native joint"); ui.close();
            }
            });
            let pending = self.state["save_recovery"]["pending_edges"].as_array();
            let pending_instance = |o: &&Value| pending.is_some_and(|edges| edges.iter().any(|edge| edge["name"] == o["name"]));
            let selected_instance = selected.as_ref().filter(|o| instances.iter().any(|i| i["name"] == o["name"]))
                .filter(|o| !self.source_recovery() || pending_instance(o))
                .or_else(|| if self.source_recovery() { instances.iter().find(pending_instance) } else { None });
            if ui.add_enabled(selected_instance.is_some_and(|o| o["instance_state"]["external_source"].is_object()), egui::Button::new("Update / relink external source...")).clicked() {
                let obj = selected_instance.unwrap();
                self.start_assembly_task(json!({"op":"assembly_relink","assembly":assembly,"object":obj["name"],
                    "external_source":obj["instance_state"]["external_source"],"solve_joints":true}), "Update / relink external source");
                if let Some(draft) = &mut self.draft {
                    draft.parameters["source_path"] = obj["instance_state"]["external_source"]["path"].clone();
                }
                ui.close();
            }
            ui.add_enabled_ui(!self.source_recovery(), |ui| {
            if ui.add_enabled(selected_instance.is_some(), egui::Button::new("Place component")).clicked() {
                self.edit_assembly_object(selected_instance.unwrap()["name"].as_str().unwrap()); ui.close();
            }
            if ui.add_enabled(selected_instance.is_some(), egui::Button::new("Ground / unground")).clicked() {
                let obj = selected_instance.unwrap();
                let name = self.unique("GroundedJoint");
                self.start_assembly_task(json!({"op":"assembly_ground","assembly":assembly,"object":obj["name"],
                    "name":name,"grounded":obj["instance_state"]["grounded"] != true}), "Ground component"); ui.close();
            }
            if ui.add_enabled(selected.as_ref().is_some_and(|o| o["properties"]["JointType"].is_string()), egui::Button::new("Edit joint")).clicked() {
                self.edit_assembly_object(selected.as_ref().unwrap()["name"].as_str().unwrap()); ui.close();
            }
            if ui.add_enabled(selected.as_ref().is_some_and(|o| o["assembly"].is_string() && o["type"] != "Assembly::JointGroup"), egui::Button::new("Remove component or joint")).clicked() {
                self.start_assembly_task(json!({"op":"assembly_remove","assembly":assembly,
                    "object":selected.as_ref().unwrap()["name"]}), "Remove assembly member"); ui.close();
            }
            if ui.add_enabled(assembly.is_some(), egui::Button::new("Solve assembly")).clicked() {
                self.start_assembly_task(json!({"op":"assembly_solve","assembly":assembly}), "Solve assembly"); ui.close();
            }
            });
        });
    }

    pub(super) fn assembly_fields(&mut self, ui: &mut egui::Ui) {
        let mut browse = None;
        let mut load = None;
        let Some(draft) = &mut self.draft else {
            return;
        };
        let baseline = draft.parameters["state"].clone();
        let mut request = draft.parameters["request"].clone();
        ui.label(&draft.name);
        egui::ScrollArea::vertical().max_height(510.).show(ui, |ui| {
            ui.add_enabled_ui(!self.busy && !draft.committing, |ui| {
                let op = request["op"].as_str().unwrap_or_default().to_owned();
                if op == "assembly_create" {
                    for key in ["name", "joints"] {
                        ui.horizontal(|ui| { ui.label(key); let mut text = request[key].as_str().unwrap_or_default().to_owned();
                            if ui.text_edit_singleline(&mut text).changed() { request[key] = json!(text); } });
                    }
                    return;
                }
                let assemblies: Vec<_> = objects(&baseline).iter().filter(|o| o["type"] == "Assembly::AssemblyObject").cloned().collect();
                if op == "assembly_insert" || (op == "assembly_joint" && request["create"] == true) {
                    choose(ui, "Assembly", &mut request["assembly"], &assemblies);
                } else { ui.label(format!("Assembly: {}", request["assembly"].as_str().unwrap_or_default())); }
                let instances: Vec<_> = objects(&baseline).iter().filter(|o| o["type"] == "App::Link"
                    && o["assembly"] == request["assembly"]).cloned().collect();
                let external = request.get("external_source").is_some();
                if external {
                    ui.label("External FCStd source is read only. Save only the root assembly.");
                    let mut path = draft.parameters["source_path"].as_str().unwrap_or_default().to_owned();
                    ui.horizontal(|ui| {
                        ui.label("Source filepath");
                        if ui.text_edit_singleline(&mut path).changed() {
                            draft.parameters["source_path"] = json!(path);
                            draft.parameters["source_catalog"] = Value::Null;
                            request["external_source"] = Value::Null;
                        }
                        if ui.button("Browse...").clicked() { browse = Some(path.clone()); }
                        if ui.button("Load source").clicked() { load = Some(path.clone()); }
                    });
                    let choices = draft.parameters["source_catalog"]["objects"].as_array().cloned().unwrap_or_default();
                    let mut selected = request["external_source"]["object"].clone();
                    choose(ui, "Native source object", &mut selected, &choices);
                    if let Some(choice) = choices.iter().find(|o| o["name"] == selected) {
                        request["external_source"] = choice["source"].clone();
                    }
                    if request["external_source"].is_object() {
                        ui.label(format!("Native UUID {} / object ID {}", request["external_source"]["document_uid"].as_str().unwrap_or_default(), request["external_source"]["object_id"]));
                        ui.weak(format!("Accepted file revision {}", request["external_source"]["sha256"].as_str().unwrap_or_default()));
                    }
                    if op == "assembly_relink" {
                        ui.label("Update every instance of this exact source in the root. UUID and object ID must match.");
                        ui.weak("Changed source already loaded at the same path requires a new revision path. Missing-source Open requires restoring the accepted file first.");
                        let mut solve = request["solve_joints"] == true;
                        ui.add_enabled(false, egui::Checkbox::new(&mut solve, "Solve joints after updating native links"));
                        ui.weak("Pinned native Assembly recompute solves automatically. Updating links without solving remains unavailable.");
                    }
                }
                if op == "assembly_insert" {
                    let sources: Vec<_> = objects(&baseline).iter().filter(|o| o["type"] == "PartDesign::Body"
                        || o["type"].as_str().is_some_and(|t| t.starts_with("Part::"))
                        || o["instance_state"]["local"] == true).cloned().collect();
                    if !external { choose(ui, "Component source", &mut request["source"], &sources); }
                    let mut name = request["name"].as_str().unwrap_or_default().to_owned();
                    ui.horizontal(|ui| { ui.label("Instance name"); if ui.text_edit_singleline(&mut name).changed() { request["name"] = json!(name); } });
                    let meta = objects(&baseline).iter().find(|o| o["name"] == request["object"])
                        .map(|o| &o["property_metadata"]["Placement"]["quantity"]).unwrap_or(&Value::Null);
                    placement_fields(ui, "Instance placement", &mut request["placement"], meta, &baseline["units"]["defaults"]);
                    if !external { ui.weak("Links reuse local editable source geometry. Hide the source in Features when needed."); }
                } else if op == "assembly_placement" {
                    ui.label(request["object"].as_str().unwrap_or_default());
                    let meta = objects(&baseline).iter().find(|o| o["name"] == request["object"])
                        .map(|o| &o["property_metadata"]["Placement"]["quantity"]).unwrap_or(&Value::Null);
                    placement_fields(ui, "Instance placement", &mut request["placement"], meta, &baseline["units"]["defaults"]);
                    ui.weak("Native constraints determine the final solved placement.");
                } else if op == "assembly_ground" {
                    ui.label(request["object"].as_str().unwrap_or_default());
                    let mut grounded = request["grounded"] == true;
                    if ui.checkbox(&mut grounded, "Ground this component").changed() { request["grounded"] = json!(grounded); }
                } else if op == "assembly_remove" {
                    ui.label(format!("Remove {}", request["object"].as_str().unwrap_or_default()));
                    ui.weak("Remove its dependent native joints and ground first. Undo restores accepted removal.");
                } else if op == "assembly_joint" {
                    let mut kind = request["joint_type"].as_str().unwrap_or("Fixed").to_owned();
                    egui::ComboBox::from_id_salt("joint_type").selected_text(&kind).show_ui(ui, |ui| {
                        for t in assembly_commands::JOINT_TYPES { ui.selectable_value(&mut kind, (*t).into(), *t); }
                    });
                    request["joint_type"] = json!(kind);
                    for (index, key) in ["reference1", "reference2"].iter().enumerate() {
                        choose(ui, &format!("Component {}", index + 1), &mut request[*key]["link"], &instances);
                        ui.push_id(*key, |ui| {
                            if ui.button("Use selected face / edge / vertex").clicked()
                                && let Some(selection) = &self.viewport.selected
                                && instances.iter().any(|o| o["name"] == selection.object) {
                                request[*key] = json!({"link":selection.object,"subelements":[selection.subelement,selection.subelement]});
                            }
                            if ui.button("Use component origin").clicked() { request[*key]["subelements"] = json!(["",""]); }
                            for (i, label) in ["Element", "Vertex or element center"].iter().enumerate() {
                                let mut text = request[*key]["subelements"][i].as_str().unwrap_or_default().to_owned();
                                ui.horizontal(|ui| { ui.label(*label); if ui.text_edit_singleline(&mut text).changed() { request[*key]["subelements"][i] = json!(text); } });
                            }
                        });
                    }
                    ui.weak("A face or edge repeated in both fields chooses its native center. Choose a native vertex for a corner connector.");
                    let existing = objects(&baseline).iter().find(|o| o["name"] == request["name"]);
                    ui.collapsing("Joint values, offsets and limits", |ui| {
                        for prop in ["Distance", "Distance2", "Angle", "LengthMin", "LengthMax", "AngleMin", "AngleMax",
                                     "EnableLengthMin", "EnableLengthMax", "EnableAngleMin", "EnableAngleMax", "Suppressed", "Offset1", "Offset2"] {
                            ui.push_id(prop, |ui| {
                                let active = request["properties"].get(prop).is_some();
                                let mut enabled = active;
                                if ui.checkbox(&mut enabled, prop).changed() {
                                    if enabled {
                                        request["properties"][prop] = existing.and_then(|o| o["properties"].get(prop)).cloned()
                                            .unwrap_or_else(|| if prop.starts_with("Offset") {identity_placement()} else if prop.starts_with("Enable") || prop == "Suppressed" {json!(false)} else {json!(0.)});
                                    } else { discard_joint_override(&mut request, prop); }
                                }
                                if !enabled { return; }
                                if prop.starts_with("Offset") { placement_fields(ui, prop, &mut request["properties"][prop], &existing.map(|o| o["property_metadata"][prop]["quantity"].clone()).unwrap_or_default(), &baseline["units"]["defaults"]); }
                                else if let Some(mut value) = request["properties"][prop].as_bool() {
                                    if ui.checkbox(&mut value, "Enabled").changed() { request["properties"][prop] = json!(value); }
                                } else {
                                    let unit = if prop.contains("Angle") { "deg" } else { "mm" };
                                    let meta = existing.map(|o| &o["property_metadata"][prop]["quantity"])
                                        .filter(|m| !m.is_null()).unwrap_or(&baseline["units"]["defaults"][unit]);
                                    super::units::quantity_field(ui, &mut request["properties"][prop], meta);
                                    let mut expression = request["expressions"].get(prop)
                                        .or_else(|| existing.and_then(|o| o["expressions"].get(prop)))
                                        .and_then(Value::as_str).unwrap_or_default().to_owned();
                                    ui.horizontal(|ui| { ui.label("Expression"); if ui.text_edit_singleline(&mut expression).changed() {
                                        request["expressions"][prop] = if expression.is_empty() {Value::Null} else {json!(expression)};
                                    } });
                                }
                            });
                        }
                    });
                }
                ui.weak("Apply validates native links and assembly solving. Nested assembly authoring remains open.");
            });
        });
        if draft.parameters["request"] != request {
            draft.parameters["request"] = request;
            draft.changed = Some(Instant::now());
            draft.valid = false;
        }
        if let Some(path) = browse {
            self.browse_file(super::file_browser::Purpose::AssemblySource, &path);
        }
        if let Some(path) = load {
            self.assembly_source_path(path);
        }
    }

    pub(super) fn assembly_source_path(&mut self, path: String) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        if draft.id != "native.assembly" {
            return;
        }
        draft.parameters["source_path"] = json!(path);
        draft.parameters["source_catalog"] = Value::Null;
        draft.parameters["request"]["external_source"] = Value::Null;
        draft.valid = false;
        self.send("assembly_sources", json!({"path":path}));
    }

    pub(super) fn assembly_source_reply(&mut self, data: Value) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        if draft.id != "native.assembly" {
            return;
        }
        let request = &draft.parameters["request"];
        let old = objects(&draft.parameters["state"])
            .iter()
            .find(|o| o["name"] == request["object"])
            .map(|o| o["instance_state"]["external_source"].clone())
            .unwrap_or_default();
        let chosen = data["objects"]
            .as_array()
            .and_then(|choices| {
                choices.iter().find(|o| {
                    request["op"] != "assembly_relink"
                        || ["document_uid", "object", "object_id", "type"]
                            .iter()
                            .all(|key| o["source"][key] == old[key])
                })
            })
            .map(|o| o["source"].clone())
            .unwrap_or_default();
        draft.parameters["request"]["external_source"] = chosen;
        draft.parameters["source_catalog"] = data;
        draft.changed = Some(Instant::now());
    }

    pub(super) fn assembly_tree(&mut self, ui: &mut egui::Ui) {
        let roots: Vec<_> = objects(&self.state)
            .iter()
            .filter(|o| o["type"] == "Assembly::AssemblyObject")
            .cloned()
            .collect();
        for root in roots {
            self.assembly_tree_row(ui, &root);
            let members: Vec<_> = root["assembly_state"]["members"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|n| objects(&self.state).iter().find(|o| o["name"] == *n))
                .cloned()
                .collect();
            ui.indent(root["name"].as_str().unwrap_or_default(), |ui| {
                for member in members {
                    self.assembly_tree_row(ui, &member);
                    if member["type"] == "Assembly::JointGroup" {
                        let joints: Vec<_> = root["assembly_state"]["joints"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|n| objects(&self.state).iter().find(|o| o["name"] == *n))
                            .cloned()
                            .collect();
                        ui.indent(member["name"].as_str().unwrap_or_default(), |ui| {
                            for joint in joints {
                                self.assembly_tree_row(ui, &joint);
                            }
                        });
                    }
                }
            });
        }
    }

    fn assembly_tree_row(&mut self, ui: &mut egui::Ui, obj: &Value) {
        let name = obj["name"].as_str().unwrap_or_default();
        let suffix = if obj["instance_state"]["grounded"] == true {
            " | grounded"
        } else {
            ""
        };
        let response = ui.selectable_label(
            self.selected_object.as_deref() == Some(name),
            format!("{}{suffix}", label(obj)),
        );
        if response.clicked() && !self.busy {
            self.selected_object = Some(name.into());
            self.viewport.clear_selection();
        }
        if response.double_clicked() && !self.busy && self.draft.is_none() {
            self.edit_assembly_object(name);
        }
    }
}

#[cfg(test)]
mod tests;
