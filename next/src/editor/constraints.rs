//! Native sketch constraint authoring and its compact editor controls.

use super::Editor;
use crate::sketch_commands::{SketchElement, Tool, angle_via_point_edits, constrain, datum_edits};
use eframe::egui::{self, Color32};
use serde_json::{Value, json};

pub(super) struct DimensionEdit {
    pub(super) index: Option<usize>,
    pub(super) kind: String,
    pub(super) selection: Vec<SketchElement>,
    pub(super) value: String,
    pub(super) expression: bool,
    focus: bool,
    submitting: bool,
    pub(super) constraint: Option<Value>,
    preview: Option<DimensionPreview>,
}

struct DimensionPreview {
    baseline: Value,
    previous_constraint: Option<Value>,
    selected_constraint: Option<Value>,
    token: String,
    seeding: bool,
    applying: bool,
    deferred: bool,
    reference: bool,
}

impl DimensionEdit {
    fn preview_reply(
        &mut self,
        op: &str,
        success: bool,
        active: bool,
        current: Option<&Value>,
    ) -> Option<&'static str> {
        let preview = self.preview.as_mut()?;
        if op == "preview" {
            if preview.seeding {
                preview.seeding = false;
                if success && let Some(current) = current {
                    self.value = quantity_text(current, "input");
                }
            } else if preview.applying {
                if success {
                    return Some("preview_commit");
                }
                preview.applying = false;
            }
        }
        if matches!(op, "preview_commit" | "preview_cancel") {
            if !active {
                return Some("close");
            }
            preview.applying = false;
        }
        if op == "transaction" && preview.deferred && preview.applying {
            if success {
                return Some("close");
            }
            preview.applying = false;
        }
        None
    }
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

fn quantity_text(constraint: &Value, field: &str) -> String {
    constraint["quantity"][field]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn dimension_unit<'a>(state: &'a Value, edit: &DimensionEdit) -> &'a str {
    let unit = if edit.kind == "Angle" {
        "rad"
    } else if matches!(edit.kind.as_str(), "Weight" | "SnellsLaw") {
        ""
    } else {
        "mm"
    };
    state["units"]["defaults"][unit]["default_unit"]
        .as_str()
        .unwrap_or("")
}

pub(super) fn dimension_context(edits: &mut [Value], default: &str) {
    for edit in edits {
        if matches!(edit["op"].as_str(), Some("datum" | "via_point")) && edit.get("value").is_some()
        {
            edit["input_default_unit"] = json!(default);
        }
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

fn same_constraint(first: &Value, second: &Value) -> bool {
    [
        "index",
        "type",
        "first",
        "first_pos",
        "second",
        "second_pos",
        "third",
        "third_pos",
    ]
    .iter()
    .all(|field| first[*field] == second[*field])
}

fn geometry(snapshot: &Value, index: i32) -> Option<&Value> {
    snapshot["sketch"]["geometry"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(snapshot["sketch"]["axes"].as_array().into_iter().flatten())
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
    fn constraint_name_id(&self, index: usize) -> egui::Id {
        egui::Id::new("native_constraint_name")
            .with(self.state["document"].as_str().unwrap_or_default())
            .with(self.sketch.as_deref().unwrap_or_default())
            .with(self.sketch_ui.session.as_deref().unwrap_or_default())
            .with(index)
    }

    pub(super) fn constraint_name_focused(&self, ctx: &egui::Context) -> bool {
        let Some(index) = self.sketch_ui.selected_constraint else {
            return false;
        };
        let id = self.constraint_name_id(index);
        self.sketch_ui.session.is_some()
            && ctx.memory(|m| m.has_focus(id) || m.had_focus_last_frame(id))
    }

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
            if let Some(preview) = self
                .sketch_ui
                .dimension
                .as_mut()
                .and_then(|edit| edit.preview.as_mut())
            {
                preview.selected_constraint = Some(constraint.clone());
            }
        }
    }

    fn refresh_dimension_selection(&mut self, preview: &mut DimensionPreview) {
        if let Some(index) = self.sketch_ui.selected_constraint {
            let current = self
                .sketch_snapshot()
                .and_then(|snapshot| native_constraint(&snapshot, index).cloned());
            if !preview
                .selected_constraint
                .as_ref()
                .zip(current.as_ref())
                .is_some_and(|(selected, current)| same_constraint(selected, current))
            {
                self.sketch_ui.selected_constraint = None;
                self.sketch_ui.constraint_name.clear();
            }
            preview.selected_constraint = current;
        }
    }

    fn restore_dimension_selection(&mut self, preview: &DimensionPreview) {
        if self.sketch_ui.selected_constraint.is_none()
            && let Some(previous) = &preview.previous_constraint
            && let Some(index) = previous["index"].as_u64().map(|i| i as usize)
            && let Some(snapshot) = self.sketch_snapshot()
            && native_constraint(&snapshot, index)
                .is_some_and(|current| same_constraint(previous, current))
        {
            self.highlight_constraint(&snapshot, index);
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
            && geometry_kind(selection[0]) == "circle"
        {
            "Diameter"
        } else if selection.len() == 1
            && selection[0].point == 0
            && ["circle", "arc"].contains(&geometry_kind(selection[0]))
        {
            "Radius"
        } else if crate::sketch_commands::parallel_lines(&snapshot, &selection) {
            "Distance"
        } else if selection.len() == 3
            || selection.len() == 2
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

    pub(super) fn place_dimension(&mut self, position: egui::Pos2) {
        let selection = self.viewport.sketch_selection.clone();
        if selection.len() == 2
            && selection.iter().all(|element| element.point != 0)
            && let (Some(a), Some(b), Some(cursor)) = (
                self.viewport.geometry_point(selection[0]),
                self.viewport.geometry_point(selection[1]),
                self.viewport.sketch_point(position),
            )
        {
            let inside_x = (a[0].min(b[0])..=a[0].max(b[0])).contains(&cursor[0]);
            let inside_y = (a[1].min(b[1])..=a[1].max(b[1])).contains(&cursor[1]);
            let kind = if a[0] == b[0] && a[1] != b[1] {
                "DistanceY"
            } else if a[1] == b[1] && a[0] != b[0] || inside_x && !inside_y {
                "DistanceX"
            } else if inside_y && !inside_x {
                "DistanceY"
            } else {
                "Distance"
            };
            self.begin_dimension(kind, selection);
            return;
        }
        self.smart_dimension();
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
                if edits[0]["kind"] == "AngleViaPoint" {
                    let previous_constraint = self
                        .sketch_ui
                        .selected_constraint
                        .and_then(|index| native_constraint(&snapshot, index))
                        .cloned();
                    self.sketch_ui.selected_constraint = None;
                    self.sketch_ui.constraint_name.clear();
                    self.prepare_constraint_tool();
                    if edits[0]["driving"] == false {
                        self.transaction("Add reference angle through point", edits);
                        return;
                    }
                    self.sequence += 1;
                    let token = format!("angle-point-{}-{}", std::process::id(), self.sequence);
                    let deferred = self.sketch_ui.branch_pending
                        || self.state["redo_count"].as_u64().unwrap_or(0) > 0;
                    self.sketch_ui.dimension = Some(DimensionEdit {
                        index: None,
                        kind: "Angle".into(),
                        selection,
                        value: String::new(),
                        expression: false,
                        focus: true,
                        submitting: false,
                        constraint: None,
                        preview: Some(DimensionPreview {
                            baseline: snapshot,
                            previous_constraint,
                            selected_constraint: None,
                            token: token.clone(),
                            seeding: !deferred,
                            applying: false,
                            deferred,
                            reference: false,
                        }),
                    });
                    if !deferred {
                        self.send(
                            "preview",
                            json!({"token":token,"label":"Angle through point","edits":edits}),
                        );
                    }
                    return;
                }
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
                    value: String::new(),
                    expression: false,
                    focus: true,
                    submitting: false,
                    constraint: None,
                    preview: None,
                });
                self.error = None;
                self.send("quantities", json!({"items":[{"value":value,"unit":if kind=="Angle" {"rad"} else if matches!(kind,"Weight"|"SnellsLaw") {""} else {"mm"}}]}));
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
            .unwrap_or_else(|| quantity_text(constraint, "input"));
        self.prepare_constraint_tool();
        self.highlight_constraint(&snapshot, index);
        self.sketch_ui.dimension = Some(DimensionEdit {
            index: Some(index),
            kind: kind.into(),
            selection: vec![],
            value,
            expression: formula.is_some(),
            focus: true,
            submitting: false,
            constraint: Some(constraint.clone()),
            preview: None,
        });
        self.error = None;
    }

    pub(super) fn dimension_reply(&mut self, op: &str, success: bool) {
        let current = self.sketch_snapshot().and_then(|snapshot| {
            snapshot["sketch"]["constraints"]
                .as_array()
                .and_then(|cs| cs.last())
                .cloned()
        });
        let Some(mut edit) = self.sketch_ui.dimension.take() else {
            return;
        };
        if op == "transaction" && edit.submitting
            || op.starts_with("preview") && edit.preview.is_some()
        {
            self.refresh_dimension_tool(success && matches!(op, "transaction" | "preview_commit"));
        }
        if op == "transaction" && edit.submitting {
            if success {
                return;
            }
            edit.submitting = false;
            edit.focus = true;
        }
        if let Some(preview) = &mut edit.preview {
            self.refresh_dimension_selection(preview);
        }
        let next = edit.preview_reply(
            op,
            success,
            !self.state["preview"].is_null(),
            current.as_ref(),
        );
        if next == Some("close") {
            if op == "preview_cancel"
                && let Some(preview) = &edit.preview
            {
                self.restore_dimension_selection(preview);
            }
            if success && matches!(op, "transaction" | "preview_commit") {
                self.sketch_ui.branch_pending = false;
            }
            return;
        }
        let token = edit.preview.as_ref().map(|p| p.token.clone());
        self.sketch_ui.dimension = Some(edit);
        if next == Some("preview_commit")
            && let Some(token) = token
        {
            self.send("preview_commit", json!({"token":token}));
        }
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
                if ui
                    .selectable_label(self.sketch_ui.dimension_tool.is_some(), "Dimension (D)")
                    .on_hover_text("Select entities and click to place their dimension")
                    .clicked()
                {
                    if self.viewport.sketch_selection.is_empty()
                        || self.sketch_ui.dimension_tool.is_some()
                    {
                        self.start_dimension_tool();
                    } else {
                        self.smart_dimension();
                    }
                }
                ui.menu_button("Dimension types", |ui| {
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
        let mut name_submission_owner = None;
        egui::Window::new("Sketch")
            .id(egui::Id::new("native_sketch_controls"))
            .default_pos([175., 85.]).default_width(235.)
            .resizable(false).collapsible(true)
            .show(ctx, |ui| {
                let name_id = egui::Id::new("native_sketch_name");
                let accepted_name = snapshot["label"].as_str().unwrap_or_default();
                let owner = json!({"document":self.state["document"],"revision":self.state["revision"],
                    "sketch":snapshot["name"],"token":self.sketch_ui.session});
                if !self.sketch_name_focused(ctx)
                    && self.sketch_ui.name_owner.is_none()
                    && self.sketch_ui.pending_name_finish.is_none()
                {
                    self.sketch_ui.name_text = accepted_name.into();
                    self.sketch_ui.name_owner = None;
                }
                ui.horizontal(|ui| {
                    ui.label("Sketch name");
                    let response = ui.add_enabled(
                        can_edit && !self.sketch_ui.name_input_blocked,
                        egui::TextEdit::singleline(&mut self.sketch_ui.name_text)
                            .id(name_id).desired_width(140.),
                    );
                    if response.gained_focus() {
                        self.sketch_ui.name_owner = Some(owner.clone());
                    }
                    if response.lost_focus() {
                        let enter = ui.input_mut(|input| input.consume_key(input.modifiers, egui::Key::Enter));
                        let changed = self.sketch_ui.name_text != accepted_name;
                        if changed && can_edit && !self.sketch_ui.name_input_blocked {
                            if self.sketch_ui.name_owner.as_ref() != Some(&owner) {
                                self.error = Some("The sketch changed; its name was not applied".into());
                            } else if self.sketch_ui.name_text.trim().is_empty() {
                                self.error = Some("Sketch name cannot be empty".into());
                            } else {
                                name_submission_owner = self.sketch_ui.name_owner.clone();
                                edits.push(json!({"op":"set","object":snapshot["name"],
                                    "property":"Label","value":self.sketch_ui.name_text.trim()}));
                            }
                        }
                        self.sketch_ui.name_owner = None;
                        if enter {
                            response.surrender_focus();
                        }
                    }
                });
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
                                    label.push_str(&format!("  {}", quantity_text(constraint, "display")));
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
                            let name_id = self.constraint_name_id(index);
                            let response = ui.add(egui::TextEdit::singleline(&mut self.sketch_ui.constraint_name).id(name_id).desired_width(115.));
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
            let label = if edits
                .iter()
                .all(|edit| edit["op"] == "set" && edit["property"] == "Label")
            {
                "Rename sketch"
            } else {
                "Edit sketch constraint"
            };
            self.transaction(label, edits);
            if !self.busy && name_submission_owner.is_some() {
                self.sketch_ui.name_owner = name_submission_owner;
            }
        }
        self.dimension_dialog(ctx, &snapshot);
    }

    fn dimension_dialog(&mut self, ctx: &egui::Context, snapshot: &Value) {
        let Some(mut edit) = self.sketch_ui.dimension.take() else {
            return;
        };
        let target_valid = edit.index.is_none()
            || edit.constraint.as_ref().is_none_or(|previous| {
                edit.index
                    .and_then(|index| native_constraint(snapshot, index))
                    .is_some_and(|current| same_constraint(previous, current))
            });
        if !target_valid && self.sketch_ui.selected_constraint == edit.index {
            self.sketch_ui.selected_constraint = None;
            self.sketch_ui.constraint_name.clear();
        }
        let can_edit = self.constraint_editable()
            && !edit.submitting
            && edit
                .preview
                .as_ref()
                .is_none_or(|p| !p.seeding && !p.applying);
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
                    if let Some(preview) = &mut edit.preview {
                        ui.checkbox(&mut preview.reference, "Reference");
                    }
                    let reference = edit.preview.as_ref().is_some_and(|p| p.reference);
                    let mut output = ui.add_enabled_ui(!reference, |ui| egui::TextEdit::singleline(&mut edit.value)
                        .hint_text(if edit.preview.is_some() { "Use native measured angle" } else { "" })
                        .desired_width(225.)
                        .show(ui)).inner;
                    if edit.focus && can_edit {
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
                    if !reference && (edit.index.is_some() || edit.preview.is_some())
                        && ui.checkbox(&mut edit.expression, "Expression").changed()
                        && !edit.expression
                    {
                        let constraint = if let Some(index) = edit.index {
                            native_constraint(snapshot,index)
                        } else if edit.preview.as_ref().is_some_and(|p| !p.deferred) {
                            snapshot["sketch"]["constraints"].as_array().and_then(|cs| cs.last())
                        } else { None };
                        edit.value = constraint.map(|c| quantity_text(c, "input")).unwrap_or_default();
                    }
                    ui.weak(if reference {
                        "Native reference angle. Its value is measured from the solved curves."
                    } else if edit.expression {
                        "Native expressions can refer to other dimensions"
                    } else if edit.kind == "SnellsLaw" {
                        "Ratio n2/n1. Endpoint selection order sets n1 then n2. Native Sketcher validates the value."
                    } else if edit.kind == "Weight" {
                        "Dimensionless weight"
                    } else if edit.preview.as_ref().is_some_and(|p| p.deferred) {
                        "Native preparation and measurement run on Apply while Redo is retained. Leave empty to use the measured angle."
                    } else {
                        "Bare numbers use this field's native displayed unit. Explicit units are accepted."
                    });
                    ui.horizontal(|ui| {
                        apply |= ui
                            .add_enabled(!edit.value.trim().is_empty() || edit.preview.is_some(), egui::Button::new("Apply"))
                            .clicked();
                        cancel |= ui.button("Cancel").clicked();
                    });
                });
            });
        cancel |= !self.busy
            && !edit.submitting
            && ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if cancel {
            if let Some(preview) = &mut edit.preview
                && !self.state["preview"].is_null()
            {
                preview.applying = true;
                let token = preview.token.clone();
                self.sketch_ui.dimension = Some(edit);
                self.send("preview_cancel", json!({"token":token}));
            } else if let Some(preview) = &mut edit.preview {
                self.refresh_dimension_selection(preview);
                self.restore_dimension_selection(preview);
            }
            return;
        }
        if apply && can_edit {
            let default = edit
                .constraint
                .as_ref()
                .and_then(|c| c["quantity"]["default_unit"].as_str())
                .unwrap_or_else(|| dimension_unit(&self.state, &edit))
                .to_owned();
            let value = edit.value.trim();
            if let Some(preview) = &mut edit.preview {
                let datum = (!value.is_empty()).then(|| value.to_owned());
                let edits = angle_via_point_edits(
                    &preview.baseline,
                    &edit.selection,
                    if preview.reference || edit.expression {
                        None
                    } else {
                        datum.as_deref()
                    },
                    (edit.expression && !preview.reference).then_some(value),
                    Some(!preview.reference),
                );
                match edits {
                    Ok(mut edits) => {
                        dimension_context(&mut edits, &default);
                        preview.applying = true;
                        let token = preview.token.clone();
                        let deferred = preview.deferred;
                        self.sketch_ui.dimension = Some(edit);
                        if deferred {
                            self.transaction("Angle through point", edits);
                        } else {
                            self.send(
                                "preview",
                                json!({"token":token,"label":"Angle through point","edits":edits}),
                            );
                        }
                        return;
                    }
                    Err(error) => self.error = Some(error),
                }
                self.sketch_ui.dimension = Some(edit);
                return;
            }
            if value.is_empty() {
                self.error = Some("Enter a dimension value or expression".into());
            } else {
                if !target_valid {
                    self.error = Some("This dimension changed. Cancel and select it again".into());
                    self.sketch_ui.dimension = Some(edit);
                    return;
                }
                let edits = if let Some(index) = edit.index {
                    datum_edits(
                        snapshot,
                        index,
                        &if edit.expression {
                            value.into()
                        } else {
                            value.to_owned()
                        },
                        edit.expression,
                    )
                } else {
                    constrain(snapshot, &edit.kind, &edit.selection, Some(value))
                };
                match edits {
                    Ok(mut edits) => {
                        dimension_context(&mut edits, &default);
                        edit.submitting = true;
                        self.sketch_ui.dimension = Some(edit);
                        self.transaction("Set sketch dimension", edits);
                        if !self.busy
                            && let Some(edit) = &mut self.sketch_ui.dimension
                        {
                            edit.submitting = false;
                            edit.focus = true;
                        }
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
mod angle_selection_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_dialog_waits_for_native_preview_commit_and_recovers_from_failures() {
        let mut edit = DimensionEdit {
            index: None,
            kind: "Angle".into(),
            selection: vec![],
            value: String::new(),
            expression: false,
            focus: true,
            submitting: false,
            constraint: None,
            preview: Some(DimensionPreview {
                baseline: json!({}),
                previous_constraint: None,
                selected_constraint: None,
                token: "angle".into(),
                seeding: true,
                applying: false,
                deferred: false,
                reference: false,
            }),
        };
        assert_eq!(
            edit.preview_reply(
                "preview",
                true,
                true,
                Some(&json!({"value":std::f64::consts::FRAC_PI_4,"quantity":{"input":"45 deg"}}))
            ),
            None
        );
        assert_eq!(edit.value, "45 deg");
        edit.preview.as_mut().unwrap().applying = true;
        edit.value = "bad formula".into();
        assert_eq!(edit.preview_reply("preview", false, false, None), None);
        assert!(!edit.preview.as_ref().unwrap().applying);
        assert_eq!(edit.value, "bad formula");
        edit.preview.as_mut().unwrap().applying = true;
        assert_eq!(
            edit.preview_reply("preview", true, true, None),
            Some("preview_commit")
        );
        assert_eq!(
            edit.preview_reply("preview_commit", false, true, None),
            None
        );
        assert_eq!(
            edit.preview_reply("preview_cancel", true, false, None),
            Some("close")
        );
        edit.preview.as_mut().unwrap().deferred = true;
        edit.preview.as_mut().unwrap().applying = true;
        assert_eq!(edit.preview_reply("transaction", false, false, None), None);
        edit.preview.as_mut().unwrap().applying = true;
        assert_eq!(
            edit.preview_reply("transaction", true, false, None),
            Some("close")
        );
    }

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
    fn native_quantity_text_is_authoritative() {
        let c = json!({"quantity":{"display":"1.00 in","input":"1 in"}});
        assert_eq!(quantity_text(&c, "display"), "1.00 in");
        assert_eq!(quantity_text(&c, "input"), "1 in");
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
