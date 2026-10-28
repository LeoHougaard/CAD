//! Sketch support is native document data. This chooser holds no transaction.
use super::{Editor, objects};
use crate::sketch_creation_commands;
use eframe::egui;
use serde_json::{Value, json};

pub(super) struct SketchCreation {
    state: Value,
    request: Value,
    pending: bool,
    show_options: bool,
    waiting_for_guide: bool,
    reversed: bool,
    detach: bool,
    position: [f64; 3],
    quantity_offset: Option<Value>,
    axis: [f64; 3],
    angle: f64,
    expression: bool,
    offset_expression: String,
}

impl SketchCreation {
    fn native_request(&self) -> Result<Value, String> {
        let mut request = self.request.clone();
        if self.reversed {
            request["reversed"] = json!(true);
        }
        if self.position != [0.; 3] || self.angle != 0. {
            if !self
                .position
                .iter()
                .chain(self.axis.iter())
                .all(|v| v.is_finite())
                || !self.angle.is_finite()
            {
                return Err("Enter finite sketch offset and rotation values".into());
            }
            if self.angle != 0. && self.axis.iter().all(|v| *v == 0.) {
                return Err("Choose a nonzero rotation axis".into());
            }
            request["offset"] = json!({"position":self.position,"rotation":[0.,0.,0.,1.],"axis_angle":{"axis":self.axis,"angle":self.angle}});
        }
        if let Some(offset) = &self.quantity_offset {
            request["offset"] = offset.clone();
        }
        if self.expression {
            request["offset_expression"] = json!(self.offset_expression);
        }
        if self.detach {
            request["detach"] = json!(true);
        }
        Ok(request)
    }
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
        self.start_sketch_creation(false);
    }

    pub(super) fn new_sketch_with_options(&mut self) {
        self.start_sketch_creation(true);
    }

    fn start_sketch_creation(&mut self, show_options: bool) {
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
        let selected = selected.or_else(|| {
            self.viewport
                .selected_standard_plane()
                .map(|plane| json!({"kind":"origin","role":plane.role()}))
        });
        let apply = selected.is_some() && !show_options;
        let waiting_for_guide = create_body && selected.is_none() && !show_options;
        self.sketch_creation = Some(SketchCreation {
            state: self.state.clone(),
            request: json!({"name":name,"body":body,"create_body":create_body,
                "support":selected.unwrap_or_else(|| json!({"kind":"origin","role":"XY_Plane"}))}),
            pending: false,
            show_options,
            waiting_for_guide,
            reversed: false,
            detach: false,
            position: [0.; 3],
            quantity_offset: None,
            axis: [0., 0., 1.],
            angle: 0.,
            expression: false,
            offset_expression: String::new(),
        });
        if apply {
            self.create_attached_sketch();
        }
    }

    pub(super) fn awaiting_standard_plane(&self) -> bool {
        self.sketch_creation
            .as_ref()
            .is_some_and(|task| task.waiting_for_guide && !task.pending)
    }

    pub(super) fn choose_standard_plane(&mut self, plane: crate::viewport::StandardPlane) {
        let Some(task) = &mut self.sketch_creation else {
            return;
        };
        if !task.waiting_for_guide || task.pending {
            return;
        }
        task.request["support"] = json!({"kind":"origin","role":plane.role()});
        task.waiting_for_guide = false;
        self.create_attached_sketch();
    }

    fn create_attached_sketch(&mut self) {
        let Some(creation) = &self.sketch_creation else {
            return;
        };
        if self.busy || creation.pending {
            return;
        }
        let request = match creation.native_request() {
            Ok(request) => request,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let edits = match sketch_creation_commands::build(&creation.state, &request) {
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
        self.send(
            "create_sketch",
            json!({"label":if request["detach"] == true {
                "Create unattached sketch"
            } else {
                "Create attached sketch"
            },"edits":edits}),
        );
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
        if task.pending {
            return;
        }
        if task.waiting_for_guide {
            let mut cancel = false;
            egui::Window::new("Select a sketch plane")
                .id(egui::Id::new("new_sketch_guide"))
                .default_pos([180., 85.])
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Select Top, Front or Right in the viewport.");
                    cancel = ui.button("Cancel").clicked();
                });
            if cancel {
                self.sketch_creation = None;
            }
            return;
        }
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
                egui::CollapsingHeader::new("Attachment options")
                    .default_open(task.show_options)
                    .show(ui, |ui| {
                        ui.checkbox(&mut task.reversed, "Reverse normal");
                        ui.checkbox(&mut task.detach, "Unattached")
                            .on_hover_text("Keep the computed plane and position without following this support");
                        ui.label("Offset in the support frame");
                        ui.horizontal(|ui| {
                            for (i, label) in ["X", "Y", "Z"].iter().enumerate() {
                                ui.label(*label);
                                let offset = task.quantity_offset.get_or_insert_with(|| json!({"position":[0.,0.,0.],"rotation":[0.,0.,0.,1.],"axis_angle":{"axis":[0.,0.,1.],"angle":0.}}));
                                ui.add_enabled_ui(!task.expression || i != 2, |ui| {
                                    super::units::quantity_field(ui, &mut offset["position"][i], &task.state["units"]["defaults"]["mm"]);
                                });
                            }
                        });
                        ui.checkbox(&mut task.expression, "Normal offset expression");
                        if task.expression {
                            ui.text_edit_singleline(&mut task.offset_expression)
                                .on_hover_text("Native length expression, for example Base.Height / 2 or 0.5 in");
                        }
                        ui.label("Rotation axis in the support frame");
                        ui.horizontal(|ui| {
                            for (i, label) in ["X", "Y", "Z"].iter().enumerate() {
                                ui.label(*label);
                                if ui.add(egui::DragValue::new(&mut task.axis[i]).speed(0.05)).changed()
                                    && let Some(offset) = &mut task.quantity_offset { offset["axis_angle"]["axis"] = json!(task.axis); }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Angle");
                            let offset = task.quantity_offset.get_or_insert_with(|| json!({"position":[0.,0.,0.],"rotation":[0.,0.,0.,1.],"axis_angle":{"axis":task.axis,"angle":0.}}));
                            super::units::quantity_field(ui, &mut offset["axis_angle"]["angle"], &task.state["units"]["defaults"]["deg"]);
                        });
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

    fn creation() -> SketchCreation {
        SketchCreation {
            state: Value::Null,
            request: json!({"name":"Sketch","body":"Body","create_body":true,
                "support":{"kind":"origin","role":"XY_Plane"}}),
            pending: false,
            show_options: true,
            waiting_for_guide: false,
            reversed: false,
            detach: false,
            position: [0.; 3],
            quantity_offset: None,
            axis: [0., 0., 1.],
            angle: 0.,
            expression: false,
            offset_expression: String::new(),
        }
    }

    #[test]
    fn unchanged_creation_options_preserve_existing_command_payload() {
        let creation = creation();
        assert_eq!(creation.native_request().unwrap(), creation.request);
    }

    #[test]
    fn rotated_offset_fields_compile_the_local_axis_and_preserve_native_formula() {
        let mut creation = creation();
        creation.position = [1., 2., 3.];
        creation.angle = 180.;
        creation.axis = [0., 2., 0.];
        creation.reversed = true;
        creation.expression = true;
        creation.offset_expression = "Base.Height / 2".into();
        creation.detach = true;
        let request = creation.native_request().unwrap();
        assert_eq!(request["offset"]["position"], json!([1., 2., 3.]));
        assert_eq!(
            request["offset"]["axis_angle"],
            json!({"axis":[0.,2.,0.],"angle":180.})
        );
        assert_eq!(request["offset_expression"], "Base.Height / 2");
        assert_eq!(request["reversed"], true);
        assert_eq!(request["detach"], true);
    }

    #[test]
    fn rotation_axis_must_define_an_actual_rotation() {
        let mut creation = creation();
        creation.angle = 30.;
        creation.axis = [0.; 3];
        assert!(creation.native_request().is_err());
        creation.angle = 0.;
        assert_eq!(creation.native_request().unwrap(), creation.request);
    }
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

#[cfg(test)]
mod native_tests;
