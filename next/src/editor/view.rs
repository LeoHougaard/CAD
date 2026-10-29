//! View controls use viewport state; visibility edits go through native document transactions.
use super::{Editor, objects};
use crate::{
    body_commands::body_of,
    viewport::{DisplayMode, SelectionMode, StandardView},
};
use eframe::egui;
use serde_json::{Value, json};
use std::collections::BTreeSet;

impl Editor {
    pub(super) fn view_controls(&mut self, ui: &mut egui::Ui) {
        let mut visibility = None;
        let mutable = !self.busy
            && self.draft.is_none()
            && self.sketch_creation.is_none()
            && self.sketch_ui.session.is_none()
            && self.sketch_ui.drag.is_none()
            && self.sketch_ui.dimension.is_none()
            && self.pending_file.is_none()
            && self.file_dialog.is_none()
            && !self.overwrite;
        let mut fit_objects = self.body_ui.selected.clone();
        if fit_objects.is_empty() {
            fit_objects.extend(self.selected_object.iter().cloned());
        }
        for name in &mut fit_objects {
            if let Some(display) = objects(&self.state)
                .iter()
                .find(|object| object["name"] == name.as_str())
                .and_then(|object| object["body_state"]["display_object"].as_str())
            {
                *name = display.to_owned();
            }
        }
        ui.menu_button("View", |ui| {
            for (label, view) in [
                ("Front", StandardView::Front),
                ("Back", StandardView::Back),
                ("Left", StandardView::Left),
                ("Right", StandardView::Right),
                ("Top", StandardView::Top),
                ("Bottom", StandardView::Bottom),
                ("Isometric", StandardView::Isometric),
                ("Trimetric", StandardView::Trimetric),
            ] {
                if ui.button(label).clicked() {
                    self.viewport.set_view(view);
                    ui.close();
                }
            }
            if ui.button("Fit all").clicked() {
                self.viewport.fit();
                ui.close();
            }
            if ui
                .add_enabled(
                    self.viewport.can_fit_objects(&fit_objects),
                    egui::Button::new("Fit selection"),
                )
                .clicked()
            {
                self.viewport.fit_objects(&fit_objects);
                ui.close();
            }
            ui.separator();
            for (label, mode) in [
                ("Shaded", DisplayMode::Shaded),
                ("Shaded with edges", DisplayMode::Edges),
                ("Wireframe", DisplayMode::Wireframe),
            ] {
                if ui
                    .selectable_value(&mut self.viewport.display_mode, mode, label)
                    .clicked()
                {
                    ui.close();
                }
            }
            ui.separator();
            ui.menu_button("Selection", |ui| {
                for (label, mode) in [
                    ("Faces and edges", SelectionMode::All),
                    ("Faces", SelectionMode::Faces),
                    ("Edges", SelectionMode::Edges),
                    ("Vertices", SelectionMode::Vertices),
                ] {
                    if mode == SelectionMode::Vertices && !self.viewport.has_vertices() {
                        continue;
                    }
                    if ui
                        .selectable_value(&mut self.viewport.selection_mode, mode, label)
                        .clicked()
                    {
                        ui.close();
                    }
                }
            });
            ui.separator();
            let has_selection = !self.viewport.selections.is_empty()
                || self.viewport.selected.is_some()
                || self.selected_object.is_some();
            if ui
                .add_enabled(mutable && has_selection, egui::Button::new("Hide selected"))
                .clicked()
            {
                visibility = Some((false, false));
                ui.close();
            }
            if ui
                .add_enabled(mutable && has_selection, egui::Button::new("Show selected"))
                .clicked()
            {
                visibility = Some((true, false));
                ui.close();
            }
            if ui
                .add_enabled(mutable, egui::Button::new("Show all"))
                .clicked()
            {
                visibility = Some((true, true));
                ui.close();
            }
        });
        if let Some((show, all)) = visibility {
            let selected: Vec<String> = if !self.viewport.selections.is_empty() {
                self.viewport
                    .selections
                    .iter()
                    .map(|selection| selection.object.clone())
                    .collect()
            } else if let Some(selection) = &self.viewport.selected {
                vec![selection.object.clone()]
            } else {
                self.selected_object.iter().cloned().collect()
            };
            let edits = visibility_edits(&self.state, &selected, show, all);
            if !edits.is_empty() {
                self.transaction(
                    if show {
                        "Show objects"
                    } else {
                        "Hide selected objects"
                    },
                    edits,
                );
            }
        }
    }
}

fn visibility_edits(
    state: &Value,
    selected: &[String],
    show: bool,
    all_objects: bool,
) -> Vec<Value> {
    let all = objects(state);
    let mut targets = BTreeSet::new();
    if all_objects {
        for object in all {
            if object["type"] == "PartDesign::Body" {
                if let Some(name) = object["name"].as_str() {
                    targets.insert(name.to_owned());
                }
                if let Some(tip) = object["body_state"]["tip"].as_str() {
                    targets.insert(tip.to_owned());
                }
            } else if object["body"].as_str().is_none()
                && object["shape"]["null"] == false
                && let Some(name) = object["name"].as_str()
            {
                targets.insert(name.to_owned());
            }
        }
    } else {
        for name in selected {
            let target = body_of(state, name).unwrap_or_else(|| name.clone());
            if show
                && let Some(object) = all.iter().find(|object| object["name"] == target)
                && let Some(tip) = object["body_state"]["tip"].as_str()
            {
                targets.insert(tip.to_owned());
            }
            targets.insert(target);
        }
    }
    targets
        .into_iter()
        .filter_map(|name| {
            let object = all.iter().find(|object| object["name"] == name)?;
            let current = object["properties"]["Visibility"].as_bool()?;
            (current != show)
                .then(|| json!({"op":"set","object":name,"property":"Visibility","value":show}))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Value {
        json!({"objects":[
            {"name":"Body","type":"PartDesign::Body","body":"Body","body_state":{"tip":"Pad"},"properties":{"Visibility":true}},
            {"name":"OldPad","type":"PartDesign::Pad","body":"Body","properties":{"Visibility":false}},
            {"name":"Pad","type":"PartDesign::Pad","body":"Body","properties":{"Visibility":true}},
            {"name":"ToolBody","type":"PartDesign::Body","body":"ToolBody","body_state":{"tip":"ToolPad","consumed_by":["Boolean"]},"properties":{"Visibility":false}},
            {"name":"ToolPad","type":"PartDesign::Pad","body":"ToolBody","properties":{"Visibility":false}}
        ]})
    }
    #[test]
    fn hiding_multiple_native_faces_changes_the_owning_body_once() {
        let state = snapshot();
        let edits = visibility_edits(
            &state,
            &["Pad".into(), "OldPad".into(), "Pad".into()],
            false,
            false,
        );
        assert_eq!(
            edits,
            vec![json!({"op":"set","object":"Body","property":"Visibility","value":false})]
        );
        assert_eq!(state["objects"][0]["properties"]["Visibility"], true);
    }
    #[test]
    fn show_selected_restores_only_selected_body_and_its_current_tip() {
        let edits = visibility_edits(&snapshot(), &["ToolPad".into()], true, false);
        let names: Vec<_> = edits
            .iter()
            .map(|edit| edit["object"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["ToolBody", "ToolPad"]);
        assert!(visibility_edits(&snapshot(), &["Missing".into()], false, false).is_empty());
    }
    #[test]
    fn show_all_restores_body_and_tip_visibility_without_resurrecting_history() {
        let edits = visibility_edits(&snapshot(), &[], true, true);
        let names: Vec<_> = edits
            .iter()
            .map(|edit| edit["object"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["ToolBody", "ToolPad"]);
        assert!(edits.iter().all(|edit| edit["value"] == true));
        // Native consumed_by still decides whether a tool Body can be displayed.
        assert_eq!(
            snapshot()["objects"][3]["body_state"]["consumed_by"],
            json!(["Boolean"])
        );
    }
}
