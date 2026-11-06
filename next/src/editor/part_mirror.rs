//! Root native Part::Mirroring task; references come from the model snapshot.
use super::*;

pub(super) fn reference(state: &Value, name: &str, target: &str, plane: bool) -> Result<Value,String> {
    if !target.is_empty() { return Err("Choose a whole solid or plane object".into()); }
    let obj=objects(state).iter().find(|o| o["name"]==name).ok_or("Selection is stale")?;
    obj[if plane {"mirror_plane_source"} else {"mirror_source"}].as_object()
        .map(|_| obj[if plane {"mirror_plane_source"} else {"mirror_source"}].clone())
        .ok_or_else(|| "Choose a solid Body/Part output or a supported native plane in its global frame".into())
}
pub(super) fn initial(state: &Value, selections: &[(String,String)]) -> Result<Value,String> {
    if selections.len()!=1 { return Err("Select one whole solid Body or Part output".into()); }
    Ok(json!({"source":reference(state,&selections[0].0,&selections[0].1,false)?,
        "plane":null,"axis":"XY","offset":0.0,"feature_id":null}))
}
impl Editor {
    pub(super) fn part_mirror_fields(&mut self, ui: &mut egui::Ui) {
        let selection=self.native_link_selection();
        let draft=self.draft.as_mut().unwrap();
        if draft.parameters.get("part_mirror").is_none() {
            draft.parameters["part_mirror"]=objects(&self.state).iter()
                .find(|o| o["name"]==draft.parameters["name"])
                .map(|o|o["part_mirror"].clone()).unwrap_or(Value::Null);
        }
        ui.label("Mirror part");
        if let Some(e)=&self.error { ui.colored_label(Color32::from_rgb(225,142,138),e); }
        ui.add_enabled_ui(!self.busy && self.state["available"]!=false, |ui| {
            let task=&mut draft.parameters["part_mirror"];
            let before=task.clone();
            ui.label(format!("Source: {}",task["source"]["link"].as_str().unwrap_or("Unavailable")));
            if ui.button("Use selected solid").clicked() {
                match selection.first().filter(|_|selection.len()==1)
                    .ok_or_else(||"Select one whole solid".to_owned())
                    .and_then(|(n,t)|reference(&self.state,n,t,false)) {
                    Ok(v)=>task["source"]=v,Err(e)=>self.error=Some(e)
                }
            }
            ui.label(format!("Plane: {}",task["plane"]["link"].as_str().unwrap_or("Origin plane")));
            if ui.button("Use selected plane").clicked() {
                match selection.first().filter(|_|selection.len()==1)
                    .ok_or_else(||"Select one native plane".to_owned())
                    .and_then(|(n,t)|reference(&self.state,n,t,true)) {
                    Ok(v)=>task["plane"]=v,Err(e)=>self.error=Some(e)
                }
            }
            ui.horizontal(|ui| { for axis in ["XY","XZ","YZ"] {
                if ui.selectable_label(task["plane"].is_null() && task["axis"]==axis,axis).clicked() {
                    task["plane"]=Value::Null; task["axis"]=json!(axis);
                }
            }});
            if task["plane"].is_null() {
                let mut offset=task["offset"].as_f64().unwrap_or(0.);
                if ui.add(egui::DragValue::new(&mut offset).prefix("Offset ").suffix(" mm")).changed() {
                    task["offset"]=json!(offset);
                }
            }
            if *task!=before {draft.changed=Some(Instant::now());draft.valid=false;}
        });
    }
}
