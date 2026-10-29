use super::Editor;
use eframe::egui;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct UnitsUi {
    selection: Option<u64>,
    document: Value,
    revision: Value,
    workspace: bool,
    pub(super) workspace_input_blocked: bool,
    pub(super) fields: BTreeMap<String, Value>,
    pending: Option<(bool, Vec<String>, bool)>,
}

fn workspace_unit_name(unit: &str) -> &str {
    match unit {
        "mm" => "Millimeter",
        "in" => "Inch",
        "kg" => "Kilogram",
        "lb" => "Pound",
        "deg" | "\u{b0}" | "\u{c2}\u{b0}" => "Degree",
        other => other,
    }
}

// These two native formats supply the decimal length choices. Their indices
// and all physical values come from the current native enumeration/metadata.
fn workspace_length_choices(state: &Value) -> Vec<&Value> {
    state["units"]["choices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|choice| {
            let label = choice["label"].as_str().unwrap_or_default();
            let unit = choice["workspace_defaults"]["length"]["default_unit"].as_str();
            choice["index"].as_u64().is_some()
                && (label.starts_with("Standard (") && unit == Some("mm")
                    || label.starts_with("Imperial decimal (") && unit == Some("in"))
        })
        .collect()
}

/// Keep the original numeric value until a human actually changes this field.
/// A changed input carries native default-unit context to the native parser.
pub(super) fn quantity_field(ui: &mut egui::Ui, value: &mut Value, metadata: &Value) -> bool {
    // Query base_value follows the requested unit; value uses native storage.
    let matched = value == &metadata["source"]
        || value == &metadata["value"]
        || value == &metadata["base_value"];
    let mut text = value["quantity_input"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            matched
                .then(|| metadata["input"].as_str().map(str::to_owned))
                .flatten()
        })
        .unwrap_or_else(|| {
            value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string())
        });
    let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(140.));
    if let Some(unit) = metadata["default_unit"].as_str().filter(|s| !s.is_empty()) {
        ui.weak(unit);
    }
    if response.changed() {
        *value = crate::document_units::input(&text, metadata);
        true
    } else {
        false
    }
}

impl UnitsUi {
    pub(super) fn active(&self) -> bool {
        self.selection.is_some()
    }
    pub(super) fn quantity_failed(&mut self) {
        self.pending = None;
    }
}

impl Editor {
    fn open_units_dialog(&mut self, workspace: bool) {
        self.units_ui.selection = self.state["units"]["index"].as_u64();
        self.units_ui.document = self.state["document"].clone();
        self.units_ui.revision = self.state["revision"].clone();
        self.units_ui.workspace = workspace;
    }

    pub(super) fn workspace_units_controls(&mut self, ui: &mut egui::Ui) {
        let available = self.units_available()
            && !self.units_ui.active()
            && !self.units_ui.workspace_input_blocked
            && !self.measurement_is_open()
            && !super::workflow_guide::active(ui.ctx());
        ui.add_enabled_ui(available, |ui| {
            ui.menu_button("Document", |ui| {
                if ui.button("Workspace units...").clicked() {
                    self.open_units_dialog(true);
                    ui.close();
                }
            });
        });
    }

    pub(super) fn quantity_reply(&mut self, data: &Value) {
        if let Some((package, paths, resume)) = self.units_ui.pending.take() {
            for (i, path) in paths.iter().enumerate() {
                self.units_ui
                    .fields
                    .insert(path.clone(), data["items"][i].clone());
                if package
                    && resume
                    && let Some(draft) = &mut self.draft
                {
                    draft.parameters[path] = data["items"][i]["base_value"].clone();
                }
            }
            if package && resume {
                self.preview();
            }
            return;
        }
        if let Some(edit) = &mut self.sketch_ui.dimension
            && let Some(text) = data["items"][0]["input"].as_str()
        {
            edit.value = text.to_owned();
            edit.constraint = Some(json!({"quantity":data["items"][0]}));
        }
    }

    pub(super) fn quantity_request(
        &mut self,
        package: bool,
        fields: Vec<(String, Value, String)>,
        resume: bool,
    ) {
        if fields.is_empty() {
            return;
        }
        let paths: Vec<_> = fields.iter().map(|(path, _, _)| path.clone()).collect();
        let items: Vec<_> = fields
            .iter()
            .map(|(_, value, unit)| {
                if let Some(text) = value["quantity_input"].as_str() {
                    json!({"unit":unit,"text":text,"default_unit":value["default_unit"]})
                } else if let Some(text) = value.as_str() {
                    json!({"unit":unit,"text":text})
                } else {
                    json!({"unit":unit,"value":value})
                }
            })
            .collect();
        self.units_ui.pending = Some((package, paths, resume));
        self.send("quantities", json!({"items":items}));
    }

    pub(super) fn package_quantities(&mut self, resume: bool) -> bool {
        let Some(draft) = &self.draft else {
            return false;
        };
        let Some(package) = self.registry.get(&draft.id) else {
            return false;
        };
        let fields: Vec<_> = package
            .parameters
            .iter()
            .filter_map(|(path, p)| {
                let unit = p.unit.as_ref()?;
                let value = &draft.parameters[path];
                if resume && value["quantity_input"].is_null() {
                    return None;
                }
                Some((path.clone(), value.clone(), unit.clone()))
            })
            .collect();
        if fields.is_empty() {
            return false;
        }
        self.quantity_request(true, fields, resume);
        true
    }

    fn units_available(&self) -> bool {
        !self.busy
            && !self.unavailable()
            && self.state["document"].is_string()
            && self.draft.is_none()
            && self.sketch_ui.session.is_none()
            && self.sketch_ui.drag.is_none()
            && self.sketch_ui.dimension.is_none()
            && self.sketch_ui.task.is_none()
            && self.sketch_creation.is_none()
            && self.pending_file.is_none()
            && self.file_dialog.is_none()
            && !self.file_browser.active()
            && !self.exchange_ui.active()
    }

    pub(super) fn units_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(self.units_available(), |ui| {
            ui.menu_button("Units", |ui| {
                ui.label(
                    self.state["units"]["label"]
                        .as_str()
                        .unwrap_or("Native document units"),
                );
                if ui.button("Document units...").clicked() {
                    self.open_units_dialog(false);
                    ui.close();
                }
            });
        });
    }

    pub(super) fn units_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut selected) = self.units_ui.selection else {
            return;
        };
        if self.units_ui.document != self.state["document"] {
            self.units_ui = UnitsUi::default();
            return;
        }
        let mut apply = false;
        let mut cancel = false;
        let workspace = self.units_ui.workspace;
        let popup_was_open = egui::Popup::is_any_open(ctx);
        egui::Modal::new(egui::Id::new("document_units")).show(ctx, |ui| {
            ui.heading(if workspace {
                "Workspace units"
            } else {
                "Document units"
            });
            if workspace {
                ui.label("Saved with this document.");
                ui.weak("Length, angle and mass use the selected unit system.");
            } else {
                ui.label("Saved with this native document.");
                ui.weak("Apply changes the setting directly. Native Undo/Redo leaves it in place.");
            }
            ui.add_enabled_ui(!self.busy, |ui| {
                if workspace {
                    let choices = workspace_length_choices(&self.state);
                    let selected_unit = self.state["units"]["choices"]
                        .as_array()
                        .and_then(|choices| {
                            choices
                                .iter()
                                .find(|choice| choice["index"].as_u64() == Some(selected))
                        })
                        .and_then(|choice| {
                            choice["workspace_defaults"]["length"]["default_unit"].as_str()
                        })
                        .unwrap_or("Unavailable");
                    ui.horizontal(|ui| {
                        ui.label("Default length unit");
                        egui::ComboBox::from_id_salt("workspace_length_unit")
                            .selected_text(workspace_unit_name(selected_unit))
                            .show_ui(ui, |ui| {
                                for choice in &choices {
                                    ui.selectable_value(
                                        &mut selected,
                                        choice["index"].as_u64().unwrap(),
                                        workspace_unit_name(
                                            choice["workspace_defaults"]["length"]["default_unit"]
                                                .as_str()
                                                .unwrap(),
                                        ),
                                    );
                                }
                            });
                    });
                    if let Some(choice) =
                        self.state["units"]["choices"]
                            .as_array()
                            .and_then(|choices| {
                                choices
                                    .iter()
                                    .find(|choice| choice["index"].as_u64() == Some(selected))
                            })
                    {
                        for (label, kind) in [
                            ("Default angle unit", "angle"),
                            ("Default mass unit", "mass"),
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(label);
                                ui.label(workspace_unit_name(
                                    choice["workspace_defaults"][kind]["default_unit"]
                                        .as_str()
                                        .unwrap_or("Unavailable"),
                                ));
                            });
                        }
                        ui.weak(choice["label"].as_str().unwrap_or_default());
                    }
                    ui.horizontal(|ui| {
                        apply = ui
                            .add_enabled(!choices.is_empty(), egui::Button::new("\u{2713}"))
                            .on_hover_text("Accept workspace units")
                            .clicked();
                        cancel = ui.button("Cancel workspace units").clicked();
                    });
                } else {
                    for choice in self.state["units"]["choices"]
                        .as_array()
                        .into_iter()
                        .flatten()
                    {
                        if let (Some(index), Some(label)) =
                            (choice["index"].as_u64(), choice["label"].as_str())
                        {
                            ui.radio_value(&mut selected, index, label);
                        }
                    }
                    ui.horizontal(|ui| {
                        apply = ui.button("Apply units").clicked();
                        cancel = ui.button("Cancel units").clicked();
                    });
                }
            });
        });
        self.units_ui.selection = Some(selected);
        let escape = !self.busy
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel || escape && !popup_was_open {
            self.units_ui = UnitsUi::default();
        } else if apply && self.units_available() {
            if self.units_ui.revision != self.state["revision"] {
                self.error = Some(
                    if workspace {
                        "Document changed. Close and reopen Workspace units."
                    } else {
                        "Document changed. Close and reopen Document units."
                    }
                    .into(),
                );
                return;
            }
            match crate::document_units::settings(&self.state, selected) {
                Ok(_) => self.send("document_settings", json!({"unit_system":selected})),
                Err(error) => self.error = Some(error),
            }
        }
    }
}

#[cfg(test)]
mod tests;
