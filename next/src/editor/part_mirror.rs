//! Native Part::Mirroring references, collected from the active field.
use super::*;

pub(super) fn reference(state: &Value, name: &str, target: &str, plane: bool) -> Result<Value,String> {
    if !target.is_empty() { return Err("Choose a whole solid or supported plane".into()); }
    let obj = objects(state).iter().find(|o| o["name"] == name).ok_or("Selection is stale")?;
    if obj["effective_visibility"] != true { return Err("Choose a visible entity".into()); }
    if plane && !matches!(obj["type"].as_str(), Some("App::Plane" | "PartDesign::Plane" | "Part::Plane")) {
        return Err("Choose a supported plane".into());
    }
    let reference = obj[if plane {"mirror_plane_source"} else {"mirror_source"}].clone();
    super::special_reference_collection::validate_identity(state, obj, &reference, "native_id")?;
    Ok(reference)
}
pub(super) fn initial(state: &Value, selections: &[(String,String)]) -> Result<Value,String> {
    if selections.len() != 1 { return Err("Select one whole solid output".into()); }
    Ok(json!({"source":reference(state,&selections[0].0,&selections[0].1,false)?,
        "plane":null,"axis":"XY","offset":0.0,"feature_id":null}))
}
impl Editor {
    pub(super) fn part_mirror_fields(&mut self, ui: &mut egui::Ui) {
        let mut activate = None;
        let draft = self.draft.as_mut().unwrap();
        if draft.parameters.get("part_mirror").is_none() {
            draft.parameters["part_mirror"] = objects(&self.state).iter()
                .find(|o| o["name"] == draft.parameters["name"])
                .map(|o| o["part_mirror"].clone()).unwrap_or(Value::Null);
        }
        ui.label("Mirror");
        if let Some(e) = &self.error { ui.colored_label(Color32::from_rgb(225,142,138),e); }
        ui.add_enabled_ui(!self.busy && self.state["available"] != false, |ui| {
            let task = &mut draft.parameters["part_mirror"];
            let before = task.clone();
            for (key, path, label) in [("Source","source","Entities to mirror"),("MirrorPlane","plane","Mirror plane")] {
                ui.push_id(key, |ui| {
                    ui.label(label);
                    let response = super::reference_field::show(ui, &mut task[path], objects(&self.state),
                        self.property_ui.active_reference.as_deref() == Some(key));
                    if response.activated || response.changed { activate = Some(key.to_owned()); }
                });
            }
            if task["plane"].is_null() {
                let label = match task["axis"].as_str() { Some("XY") => "XY plane", Some("XZ") => "XZ plane", Some("YZ") => "YZ plane", _ => "Select a mirror plane" };
                ui.weak(label);
            }
            if std::env::var_os("CAD_NATIVE_TOOLS").is_some() {
                ui.collapsing("Native mirror options", |ui| {
                    ui.horizontal(|ui| { for axis in ["XY","XZ","YZ"] {
                        if ui.selectable_label(task["plane"].is_null() && task["axis"] == axis,axis).clicked() {
                            task["plane"] = Value::Null; task["axis"] = json!(axis);
                        }
                    }});
                    if task["plane"].is_null() {
                        let mut offset = task["offset"].as_f64().unwrap_or(0.);
                        if ui.add(egui::DragValue::new(&mut offset).prefix("Offset ").suffix(" mm")).changed() { task["offset"] = json!(offset); }
                    }
                });
            }
            if *task != before { draft.changed = Some(Instant::now()); draft.valid = false; }
        });
        if let Some(key) = activate { self.activate_special_reference(key); }
    }
}
