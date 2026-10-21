//! Interaction state only: every accepted geometry change is solved natively.
use super::{Editor, constraints::DimensionEdit};
use crate::sketch_commands::{SketchElement, Tool, draw_edits};
use eframe::egui::{self, Color32, Key};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

pub(super) struct SketchUi {
    pub(super) tool: Tool,
    points: Vec<[f64; 3]>,
    snaps: Vec<Option<SketchElement>>,
    pub(super) construction: bool,
    pub(super) dimension: Option<DimensionEdit>,
    pub(super) selected_constraint: Option<usize>,
    pub(super) constraint_name: String,
    pub(super) session: Option<String>,
    pub(super) drag: Option<Drag>,
    pub(super) trim: bool,
    pending_begin: bool,
    new_sketch: bool,
    dirty_before: bool,
    cleanup: bool,
    after_finish: Option<String>,
    continuation: Option<usize>,
    box_start: Option<egui::Pos2>,
    branch_pending: bool,
}
impl Default for SketchUi {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            points: vec![],
            snaps: vec![],
            construction: false,
            dimension: None,
            selected_constraint: None,
            constraint_name: String::new(),
            session: None,
            drag: None,
            trim: false,
            pending_begin: false,
            new_sketch: false,
            dirty_before: false,
            cleanup: false,
            after_finish: None,
            continuation: None,
            box_start: None,
            branch_pending: false,
        }
    }
}
impl SketchUi {
    pub(super) fn clear_points(&mut self) {
        self.points.clear();
        self.snaps.clear();
        self.continuation = None;
    }
}
pub(super) struct Drag {
    element: SketchElement,
    token: String,
    origin: [f64; 3],
    anchor: [f64; 3],
    target: [f64; 3],
    submitted: Option<[f64; 3]>,
    released: bool,
    cancel: bool,
    valid: bool,
    started: bool,
    last: Instant,
    original: Value,
}

impl Editor {
    pub(super) fn prepare_new_sketch(&mut self) {
        self.sketch_ui = SketchUi {
            pending_begin: true,
            new_sketch: true,
            dirty_before: self.dirty,
            branch_pending: self.state["redo_count"].as_u64().unwrap_or(0) > 0,
            ..SketchUi::default()
        };
    }
    fn begin_sketch(&mut self) {
        let Some(name) = self.sketch.clone() else {
            return;
        };
        self.sequence += 1;
        let token = format!("sketch-{}-{}", std::process::id(), self.sequence);
        self.sketch_ui.pending_begin = false;
        self.sketch_ui.session = Some(token.clone());
        self.send("sketch_begin", json!({"object":name,"token":token}));
    }
    pub(super) fn request_sketch(&mut self, name: &str) {
        if self.sketch_ui.session.is_some() {
            if self.sketch.as_deref() != Some(name) {
                self.error = Some("Finish or cancel the current sketch first".into());
            }
            return;
        }
        self.sketch_ui = SketchUi {
            dirty_before: self.dirty,
            branch_pending: self.state["redo_count"].as_u64().unwrap_or(0) > 0,
            ..SketchUi::default()
        };
        self.sketch = Some(name.into());
        self.sync_sketch();
        self.begin_sketch();
    }
    pub(super) fn sketch_reply(&mut self, op: &str, success: bool) {
        if op == "transaction" && self.sketch_ui.pending_begin {
            if success {
                self.begin_sketch();
            } else {
                self.sketch_ui.pending_begin = false;
                self.sketch = None;
            }
            return;
        }
        if op == "sketch_begin" {
            if success {
                self.viewport.align_sketch();
                self.viewport.fit();
            } else {
                self.sketch_ui.session = None;
                self.sketch = None;
                self.viewport.set_sketch(None);
            }
        }
        if op == "transaction" && success && self.sketch_ui.session.is_some() {
            self.sketch_ui.branch_pending = false;
            if let Some(index) = self.sketch_ui.continuation.take() {
                let element = SketchElement {
                    index: index as i32,
                    point: 2,
                };
                if let Some(point) = self.viewport.geometry_point(element) {
                    self.sketch_ui.points = vec![point];
                    self.sketch_ui.snaps = vec![Some(element)];
                }
            }
        }
        if op == "transaction" && !success {
            self.sketch_ui.continuation = None;
        }
        if op == "preview"
            && let Some(drag) = &mut self.sketch_ui.drag
        {
            drag.valid = success;
            drag.started = !self.state["preview"].is_null();
        }
        if matches!(op, "preview_commit" | "preview_cancel") && self.sketch_ui.drag.is_some() {
            if self.state["preview"].is_null() {
                self.sketch_ui.drag = None;
            } else if let Some(drag) = &mut self.sketch_ui.drag {
                drag.cancel = true;
            }
            if op == "preview_commit" && success {
                self.sketch_ui.branch_pending = false;
            }
        }
        if op == "transaction" && self.sketch_ui.drag.as_ref().is_some_and(|d| d.released) {
            self.sketch_ui.drag = None;
        }
        if success && matches!(op, "sketch_finish" | "sketch_cancel") {
            let cancel = op == "sketch_cancel";
            self.sketch_ui.session = None;
            self.sketch_ui.drag = None;
            self.sketch_ui.clear_points();
            self.sketch_ui.dimension = None;
            self.sketch = None;
            self.viewport.set_sketch(None);
            self.viewport.iso();
            self.viewport.fit();
            if cancel {
                if self.sketch_ui.new_sketch {
                    self.sketch_ui.cleanup = true;
                    self.send("undo", json!({}));
                } else {
                    self.dirty = self.sketch_ui.dirty_before;
                }
            } else {
                self.dirty = true;
                if let Some(feature) = self.sketch_ui.after_finish.take() {
                    self.start_feature(&feature);
                }
            }
        }
        if !success && op == "sketch_finish" {
            self.sketch_ui.after_finish = None;
        }
        if op == "undo" && success && self.sketch_ui.cleanup {
            self.sketch_ui.cleanup = false;
            self.dirty = self.sketch_ui.dirty_before;
        }
    }
    pub(super) fn feature_from_sketch(&mut self, id: &str) {
        if self.sketch_ui.dimension.is_some() || self.sketch_ui.drag.is_some() {
            return;
        }
        if self.sketch_ui.session.is_some() {
            self.sketch_ui.after_finish = Some(id.into());
            self.finish_sketch(true);
        } else {
            self.start_feature(id);
        }
    }
    fn finish_sketch(&mut self, commit: bool) {
        if self.busy || self.sketch_ui.drag.is_some() || self.sketch_ui.dimension.is_some() {
            return;
        }
        if let Some(token) = self.sketch_ui.session.clone() {
            self.send(
                if commit {
                    "sketch_finish"
                } else {
                    "sketch_cancel"
                },
                json!({"token":token}),
            );
        }
    }
    pub(super) fn sketch_toolbar(&mut self, ui: &mut egui::Ui) {
        let ready = self.sketch_ui.session.is_some()
            && self.sketch_ui.drag.is_none()
            && self.sketch_ui.dimension.is_none();
        ui.add_enabled_ui(ready, |ui| {
            for (label, tool) in [
                ("Select", Tool::Select),
                ("Point", Tool::Point),
                ("Line", Tool::Line),
                ("Polyline", Tool::Polyline),
                ("Rectangle", Tool::Rectangle),
                ("Circle", Tool::Circle),
                ("Arc", Tool::Arc),
            ] {
                if ui
                    .selectable_label(self.sketch_ui.tool == tool && !self.sketch_ui.trim, label)
                    .clicked()
                {
                    self.sketch_ui.tool = tool;
                    self.sketch_ui.trim = false;
                    self.sketch_ui.clear_points();
                }
            }
            if ui.selectable_label(self.sketch_ui.trim, "Trim").clicked() {
                self.sketch_ui.trim = !self.sketch_ui.trim;
                self.sketch_ui.tool = Tool::Select;
                self.sketch_ui.clear_points();
            }
            if ui
                .checkbox(&mut self.sketch_ui.construction, "Construction")
                .changed()
            {
                self.toggle_construction();
            }
            self.constraint_toolbar(ui);
            if ui.button("Finish").clicked() {
                self.finish_sketch(true);
            }
            if ui.button("Cancel").clicked() {
                self.finish_sketch(false);
            }
        });
    }
    fn toggle_construction(&mut self) {
        let Some(object) = self.sketch.clone() else {
            return;
        };
        let mut indices: Vec<_> = self
            .viewport
            .sketch_selection
            .iter()
            .filter(|s| s.index >= 0)
            .map(|s| s.index)
            .collect();
        indices.sort();
        indices.dedup();
        if !indices.is_empty() {
            let edits=indices.into_iter().map(|index|json!({"op":"construction","object":object,"index":index,"construction":self.sketch_ui.construction})).collect();
            self.transaction("Toggle construction geometry", edits);
        }
    }
    fn delete_geometry(&mut self) {
        let Some(object) = self.sketch.clone() else {
            return;
        };
        let mut indices: Vec<_> = self
            .viewport
            .sketch_selection
            .iter()
            .filter(|s| s.index >= 0)
            .map(|s| s.index)
            .collect();
        indices.sort();
        indices.dedup();
        indices.reverse();
        if !indices.is_empty() {
            self.transaction(
                "Delete sketch geometry",
                indices
                    .into_iter()
                    .map(|index| json!({"op":"delete_geometry","object":object,"index":index}))
                    .collect(),
            );
            self.viewport.clear_sketch_selection();
            self.sketch_ui.selected_constraint = None;
        }
    }
    fn snap(&self, pos: egui::Pos2, disabled: bool) -> Option<([f64; 3], Option<SketchElement>)> {
        let mut point = self.viewport.sketch_point(pos)?;
        if !disabled && let Some(hit) = self.viewport.sketch_hit(pos) {
            if hit.point != 0
                && let Some(exact) = self.viewport.geometry_point(hit)
            {
                point = exact;
            }
            return Some((point, Some(hit)));
        }
        if !disabled
            && matches!(self.sketch_ui.tool, Tool::Line | Tool::Polyline)
            && let Some(first) = self.sketch_ui.points.first()
        {
            let screen = self.viewport.sketch_screen_point(*first);
            if (screen.x - pos.x).abs() < 6. {
                point[0] = first[0];
            }
            if (screen.y - pos.y).abs() < 6. {
                point[1] = first[1];
            }
        }
        Some((point, None))
    }
    fn draw_point(&mut self, point: [f64; 3], snap: Option<SketchElement>, infer: bool) {
        let Some(object) = self.sketch.clone() else {
            return;
        };
        self.sketch_ui.points.push(point);
        self.sketch_ui.snaps.push(snap);
        let tool = self.sketch_ui.tool;
        if self.sketch_ui.points.len() < tool.required_points() {
            return;
        }
        let count = self
            .sketch_snapshot()
            .and_then(|s| s["sketch"]["geometry_count"].as_u64())
            .unwrap_or(0) as usize;
        let mut edits = match draw_edits(
            &object,
            tool,
            &self.sketch_ui.points,
            count,
            self.sketch_ui.construction,
        ) {
            Ok(e) => e,
            Err(e) => {
                self.error = Some(e);
                self.sketch_ui.points.pop();
                self.sketch_ui.snaps.pop();
                return;
            }
        };
        let refs: Vec<(usize, usize, i32)> = match tool {
            Tool::Point => vec![(0, count, 1)],
            Tool::Line | Tool::Polyline | Tool::Arc => {
                vec![(0, count, 1), (tool.required_points() - 1, count, 2)]
            }
            Tool::Circle => vec![(0, count, 3)],
            Tool::Rectangle => {
                let p = &self.sketch_ui.points;
                let minx = p[0][0].min(p[1][0]);
                let miny = p[0][1].min(p[1][1]);
                (0..2)
                    .map(|click| {
                        let right = p[click][0] > minx;
                        let top = p[click][1] > miny;
                        let corner = match (right, top) {
                            (false, false) => 0,
                            (true, false) => 1,
                            (true, true) => 2,
                            (false, true) => 3,
                        };
                        (click, count + corner, 1)
                    })
                    .collect()
            }
            _ => vec![],
        };
        for (click, index, point) in refs {
            if let Some(target) = self.sketch_ui.snaps[click] {
                if target.point == 0 {
                    edits.push(json!({"op":"constraint","object":object,"kind":"PointOnObject","args":[index,point,target.index]}));
                } else {
                    edits.push(json!({"op":"constraint","object":object,"kind":"Coincident","args":[index,point,target.index,target.point]}));
                }
            }
        }
        if tool == Tool::Circle
            && let Some(target) = self.sketch_ui.snaps[1].filter(|t| t.point != 0)
        {
            edits.push(json!({"op":"constraint","object":object,"kind":"PointOnObject","args":[target.index,target.point,count]}));
        }
        if infer
            && matches!(tool, Tool::Line | Tool::Polyline)
            && self.sketch_ui.snaps.iter().filter(|s| s.is_some()).count() < 2
        {
            let p = &self.sketch_ui.points;
            let kind = if (p[0][0] - p[1][0]).abs() < 1e-9 {
                Some("Vertical")
            } else if (p[0][1] - p[1][1]).abs() < 1e-9 {
                Some("Horizontal")
            } else {
                None
            };
            if let Some(kind) = kind {
                edits.push(json!({"op":"constraint","object":object,"kind":kind,"args":[count]}));
            }
        }
        self.sketch_ui.clear_points();
        if tool == Tool::Polyline {
            self.sketch_ui.continuation = Some(count);
        }
        self.transaction("Draw sketch geometry", edits);
    }
    pub(super) fn sketch_canvas(&mut self, ui: &mut egui::Ui) {
        if self.sketch_ui.session.is_none() || self.draft.is_some() {
            return;
        }
        let Some(response) = self.viewport.response.clone() else {
            return;
        };
        let pos = ui.input(|i| i.pointer.interact_pos());
        let alt = ui.input(|i| i.modifiers.alt);
        if let Some(pos) = pos
            && response.hovered()
            && self.sketch_ui.tool != Tool::Select
        {
            let cursor = self.snap(pos, alt);
            super::drawing::preview(
                ui.painter(),
                &self.viewport,
                self.sketch_ui.tool,
                &self.sketch_ui.points,
                cursor.map(|p| p.0),
            );
            if cursor.is_some_and(|p| p.1.is_some()) {
                ui.painter().circle_stroke(
                    pos,
                    6.,
                    egui::Stroke::new(1_f32, Color32::from_rgb(110, 182, 177)),
                );
            }
        }
        if self.sketch_ui.dimension.is_some() {
            return;
        }
        if let Some(drag) = &mut self.sketch_ui.drag {
            if !drag.released
                && let Some(local) = pos.and_then(|p| self.viewport.sketch_point(p))
            {
                drag.target = std::array::from_fn(|i| drag.anchor[i] + local[i] - drag.origin[i]);
            }
            if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                drag.released = true;
            }
            return;
        }
        if let Some(start) = self.sketch_ui.box_start {
            if let Some(end) = pos {
                let rect = egui::Rect::from_two_pos(start, end);
                ui.painter().rect_stroke(
                    rect,
                    0.,
                    egui::Stroke::new(1_f32, Color32::from_rgb(110, 182, 177)),
                    egui::StrokeKind::Inside,
                );
                if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                    let selected = self.viewport.sketch_in_rect(rect, end.x < start.x);
                    if !ui.input(|i| i.modifiers.shift || i.modifiers.ctrl) {
                        self.viewport.clear_sketch_selection();
                    }
                    for s in selected {
                        if !self.viewport.sketch_selection.contains(&s) {
                            self.viewport.sketch_selection.push(s);
                        }
                    }
                    self.sketch_ui.box_start = None;
                }
            }
            return;
        }
        if self.busy {
            return;
        }
        if response.double_clicked()
            && self.sketch_ui.tool == Tool::Select
            && let Some(index) = pos.and_then(|p| self.viewport.dimension_hit(p))
        {
            self.dimension_at(index);
            return;
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && self.sketch_ui.tool == Tool::Select
            && !self.sketch_ui.trim
        {
            let Some(start) = ui.input(|i| i.pointer.press_origin()) else {
                return;
            };
            if let Some(element) = self.viewport.sketch_hit(start).filter(|h| h.index >= 0) {
                let Some(origin) = self.viewport.sketch_point(start) else {
                    return;
                };
                let Some(anchor) = self.viewport.geometry_point(element) else {
                    return;
                };
                self.sequence += 1;
                self.sketch_ui.drag = Some(Drag {
                    element,
                    token: format!("drag-{}-{}", std::process::id(), self.sequence),
                    origin,
                    anchor,
                    target: anchor,
                    submitted: None,
                    released: false,
                    cancel: false,
                    valid: false,
                    started: false,
                    last: Instant::now() - Duration::from_secs(1),
                    original: self
                        .sketch_snapshot()
                        .map(|s| s["sketch"]["geometry"].clone())
                        .unwrap_or_default(),
                });
            } else {
                self.sketch_ui.box_start = Some(start);
            }
        }
        if response.clicked()
            && let Some(pos) = pos
        {
            if self.sketch_ui.trim {
                if let Some(hit) = self.viewport.sketch_hit(pos).filter(|h| h.index >= 0)
                    && let Some(position) = self.viewport.sketch_point(pos)
                {
                    self.viewport.clear_sketch_selection();
                    self.sketch_ui.selected_constraint = None;
                    self.transaction("Trim sketch geometry",vec![json!({"op":"trim_geometry","object":self.sketch,"index":hit.index,"position":position})]);
                }
                return;
            }
            if self.sketch_ui.tool != Tool::Select
                && let Some((point, snap)) = self.snap(pos, alt)
            {
                self.draw_point(point, snap, !alt);
            } else {
                self.sketch_ui.selected_constraint = None;
            }
        }
    }
    pub(super) fn sketch_escape(&mut self) {
        if self.sketch_ui.dimension.is_some() {
            return;
        }
        if let Some(drag) = &mut self.sketch_ui.drag {
            drag.cancel = true;
            return;
        }
        self.sketch_ui.clear_points();
        self.sketch_ui.tool = Tool::Select;
        self.sketch_ui.trim = false;
        self.sketch_ui.box_start = None;
    }
    pub(super) fn sketch_pump(&mut self, ctx: &egui::Context) {
        if !self.busy
            && let Some(drag) = &self.sketch_ui.drag
        {
            let token = drag.token.clone();
            if drag.cancel {
                if drag.started {
                    self.send("preview_cancel", json!({"token":token}));
                } else {
                    self.sketch_ui.drag = None;
                }
                return;
            }
            let changed = drag.submitted != Some(drag.target);
            if self.sketch_ui.branch_pending && !drag.released {
                return;
            }
            if changed && (drag.released || drag.last.elapsed() >= Duration::from_millis(85)) {
                let edit = json!({"op":"move_geometry","object":self.sketch,"index":drag.element.index,"point":drag.element.point,"position":drag.target});
                let deferred = self.sketch_ui.branch_pending;
                if let Some(d) = &mut self.sketch_ui.drag {
                    d.submitted = Some(d.target);
                    d.last = Instant::now();
                }
                if deferred {
                    self.transaction("Drag sketch geometry", vec![edit]);
                } else {
                    self.send(
                        "preview",
                        json!({"token":token,"label":"Drag sketch geometry","edits":[edit]}),
                    );
                }
                return;
            }
            if drag.released {
                if !drag.started {
                    self.sketch_ui.drag = None;
                    return;
                }
                let unchanged = self
                    .sketch_snapshot()
                    .is_some_and(|s| s["sketch"]["geometry"] == drag.original);
                let commit = drag.valid && !unchanged;
                self.send(
                    if commit {
                        "preview_commit"
                    } else {
                        "preview_cancel"
                    },
                    json!({"token":token}),
                );
                return;
            }
            if changed {
                ctx.request_repaint_after(Duration::from_millis(85));
            }
        }
        if !self.busy
            && self.sketch_ui.session.is_some()
            && self.sketch_ui.drag.is_none()
            && self.sketch_ui.dimension.is_none()
            && !ctx.wants_keyboard_input()
        {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Delete)) {
                if let Some(index) = self.sketch_ui.selected_constraint.take() {
                    self.transaction(
                        "Delete sketch constraint",
                        vec![json!({"op":"delete_constraint","object":self.sketch,"index":index})],
                    );
                    self.viewport.clear_sketch_selection();
                } else {
                    self.delete_geometry();
                }
                return;
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::D)) {
                self.smart_dimension();
                return;
            }
            for (key, tool) in [
                (Key::L, Tool::Line),
                (Key::P, Tool::Polyline),
                (Key::R, Tool::Rectangle),
                (Key::C, Tool::Circle),
                (Key::A, Tool::Arc),
            ] {
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                    self.sketch_ui.clear_points();
                    self.sketch_ui.tool = tool;
                    self.sketch_ui.trim = false;
                }
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::G)) {
                self.sketch_ui.construction = !self.sketch_ui.construction;
                self.toggle_construction();
            }
        }
    }
}
