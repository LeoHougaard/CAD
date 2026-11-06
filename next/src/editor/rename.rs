//! Rename a completed sketch through its native Label transaction.
use super::{Editor, consume_exact_key, objects, workflow_guide};
use eframe::egui::{self, Key};
use serde_json::{Value, json};

fn id() -> egui::Id {
    egui::Id::new("completed_sketch_rename")
}
fn field_id() -> egui::Id {
    egui::Id::new("completed_sketch_rename_field")
}

#[derive(Clone)]
struct Rename {
    document: Value,
    revision: Value,
    name: String,
    original: String,
    text: String,
    focus: bool,
    validation: Option<&'static str>,
}

pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Rename>(id()).is_some())
}
pub(super) fn cancel(ctx: &egui::Context) -> bool {
    if !is_open(ctx) {
        return false;
    }
    ctx.data_mut(|d| d.remove::<Rename>(id()));
    ctx.memory_mut(|m| m.surrender_focus(field_id()));
    true
}

impl Editor {
    fn rename_owner(&self, rename: &Rename) -> bool {
        self.state["document"] == rename.document
            && self.state["revision"] == rename.revision
            && self.sketch_ui.session.is_none()
            && objects(&self.state).iter().any(|o| {
                o["name"] == rename.name
                    && o["type"] == "Sketcher::SketchObject"
                    && crate::property_commands::writable(o, "Label")
            })
    }

    pub(super) fn rename_shortcut(&mut self, ctx: &egui::Context, was_busy: bool, blocked: bool) {
        if was_busy
            || blocked
            || self.busy
            || self.unavailable()
            || self.state["recovery_only"] == true
            || self.sketch_ui.session.is_some()
            || workflow_guide::active(ctx)
            || egui::Popup::is_any_open(ctx)
            || ctx.wants_keyboard_input()
            || is_open(ctx)
            || self.tree_selections().len() != 1
        {
            return;
        }
        let Some(object) = objects(&self.state).iter().find(|o| {
            o["name"].as_str() == self.tree_selections().first().map(String::as_str)
                && o["type"] == "Sketcher::SketchObject"
                && crate::property_commands::writable(o, "Label")
        }) else {
            return;
        };
        let (Some(name), Some(label), Some(_document), Some(_revision)) = (
            object["name"].as_str(),
            object["label"].as_str(),
            self.state["document"].as_str(),
            self.state["revision"].as_u64(),
        ) else {
            return;
        };
        let rename = Rename {
            document: self.state["document"].clone(),
            revision: self.state["revision"].clone(),
            name: name.into(),
            original: label.into(),
            text: label.into(),
            focus: true,
            validation: None,
        };
        if consume_exact_key(ctx, egui::Modifiers::SHIFT, Key::N) {
            ctx.data_mut(|d| d.insert_temp(id(), rename));
        }
    }

    pub(super) fn rename_dialog(&mut self, ctx: &egui::Context, was_busy: bool) {
        let Some(mut rename) = ctx.data(|d| d.get_temp::<Rename>(id())) else {
            return;
        };
        if was_busy || self.busy || self.unavailable() || !self.rename_owner(&rename) {
            cancel(ctx);
            return;
        }
        if workflow_guide::active(ctx)
            || egui::Popup::is_any_open(ctx)
            || self.units_ui.active()
            || self.measurement_is_open()
            || self.pending_file.is_some()
            || self.file_dialog.is_some()
            || self.overwrite
            || self.exchange_ui.active()
            || self.file_browser.active()
            || self.sketch_creation.is_some()
            || self.draft.is_some()
        {
            return;
        }
        let mut apply = false;
        let mut dismiss = false;
        egui::Modal::new(id()).show(ctx, |ui| {
            ui.heading("Rename sketch");
            let mut output = egui::TextEdit::singleline(&mut rename.text)
                .id(field_id())
                .desired_width(240.)
                .show(ui);
            if output.response.changed() {
                rename.validation = None;
            }
            if rename.focus {
                output.response.request_focus();
                output
                    .state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(rename.text.chars().count()),
                    )));
                output.state.store(ctx, output.response.id);
                rename.focus = false;
            }
            if let Some(message) = rename.validation {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
            if output.response.lost_focus()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter))
            {
                apply = true;
            }
            ui.horizontal(|ui| {
                apply |= ui.button("Rename").clicked();
                dismiss |= ui.button("Cancel").clicked();
            });
        });
        if dismiss {
            cancel(ctx);
        } else if apply {
            let text = rename.text.trim();
            if text.is_empty() {
                rename.validation = Some("Sketch name cannot be empty");
                rename.focus = true;
            } else {
                cancel(ctx);
                if text != rename.original && self.rename_owner(&rename) {
                    self.send(
                        "transaction",
                        json!({"label":"Rename sketch",
                        "edits":[{"op":"set", "object":rename.name,
                            "property":"Label", "value":text}]}),
                    );
                }
                return;
            }
        }
        if !dismiss {
            ctx.data_mut(|d| d.insert_temp(id(), rename));
        }
    }
}
