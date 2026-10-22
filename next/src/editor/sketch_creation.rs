//! Sketch support is native document data. This chooser holds no transaction.
use super::{Editor, objects};
use crate::sketch_creation_commands;
use eframe::egui;
use serde_json::{Value, json};

pub(super) struct SketchCreation {
    state: Value,
    request: Value,
    pending: bool,
}

fn reference_support(state: &Value, body: &str, name: &str, face: Option<&str>) -> Option<Value> {
    let object = objects(state)
        .iter()
        .find(|object| object["name"] == name)?;
    if object["body"] != body {
        return None;
    }
    if let Some(face) = face.filter(|face| face.starts_with("Face")) {
        return Some(
            json!({"kind":"reference","object":name,"subelements":[face],"mode":"FlatFace"}),
        );
    }
    let mode = match object["type"].as_str()? {
        "App::Plane" | "PartDesign::Plane" => "FlatFace",
        "Sketcher::SketchObject" => "ObjectXY",
        _ => return None,
    };
    Some(json!({"kind":"reference","object":name,"subelements":[""],"mode":mode}))
}

fn support_label(state: &Value, support: &Value) -> String {
    if support["kind"] == "origin" {
        return support["role"]
            .as_str()
            .unwrap_or_default()
            .replace('_', " ");
    }
    let name = support["object"].as_str().unwrap_or_default();
    let label = objects(state)
        .iter()
        .find(|object| object["name"] == name)
        .and_then(|object| object["label"].as_str())
        .unwrap_or(name);
    let element = support["subelements"][0].as_str().unwrap_or_default();
    if element.is_empty() {
        label.to_owned()
    } else {
        format!("{label}.{element}")
    }
}

impl Editor {
    pub(super) fn new_sketch(&mut self) {
        if self.busy
            || self.sketch_creation.is_some()
            || self.draft.is_some()
            || self.sketch_ui.session.is_some()
        {
            return;
        }
        let name = self.unique("Sketch");
        let existing_body = self.body();
        let create_body = existing_body.is_none();
        let body = existing_body.unwrap_or_else(|| self.unique("Body"));
        let selected = self
            .viewport
            .selected
            .as_ref()
            .and_then(|selection| {
                reference_support(
                    &self.state,
                    &body,
                    &selection.object,
                    Some(&selection.subelement),
                )
            })
            .or_else(|| {
                self.selected_object
                    .as_deref()
                    .and_then(|name| reference_support(&self.state, &body, name, None))
            });
        let apply = selected.is_some();
        self.sketch_creation = Some(SketchCreation {
            state: self.state.clone(),
            request: json!({"name":name,"body":body,"create_body":create_body,
                "support":selected.unwrap_or_else(|| json!({"kind":"origin","role":"XY_Plane"}))}),
            pending: false,
        });
        if apply {
            self.create_attached_sketch();
        }
    }

    fn create_attached_sketch(&mut self) {
        let Some(creation) = &self.sketch_creation else {
            return;
        };
        if self.busy || creation.pending {
            return;
        }
        let edits = match sketch_creation_commands::build(&creation.state, &creation.request) {
            Ok(edits) => edits,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let name = creation.request["name"].as_str().unwrap().to_owned();
        self.sketch_creation.as_mut().unwrap().pending = true;
        self.prepare_new_sketch();
        self.selected_object = Some(name.clone());
        self.sketch = Some(name);
        self.transaction("Create attached sketch", edits);
    }

    pub(super) fn sketch_creation_reply(&mut self, op: &str, success: bool) {
        if op != "transaction"
            || !self
                .sketch_creation
                .as_ref()
                .is_some_and(|task| task.pending)
        {
            return;
        }
        if success {
            let task = self.sketch_creation.take().unwrap();
            self.body_ui.active = task.request["body"].as_str().map(str::to_owned);
            let name = task.request["name"].as_str().unwrap().to_owned();
            self.body_ui.selected = vec![name.clone()];
            self.selected_object = Some(name);
            self.viewport.clear_selection();
        } else if let Some(task) = &mut self.sketch_creation {
            task.pending = false;
            // Failed native rollback may change revision and generated aliases.
            task.state = self.state.clone();
        }
    }

    pub(super) fn sketch_creation_dialog(&mut self, ctx: &egui::Context) {
        let Some(task) = &mut self.sketch_creation else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("new_sketch_support")).show(ctx, |ui| {
            ui.heading("Sketch plane");
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::from_rgb(225, 142, 138), error);
            }
            ui.add_enabled_ui(!self.busy && !task.pending, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(330.)
                    .show(ui, |ui| {
                        for role in ["XY_Plane", "XZ_Plane", "YZ_Plane"] {
                            ui.selectable_value(
                                &mut task.request["support"],
                                json!({"kind":"origin","role":role}),
                                role.replace('_', " "),
                            );
                        }
                        if task.request["create_body"] != true {
                            let body = task.request["body"].as_str().unwrap_or_default().to_owned();
                            for object in objects(&task.state) {
                                let name = object["name"].as_str().unwrap_or_default();
                                if matches!(
                                    object["type"].as_str(),
                                    Some("PartDesign::Plane" | "Sketcher::SketchObject")
                                ) && let Some(support) =
                                    reference_support(&task.state, &body, name, None)
                                {
                                    let label = support_label(&task.state, &support);
                                    ui.selectable_value(
                                        &mut task.request["support"],
                                        support,
                                        label,
                                    );
                                }
                            }
                        }
                        // Retain an explicitly picked face when retrying a rejected
                        // native attachment; changing to an origin plane is explicit.
                        if task.request["support"]["kind"] == "reference" {
                            ui.label(support_label(&task.state, &task.request["support"]));
                        }
                    });
                ui.horizontal(|ui| {
                    apply = ui.button("Create sketch").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        });
        if cancel {
            self.sketch_creation = None;
        } else if apply {
            self.create_attached_sketch();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_support_preserves_native_frames_and_rejects_other_bodies_and_nonplanes() {
        let state = json!({"objects":[
            {"name":"Plane","type":"PartDesign::Plane","body":"Body"},
            {"name":"Sketch","type":"Sketcher::SketchObject","body":"Body"},
            {"name":"Line","type":"PartDesign::Line","body":"Body"},
            {"name":"Foreign","type":"PartDesign::Plane","body":"Other"},
            {"name":"Pad","type":"PartDesign::Pad","body":"Body"}
        ]});
        assert_eq!(
            reference_support(&state, "Body", "Plane", None).unwrap(),
            json!({"kind":"reference","object":"Plane","subelements":[""],"mode":"FlatFace"})
        );
        assert_eq!(
            reference_support(&state, "Body", "Sketch", None).unwrap()["mode"],
            "ObjectXY"
        );
        assert_eq!(
            reference_support(&state, "Body", "Pad", Some("Face7")).unwrap()["subelements"],
            json!(["Face7"])
        );
        for name in ["Foreign", "Line", "Pad"] {
            assert!(reference_support(&state, "Body", name, None).is_none());
        }
    }
}
