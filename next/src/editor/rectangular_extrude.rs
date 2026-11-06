//! Bounded semantic route. Native descriptors and quantity replies are authority.
use super::{Draft, Editor, objects};
use eframe::egui;
use serde_json::{Value, json};
use std::time::Instant;

pub(super) const ID: &str = "native.rectangular_extrude";

pub(super) fn compile(p: &Value) -> Result<Vec<Value>, String> {
    if p["operation"] != "New" || p["termination"] != "Blind" || p["side"] != "one-side"
        || p["solid"] != true || p["expression"] != "" || p["direction"] != "Normal"
        || p["profile_input"] != p["source"]["name"] {
        return Err("Rectangle supports Solid, New, Blind, One side and literal quantities. Cancel to start another route.".into());
    }
    if p["parsed_input"] != p["magnitude"] || !p["quantity_pending"].is_null() {
        return Err("Wait for the current native draft quantity".into());
    }
    let angle = crate::extrusion_commands::signed_magnitude(&p["parsed_magnitude"], &p["inward"])?;
    let length = p["length"].as_str().filter(|s| !s.is_empty()).ok_or("Enter Length")?;
    let mut edit = json!({"op":if p["target"].is_object() {"update_rectangular_draft"} else {"create_rectangular_draft"},
        "operation":p["operation"],"termination":p["termination"],"side":p["side"],
        "length":length,"angle":format!("{angle} deg"),"reversed":p["reversed"]});
    if p["target"].is_object() { edit["target"] = p["target"].clone(); }
    else { for key in ["source","body","boundary","name"] { edit[key] = p[key].clone(); } }
    Ok(vec![edit])
}

impl Editor {
    pub(super) fn rectangular_active(&self) -> bool { self.draft.as_ref().is_some_and(|d| d.id == ID) }

    // Called before any modeling preview, after the ordinary native schema query.
    // Returns true when preview must stop, including no profile collected yet.
    pub(super) fn select_rectangular_route(&mut self) -> bool {
        let Some(d) = &self.draft else { return false; };
        if d.id != "native.properties" || d.parameters["create"] != true || d.parameters["type"] != "PartDesign::Pad" { return false; }
        if d.parameters["rectangular_route"] == "stock" {return false;}
        let p = &d.parameters;
        let Some(name) = p["properties"]["Profile"]["link"].as_str().filter(|s| !s.is_empty()) else {
            if let Some(d) = &mut self.draft { d.valid=false; d.changed=None; }
            return true;
        };
        let Some(source) = objects(&self.state).iter().find(|o| o["name"] == name) else { self.error=Some("Profile is stale".into()); return true; };
        let binding = source["rectangular_profile"].clone();
        // Preserve the ordinary target Body and copy/reference task. A custom
        // source-only feature must never take ownership of a foreign Body.
        if p.get("reference_choice").is_some() || source["body"] != p["body"] {
            self.draft.as_mut().unwrap().parameters["rectangular_route"]=json!("stock");return false;
        }
        if binding["eligible"] == false { self.draft.as_mut().unwrap().parameters["rectangular_route"]=json!("stock");return false; }
        if binding["eligible"] != true || self.state["rectangular_capabilities"]["version"] != 1 {
            self.error=Some("No validated rectangular profile capability. Inspect and restart.".into());return true;
        }
        // Explicit alternate stock configuration remains stock before ownership starts.
        let props=&p["properties"];
        if p["body"] != binding["body"]["name"]
            || p["expressions"].as_object().is_some_and(|v| !v.is_empty())
            || props["Type"].as_str().is_some_and(|v| v!="Length")
            || props["SideType"].as_str().is_some_and(|v| v!="One side")
            || props["Midplane"] == true || props["UseCustomVector"] == true
            || p["draft_angles"].is_object()
            || props.as_object().is_some_and(|v| v.keys().any(|k| !["Profile","Length","Reversed","Type","SideType","Midplane","UseCustomVector"].contains(&k.as_str()))) {
            self.draft.as_mut().unwrap().parameters["rectangular_route"]=json!("stock");return false;
        }
        let length=props.get("Length").unwrap_or(&self.property_ui.schema["properties"]["Length"]);
        let length=if let Some(v)=length.as_str() {v.to_owned()} else if let Some(v)=length.as_f64() {format!("{v} mm")} else {
            self.error=Some("Native Length default unavailable".into());return true;
        };
        let feature_name=p["name"].clone();
        let reversed=props["Reversed"]==true;
        self.begin_rectangular(binding, None, feature_name, length, 0., reversed);
        false
    }

    fn begin_rectangular(&mut self, binding: Value, target: Option<Value>, name: Value, length: String, angle: f64, reversed: bool) {
        self.sequence+=1;
        let mut p=binding.clone();
        p["binding"]=binding;
        p["name"]=name;p["target"]=target.unwrap_or(Value::Null);
        p["operation"]=json!("New");p["termination"]=json!("Blind");p["side"]=json!("one-side");p["solid"]=json!(true);
        p["direction"]=json!("Normal");p["profile_input"]=p["source"]["name"].clone();
        p["expression"]=json!("");p["length"]=json!(length);p["magnitude"]=json!(format!("{} deg",angle.abs()));
        p["inward"]=json!(angle<0.);p["reversed"]=json!(reversed);
        self.draft=Some(Draft{id:ID.into(),name:"Extrude".into(),token:format!("rectangle-{}-{}",std::process::id(),self.sequence),
            parameters:p,changed:Some(Instant::now()),valid:false,all_parameters:false,
            deferred:self.state["redo_count"].as_u64().unwrap_or(0)>0,committing:false});
        self.sketch=None;self.viewport.set_sketch(None);
    }

    pub(super) fn edit_rectangular(&mut self, object: &Value) -> bool {
        if object["type"] != "PartDesign::FeaturePython" { return false; }
        let m=&object["rectangular_draft"];
        if m["version"] != 1 {self.error=Some("This Python feature has no supported semantic editor".into());return true;}
        self.begin_rectangular(m.clone(),Some(m["target"].clone()),object["name"].clone(),
            format!("{} mm",m["length"].as_f64().unwrap()),m["angle"].as_f64().unwrap(),m["reversed"]==true);
        true
    }

    pub(super) fn rectangular_owner(&self) -> Result<(), String> {
        let d=self.draft.as_ref().ok_or("No rectangular task")?;let p=&d.parameters;let b=&p["binding"];
        if b["version"]!=1 || b["document"]!=self.state["document"] || b["document_uid"]!=self.state["document_uid"] || b["revision"]!=self.state["revision"] {
            return Err("Rectangular owner changed. Inspect, then Cancel and restart.".into());
        }
        for key in ["source","body"] {
            if !objects(&self.state).iter().any(|o| o["name"]==b[key]["name"] && o["native_id"]==b[key]["native_id"]) {
                return Err("Rectangular native identity changed".into());
            }
        }
        if !self.state["preview"].is_null() {
            if self.state["preview"]["token"]!=d.token {return Err("Foreign preview lease".into());}
            let current=objects(&self.state).iter().find(|o| o["name"]==p["preview_target"]["name"] && o["native_id"]==p["preview_target"]["native_id"]);
            if !current.is_some_and(|o| ["source","body","boundary"].iter().all(|k| o["rectangular_draft"][k]==b[k])) {return Err("Preview source identity changed".into());}
        } else {
            let key=if p["target"].is_object(){"rectangular_draft"}else{"rectangular_profile"};
            let reference=if p["target"].is_object(){&p["target"]}else{&b["source"]};
            if !objects(&self.state).iter().any(|o| o["name"]==reference["name"] && o["native_id"]==reference["native_id"] && ["source","body","boundary"].iter().all(|k| o[key][k]==b[k])) {return Err("Source descriptor changed".into());}
        }
        Ok(())
    }

    pub(super) fn rectangular_validate(&mut self, commit: bool) -> bool {
        if !self.rectangular_active(){return true;}
        let result=self.rectangular_owner().and_then(|_| {
            let d=self.draft.as_ref().unwrap();
            if commit && (!d.valid || d.changed.is_some() || d.parameters["accepted_inputs"]!=rectangular_inputs(&d.parameters)) { return Err("Wait for the visible values to have an accepted preview".into()); }
            compile(&d.parameters).map(|_| ())
        });
        if let Err(error)=result { self.error=Some(error);let d=self.draft.as_mut().unwrap();d.valid=false;d.committing=false;false }else{true}
    }

    pub(super) fn rectangular_reply(&mut self, op: &str, accepted: bool) {
        if !self.rectangular_active(){return;}
        let d=self.draft.as_mut().unwrap();
        if !accepted {d.valid=false;d.committing=false;return;}
        if op=="preview" {
            // Only bind the successful returned native feature. Never repair an error replay.
            let name=if d.parameters["target"].is_object(){&d.parameters["target"]["name"]}else{&d.parameters["name"]};
            if let Some(o)=objects(&self.state).iter().find(|o| o["name"]==*name && o["rectangular_draft"].is_object()) {
                let m=&o["rectangular_draft"];let b=&d.parameters["binding"];
                if m["version"]!=1 || m["document_uid"]!=b["document_uid"] || m["document"]!=b["document"]
                    || m["revision"]!=self.state["revision"] || !["source","body","boundary"].iter().all(|k| m[k]==b[k]) {
                    d.valid=false;d.committing=false;return;
                }
                d.parameters["preview_target"]=o["rectangular_draft"]["target"].clone();
                d.parameters["binding"]["revision"]=self.state["revision"].clone();
                d.parameters["accepted_inputs"]=d.parameters["sent_inputs"].clone();
                d.valid=d.parameters["accepted_inputs"]==rectangular_inputs(&d.parameters) && self.state["preview"]["token"]==d.token;
            }else{d.valid=false;}
        }
    }

    pub(super) fn rectangular_fields(&mut self, ui: &mut egui::Ui) {
        let d=self.draft.as_mut().unwrap();let p=&mut d.parameters;let mut changed=false;
        ui.heading("Extrude");ui.label(format!("Profile: {}",p["source"]["name"].as_str().unwrap_or("")));
        ui.weak("This rectangle task supports New, Blind, One side and literal quantities. Other configurations require Cancel and a new task.");
        for (key,label,choices) in [("operation","Operation",&["New","Add","Pocket"][..]),("termination","Termination",&["Blind","Through all","Up to face","Up to first","Up to shape","Up to vertex","Two lengths"][..]),("side","Side",&["one-side","Two sides","Symmetric"][..]),("direction","Direction",&["Normal","Custom direction"][..])] {
            ui.horizontal(|ui| { ui.label(label);egui::ComboBox::from_id_salt((ID,key)).selected_text(p[key].as_str().unwrap_or("")).show_ui(ui,|ui| {
                for choice in choices {changed|=ui.selectable_value(&mut p[key],json!(choice),*choice).changed();}
            });});
        }
        for (key,label) in [("profile_input","Profile"),("length","Length"),("magnitude","Draft magnitude"),("expression","Expression, unsupported for this feature")] {
            ui.horizontal(|ui| {
                ui.label(label);let mut text=p[key].as_str().unwrap_or("").to_owned();
                let response=ui.text_edit_singleline(&mut text);
                #[cfg(test)]
                if key=="profile_input" {
                    ui.ctx().data_mut(|data|data.insert_temp(egui::Id::new("convergence_profile_response"),
                        (response.clone(),ui.clip_rect(),ui.layer_id(),json!({"link":p["source"]["name"]}))));
                }
                if response.changed(){p[key]=json!(text);changed=true;}
            });
        }
        for (key,label) in [("solid","Solid"),("inward","Inward"),("reversed","Reversed")] {let mut value=p[key]==true;if ui.checkbox(&mut value,label).changed(){p[key]=json!(value);changed=true;}}
        if changed {d.changed=Some(Instant::now());d.valid=false;}
    }
}

pub(super) fn rectangular_inputs(p:&Value)->Value {
    json!([p["length"],p["magnitude"],p["inward"],p["reversed"],p["solid"],p["operation"],p["termination"],p["side"],p["expression"],p["direction"],p["profile_input"]])
}
