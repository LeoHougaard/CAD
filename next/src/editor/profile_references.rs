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
    ) -> Result<Vec<(String, String)>, String> {
        // The edited feature and its temporary copy are intentional exclusions.
        // Every other identity must be checked, rather than silently dropping
        // a stale pick and accepting only the surviving part of a selection.
        let references: Vec<_> = selected
            .iter()
            .filter(|(name, _)| name != own && name != &self.name)
            .cloned()
            .collect();
        crate::property_commands::validate_selected_objects(self.objects(), &references)?;
        Ok(references)
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
    fn missing_picks_reject_before_filtering_and_own_copy_exclusion_is_explicit() {
        let model = ProfileReferenceUi::new(
            &json!({"objects":[{"name":"A"},{"name":"B"}]}),
            "Copy".into(),
        );
        let picks = |items: &[(&str, &str)]| {
            items
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            model
                .selections(
                    "Result",
                    &picks(&[
                        ("Result", "Face1"),
                        ("Copy", "Face2"),
                        ("A", "Face2"),
                        ("A", "Face1")
                    ])
                )
                .unwrap(),
            picks(&[("A", "Face2"), ("A", "Face1")])
        );
        assert!(
            model
                .selections("Result", &picks(&[("Result", "Face1"), ("Copy", "")]))
                .unwrap()
                .is_empty()
        );
        for selected in [
            picks(&[("A", ""), ("Gone", "")]),
            picks(&[("Result", "Face1"), ("Gone", "Face2")]),
            picks(&[("Gone", "")]),
        ] {
            let error = model.selections("Result", &selected).unwrap_err();
            assert!(error.contains("Gone") && error.contains("stale"));
        }
    }

    #[test]
    fn native_default_and_source_stay_stable_across_copy_previews_and_local_selection_clears_choice()
     {
        let state = json!({"objects":[
            {"name":"Foreign","type":"Sketcher::SketchObject","body":"SourceBody"},
            {"name":"Local","type":"Sketcher::SketchObject","body":"TargetBody"}
        ]});
        let model = ProfileReferenceUi::new(&state, "Reference1".into());
        assert_eq!(
            model
                .selections(
                    "Pad",
                    &[
                        ("Reference1".into(), "Face1".into()),
                        ("Pad".into(), "Face1".into()),
                        ("Foreign".into(), String::new())
                    ]
                )
                .unwrap(),
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
