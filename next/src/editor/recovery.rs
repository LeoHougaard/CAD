//! An unavailable document can only be closed. Keep its owner and UI state until
//! the native Close succeeds, with explicit confirmation before discarding work.
use super::*;

impl Editor {
    pub(super) fn unavailable(&self) -> bool {
        self.state["available"] == false
    }

    pub(super) fn source_recovery(&self) -> bool {
        self.state["recovery_only"] == true
            && self.state["recovery_available"] == true
            && self.state["save_recovery"]["dirty"] == true
            && self.state["save_recovery"]["live_graph_restored"] == false
            && self.state["save_recovery"]["pending_edges"]
                .as_array()
                .is_some_and(|edges| !edges.is_empty())
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
        if let Some(dependents) = self.state["close_dependents"].as_array() {
            for (index, owner) in dependents.iter().enumerate() {
                if let (Some(document), Some(revision), Some(owner_revision)) = (
                    owner["document"].as_str(),
                    owner["revision"].as_u64(),
                    self.state["revision"].as_u64(),
                ) {
                    if ui
                        .add_enabled(
                            !self.busy && self.pending_file.is_none(),
                            egui::Button::new(format!("Close dependent {document}...")),
                        )
                        .clicked()
                    {
                        self.pending_file = Some(FileAction::RecoverDependent {
                            index,
                            owner_revision,
                            revision,
                            remove_references: false,
                        });
                    }
                    if owner["reference_removals"]
                        .as_array()
                        .is_some_and(|edits| !edits.is_empty())
                        && ui
                            .add_enabled(
                                !self.busy && self.pending_file.is_none(),
                                egui::Button::new(format!("Remove references in {document}...")),
                            )
                            .clicked()
                    {
                        self.pending_file = Some(FileAction::RecoverDependent {
                            index,
                            owner_revision,
                            revision,
                            remove_references: true,
                        });
                    }
                }
            }
        }
    }

    pub(super) fn file_action_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_file else {
            return;
        };
        let recovery = matches!(
            action,
            FileAction::Recover | FileAction::RecoverDependent { .. }
        );
        egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
            if let FileAction::RecoverDependent { index, remove_references, .. } = action {
                let owner = &self.state["close_dependents"][index];
                ui.heading(format!("{} {}?", if remove_references { "Remove references in" } else { "Close dependent" }, owner["document"].as_str().unwrap_or_default()));
                ui.label(owner["path"].as_str().unwrap_or_default());
                if remove_references {
                    ui.label("Remove only these native Link objects. Both documents stay open; other unsaved work is retained.");
                    for edit in owner["reference_removals"].as_array().into_iter().flatten() {
                        ui.label(format!("{} (native ID {})", edit["object"].as_str().unwrap_or_default(), edit["native_id"]));
                    }
                } else {
                    ui.label("This dependent's unsaved changes will be lost. The unavailable document stays open.");
                }
            } else if recovery {
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
                        egui::Button::new(if matches!(action, FileAction::RecoverDependent { remove_references: true, .. }) {
                            "Remove listed references"
                        } else if recovery {
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
