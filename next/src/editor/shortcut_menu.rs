//! Context-sensitive tool access. Opening or dismissing this menu never edits a model.
use super::{Editor, consume_exact_key, workflow_guide};
use crate::sketch_commands::Tool;
use eframe::egui::{self, Key};
use serde_json::{Value, json};

fn id() -> egui::Id {
    egui::Id::new("tool_shortcut_menu")
}

pub(super) fn is_open(ctx: &egui::Context) -> bool {
    egui::Popup::is_id_open(ctx, id())
}

#[derive(Clone)]
struct MenuOwner {
    state: Value,
    position: egui::Pos2,
}

impl Editor {
    pub(super) fn shortcut_menu(&mut self, ctx: &egui::Context, was_busy: bool, blocked: bool) {
        let ready = !was_busy
            && !self.busy
            && !self.unavailable()
            && self.state["document"].is_string()
            && !self.units_ui.active()
            && !self.measurement_is_open()
            && self.pending_file.is_none()
            && self.file_dialog.is_none()
            && !self.overwrite
            && !self.exchange_ui.active()
            && !self.file_browser.active()
            && self.sketch_creation.is_none()
            && self.draft.is_none()
            && self.sketch_ui.drag.is_none()
            && self.sketch_ui.dimension.is_none()
            && self.sketch_ui.task.is_none()
            && !workflow_guide::active(ctx)
            && !ctx.wants_keyboard_input();
        let owner = json!({"document":self.state["document"],"revision":self.state["revision"],
            "sketch":self.sketch,"session":self.sketch_ui.session});
        if ready && !blocked && consume_exact_key(ctx, egui::Modifiers::NONE, Key::S) {
            let position = ctx
                .pointer_latest_pos()
                .unwrap_or_else(|| ctx.content_rect().center());
            ctx.data_mut(|d| {
                d.insert_temp(
                    id(),
                    MenuOwner {
                        state: owner.clone(),
                        position,
                    },
                )
            });
            egui::Popup::open_id(ctx, id());
        }
        if !is_open(ctx) {
            return;
        }
        let captured = ctx.data(|d| d.get_temp::<MenuOwner>(id()));
        let Some(captured) = captured.filter(|s| ready && s.state == owner) else {
            egui::Popup::close_id(ctx, id());
            return;
        };
        enum Action {
            Sketch,
            Extrude,
            Revolve,
            Draw(Tool),
            Dimension,
        }
        let mut action = None;
        let layer = ctx
            .layer_id_at(captured.position)
            .unwrap_or(egui::LayerId::background());
        egui::Popup::new(id(), ctx.clone(), captured.position, layer)
            .open_memory(None)
            .width(260.)
            .show(|ui| {
                if self.sketch_ui.session.is_some() {
                    egui::Grid::new("sketch_shortcut_tools")
                        .num_columns(4)
                        .show(ui, |ui| {
                            for (i, (label, tool)) in [
                                ("Select", Tool::Select),
                                ("Line", Tool::Line),
                                ("Rectangle", Tool::Rectangle),
                                ("Circle", Tool::Circle),
                                ("Point", Tool::Point),
                                ("Polyline", Tool::Polyline),
                                ("Center rectangle", Tool::CenterRectangle),
                                ("Arc", Tool::Arc),
                            ]
                            .into_iter()
                            .enumerate()
                            {
                                if ui.button(label).clicked() {
                                    action = Some(Action::Draw(tool));
                                    ui.close();
                                }
                                if i % 4 == 3 {
                                    ui.end_row();
                                }
                            }
                            if ui.button("Dimension").clicked() {
                                action = Some(Action::Dimension);
                                ui.close();
                            }
                            if ui.button("Revolve").clicked() {
                                action = Some(Action::Revolve);
                                ui.close();
                            }
                        });
                } else {
                    ui.horizontal(|ui| {
                        for (label, command) in [
                            ("Sketch", Action::Sketch),
                            ("Extrude", Action::Extrude),
                            ("Revolve", Action::Revolve),
                        ] {
                            if ui.button(label).clicked() {
                                action = Some(command);
                                ui.close();
                            }
                        }
                    });
                }
            });
        match action {
            Some(Action::Sketch) => self.new_sketch(),
            Some(Action::Extrude) => self.feature_from_sketch("feature.pad"),
            Some(Action::Revolve) => self.feature_from_sketch("feature.revolution"),
            Some(Action::Draw(tool)) => self.activate_sketch_drawing_tool(tool),
            Some(Action::Dimension) => self.start_dimension_tool(),
            None => {}
        }
    }
}
