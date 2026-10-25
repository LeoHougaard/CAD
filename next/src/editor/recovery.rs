//! An unavailable document can only be closed. Keep its owner and UI state until
//! the native Close succeeds, with explicit confirmation before discarding work.
use super::*;

impl Editor {
    pub(super) fn unavailable(&self) -> bool {
        self.state["available"] == false
    }

    pub(super) fn reset_document_interaction(&mut self) {
        self.measurement_ui = measurements::MeasurementUi::default();
        self.sketch = None;
        self.selected_object = None;
        self.sketch_ui = SketchUi::default();
        self.sketch_creation = None;
        self.body_ui = BodyUi::default();
        self.property_ui = properties::PropertyUi::default();
        self.transform_ui = transforms::TransformUi::default();
        self.exchange_ui = exchange::ExchangeUi::default();
        self.draft = None;
        self.pending_file = None;
        self.file_dialog = None;
        self.file_browser = file_browser::FileBrowser::default();
        self.overwrite = false;
        self.fit_next = false;
        self.viewport.clear_selection();
        self.viewport.clear_sketch_selection();
    }

    pub(super) fn recovery_controls(&mut self, ui: &mut egui::Ui) {
        if ui
            .add_enabled(
                !self.busy && self.pending_file.is_none(),
                egui::Button::new("Close unavailable document"),
            )
            .clicked()
        {
            self.pending_file = Some(FileAction::Recover);
        }
    }

    pub(super) fn file_action_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_file else {
            return;
        };
        let recovery = matches!(action, FileAction::Recover);
        egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
            if recovery {
                ui.heading("Close unavailable document?");
                ui.label("Recovery failed. Close this document, then use New or Open to continue.");
                ui.label("Unsaved changes and the active operation will be lost. Saved files will not change.");
            } else {
                ui.label("Discard unsaved changes to this document?");
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.busy && (recovery || self.draft.is_none()),
                        egui::Button::new(if recovery {
                            "Close document and discard"
                        } else {
                            "Discard"
                        }),
                    )
                    .clicked()
                {
                    self.pending_file = None;
                    self.file_action(action);
                }
                if ui.button("Cancel").clicked() {
                    self.pending_file = None;
                }
            });
        });
    }
}
