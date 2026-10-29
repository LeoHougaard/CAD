//! Optional length entry for the just-completed native line.
use super::super::constraints::dimension_context;
use super::*;
use crate::sketch_commands::constrain;

pub(super) struct InlineLine {
    document: Value,
    revision: Value,
    sketch: String,
    session: String,
    index: usize,
    geometry_id: Value,
    snapshot: Value,
    value: String,
    pub(super) editing: bool,
    focus: bool,
    submitting: bool,
}

impl Editor {
    pub(super) fn inline_line_begin(&mut self, index: usize) {
        let (Some(sketch), Some(session), Some(snapshot)) = (
            self.sketch.clone(),
            self.sketch_ui.session.clone(),
            self.sketch_snapshot(),
        ) else {
            return;
        };
        let geometry = &snapshot["sketch"]["geometry"][index];
        let id = geometry["persistent_id"].clone();
        if geometry["kind"] != "line"
            || geometry["index"].as_u64() != Some(index as u64)
            || id.is_null()
        {
            return;
        }
        self.sketch_ui.inline_line = Some(InlineLine {
            document: self.state["document"].clone(),
            revision: self.state["revision"].clone(),
            sketch,
            session,
            index,
            geometry_id: id,
            snapshot,
            value: String::new(),
            editing: false,
            focus: false,
            submitting: false,
        });
    }
    fn inline_line_owner(&self, p: &InlineLine, revision: bool) -> bool {
        self.state["document"] == p.document
            && (!revision || self.state["revision"] == p.revision)
            && self.sketch.as_deref() == Some(p.sketch.as_str())
            && self.sketch_ui.session.as_deref() == Some(p.session.as_str())
            && matches!(self.sketch_ui.tool, Tool::Line | Tool::Polyline)
            && self
                .sketch_snapshot()
                .is_some_and(|s| s["sketch"]["geometry"][p.index]["persistent_id"] == p.geometry_id)
    }
    pub(super) fn inline_line_reply(&mut self, op: &str, success: bool) {
        let Some(mut p) = self.sketch_ui.inline_line.take() else {
            return;
        };
        if !p.submitting || op != "transaction" {
            self.sketch_ui.inline_line = Some(p);
            return;
        }
        if !self.inline_line_owner(&p, false) {
            return;
        }
        if success {
            // A reply can rebind the chain only when the accepted snapshot
            // contains exactly the intended new native driving constraint.
            let Some(current) = self.sketch_snapshot() else {
                return;
            };
            let before = &p.snapshot["sketch"];
            let after = &current["sketch"];
            let Some(old_constraints) = before["constraints"].as_array() else {
                return;
            };
            let Some(new_constraints) = after["constraints"].as_array() else {
                return;
            };
            let same_ids = before["geometry"]
                .as_array()
                .zip(after["geometry"].as_array())
                .is_some_and(|(a, b)| {
                    a.len() == b.len()
                        && a.iter()
                            .zip(b)
                            .all(|(x, y)| x["persistent_id"] == y["persistent_id"])
                });
            if self.state["revision"] == p.revision
                || !same_ids
                || new_constraints.len() != old_constraints.len() + 1
                || !new_constraints[..old_constraints.len()]
                    .iter()
                    .zip(old_constraints)
                    .all(|(a, b)| {
                        [
                            "type",
                            "first",
                            "first_pos",
                            "second",
                            "second_pos",
                            "third",
                            "third_pos",
                            "driving",
                            "active",
                            "virtual",
                            "name",
                        ]
                        .iter()
                        .all(|key| a[*key] == b[*key])
                    })
                || new_constraints.last().is_none_or(|c| {
                    c["type"] != "Distance"
                        || c["first"].as_i64() != Some(p.index as i64)
                        || c["driving"] != true
                })
            {
                return;
            }
            let endpoint = SketchElement {
                index: p.index as i32,
                point: 2,
            };
            if let Some(point) = self.viewport.geometry_point(endpoint) {
                self.sketch_ui.points = vec![point];
                self.sketch_ui.snaps = vec![Some(endpoint)];
            }
        } else if self.sketch_snapshot().as_ref() == Some(&p.snapshot) {
            // A native rollback may advance only its revision.
            p.revision = self.state["revision"].clone();
            p.submitting = false;
            p.focus = true;
            self.sketch_ui.inline_line = Some(p);
        }
    }
    fn inline_line_blocked(&self, ctx: &egui::Context, was_busy: bool) -> bool {
        was_busy
            || self.busy
            || self.unavailable()
            || self.units_ui.active()
            || self.measurement_is_open()
            || self.pending_file.is_some()
            || self.file_dialog.is_some()
            || self.overwrite
            || self.exchange_ui.active()
            || self.file_browser.active()
            || self.sketch_creation.is_some()
            || self.draft.is_some()
            || self.sketch_ui.task.is_some()
            || self.sketch_ui.dimension.is_some()
            || self.sketch_ui.drag.is_some()
            || egui::Popup::is_any_open(ctx)
            || super::super::workflow_guide::active(ctx)
    }
    pub(in crate::editor) fn inline_line_input(&mut self, ctx: &egui::Context, was_busy: bool) {
        let Some(p) = self.sketch_ui.inline_line.as_ref() else {
            return;
        };
        if !p.submitting && !self.inline_line_owner(p, true) {
            self.sketch_ui.inline_line = None;
            return;
        }
        if p.editing
            || p.submitting
            || self.inline_line_blocked(ctx, was_busy)
            || ctx.wants_keyboard_input()
        {
            return;
        }
        let text = ctx.input(|i| {
            if !i.modifiers.matches_exact(egui::Modifiers::NONE) {
                return None;
            }
            i.events.iter().find_map(|e| match e {
                egui::Event::Text(t)
                    if t.chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_digit() || matches!(c, '.' | '+' | '-')) =>
                {
                    Some(t.clone())
                }
                _ => None,
            })
        });
        if let Some(text) = text {
            ctx.input_mut(|i| {
                if let Some(at) = i
                    .events
                    .iter()
                    .position(|e| matches!(e, egui::Event::Text(t) if t == &text))
                {
                    i.events.remove(at);
                }
            });
            if let Some(p) = &mut self.sketch_ui.inline_line {
                p.value = text;
                p.editing = true;
                p.focus = true;
            }
        }
    }
    pub(in crate::editor) fn inline_line_foreign_focus(&self, ctx: &egui::Context) -> bool {
        self.sketch_ui.inline_line.is_some()
            && ctx.wants_keyboard_input()
            && ctx.memory(|m| {
                m.focused()
                    .is_some_and(|id| id != egui::Id::new("inline_line_length"))
            })
    }
    pub(in crate::editor) fn inline_line_cancel(
        &mut self,
        ctx: &egui::Context,
        was_busy: bool,
    ) -> bool {
        let Some(prompt) = &self.sketch_ui.inline_line else {
            return false;
        };
        if self.busy || was_busy {
            return true;
        }
        if prompt.editing {
            self.sketch_ui.inline_line = None;
            ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("inline_line_length")));
        } else {
            // An untouched prompt is part of the drawing command, so the
            // same Escape ends Line and clears its unfinished chain.
            self.sketch_escape();
        }
        true
    }
    pub(in crate::editor) fn inline_line_ui(&mut self, ctx: &egui::Context, was_busy: bool) {
        let Some(mut p) = self.sketch_ui.inline_line.take() else {
            return;
        };
        if !p.submitting && !self.inline_line_owner(&p, true) {
            return;
        }
        let blocked = self.inline_line_blocked(ctx, was_busy);
        if !p.editing || blocked || p.submitting {
            self.sketch_ui.inline_line = Some(p);
            return;
        }
        let endpoint = SketchElement {
            index: p.index as i32,
            point: 2,
        };
        let Some(point) = self.viewport.geometry_point(endpoint) else {
            return;
        };
        let position = self.viewport.sketch_screen_point(point) + egui::vec2(12., 12.);
        let mut apply = false;
        egui::Area::new(egui::Id::new("inline_line_length_area"))
            .fixed_pos(position)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut p.value)
                            .id(egui::Id::new("inline_line_length"))
                            .hint_text("Length")
                            .desired_width(100.),
                    );
                    if p.focus && (!ctx.wants_keyboard_input() || response.has_focus()) {
                        response.request_focus();
                        p.focus = false;
                    }
                    apply = (response.has_focus() || response.lost_focus())
                        && ctx.memory(|m| m.focused().is_none_or(|id| id == response.id))
                        && ctx.input(|i| {
                            i.modifiers.matches_exact(egui::Modifiers::NONE)
                                && i.key_pressed(Key::Enter)
                        });
                });
            });
        if apply {
            let selection = [SketchElement {
                index: p.index as i32,
                point: 0,
            }];
            let result = constrain(&p.snapshot, "Distance", &selection, Some(p.value.trim()));
            match result {
                Ok(mut edits) => {
                    let unit = self.state["units"]["defaults"]["mm"]["default_unit"]
                        .as_str()
                        .unwrap_or("")
                        .to_owned();
                    dimension_context(&mut edits, &unit);
                    p.submitting = true;
                    self.sketch_ui.inline_line = Some(p);
                    self.transaction("Set line length", edits);
                    if !self.busy
                        && let Some(p) = &mut self.sketch_ui.inline_line
                    {
                        p.submitting = false;
                        p.focus = true;
                    }
                    return;
                }
                Err(error) => self.error = Some(error),
            }
        }
        self.sketch_ui.inline_line = Some(p);
    }
}
