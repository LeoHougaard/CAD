//! Native sketch constraint authoring and its compact editor controls.

use super::Editor;
use crate::sketch_commands::{SketchElement, Tool, constrain, datum_edits};
use eframe::egui::{self, Color32};
use serde_json::{Value, json};

pub(super) struct DimensionEdit {
    pub(super) index: Option<usize>,
    pub(super) kind: String,
    pub(super) selection: Vec<SketchElement>,
    pub(super) value: String,
    pub(super) expression: bool,
    focus: bool,
}

fn dimensional(kind: &str) -> bool {
    [
        "Distance",
        "DistanceX",
        "DistanceY",
        "Radius",
        "Diameter",
        "Angle",
        "Weight",
        "SnellsLaw",
    ]
    .contains(&kind)
}

fn number(value: f64) -> String {
    let text = format!("{value:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

fn dimension_value(kind: &str, value: f64) -> String {
    if matches!(kind, "Weight" | "SnellsLaw") {
        return number(value);
    }
    if kind == "Angle" {
        format!("{} deg", number(value.to_degrees()))
    } else {
        format!("{} mm", number(value))
    }
}

fn dimension_input(kind: &str, value: f64) -> String {
    if matches!(kind, "Weight" | "SnellsLaw") {
        return value.to_string();
    }
    if kind == "Angle" {
        format!("{} deg", value.to_degrees())
    } else {
        format!("{value} mm")
    }
}

fn dimension_datum(kind: &str, value: &str) -> String {
    let value = value.trim();
    if matches!(kind, "Weight" | "SnellsLaw") {
        return value.into();
    }
    if value.parse::<f64>().is_ok_and(f64::is_finite) {
        format!("{value} {}", if kind == "Angle" { "deg" } else { "mm" })
    } else {
        value.into()
    }
}

fn dimension_submitted(response: &egui::Response) -> bool {
    (response.has_focus() || response.lost_focus())
        && response
            .ctx
            .input(|input| input.key_pressed(egui::Key::Enter))
}

fn expression(snapshot: &Value, index: usize) -> Option<String> {
    let path = format!("Constraints[{index}]");
    let name = native_constraint(snapshot, index)
        .and_then(|constraint| constraint["name"].as_str())
        .filter(|name| !name.is_empty())
        .map(|name| format!("Constraints.{name}"));
    snapshot["properties"]["ExpressionEngine"]
        .as_array()?
        .iter()
        .find(|entry| {
            entry[0].as_str().is_some_and(|candidate| {
                let candidate = candidate.trim_start_matches('.');
                candidate == path || name.as_deref() == Some(candidate)
            })
        })?
        .get(1)?
        .as_str()
        .map(str::to_owned)
}

fn native_constraint(snapshot: &Value, index: usize) -> Option<&Value> {
    snapshot["sketch"]["constraints"]
        .as_array()?
        .iter()
        .find(|constraint| constraint["index"].as_u64() == Some(index as u64))
}

fn geometry(snapshot: &Value, index: i32) -> Option<&Value> {
    snapshot["sketch"]["geometry"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(
            snapshot["sketch"]["externals"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|entry| &entry["geometry"]),
        )
        .find(|entry| entry["index"].as_i64() == Some(index as i64))
}

fn related_geometry(snapshot: &Value, constraint: &Value) -> Vec<SketchElement> {
    let mut selection = Vec::new();
    for (entity, point) in [
        ("first", "first_pos"),
        ("second", "second_pos"),
        ("third", "third_pos"),
    ] {
        let Some(index) = constraint[entity]
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
        else {
            continue;
        };
        let point = constraint[point]
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(0);
        let exists = geometry(snapshot, index).is_some();
        if exists || (index == -1 && point == 1) {
            let element = SketchElement { index, point };
            if !selection.contains(&element) {
                selection.push(element);
            }
        }
    }
    selection
}

impl Editor {
    fn constraint_editable(&self) -> bool {
        !self.busy
            && self.draft.is_none()
            && self.sketch.is_some()
            && self.sketch_ui.session.is_some()
            && self.sketch_ui.drag.is_none()
            && self.sketch_ui.task.is_none()
    }

    fn prepare_constraint_tool(&mut self) {
        self.sketch_ui.clear_points();
        self.sketch_ui.tool = Tool::Select;
        self.sketch_ui.trim = false;
        self.sketch_ui.external = false;
    }

    fn highlight_constraint(&mut self, snapshot: &Value, index: usize) {
        if let Some(constraint) = native_constraint(snapshot, index) {
            self.sketch_ui.selected_constraint = Some(index);
            self.sketch_ui.constraint_name = constraint["name"].as_str().unwrap_or_default().into();
            self.viewport.sketch_selection = related_geometry(snapshot, constraint);
        }
    }

    fn apply_geometric_constraint(&mut self, kind: &str) {
        if !self.constraint_editable() {
            return;
        }
        let Some(snapshot) = self.sketch_snapshot() else {
            return;
        };
        match constrain(&snapshot, kind, &self.viewport.sketch_selection, None) {
            Ok(edits) => {
                self.prepare_constraint_tool();
                self.sketch_ui.selected_constraint = None;
                self.transaction(&format!("Constrain {kind}"), edits);
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn smart_dimension(&mut self) {
        if !self.constraint_editable() {
            return;
        }
        let Some(snapshot) = self.sketch_snapshot() else {
            return;
        };
        let geometry_kind = |element: SketchElement| {
            geometry(&snapshot, element.index)
                .and_then(|geometry| geometry["kind"].as_str())
                .unwrap_or_default()
        };
        let mut selection = self.viewport.sketch_selection.clone();
        if selection.len() == 1
            && selection[0].point == 0
            && self.begin_pole_weight(selection[0].index)
        {
            return;
        }

        let kind = if selection.len() == 1
            && selection[0].point == 0
            && ["circle", "arc"].contains(&geometry_kind(selection[0]))
        {
            "Radius"
        } else if selection.len() == 2
            && selection
                .iter()
                .all(|element| element.point == 0 && geometry_kind(*element) == "line")
        {
            "Angle"
        } else {
            for element in &mut selection {
                if element.point == 0 && ["circle", "arc"].contains(&geometry_kind(*element)) {
                    element.point = 3;
                }
            }
            "Distance"
        };
        self.begin_dimension(kind, selection);
    }

    pub(super) fn new_dimension(&mut self, kind: &str) {
        if matches!(kind, "Radius" | "Diameter")
            && self.viewport.sketch_selection.len() == 1
            && self.begin_pole_weight(self.viewport.sketch_selection[0].index)
        {
            return;
        }
        self.begin_dimension(kind, self.viewport.sketch_selection.clone());
    }

    fn begin_dimension(&mut self, kind: &str, selection: Vec<SketchElement>) {
        if !self.constraint_editable() {
            return;
        }
        let Some(snapshot) = self.sketch_snapshot() else {
            return;
        };
        match constrain(&snapshot, kind, &selection, None) {
            Ok(edits) => {
                let value = edits[0]["args"]
                    .as_array()
                    .and_then(|args| args.last())
                    .and_then(Value::as_f64)
                    .unwrap_or(0.);
                self.prepare_constraint_tool();
                if !selection.is_empty() && selection.iter().all(|element| element.index < 0) {
                    let index = snapshot["sketch"]["constraint_count"].as_u64().unwrap_or(0);
                    let mut edits = edits;
                    edits.push(json!({"op":"constraint_state", "object":self.sketch, "index":index, "driving":false}));
                    self.transaction("Add reference dimension", edits);
                    return;
                }
                self.sketch_ui.dimension = Some(DimensionEdit {
                    index: None,
                    kind: kind.into(),
                    selection,
                    value: dimension_input(kind, value),
                    expression: false,
                    focus: true,
                });
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn dimension_at(&mut self, index: usize) {
        if !self.constraint_editable() {
            return;
        }
        let Some(snapshot) = self.sketch_snapshot() else {
            return;
        };
        let Some(constraint) = native_constraint(&snapshot, index) else {
            return;
        };
        let kind = constraint["type"].as_str().unwrap_or_default();
        if !dimensional(kind) {
            return;
        }
        if constraint["driving"] == false {
            self.error =
                Some("Switch this reference dimension to Driving before changing its value".into());
            return;
        }
        let formula = expression(&snapshot, index);
        let value = formula
            .clone()
            .unwrap_or_else(|| dimension_input(kind, constraint["value"].as_f64().unwrap_or(0.)));
        self.prepare_constraint_tool();
        self.highlight_constraint(&snapshot, index);
        self.sketch_ui.dimension = Some(DimensionEdit {
            index: Some(index),
            kind: kind.into(),
            selection: vec![],
            value,
            expression: formula.is_some(),
            focus: true,
        });
        self.error = None;
    }

    pub(super) fn constraint_toolbar(&mut self, ui: &mut egui::Ui) {
        let snapshot = self.sketch_snapshot();
        ui.add_enabled_ui(
            self.constraint_editable() && self.sketch_ui.dimension.is_none(),
            |ui| {
                ui.menu_button("Constrain", |ui| {
                    for (label, kind) in [
                        ("Horizontal", "Horizontal"),
                        ("Vertical", "Vertical"),
                        ("Coincident", "Coincident"),
                        ("Point on object", "PointOnObject"),
                        ("Parallel", "Parallel"),
                        ("Perpendicular", "Perpendicular"),
                        ("Tangent", "Tangent"),
                        ("Equal", "Equal"),
                        ("Symmetric", "Symmetric"),
                        ("Fix", "Block"),
                    ] {
                        let validation = snapshot
                            .as_ref()
                            .ok_or_else(|| "Choose a sketch".to_owned())
                            .and_then(|snapshot| {
                                constrain(snapshot, kind, &self.viewport.sketch_selection, None)
                            });
                        let response = ui.add_enabled(validation.is_ok(), egui::Button::new(label));
                        if response.clicked() {
                            self.apply_geometric_constraint(kind);
                            ui.close();
                        } else if let Err(error) = validation {
                            response.on_disabled_hover_text(error);
                        }
                    }
                });
                ui.menu_button("Dimension", |ui| {
                    if ui
                        .add_enabled(
                            !self.viewport.sketch_selection.is_empty(),
                            egui::Button::new("Smart dimension"),
                        )
                        .on_hover_text("Length, radius, or angle from the selected geometry")
                        .clicked()
                    {
                        self.smart_dimension();
                        ui.close();
                    }
                    ui.separator();
                    for (label, kind) in [
                        ("Length", "Distance"),
                        ("Horizontal distance", "DistanceX"),
                        ("Vertical distance", "DistanceY"),
                        ("Radius", "Radius"),
                        ("Diameter", "Diameter"),
                        ("Angle", "Angle"),
                        ("Refraction ratio n2/n1", "SnellsLaw"),
                    ] {
                        let validation = snapshot
                            .as_ref()
                            .ok_or_else(|| "Choose a sketch".to_owned())
                            .and_then(|snapshot| {
                                constrain(snapshot, kind, &self.viewport.sketch_selection, None)
                            });
                        let response = ui.add_enabled(validation.is_ok(), egui::Button::new(label));
                        if response.clicked() {
                            self.new_dimension(kind);
                            ui.close();
                        } else if let Err(error) = validation {
                            response.on_disabled_hover_text(error);
                        }
                    }
                });
            },
        );
    }

    pub(super) fn sketch_panel(&mut self, ctx: &egui::Context) {
        if self.sketch_ui.task.is_some() {
            self.sketch_task_panel(ctx);
            return;
        }
        let Some(snapshot) = self.sketch_snapshot() else {
            return;
        };
        if self.draft.is_some() {
            return;
        }
        let can_edit = self.constraint_editable() && self.sketch_ui.dimension.is_none();
        let constraints = snapshot["sketch"]["constraints"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut edits = Vec::new();
        egui::Window::new("Sketch")
            .id(egui::Id::new("native_sketch_controls"))
            .default_pos([175., 85.]).default_width(235.)
            .resizable(false).collapsible(true)
            .show(ctx, |ui| {
                let freedom = snapshot["sketch"]["degrees_of_freedom"].as_i64().unwrap_or(0);
                ui.colored_label(if freedom == 0 { Color32::from_rgb(110, 182, 177) } else { ui.visuals().text_color() },
                    if freedom == 0 { "Fully constrained".into() } else { format!("{freedom} degrees of freedom") });
                if let Some(support) = snapshot["properties"]["AttachmentSupport"].as_array().and_then(|supports| supports.first()) {
                    let object = support["link"].as_str().unwrap_or("Face");
                    let face = support["subelements"].as_array().and_then(|elements| elements.first()).and_then(Value::as_str).unwrap_or_default();
                    ui.weak(format!("On {object} {face}"));
                }
                ui.weak("Shift-click to select multiple points or curves, including coincident endpoints");
                egui::CollapsingHeader::new(format!("Constraints ({})", constraints.len()))
                    .id_salt("native_constraint_list").show(ui, |ui| {
                        egui::ScrollArea::vertical().id_salt("native_constraints_scroll").max_height(205.).show(ui, |ui| {
                            for constraint in &constraints {
                                let index = constraint["index"].as_u64().unwrap_or(0) as usize;
                                let kind = constraint["type"].as_str().unwrap_or("Constraint");
                                let name = constraint["name"].as_str().unwrap_or_default();
                                let mut label = if name.is_empty() { format!("{}  {kind}", index + 1) } else { name.into() };
                                if dimensional(kind) {
                                    label.push_str(&format!("  {}", dimension_value(kind, constraint["value"].as_f64().unwrap_or(0.))));
                                }
                                if constraint["driving"] == false && dimensional(kind) { label.push_str("  ref"); }
                                if constraint["active"] == false { label.push_str("  off"); }
                                let response = ui.add_enabled(!self.busy, egui::Button::selectable(self.sketch_ui.selected_constraint == Some(index), label));
                                if response.clicked() { self.highlight_constraint(&snapshot, index); }
                                if response.double_clicked() && can_edit { self.dimension_at(index); }
                            }
                        });
                    });
                if let Some(index) = self.sketch_ui.selected_constraint
                    && let Some(constraint) = native_constraint(&snapshot, index) {
                    let kind = constraint["type"].as_str().unwrap_or_default();
                    ui.separator();
                    ui.add_enabled_ui(can_edit, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Name");
                            let response = ui.add(egui::TextEdit::singleline(&mut self.sketch_ui.constraint_name).desired_width(115.));
                            if ui.small_button("Rename").clicked() || response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                                edits.push(json!({"op":"constraint_state","object":snapshot["name"],"index":index,"name":self.sketch_ui.constraint_name.trim()}));
                            }
                        });
                        ui.horizontal(|ui| {
                            let mut active = constraint["active"] != false;
                            if ui.checkbox(&mut active, "Active").changed() {
                                edits.push(json!({"op":"constraint_state","object":snapshot["name"],"index":index,"active":active}));
                            }
                            if dimensional(kind) {
                                let mut driving = constraint["driving"] != false;
                                if ui.checkbox(&mut driving, "Driving").changed() {
                                    edits.push(json!({"op":"constraint_state","object":snapshot["name"],"index":index,"driving":driving}));
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            if dimensional(kind) && ui.add_enabled(constraint["driving"] != false, egui::Button::new("Edit value")).clicked() {
                                self.dimension_at(index);
                            }
                            if ui.button("Delete constraint").clicked() {
                                edits.push(json!({"op":"delete_constraint","object":snapshot["name"],"index":index}));
                                self.sketch_ui.selected_constraint = None;
                            }
                        });
                    });
                }
            });
        if !edits.is_empty() {
            self.transaction("Edit sketch constraint", edits);
        }
        self.dimension_dialog(ctx, &snapshot);
    }

    fn dimension_dialog(&mut self, ctx: &egui::Context, snapshot: &Value) {
        let Some(mut edit) = self.sketch_ui.dimension.take() else {
            return;
        };
        let can_edit = self.constraint_editable();
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Dimension")
            .id(egui::Id::new("native_dimension_editor"))
            .default_pos([255., 125.])
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(&edit.kind);
                ui.add_enabled_ui(can_edit, |ui| {
                    let mut output = egui::TextEdit::singleline(&mut edit.value)
                        .desired_width(225.)
                        .show(ui);
                    if edit.focus {
                        output.response.request_focus();
                        output
                            .state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::two(
                                egui::text::CCursor::new(0),
                                egui::text::CCursor::new(edit.value.chars().count()),
                            )));
                        output.state.store(ui.ctx(), output.response.id);
                        edit.focus = false;
                    }
                    let response = output.response;
                    if dimension_submitted(&response) {
                        apply = true;
                    }
                    if edit.index.is_some()
                        && ui.checkbox(&mut edit.expression, "Expression").changed()
                        && !edit.expression
                        && let Some(constraint) = edit
                            .index
                            .and_then(|index| native_constraint(snapshot, index))
                    {
                        edit.value =
                            dimension_input(&edit.kind, constraint["value"].as_f64().unwrap_or(0.));
                    }
                    ui.weak(if edit.expression {
                        "Native expressions can refer to other dimensions"
                    } else if edit.kind == "SnellsLaw" {
                        "Ratio n2/n1. Endpoint selection order sets n1 then n2. Native Sketcher validates the value."
                    } else if edit.kind == "Weight" {
                        "Dimensionless weight"
                    } else {
                        "mm or degrees by default. Other units are accepted."
                    });
                    ui.horizontal(|ui| {
                        apply |= ui
                            .add_enabled(!edit.value.trim().is_empty(), egui::Button::new("Apply"))
                            .clicked();
                        cancel |= ui.button("Cancel").clicked();
                    });
                });
            });
        cancel |= !self.busy && ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if cancel {
            return;
        }
        if apply && can_edit {
            let value = edit.value.trim();
            if value.is_empty() {
                self.error = Some("Enter a dimension value or expression".into());
            } else {
                let edits = if let Some(index) = edit.index {
                    datum_edits(
                        snapshot,
                        index,
                        &if edit.expression {
                            value.into()
                        } else {
                            dimension_datum(&edit.kind, value)
                        },
                        edit.expression,
                    )
                } else {
                    constrain(
                        snapshot,
                        &edit.kind,
                        &edit.selection,
                        Some(&dimension_datum(&edit.kind, value)),
                    )
                };
                match edits {
                    Ok(edits) => {
                        self.transaction("Set sketch dimension", edits);
                        return;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
        self.sketch_ui.dimension = Some(edit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_submits_single_line_dimension_after_the_widget_releases_focus() {
        let context = egui::Context::default();
        let mut value = String::from("40 mm");
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                ui.add(egui::TextEdit::singleline(&mut value).id_salt("dimension_enter_test"))
                    .request_focus();
            });
        });
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: Some(egui::Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut lost_focus = false;
        let mut submitted = false;
        let _ = context.run(input.clone(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let response =
                    ui.add(egui::TextEdit::singleline(&mut value).id_salt("dimension_enter_test"));
                lost_focus = response.lost_focus();
                submitted = dimension_submitted(&response);
            });
        });
        assert!(
            lost_focus,
            "The test must reproduce single-line Enter dropping focus"
        );
        assert!(
            submitted,
            "Return must apply the dimension despite the focus change"
        );
        let mut unfocused_submission = false;
        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let response =
                    ui.add(egui::TextEdit::singleline(&mut value).id_salt("dimension_enter_test"));
                unfocused_submission = dimension_submitted(&response);
            });
        });
        assert!(
            !unfocused_submission,
            "Unfocused Return must not apply a dimension"
        );
    }

    #[test]
    fn reads_native_expressions_and_formats_native_angles() {
        let snapshot =
            json!({"properties":{"ExpressionEngine":[["Constraints[2]","Constraints[1] * 2"]]}});
        assert_eq!(
            expression(&snapshot, 2).as_deref(),
            Some("Constraints[1] * 2")
        );
        assert_eq!(expression(&snapshot, 1), None);
        let named = json!({"properties":{"ExpressionEngine":[[".Constraints.Width","20 mm"]]},
            "sketch":{"constraints":[{"index":2,"name":"Width"}]}});
        assert_eq!(expression(&named, 2).as_deref(), Some("20 mm"));
        assert_eq!(
            dimension_value("Angle", std::f64::consts::FRAC_PI_4),
            "45 deg"
        );
        assert_eq!(dimension_value("Distance", -2.5), "-2.5 mm");
        assert_eq!(dimension_input("Distance", 12.345678901), "12.345678901 mm");
        assert_eq!(dimension_datum("Distance", "12"), "12 mm");
        assert_eq!(dimension_datum("Angle", "45"), "45 deg");
        assert_eq!(dimension_datum("Distance", "0.5 in"), "0.5 in");
        assert!(dimensional("SnellsLaw"));
        assert_eq!(dimension_value("SnellsLaw", 1.5), "1.5");
        assert_eq!(dimension_input("SnellsLaw", 1.23456789), "1.23456789");
        assert_eq!(dimension_datum("SnellsLaw", " 1.25 "), "1.25");
        assert!(dimensional("Weight"));
        assert_eq!(dimension_value("Weight", 0.75), "0.75");
        assert_eq!(dimension_input("Weight", 0.123456789), "0.123456789");
        assert_eq!(dimension_datum("Weight", " 2.5 "), "2.5");
    }

    #[test]
    fn constraint_highlight_uses_native_entities_and_omits_undefined_slots() {
        let snapshot = json!({"sketch":{"geometry":[{"index":4}]}});
        let constraint =
            json!({"first":4,"first_pos":2,"second":-1,"second_pos":1,"third":-2000,"third_pos":0});
        assert_eq!(
            related_geometry(&snapshot, &constraint),
            vec![
                SketchElement { index: 4, point: 2 },
                SketchElement {
                    index: -1,
                    point: 1
                }
            ]
        );
    }
}
