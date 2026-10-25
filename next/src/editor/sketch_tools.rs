//! Sketch task forms compile one native preview against a captured baseline.
use super::{Editor, objects};
use crate::{
    sketch_commands::{SketchElement, Tool},
    sketch_operation_commands,
};
use eframe::egui;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

pub(super) struct SketchTask {
    kind: String,
    baseline: Value,
    selection: Vec<SketchElement>,
    parameters: Value,
    token: String,
    changed: Option<Instant>,
    valid: bool,
    deferred: bool,
    finishing: bool,
    committing: bool,
    control_choices: Option<Value>,
    exposing_controls: bool,
}
fn control_choices(snapshot: &Value, selection: &[SketchElement]) -> Option<Value> {
    let index = selection.first()?.index;
    let count = curve(snapshot, index)?["poles"].as_array()?.len();
    let controls = snapshot["sketch"]["spline_controls"].as_array()?;
    if count == 0
        || !(0..count).all(|pole| {
            controls.iter().any(|m| {
                m["spline_index"].as_i64() == Some(index as i64)
                    && m["pole_index"].as_u64() == Some(pole as u64)
            })
        })
    {
        return None;
    }
    Some(json!({"spline_controls":controls,"constraints":snapshot["sketch"]["constraints"]}))
}
fn weight_equal_choices<'a>(choices: &'a Value, index: i32, pole: &Value) -> Vec<&'a Value> {
    let control = choices["spline_controls"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["spline_index"].as_i64() == Some(index as i64) && m["pole_index"] == *pole)
        .and_then(|m| m["control_index"].as_i64());
    choices["constraints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| {
            c["type"] == "Equal"
                && control.is_some_and(|id| {
                    c["first"].as_i64() == Some(id) || c["second"].as_i64() == Some(id)
                })
        })
        .collect()
}
impl SketchTask {
    fn reply(
        &mut self,
        op: &str,
        success: bool,
        preview_active: bool,
        current: Option<&Value>,
    ) -> bool {
        if op == "preview" {
            if self.exposing_controls && success {
                self.control_choices = current.and_then(|s| control_choices(s, &self.selection));
                self.exposing_controls = self.control_choices.is_none();
                self.valid = false;
                if !self.exposing_controls {
                    self.changed = Some(Instant::now());
                }
            } else {
                self.valid = success;
            }
        }
        let closed = (self.finishing
            && matches!(op, "preview_commit" | "preview_cancel")
            && !preview_active)
            || self.committing && op == "transaction" && success;
        if self.committing && op == "transaction" && !success {
            self.committing = false;
            self.valid = false;
        }
        if self.finishing && matches!(op, "preview_commit" | "preview_cancel") && !closed {
            self.finishing = false;
            self.valid = false;
        }
        closed
    }
}
fn curve(snapshot: &Value, index: i32) -> Option<&Value> {
    snapshot["sketch"]["geometry"]
        .as_array()?
        .iter()
        .find(|g| g["index"].as_i64() == Some(index as i64))
}
fn label(kind: &str) -> &str {
    match kind {
        "fillet" => "Sketch fillet",
        "chamfer" => "Sketch chamfer",
        "extend" => "Extend curve",
        "carbon_copy" => "Carbon Copy",
        "mirror" => "Mirror",
        "spline_degree" => "Spline degree",
        "spline_knot" => "Spline knots",
        "spline_pole" => "Spline pole position",
        "spline_weight" => "Spline pole weight",
        _ => kind,
    }
}
fn defaults(kind: &str, snapshot: &Value, selection: &[SketchElement]) -> Value {
    let geo = selection
        .first()
        .and_then(|s| curve(snapshot, s.index))
        .cloned()
        .unwrap_or_default();
    match kind {
        "fillet" | "chamfer" => json!({"radius":"1 mm","trim":true,"preserve_corner":true}),
        "extend" => json!({"increment":if geo["kind"]=="arc" {"10 deg"}else{"1 mm"}}),
        "carbon_copy" => json!({"source":"","construction":false}),
        "mirror" => {
            json!({"reference":{"index":-1,"point":0},"delete_original":false,"create_symmetry_constraints":false})
        }
        "spline_degree" => json!({"change":1}),
        "spline_knot" => {
            json!({"parameter":(geo["first_parameter"].as_f64().unwrap_or(0.)+geo["last_parameter"].as_f64().unwrap_or(1.))*0.5,"multiplicity":1})
        }
        "spline_pole" => json!({"pole":0,"position":geo["poles"][0]}),
        "spline_weight" => json!({"pole":0,"value":geo["weights"][0],"release_constraints":[]}),
        _ => json!({}),
    }
}

impl Editor {
    pub(super) fn sketch_task_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Edit tools", |ui| {
            if ui
                .selectable_label(self.sketch_ui.split, "Split curve")
                .clicked()
            {
                self.sketch_ui.split = !self.sketch_ui.split;
                self.sketch_ui.trim = false;
                self.sketch_ui.external = false;
                self.sketch_ui.tool = Tool::Select;
                self.sketch_ui.clear_points();
                self.viewport.clear_sketch_selection();
                ui.close();
            }
            for (kind, name) in [
                ("fillet", "Fillet"),
                ("chamfer", "Chamfer"),
                ("extend", "Extend"),
                ("carbon_copy", "Carbon Copy"),
                ("mirror", "Mirror"),
                ("spline_degree", "Spline degree"),
                ("spline_knot", "Spline knots"),
                ("spline_pole", "Spline pole position"),
                ("spline_weight", "Spline pole weight"),
            ] {
                if ui.button(name).clicked() {
                    self.begin_sketch_task(kind, None);
                    ui.close();
                }
            }
        });
    }
    pub(super) fn begin_sketch_task(&mut self, kind: &str, selection: Option<Vec<SketchElement>>) {
        if self.busy
            || self.sketch_ui.session.is_none()
            || self.sketch_ui.drag.is_some()
            || self.sketch_ui.dimension.is_some()
            || self.sketch_ui.task.is_some()
        {
            return;
        }
        let Some(baseline) = self.sketch_snapshot() else {
            return;
        };
        let selection = selection.unwrap_or_else(|| self.viewport.sketch_selection.clone());
        let mut parameters = defaults(kind, &baseline, &selection);
        if matches!(kind, "fillet" | "chamfer") && selection.len() == 2 {
            let positions: Option<Vec<_>> = selection
                .iter()
                .map(|s| {
                    self.viewport
                        .sketch_pick_position(*s)
                        .or_else(|| self.viewport.geometry_point(*s))
                })
                .collect();
            parameters["positions"] = json!(positions.unwrap_or_default());
        }
        if kind == "carbon_copy" {
            parameters["source"] = objects(&self.state)
                .iter()
                .find(|o| o["type"] == "Sketcher::SketchObject" && o["name"] != baseline["name"])
                .map(|o| o["name"].clone())
                .unwrap_or_else(|| json!(""));
            parameters["construction"] = json!(self.sketch_ui.construction);
        }
        if let Err(error) =
            sketch_operation_commands::build(&baseline, kind, &selection, &parameters)
        {
            self.error = Some(error);
            return;
        }
        self.sequence += 1;
        let deferred =
            self.sketch_ui.branch_pending || self.state["redo_count"].as_u64().unwrap_or(0) > 0;
        let control_choices = if kind == "spline_weight" {
            control_choices(&baseline, &selection)
        } else {
            None
        };
        let exposing_controls = kind == "spline_weight" && control_choices.is_none();
        if deferred && exposing_controls {
            self.error = Some("Expose the spline's internal geometry before editing weights while native redo history exists. Expose creates a new undo step; Cancel preserves redo.".into());
            return;
        }
        self.sketch_ui.task = Some(SketchTask {
            kind: kind.into(),
            baseline,
            selection,
            parameters,
            token: format!("sketch-task-{}-{}", std::process::id(), self.sequence),
            changed: Some(Instant::now()),
            valid: false,
            deferred,
            finishing: false,
            committing: false,
            control_choices,
            exposing_controls,
        });
        self.sketch_ui.clear_points();
        self.sketch_ui.tool = Tool::Select;
        self.sketch_ui.trim = false;
        self.sketch_ui.external = false;
        self.sketch_ui.split = false;
        self.sketch_ui.selected_constraint = None;
        self.viewport.clear_sketch_selection();
    }
    pub(super) fn begin_pole_weight(&mut self, control: i32) -> bool {
        let Some(snapshot) = self.sketch_snapshot() else {
            return false;
        };
        let Some(mapping) = snapshot["sketch"]["spline_controls"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|entry| entry["control_index"].as_i64() == Some(control as i64))
        else {
            return false;
        };
        let Some(index) = mapping["spline_index"]
            .as_i64()
            .and_then(|i| i32::try_from(i).ok())
        else {
            return false;
        };
        let pole = mapping["pole_index"].as_u64().unwrap_or(0) as usize;
        self.begin_sketch_task(
            "spline_weight",
            Some(vec![SketchElement { index, point: 0 }]),
        );
        if let Some(task) = &mut self.sketch_ui.task {
            task.parameters["pole"] = json!(pole);
            task.parameters["value"] = curve(&task.baseline, index)
                .map(|g| g["weights"][pole].clone())
                .unwrap_or_else(|| json!(1.));
        }
        true
    }
    pub(super) fn split_sketch_curve(&mut self, hit: SketchElement, position: [f64; 3]) {
        let Some(snapshot) = self.sketch_snapshot() else {
            return;
        };
        match sketch_operation_commands::build(
            &snapshot,
            "split",
            &[SketchElement { point: 0, ..hit }],
            &json!({"position":position}),
        ) {
            Ok(edits) => {
                self.viewport.clear_sketch_selection();
                self.transaction("Split sketch curve", edits);
            }
            Err(error) => self.error = Some(error),
        }
    }
    pub(super) fn sketch_task_reply(&mut self, op: &str, success: bool) {
        let current = self.sketch_snapshot();
        let Some(task) = &mut self.sketch_ui.task else {
            return;
        };
        let closed = task.reply(
            op,
            success,
            !self.state["preview"].is_null(),
            current.as_ref(),
        );
        if closed {
            self.sketch_ui.task = None;
            self.viewport.clear_sketch_selection();
        }
    }
    pub(super) fn finish_sketch_task(&mut self, commit: bool) {
        if self.busy {
            return;
        }
        let Some(task) = &mut self.sketch_ui.task else {
            return;
        };
        if commit && (!task.valid || task.changed.is_some()) {
            return;
        }
        if task.deferred {
            if commit {
                match sketch_operation_commands::build(
                    &task.baseline,
                    &task.kind,
                    &task.selection,
                    &task.parameters,
                ) {
                    Ok(edits) => {
                        task.committing = true;
                        let name = label(&task.kind).to_owned();
                        self.transaction(&name, edits);
                    }
                    Err(error) => self.error = Some(error),
                }
            } else {
                self.sketch_ui.task = None;
            }
            return;
        }
        if self.state["preview"].is_null() {
            if !commit {
                self.sketch_ui.task = None;
            }
            return;
        }
        let token = task.token.clone();
        let unchanged = self.sketch_snapshot().is_some_and(|current| {
            current["sketch"] == self.sketch_ui.task.as_ref().unwrap().baseline["sketch"]
                && current["expressions"]
                    == self.sketch_ui.task.as_ref().unwrap().baseline["expressions"]
        });
        self.sketch_ui.task.as_mut().unwrap().finishing = true;
        self.send(
            if commit && !unchanged {
                "preview_commit"
            } else {
                "preview_cancel"
            },
            json!({"token":token}),
        );
    }
    pub(super) fn sketch_task_pump(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let Some(task) = &mut self.sketch_ui.task else {
            return;
        };
        let Some(changed) = task.changed else {
            return;
        };
        if changed.elapsed() < Duration::from_millis(150) {
            ctx.request_repaint_after(Duration::from_millis(150) - changed.elapsed());
            return;
        }
        task.changed = None;
        let edits = if task.exposing_controls {
            Ok(vec![
                json!({"op":"expose_internal","object":task.baseline["name"],"index":task.selection[0].index}),
            ])
        } else {
            sketch_operation_commands::build(
                &task.baseline,
                &task.kind,
                &task.selection,
                &task.parameters,
            )
        };
        match edits {
            Ok(edits) => {
                if task.deferred {
                    task.valid = true;
                    return;
                }
                let token = task.token.clone();
                let name = label(&task.kind).to_owned();
                self.send("preview", json!({"token":token,"label":name,"edits":edits}));
            }
            Err(error) => {
                task.valid = false;
                self.error = Some(error);
            }
        }
    }
    pub(super) fn sketch_task_panel(&mut self, ctx: &egui::Context) {
        let Some(mut task) = self.sketch_ui.task.take() else {
            return;
        };
        let choices: Vec<_> = objects(&self.state)
            .iter()
            .filter(|o| o["type"] == "Sketcher::SketchObject" && o["name"] != task.baseline["name"])
            .filter_map(|o| o["name"].as_str().map(str::to_owned))
            .collect();
        let mut apply = false;
        let mut cancel = false;
        let mut changed = false;
        egui::Window::new(label(&task.kind))
            .id(egui::Id::new("native_sketch_task"))
            .default_pos([175., 85.])
            .default_width(310.)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::from_rgb(225, 142, 138), error);
                }
                ui.add_enabled_ui(!self.busy && !task.committing && !task.finishing, |ui| {
                    if task.exposing_controls {
                        ui.weak("Reading native spline controls for weight constraints...");
                    } else {
                        changed |= task_fields(ui, &mut task, &choices);
                    }
                    if task.deferred {
                        ui.weak("Apply branches native redo history. Cancel preserves it.");
                    }
                    if ui.button("Recompute preview").clicked() {
                        changed = true;
                    }
                    ui.horizontal(|ui| {
                        apply = ui
                            .add_enabled(
                                task.valid
                                    && !task.exposing_controls
                                    && task.changed.is_none()
                                    && !changed,
                                egui::Button::new("Apply"),
                            )
                            .clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });
            });
        if changed {
            task.changed = Some(Instant::now());
            task.valid = false;
        }
        self.sketch_ui.task = Some(task);
        if apply {
            self.finish_sketch_task(true);
        } else if cancel {
            self.finish_sketch_task(false);
        }
    }
}

fn number(ui: &mut egui::Ui, label: &str, value: &mut Value, integer: bool) -> bool {
    ui.label(label);
    if integer {
        let mut n = value.as_i64().unwrap_or(1);
        if ui.add(egui::DragValue::new(&mut n).speed(0.1)).changed() {
            *value = json!(n);
            return true;
        }
    } else {
        let mut n = value.as_f64().unwrap_or(0.);
        if ui.add(egui::DragValue::new(&mut n).speed(0.05)).changed() {
            *value = json!(n);
            return true;
        }
    }
    false
}
fn text(ui: &mut egui::Ui, label: &str, value: &mut Value) -> bool {
    ui.label(label);
    let mut s = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    if ui.text_edit_singleline(&mut s).changed() {
        *value = json!(s);
        true
    } else {
        false
    }
}
fn point(ui: &mut egui::Ui, value: &mut Value) -> bool {
    let mut changed = false;
    for (axis, name) in ["X (mm)", "Y (mm)"].into_iter().enumerate() {
        changed |= number(ui, name, &mut value[axis], false);
    }
    changed
}
fn task_fields(ui: &mut egui::Ui, task: &mut SketchTask, choices: &[String]) -> bool {
    let mut changed = false;
    let params = &mut task.parameters;
    let index = task.selection.first().map_or(-1, |s| s.index);
    let geo = curve(&task.baseline, index).cloned().unwrap_or_default();
    match task.kind.as_str() {
        "mirror" => {
            ui.label(format!(
                "Source native IDs: {}",
                sketch_operation_commands::mirror_sources(&task.baseline, &task.selection)
                    .unwrap_or_default()
                    .iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            let references = sketch_operation_commands::mirror_references(&task.baseline);
            let selected = references
                .iter()
                .find(|(r, _)| json!(r) == params["reference"])
                .map_or("Choose reference", |(_, name)| name.as_str());
            egui::ComboBox::from_id_salt("mirror_reference")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (reference, name) in &references {
                        if ui
                            .selectable_label(json!(reference) == params["reference"], name)
                            .clicked()
                        {
                            params["reference"] = json!(reference);
                            changed = true;
                        }
                    }
                });
            for (key, other, name) in [
                (
                    "delete_original",
                    "create_symmetry_constraints",
                    "Delete original geometries",
                ),
                (
                    "create_symmetry_constraints",
                    "delete_original",
                    "Create symmetry constraints",
                ),
            ] {
                let mut value = params[key] == true;
                let capability = &task.baseline["sketch"]["task_capabilities"]["mirror"]["create_symmetry_constraints"];
                let available =
                    key != "create_symmetry_constraints" || capability["available"] == true;
                if ui
                    .add_enabled(available, egui::Checkbox::new(&mut value, name))
                    .changed()
                {
                    params[key] = json!(value);
                    if value {
                        params[other] = json!(false);
                    }
                    changed = true;
                }
                if !available {
                    ui.weak(capability["reason"].as_str().unwrap_or(
                        "Constrained Mirror availability is unknown; refresh the native sketch",
                    ));
                }
            }
        }
        "fillet" | "chamfer" => {
            changed |= text(ui, "Radius", &mut params["radius"]);
            if task.selection.len() == 2 {
                for pick in 0..2 {
                    ui.label(format!(
                        "Curve {} pick position",
                        task.selection[pick].index
                    ));
                    changed |= point(ui, &mut params["positions"][pick]);
                }
            }
            for (key, name) in [
                ("trim", "Trim original curves"),
                ("preserve_corner", "Preserve construction corner"),
            ] {
                let mut value = params[key] == true;
                if ui.checkbox(&mut value, name).changed() {
                    params[key] = json!(value);
                    changed = true;
                }
            }
            ui.weak("Constrain the generated arc to drive its radius.");
        }
        "extend" => {
            changed |= text(
                ui,
                if geo["kind"] == "arc" {
                    "Angular extension"
                } else {
                    "Length extension"
                },
                &mut params["increment"],
            );
        }
        "carbon_copy" => {
            ui.label("Source sketch");
            egui::ComboBox::from_id_salt("carbon_copy_source")
                .selected_text(params["source"].as_str().unwrap_or("Choose sketch"))
                .show_ui(ui, |ui| {
                    for name in choices {
                        if ui
                            .selectable_label(params["source"] == *name, name)
                            .clicked()
                        {
                            params["source"] = json!(name);
                            changed = true;
                        }
                    }
                });
            let mut construction = params["construction"] == true;
            if ui
                .checkbox(&mut construction, "Construction geometry")
                .changed()
            {
                params["construction"] = json!(construction);
                changed = true;
            }
        }
        "spline_degree" => {
            changed |= number(ui, "Degree change", &mut params["change"], true);
            ui.weak("Native degree reduction replaces the spline and removes its old constraints.");
        }
        "spline_knot" => {
            let mut inserting = params.get("parameter").is_some();
            if ui.checkbox(&mut inserting, "Insert knot").changed() {
                *params = if inserting {
                    json!({"parameter":(geo["first_parameter"].as_f64().unwrap_or(0.)+geo["last_parameter"].as_f64().unwrap_or(1.))*0.5,"multiplicity":1})
                } else {
                    json!({"knot":1,"change":-1})
                };
                changed = true;
            }
            if inserting {
                changed |= number(ui, "Native parameter", &mut params["parameter"], false);
                changed |= number(ui, "Multiplicity", &mut params["multiplicity"], true);
            } else {
                changed |= number(ui, "Native knot index (1-based)", &mut params["knot"], true);
                changed |= number(ui, "Multiplicity change", &mut params["change"], true);
            }
        }
        "spline_pole" | "spline_weight" => {
            let poles = geo["poles"].as_array().map_or(0, Vec::len);
            let mut pole = params["pole"].as_u64().unwrap_or(0) + 1;
            ui.label("Pole");
            if ui
                .add(egui::DragValue::new(&mut pole).range(1..=poles.max(1) as u64))
                .changed()
            {
                params["pole"] = json!(pole - 1);
                if task.kind == "spline_pole" {
                    params["position"] = geo["poles"][pole as usize - 1].clone();
                } else {
                    params["value"] = geo["weights"][pole as usize - 1].clone();
                    params["release_constraints"] = json!([]);
                }
                changed = true;
            }
            if task.kind == "spline_pole" {
                changed |= point(ui, &mut params["position"]);
            } else {
                changed |= text(ui, "Dimensionless weight", &mut params["value"]);
                let equal = task
                    .control_choices
                    .as_ref()
                    .map(|choices| weight_equal_choices(choices, index, &params["pole"]))
                    .unwrap_or_default();
                if !equal.is_empty() {
                    ui.label("Release selected native Equal constraints");
                }
                for c in equal {
                    let id = c["index"].clone();
                    let mut selected = params["release_constraints"]
                        .as_array()
                        .is_some_and(|list| list.contains(&id));
                    if ui
                        .checkbox(
                            &mut selected,
                            format!("Equal {}: {} / {}", id, c["first"], c["second"]),
                        )
                        .changed()
                    {
                        let releases = params["release_constraints"].as_array_mut().unwrap();
                        if selected {
                            releases.push(id);
                        } else {
                            releases.retain(|i| *i != id);
                        }
                        changed = true;
                    }
                }
            }
        }
        _ => {}
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task() -> SketchTask {
        SketchTask {
            kind: "spline_weight".into(),
            baseline: Value::Null,
            selection: vec![SketchElement { index: 0, point: 0 }],
            parameters: json!({"pole":1,"value":2,"release_constraints":[]}),
            token: "weight-test".into(),
            changed: None,
            valid: false,
            deferred: false,
            finishing: false,
            committing: false,
            control_choices: None,
            exposing_controls: true,
        }
    }
    #[test]
    fn weight_choices_keep_expose_baseline_ids_across_deleted_constraints_and_pole_switches() {
        let native = json!({"sketch":{
            "geometry":[{"index":0,"poles":[[0,0,0],[1,2,0],[3,0,0]]}],
            "spline_controls":[{"spline_index":0,"pole_index":0,"control_index":4},
                {"spline_index":0,"pole_index":1,"control_index":5},
                {"spline_index":0,"pole_index":2,"control_index":6}],
            "constraints":[{"index":10,"type":"Weight","first":4},
                {"index":11,"type":"Equal","first":4,"second":5},
                {"index":12,"type":"Equal","first":4,"second":6}]
        }});
        let mut task = task();
        assert!(!task.reply("preview", true, true, Some(&native)));
        assert!(!task.exposing_controls && !task.valid && task.changed.is_some());
        let mut candidate = native.clone();
        candidate["sketch"]["constraints"] = json!([
            {"index":10,"type":"Weight","first":4},
            {"index":11,"type":"Equal","first":4,"second":6},
            {"index":12,"type":"Weight","first":5}]);
        assert!(!task.reply("preview", true, true, Some(&candidate)));
        let choices = task.control_choices.as_ref().unwrap();
        // Releasing Equal must not remove its checkbox or renumber a different pole's choice.
        assert_eq!(weight_equal_choices(choices, 0, &json!(1))[0]["index"], 11);
        assert_eq!(weight_equal_choices(choices, 0, &json!(2))[0]["index"], 12);
        assert_eq!(weight_equal_choices(choices, 0, &json!(0)).len(), 2);
    }
    #[test]
    fn failed_preview_close_and_deferred_commit_leave_task_editable() {
        for op in ["preview_commit", "preview_cancel"] {
            let mut task = task();
            task.finishing = true;
            task.valid = true;
            assert!(!task.reply(op, false, true, None));
            assert!(!task.finishing && !task.valid);
            task.finishing = true;
            assert!(task.reply("preview_cancel", true, false, None));
        }
        let mut task = task();
        task.committing = true;
        task.deferred = true;
        assert!(!task.reply("transaction", false, false, None));
        assert!(!task.committing && !task.valid);
    }
}
