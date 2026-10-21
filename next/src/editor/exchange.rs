//! Native geometry exchange uses the same guarded worker and authoritative document.
use super::{Editor, objects};
use eframe::egui;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Import,
    Export,
}

#[derive(Default)]
pub(super) struct ExchangeUi {
    mode: Option<Mode>,
    path: String,
    selected: Vec<String>,
    overwrite: bool,
    pending: Option<Mode>,
}
impl ExchangeUi {
    pub(super) fn active(&self) -> bool {
        self.mode.is_some() || self.pending.is_some()
    }
    pub(super) fn cancel(&mut self) -> bool {
        if self.pending.is_some() {
            return false;
        }
        self.mode.take().is_some()
    }
}

fn supported_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            ["step", "stp", "iges", "igs"].contains(&extension.to_ascii_lowercase().as_str())
        })
}
fn exportable(entry: &Value) -> bool {
    let shape = &entry["shape"];
    shape["null"] == false
        && shape["valid"] == true
        && shape["vertices"].as_u64().unwrap_or(0) > 0
        && ["min", "max"].iter().all(|key| {
            shape["bounds"][key].as_array().is_some_and(|coordinates| {
                coordinates.len() == 3
                    && coordinates.iter().all(|value| {
                        value
                            .as_f64()
                            .is_some_and(|value| value.is_finite() && value.abs() < 1e50)
                    })
            })
        })
}

impl Editor {
    fn exchange_ready(&self) -> bool {
        !self.busy
            && self.state["document"].is_string()
            && self.sketch_ui.session.is_none()
            && self.draft.is_none()
            && self.file_dialog.is_none()
            && !self.overwrite
            && self.pending_file.is_none()
            && !self.exchange_ui.active()
    }
    pub(super) fn exchange_controls(&mut self, ui: &mut egui::Ui) {
        let ready = self.exchange_ready();
        if ui
            .add_enabled(ready, egui::Button::new("Import"))
            .on_hover_text("Import STEP or IGES into this native document")
            .clicked()
        {
            self.begin_exchange(false);
        }
        if ui
            .add_enabled(
                ready && objects(&self.state).iter().any(exportable),
                egui::Button::new("Export"),
            )
            .on_hover_text("Export selected native objects as STEP or IGES")
            .clicked()
        {
            self.begin_exchange(true);
        }
    }
    pub(super) fn begin_exchange(&mut self, export: bool) {
        if !self.exchange_ready() {
            return;
        }
        self.exchange_ui.mode = Some(if export { Mode::Export } else { Mode::Import });
        self.exchange_ui.overwrite = false;
        self.exchange_ui.selected.clear();
        if export {
            let selected = if !self.tree_selections().is_empty() {
                self.tree_selections().to_vec()
            } else if !self.viewport.selections.is_empty() {
                self.viewport
                    .selections
                    .iter()
                    .map(|entry| entry.object.clone())
                    .collect()
            } else {
                self.selected_object.iter().cloned().collect()
            };
            for name in selected {
                if objects(&self.state)
                    .iter()
                    .any(|entry| entry["name"] == name && exportable(entry))
                    && !self.exchange_ui.selected.contains(&name)
                {
                    self.exchange_ui.selected.push(name);
                }
            }
            if self.exchange_ui.path.is_empty() && !self.path.is_empty() {
                self.exchange_ui.path = PathBuf::from(&self.path)
                    .with_extension("step")
                    .to_string_lossy()
                    .into_owned();
            }
        }
    }
    pub(super) fn exchange_dialog(&mut self, ctx: &egui::Context) {
        let Some(mode) = self.exchange_ui.mode else {
            return;
        };
        let exporting = mode == Mode::Export;
        let choices: Vec<_> = objects(&self.state)
            .iter()
            .filter(|entry| exportable(entry))
            .filter_map(|entry| {
                Some((
                    entry["name"].as_str()?.to_owned(),
                    entry["label"].as_str().unwrap_or_default().to_owned(),
                ))
            })
            .collect();
        let mut execute = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("exchange_dialog")).show(ctx,|ui| {
            ui.heading(if exporting {"Export geometry"} else {"Import geometry"});
            ui.label("STEP / IGES path");
            if ui.add(egui::TextEdit::singleline(&mut self.exchange_ui.path).desired_width(420.)).changed() {
                self.exchange_ui.overwrite=false;
            }
            if exporting {
                ui.label("Native objects").on_hover_text("Selected root objects retain native local placement. Select an enclosing Part to retain its assembly hierarchy and placement.");
                egui::ScrollArea::vertical().max_height(200.).show(ui,|ui| {
                    for (name,label) in &choices {
                        let mut selected=self.exchange_ui.selected.contains(name);
                        let text=if label.is_empty() || label==name {name.clone()} else {format!("{label} ({name})")};
                        if ui.checkbox(&mut selected,text).changed() {
                            if selected {self.exchange_ui.selected.push(name.clone());}
                            else {self.exchange_ui.selected.retain(|existing|existing!=name);}
                        }
                    }
                });
                if Path::new(&self.exchange_ui.path).exists() {
                    ui.checkbox(&mut self.exchange_ui.overwrite,"Replace existing file");
                }
            }
            let ready=!self.busy && supported_path(&self.exchange_ui.path) && (!exporting ||
                (!self.exchange_ui.selected.is_empty() && (!Path::new(&self.exchange_ui.path).exists() || self.exchange_ui.overwrite)));
            ui.horizontal(|ui| {
                execute=ui.add_enabled(ready,egui::Button::new(if exporting {"Export"} else {"Import"})).clicked();
                cancel=ui.add_enabled(!self.busy,egui::Button::new("Cancel")).clicked();
            });
        });
        if cancel {
            self.exchange_ui.mode = None;
        }
        if execute {
            self.exchange_ui.mode = None;
            self.exchange_ui.pending = Some(mode);
            if exporting {
                self.send("export_exchange",json!({"path":self.exchange_ui.path,"objects":self.exchange_ui.selected,"overwrite":self.exchange_ui.overwrite}));
            } else {
                self.transaction(
                    "Import STEP/IGES",
                    vec![json!({"op":"import_exchange","path":self.exchange_ui.path})],
                );
            }
            if !self.busy {
                self.exchange_ui.pending = None;
                self.exchange_ui.mode = Some(mode);
            }
        }
    }
    pub(super) fn exchange_reply(&mut self, op: &str, accepted: bool) {
        let Some(mode) = self.exchange_ui.pending else {
            return;
        };
        if (mode == Mode::Import && op != "transaction")
            || (mode == Mode::Export && op != "export_exchange")
        {
            return;
        }
        self.exchange_ui.pending = None;
        if accepted {
            if mode == Mode::Import {
                self.viewport.fit();
            } else {
                self.status = format!("Exported {}", self.exchange_ui.path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supported_extensions_are_explicit_and_case_insensitive() {
        for path in ["part.step", "part.STP", "assembly.IGES", "surface.igs"] {
            assert!(supported_path(path));
        }
        for path in [
            "part.FCStd",
            "part.cadpart",
            "part.obj",
            "part.step.exe",
            "",
        ] {
            assert!(!supported_path(path));
        }
    }
    #[test]
    fn native_export_selection_includes_surfaces_and_rejects_infinite_datums() {
        let mut entry = json!({"shape":{"null":false,"valid":true,"solids":0,"faces":2,"vertices":8,"bounds":{"min":[0,0,0],"max":[10,20,30]}}});
        assert!(exportable(&entry));
        entry["shape"]["bounds"]["max"][0] = json!(1e100);
        assert!(!exportable(&entry));
        entry["shape"]["bounds"]["max"][0] = json!(10);
        entry["shape"]["valid"] = json!(false);
        assert!(!exportable(&entry));
    }
    #[test]
    fn escape_cancels_only_an_idle_exchange_dialog() {
        let mut ui = ExchangeUi {
            mode: Some(Mode::Import),
            ..Default::default()
        };
        assert!(ui.cancel());
        assert!(!ui.active());
        assert!(!ui.cancel());
        ui.mode = Some(Mode::Export);
        ui.pending = Some(Mode::Export);
        assert!(!ui.cancel());
        assert!(ui.active() && ui.mode == Some(Mode::Export));
    }
}
