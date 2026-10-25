//! Measurement interaction never owns or cancels a native modeling gesture.
use super::*;
use crate::measurement_types::{Measurement, Reference, SketchPick};

#[derive(Default)]
pub(super) struct MeasurementUi {
    open: bool,
    attempted: Option<Value>,
    pending: Option<Value>,
    result: Option<Measurement>,
    error: Option<String>,
}

fn selectable(ui: &mut egui::Ui, label: &str, text: &str, exact: impl std::fmt::Debug) {
    ui.horizontal(|ui| {
        ui.weak(label);
        ui.add(egui::Label::new(text).selectable(true))
            .on_hover_text(format!("{exact:?}"));
    });
}

impl Editor {
    fn measurement_references(&self) -> Vec<Reference> {
        if let Some(object) = &self.sketch
            && !self.viewport.sketch_selection.is_empty()
        {
            return self
                .viewport
                .sketch_selection
                .iter()
                .map(|pick| Reference {
                    object: object.clone(),
                    subelement: String::new(),
                    sketch_geometry: Some(SketchPick {
                        index: pick.index,
                        point: pick.point,
                    }),
                })
                .collect();
        }
        self.tree_selections()
            .iter()
            .map(|name| Reference {
                object: name.clone(),
                subelement: String::new(),
                sketch_geometry: None,
            })
            .chain(self.viewport.selections.iter().map(|pick| Reference {
                object: pick.object.clone(),
                subelement: pick.subelement.clone(),
                sketch_geometry: None,
            }))
            .collect()
    }

    pub(super) fn measurement_keeps_selection(&self, additive: bool) -> bool {
        self.measurement_ui.open && additive
    }

    pub(super) fn measurement_invalidate(&mut self) {
        self.measurement_ui.result = None;
        self.measurement_ui.error = None;
        self.measurement_ui.attempted = None;
    }

    fn measurement_key(&self) -> Value {
        json!({"document":self.state["document"],"expected_revision":self.state["revision"],
            "selections":self.measurement_references()})
    }

    fn measurement_available(&self) -> bool {
        !self.unavailable()
            && self.state["document"].is_string()
            && self.pending_file.is_none()
            && self.file_dialog.is_none()
            && !self.overwrite
            && !self.exchange_ui.active()
            && !self.file_browser.active()
            && self.sketch_creation.is_none()
            && self.sketch_ui.drag.is_none()
            && self.sketch_ui.dimension.is_none()
            && self.sketch_ui.task.is_none()
            && !self.sketch_ui.external
            && !self.sketch_ui.trim
            && !self.sketch_ui.split
            && (self.sketch_ui.session.is_none() || self.sketch_ui.tool == Draw::Select)
    }

    pub(super) fn measurement_controls(&mut self, ui: &mut egui::Ui) {
        if ui
            .add_enabled(self.measurement_available(), egui::Button::new("Measure"))
            .on_hover_text("Inspect native geometry | Ctrl+M")
            .clicked()
        {
            self.measurement_ui.open = true;
        }
    }

    pub(super) fn measurement_escape(&mut self) -> bool {
        if !self.measurement_ui.open {
            return false;
        }
        self.measurement_ui = MeasurementUi::default();
        true
    }

    pub(super) fn measurement_reply(&mut self, reply: Reply) {
        let key = self.measurement_ui.pending.take();
        if !self.measurement_ui.open || key.as_ref() != Some(&self.measurement_key()) {
            // A discarded request has neither a current result nor an error to
            // cache. Release only its own attempt so revisiting that selection
            // can queue a fresh read. Current native errors stay cached.
            if key.is_some() && self.measurement_ui.attempted.as_ref() == key.as_ref() {
                self.measurement_ui.attempted = None;
            }
            return;
        }
        match if reply.accepted {
            serde_json::from_value::<Measurement>(reply.data).map_err(|e| e.to_string())
        } else {
            Err(reply
                .error
                .unwrap_or_else(|| "Native measurement failed".into()))
        } {
            Ok(result)
                if json!(result.document) == self.state["document"]
                    && json!(result.revision) == self.state["revision"]
                    && result
                        .selections
                        .iter()
                        .map(|s| s.reference.clone())
                        .collect::<Vec<_>>()
                        == self.measurement_references() =>
            {
                self.measurement_ui.result = Some(result);
                self.measurement_ui.error = None;
            }
            Ok(_) => {
                self.measurement_ui.error = Some("Measurement snapshot changed; recompute".into())
            }
            Err(error) => self.measurement_ui.error = Some(error),
        }
    }

    pub(super) fn measurement_pump(&mut self) {
        if !self.measurement_ui.open {
            return;
        }
        let key = self.measurement_key();
        if self.measurement_ui.attempted.as_ref() != Some(&key) {
            self.measurement_ui.result = None;
            self.measurement_ui.error = None;
        } else {
            return;
        }
        if self.busy || !self.measurement_available() {
            return;
        }
        self.measurement_ui.attempted = Some(key.clone());
        match crate::measurement_commands::request(&self.state, &self.measurement_references()) {
            Ok(parameters) => {
                self.measurement_ui.pending = Some(key);
                // A read must preserve an existing modeling error and all gesture
                // state. Its reply bypasses snapshot/display and dirty handling.
                if self
                    .jobs
                    .try_send(Job {
                        op: "measure".into(),
                        parameters,
                    })
                    .is_ok()
                {
                    self.busy = true;
                } else {
                    self.measurement_ui.pending = None;
                    self.measurement_ui.error = Some("Model thread stopped".into());
                }
            }
            Err(error) => self.measurement_ui.error = Some(error),
        }
    }

    pub(super) fn measurement_ui(&mut self, ctx: &egui::Context) {
        if self.measurement_available()
            && !ctx.wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::M))
        {
            self.measurement_ui.open = true;
        }
        // Invalidate before drawing, including during a pending model mutation.
        self.measurement_pump();
        if !self.measurement_ui.open {
            return;
        }
        let mut open = true;
        let mut clear = false;
        let mut recompute = false;
        egui::Window::new("Measure / Inspect")
            .id(egui::Id::new("measure"))
            .open(&mut open)
            .default_pos([850., 90.])
            .default_width(340.)
            .default_height(360.)
            .vscroll(true)
            .show(ctx, |ui| {
                ui.label("Select one or two native elements. Shift-click adds or removes.");
                ui.horizontal(|ui| {
                    clear = ui.button("Clear selection").clicked();
                    recompute = ui
                        .add_enabled(!self.busy, egui::Button::new("Recompute"))
                        .clicked();
                });
                if self.measurement_ui.pending.is_some() {
                    ui.weak("Measuring native geometry...");
                }
                if let Some(error) = &self.measurement_ui.error {
                    ui.colored_label(Color32::from_rgb(225, 142, 138), error);
                }
                if let Some(result) = &self.measurement_ui.result {
                    ui.weak(format!("{} | world coordinates", result.unit_system));
                    if let Some(distance) = &result.distance {
                        selectable(
                            ui,
                            &distance.metric.label,
                            &distance.metric.display,
                            (distance.metric.value, &distance.metric.unit),
                        );
                        for (index, pair) in distance.witnesses.iter().enumerate() {
                            ui.collapsing(format!("Witness pair {}", index + 1), |ui| {
                                selectable(ui, "First", &pair[0].display, pair[0].value);
                                selectable(ui, "Second", &pair[1].display, pair[1].value);
                            });
                        }
                    }
                    if let Some(angle) = &result.angle {
                        selectable(ui, &angle.label, &angle.display, (angle.value, &angle.unit));
                    }
                    if let Some(reason) = &result.angle_unavailable {
                        ui.weak(reason);
                    }
                    for selection in &result.selections {
                        ui.separator();
                        let r = &selection.reference;
                        let element = r
                            .sketch_geometry
                            .as_ref()
                            .map(|pick| format!("geometry {} point {}", pick.index, pick.point))
                            .unwrap_or_else(|| r.subelement.clone());
                        ui.label(format!(
                            "{} {} | {}",
                            r.object, element, selection.shape_type
                        ));
                        if let Some(kind) = &selection.geometry_type {
                            ui.weak(kind);
                        }
                        for metric in &selection.metrics {
                            selectable(
                                ui,
                                &metric.label,
                                &metric.display,
                                (metric.value, &metric.unit),
                            );
                        }
                        selectable(
                            ui,
                            if selection.shape_type == "Vertex" {
                                "Position"
                            } else {
                                "Center of gravity"
                            },
                            &selection.center.display,
                            selection.center.value,
                        );
                        if let Some(center) = &selection.geometry_center {
                            selectable(ui, "Geometry center", &center.display, center.value);
                        }
                        for (label, value) in [
                            ("Axis", selection.axis),
                            ("Direction / normal", selection.direction),
                        ] {
                            if let Some(value) = value {
                                selectable(
                                    ui,
                                    label,
                                    &format!("{:.6}, {:.6}, {:.6}", value[0], value[1], value[2]),
                                    value,
                                );
                            }
                        }
                        ui.collapsing("Native properties", |ui| {
                            for property in &selection.properties {
                                selectable(ui, &property.label, &property.value, &property.value);
                            }
                        });
                    }
                }
            });
        if !open {
            self.measurement_ui = MeasurementUi::default();
        }
        if clear {
            self.viewport.clear_selection();
            self.viewport.clear_sketch_selection();
            self.body_ui.selected.clear();
            self.selected_object = None;
            self.measurement_ui.attempted = None;
            self.measurement_ui.result = None;
        }
        if recompute {
            self.measurement_ui.attempted = None;
            self.measurement_ui.result = None;
        }
        self.measurement_pump();
    }
}

#[cfg(test)]
mod tests;
