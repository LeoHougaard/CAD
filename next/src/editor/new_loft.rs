//! Ordered semantic sections; native Part::Loft owns all geometry.
use super::*;

pub(super) fn section_source(state: &Value, name: &str, target: &str) -> Result<Value, String> {
    let source = objects(state).iter().find(|o| o["name"] == name)
        .ok_or("Selected source is stale; select it again")?;
    if source["effective_visibility"] != true { return Err("Choose a visible source".into()); }
    if !source["loft_source"].is_object() {
        return Err("Choose a sketch section, origin or vertex; datum axes are not sections".into());
    }
    if target == "Origin" && source["type"] != "Sketcher::SketchObject" {
        return Err("Choose a sketch origin".into());
    }
    if !target.is_empty() && target != "Origin" && !target.starts_with("Vertex") {
        // A sketch face explicitly means its complete outside profile.
        if !target.starts_with("Face") || source["type"] != "Sketcher::SketchObject" { return Err("Choose a sketch face, origin or vertex".into()); }
    }
    let mut reference = source["loft_source"].clone();
    reference["target"] = json!(if target.starts_with("Face") { "" } else { target });
    if target.starts_with("Vertex") {
        let mapped = source["loft_vertices"][target].as_str().filter(|s| !s.is_empty())
            .ok_or("Native vertex identity is stale; select it again")?;
        reference["mapped_name"] = json!(mapped);
    }
    Ok(reference)
}

impl Editor {
    pub(super) fn new_loft_fields(&mut self, ui: &mut egui::Ui) {
        let selected = self.native_link_selection();
        let draft = self.draft.as_mut().unwrap();
        let state = &self.state;
        if draft.parameters.get("loft_sections").is_none() {
            let existing = objects(state).iter().find(|o| o["name"] == draft.parameters["name"]);
            draft.parameters["loft_sections"] = existing.map(|o| o["loft_sections"].clone())
                .filter(|v| v.is_array()).unwrap_or(json!([]));
            if draft.parameters["create"] != true {
                draft.parameters["loft_feature_id"] = existing.map(|o| o["native_id"].clone()).unwrap_or(Value::Null);
            }
        }
        ui.label(&draft.name);
        if let Some(error) = &self.error { ui.colored_label(Color32::from_rgb(225,142,138), error); }
        ui.add_enabled_ui(!self.busy && state["available"] != false, |ui| {
            ui.toggle_value(&mut self.property_ui.loft_point_pick, "Show sketch origins");
            let before = draft.parameters.clone();
            let sections = draft.parameters["loft_sections"].as_array_mut().unwrap();
            let mut remove = None;
            let mut move_up = None;
            let mut correct = None;
            for (index, section) in sections.iter().enumerate() {
                ui.push_id(index, |ui| { ui.horizontal(|ui| {
                    ui.label(format!("{}  {} {}", index+1, section["link"].as_str().unwrap_or_default(),
                        match section["target"].as_str().unwrap_or_default() {"Origin"=>"origin", ""=>"profile", v=>v}));
                    if index > 0 && ui.small_button("Up").clicked() { move_up=Some(index); }
                    if ui.small_button("Correct").clicked() { correct=Some(index); }
                    if ui.small_button("Remove").clicked() { remove=Some(index); }
                }); });
            }
            let picks = selected.iter().map(|(name,target)| section_source(state,name,target)).collect::<Result<Vec<_>,_>>();
            if let Some(i) = correct {
                match &picks { Ok(p) if p.len()==1 => sections[i]=p[0].clone(),
                    _ => self.error=Some("Select one source to correct this section".into()) }
            }
            if let Some(i)=remove {sections.remove(i);}
            if let Some(i)=move_up {sections.swap(i-1,i);}
            if ui.button("Append selection").clicked() {
                match picks {Ok(p) if !p.is_empty()=>sections.extend(p),
                    Ok(_)=>self.error=Some("Select a source to append".into()),Err(e)=>self.error=Some(e)}
            }
            ui.menu_button("Append origin", |ui| {
                for source in objects(state).iter().filter(|o| o["type"] == "Sketcher::SketchObject" && o["loft_source"].is_object() && o["effective_visibility"]==true) {
                    let name=source["name"].as_str().unwrap_or_default();
                    if ui.button(format!("{name} origin")).clicked() {
                        if let Ok(section)=section_source(state,name,"Origin") { sections.push(section); }
                        ui.close();
                    }
                }
            });
            for property in ["Solid","Ruled","Closed"] {
                let mut value = draft.parameters["properties"].get(property).and_then(Value::as_bool)
                    .or_else(|| self.property_ui.schema["properties"][property].as_bool()).unwrap_or(property=="Solid");
                if ui.checkbox(&mut value, property).changed() { draft.parameters["properties"][property]=json!(value); }
            }
            if draft.parameters != before {draft.changed=Some(Instant::now());draft.valid=false;}
        });
    }
}
