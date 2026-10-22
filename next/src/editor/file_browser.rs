//! One modal browser for native documents and geometry exchange.
use super::Editor;
use eframe::egui;
use egui_file_dialog::{DialogState, FileDialog, FileDialogStorage, OpeningMode};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Purpose {
    Open,
    Save,
    Import,
    Export,
}
impl Purpose {
    fn saving(self) -> bool {
        matches!(self, Self::Save | Self::Export)
    }
    fn accepts(self, path: &Path) -> bool {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        match self {
            Self::Open | Self::Save => extension.eq_ignore_ascii_case("FCStd"),
            Self::Import | Self::Export => ["step", "stp", "iges", "igs"]
                .iter()
                .any(|allowed| extension.eq_ignore_ascii_case(allowed)),
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Open => "Open native document",
            Self::Save => "Save native document",
            Self::Import => "Import geometry",
            Self::Export => "Choose export destination",
        }
    }
}

#[derive(Default)]
pub(super) struct FileBrowser {
    active: Option<(Purpose, FileDialog)>,
    storage: FileDialogStorage,
}
impl FileBrowser {
    pub(super) fn active(&self) -> bool {
        self.active.is_some()
    }
    fn begin(&mut self, purpose: Purpose, initial: &Path) {
        let directory = if initial.as_os_str().is_empty() {
            self.storage
                .last_visited_dir
                .clone()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        } else if initial.is_dir() {
            initial.to_owned()
        } else {
            initial
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(Path::to_owned)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        };
        let default = initial
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(if purpose == Purpose::Save {
                "Part.FCStd"
            } else {
                "Part.step"
            });
        let mut dialog = FileDialog::new()
            .id("cad_file_browser")
            .title(purpose.title())
            .as_modal(true)
            .storage(self.storage.clone())
            .opening_mode(OpeningMode::AlwaysInitialDir)
            .initial_directory(directory)
            .default_file_name(default)
            .default_size(egui::vec2(760., 500.))
            .min_size(egui::vec2(360., 280.))
            .allow_file_overwrite(true);
        if matches!(purpose, Purpose::Open | Purpose::Save) {
            dialog = dialog
                .add_file_filter(
                    "FreeCAD (.FCStd)",
                    Arc::new(move |path| purpose.accepts(path)),
                )
                .default_file_filter("FreeCAD (.FCStd)")
                .add_save_extension("FreeCAD (.FCStd)", "FCStd")
                .default_save_extension("FreeCAD (.FCStd)");
        } else {
            dialog = dialog
                .add_file_filter("STEP / IGES", Arc::new(move |path| purpose.accepts(path)))
                .default_file_filter("STEP / IGES")
                .add_save_extension("STEP (.step)", "step")
                .add_save_extension("IGES (.iges)", "iges")
                .default_save_extension(
                    if initial
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| {
                            extension.eq_ignore_ascii_case("iges")
                                || extension.eq_ignore_ascii_case("igs")
                        })
                    {
                        "IGES (.iges)"
                    } else {
                        "STEP (.step)"
                    },
                );
        }
        if purpose.saving() {
            dialog.save_file();
        } else {
            dialog.pick_file();
        }
        self.active = Some((purpose, dialog));
    }
    /// Drop only browser interaction; no native model operation has started yet.
    fn cancel(&mut self) -> Option<Purpose> {
        let (purpose, mut dialog) = self.active.take()?;
        self.storage = dialog.storage_mut().clone();
        Some(purpose)
    }
    fn update(&mut self, ctx: &egui::Context) -> Option<(Purpose, Option<PathBuf>)> {
        let (purpose, dialog) = self.active.as_mut()?;
        dialog.config_mut().max_size = Some(ctx.content_rect().size() - egui::vec2(24., 24.));
        dialog.update(ctx);
        let purpose = *purpose;
        if let Some(path) = dialog.take_picked() {
            self.cancel();
            Some((purpose, Some(path)))
        } else if matches!(dialog.state(), DialogState::Cancelled | DialogState::Closed) {
            self.cancel();
            Some((purpose, None))
        } else {
            None
        }
    }
}

impl Editor {
    pub(super) fn browse_file(&mut self, purpose: Purpose, initial: &str) {
        self.file_browser.begin(purpose, Path::new(initial));
    }
    fn browser_canceled(&mut self, purpose: Purpose) {
        match purpose {
            Purpose::Open | Purpose::Save => self.file_dialog = None,
            Purpose::Import | Purpose::Export => self.exchange_browser_result(purpose, None),
        }
    }
    pub(super) fn cancel_file_browser(&mut self) -> bool {
        let Some(purpose) = self.file_browser.cancel() else {
            return false;
        };
        self.browser_canceled(purpose);
        true
    }
    pub(super) fn file_browser_ui(&mut self, ctx: &egui::Context) {
        let Some((purpose, path)) = self.file_browser.update(ctx) else {
            return;
        };
        let Some(path) = path else {
            self.browser_canceled(purpose);
            return;
        };
        if !purpose.accepts(&path) {
            self.browser_canceled(purpose);
            self.error = Some(
                if matches!(purpose, Purpose::Open | Purpose::Save) {
                    "Choose a native FreeCAD filename ending in .FCStd"
                } else {
                    "Choose a STEP or IGES filename (.step, .stp, .iges, .igs)"
                }
                .into(),
            );
            return;
        }
        match purpose {
            Purpose::Open | Purpose::Save => {
                self.file_dialog = None;
                self.file_input = path.to_string_lossy().into_owned();
                // SaveFile has already confirmed any existing native destination.
                self.send(
                    if purpose == Purpose::Save {
                        "save"
                    } else {
                        "open"
                    },
                    json!({"path":self.file_input}),
                );
            }
            Purpose::Import | Purpose::Export => self.exchange_browser_result(purpose, Some(path)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_formats_do_not_route_legacy_or_unrelated_files_to_native_document_open() {
        for purpose in [Purpose::Open, Purpose::Save] {
            assert!(purpose.accepts(Path::new("Unicode part caf\u{00e9}.FCStd")));
            assert!(purpose.accepts(Path::new("part.fcstd")));
            for path in ["part.cadpart", "part.step", "part.FCStd1", "part"] {
                assert!(!purpose.accepts(Path::new(path)));
            }
        }
        for purpose in [Purpose::Import, Purpose::Export] {
            for path in ["part.STEP", "part.stp", "surface.IGES", "curve.igs"] {
                assert!(purpose.accepts(Path::new(path)));
            }
            assert!(!purpose.accepts(Path::new("part.FCStd")));
        }
    }
    #[test]
    fn canceling_browser_returns_its_action_without_starting_a_document_operation() {
        let mut browser = FileBrowser::default();
        assert!(!browser.active());
        browser.begin(Purpose::Save, Path::new("Part.FCStd"));
        assert!(browser.active());
        assert_eq!(browser.cancel(), Some(Purpose::Save));
        assert!(!browser.active());
        assert_eq!(browser.cancel(), None);
    }
}
