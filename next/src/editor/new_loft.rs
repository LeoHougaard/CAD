//! Ordered semantic sections; native Part::Loft owns all geometry.
use super::*;

pub(super) fn section_source(state: &Value, name: &str, target: &str) -> Result<Value, String> {
    let source = objects(state).iter().find(|o| o["name"] == name)
        .ok_or("Selected source is stale; select it again")?;
    if source["effective_visibility"] != true { return Err("Choose a visible source".into()); }
    if matches!(source["type"].as_str(), Some("App::Line" | "PartDesign::Line" | "App::Origin")) {
        return Err("Choose a profile or endpoint, not an axis".into());
    }
    let mut reference = source["loft_source"].clone();
    super::special_reference_collection::validate_identity(state, source, &reference, "source_id")?;
    if target == "Origin" && source["type"] != "Sketcher::SketchObject" {
        return Err("Choose a sketch origin".into());
    }
    let vertex = super::special_reference_collection::element_index(target, "Vertex");
    let face = super::special_reference_collection::element_index(target, "Face");
    if !target.is_empty() && target != "Origin" && !vertex
        && !(face && source["type"] == "Sketcher::SketchObject") {
        return Err("Choose a profile, sketch origin or vertex".into());
    }
    reference["target"] = json!(if face { "" } else { target });
    if vertex {
        let mapped = source["loft_vertices"][target].as_str().filter(|s| !s.is_empty())
            .ok_or("Native vertex identity is stale; select it again")?;
        reference["mapped_name"] = json!(mapped);
    }
    Ok(reference)
}

impl Editor {
    pub(super) fn new_loft_fields(&mut self, ui: &mut egui::Ui) {
        let mut activate = None;
        let draft = self.draft.as_mut().unwrap();
        if draft.parameters.get("loft_sections").is_none() {
            let existing = objects(&self.state).iter().find(|o| o["name"] == draft.parameters["name"]);
            draft.parameters["loft_sections"] = existing.map(|o| o["loft_sections"].clone())
                .filter(Value::is_array).unwrap_or(json!([]));
            if draft.parameters["create"] != true {
                draft.parameters["loft_feature_id"] = existing.map(|o| o["native_id"].clone()).unwrap_or(Value::Null);
            }
        }
        ui.label("Loft");
        if let Some(error) = &self.error { ui.colored_label(Color32::from_rgb(225,142,138), error); }
        ui.add_enabled_ui(!self.busy && self.state["available"] != false, |ui| {
            let before = draft.parameters.clone();
            ui.label("Profiles");
            let sections = draft.parameters["loft_sections"].as_array_mut().unwrap();
            let mut remove = None;
            let mut move_row = None;
            // A single ordered field, with each occupied row clickable for correction.
            egui::Frame::group(ui.style()).show(ui, |ui| {
                for (index, section) in sections.iter().enumerate() {
                    ui.push_id(index, |ui| { ui.horizontal(|ui| {
                        ui.weak(format!("{}", index + 1));
                        let key = format!("Sections:{index}");
                        let mut display = super::special_reference_collection::loft_display(section);
                        let response = super::reference_field::show(ui, &mut display, objects(&self.state),
                            self.property_ui.active_reference.as_deref() == Some(key.as_str()));
                        if response.activated { activate = Some(key); }
                        if response.changed { remove = Some(index); }
                        ui.menu_button("?", |ui| {
                            if index > 0 && ui.button("Move up").clicked() { move_row = Some((index,index-1)); ui.close(); }
                            if index+1 < sections.len() && ui.button("Move down").clicked() { move_row = Some((index,index+1)); ui.close(); }
                        });
                    }); });
                }
                ui.push_id("append_profile", |ui| {
                    let mut empty = Value::Null;
                    if super::reference_field::show(ui, &mut empty, objects(&self.state),
                        self.property_ui.active_reference.as_deref() == Some("Sections")).activated {
                        activate = Some("Sections".to_owned());
                    }
                });
            });
            if let Some(index) = remove { sections.remove(index); activate = Some("Sections".into()); }
            if let Some((a,b)) = move_row {
                let mut ordered = sections.clone(); ordered.swap(a,b);
                match super::special_reference_collection::sections_valid(&self.state, &ordered) {
                    Ok(()) => { *sections = ordered; activate = Some("Sections".into()); }
                    Err(error) => self.error = Some(error),
                }
            }
            if std::env::var_os("CAD_NATIVE_TOOLS").is_some() {
                ui.collapsing("Native loft options", |ui| {
                    for property in ["Solid", "Ruled", "Closed"] {
                        let mut value = draft.parameters["properties"].get(property).and_then(Value::as_bool)
                            .or_else(|| self.property_ui.schema["properties"][property].as_bool()).unwrap_or(property == "Solid");
                        if ui.checkbox(&mut value, property).changed() { draft.parameters["properties"][property] = json!(value); }
                    }
                });
            }
            if draft.parameters != before { draft.changed = Some(Instant::now()); draft.valid = false; }
        });
        if let Some(key) = activate { self.activate_special_reference(key); }
    }
}
