//! Reference choices compile against the feature task's original native state.
use super::objects;
use eframe::egui;
use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct ProfileReferenceUi {
    state: Value,
    name: String,
}

impl ProfileReferenceUi {
    pub(super) fn new(state: &Value, name: String) -> Self {
        Self {
            state: state.clone(),
            name,
        }
    }

    pub(super) fn prepare(&self, parameters: &mut Value, value: &Value) {
        if self.foreign(parameters, value) {
            parameters["reference_choice"] = self.choice(parameters, value, "independent");
        }
    }

    pub(super) fn objects(&self) -> &[Value] {
        objects(&self.state)
    }

    pub(super) fn selections(
        &self,
        own: &str,
        selected: &[(String, String)],
    ) -> Vec<(String, String)> {
        selected
            .iter()
            .filter(|(name, _)| {
                name != own && self.objects().iter().any(|object| object["name"] == *name)
            })
            .cloned()
            .collect()
    }

    fn foreign(&self, parameters: &Value, value: &Value) -> bool {
        let source = value["link"].as_str().unwrap_or_default();
        let body = parameters["body"].as_str().unwrap_or_default();
        objects(&self.state)
            .iter()
            .find(|object| object["name"] == source)
            .is_some_and(|object| {
                object["type"] == "Sketcher::SketchObject" && object["body"] != body
            })
    }

    fn choice(&self, parameters: &Value, value: &Value, mode: &str) -> Value {
        json!({"state":self.state,"request":{
            "source":value["link"],"body":parameters["body"],"name":self.name,"mode":mode}})
    }

    pub(super) fn show(
        &self,
        ui: &mut egui::Ui,
        parameters: &mut Value,
        value: &Value,
        profile_changed: bool,
    ) -> bool {
        if !self.foreign(parameters, value) {
            return parameters
                .as_object_mut()
                .unwrap()
                .remove("reference_choice")
                .is_some();
        }
        let source = value["link"].as_str().unwrap_or_default();
        let previous = parameters.get("reference_choice");
        let had_choice = previous.is_some();
        let creating = parameters["create"] == true;
        let mut mode = previous
            .as_ref()
            .filter(|choice| choice["request"]["source"] == source)
            .and_then(|choice| choice["request"]["mode"].as_str())
            .unwrap_or(if creating || profile_changed {
                "independent"
            } else {
                "cross_reference"
            })
            .to_owned();
        ui.weak("Profile outside the target Body");
        for (id, label) in [
            ("independent", "Independent copy"),
            ("dependent", "Dependent copy"),
            ("cross_reference", "Cross-reference"),
        ] {
            ui.radio_value(&mut mode, id.to_owned(), label);
        }
        if !creating
            && !had_choice
            && !profile_changed
            && parameters["properties"].get("Profile").is_none()
            && mode == "cross_reference"
        {
            return false;
        }
        let request =
            json!({"source":source,"body":parameters["body"],"name":self.name,"mode":mode});
        let changed = previous.is_none_or(|choice| choice["request"] != request);
        if changed {
            parameters["reference_choice"] = json!({"state":self.state,"request":request});
            parameters["properties"]["Profile"] = value.clone();
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_default_and_source_stay_stable_across_copy_previews_and_local_selection_clears_choice()
     {
        let state = json!({"objects":[
            {"name":"Foreign","type":"Sketcher::SketchObject","body":"SourceBody"},
            {"name":"Local","type":"Sketcher::SketchObject","body":"TargetBody"}
        ]});
        let model = ProfileReferenceUi::new(&state, "Reference1".into());
        assert_eq!(
            model.selections(
                "Pad",
                &[
                    ("Reference1".into(), "Face1".into()),
                    ("Pad".into(), "Face1".into()),
                    ("Foreign".into(), String::new())
                ]
            ),
            vec![("Foreign".into(), String::new())]
        );
        let ctx = egui::Context::default();
        let mut parameters =
            json!({"create":true,"body":"TargetBody","properties":{"Profile":{"link":"Foreign"}}});
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(model.show(ui, &mut parameters, &json!({"link":"Foreign"}), false));
                assert_eq!(
                    parameters["reference_choice"]["request"]["mode"],
                    "independent"
                );
                assert!(!model.show(ui, &mut parameters, &json!({"link":"Foreign"}), false));
                parameters["reference_choice"]["request"]["mode"] = json!("dependent");
                assert!(!model.show(ui, &mut parameters, &json!({"link":"Foreign"}), false));
                assert_eq!(
                    parameters["reference_choice"]["request"]["source"],
                    "Foreign"
                );
                assert!(model.show(ui, &mut parameters, &json!({"link":"Local"}), true));
                assert!(parameters.get("reference_choice").is_none());
                let mut edit = json!({"create":false,"body":"TargetBody","properties":{}});
                assert!(!model.show(ui, &mut edit, &json!({"link":"Foreign"}), false));
                assert!(edit["properties"].as_object().unwrap().is_empty());
            });
        });
    }
}
