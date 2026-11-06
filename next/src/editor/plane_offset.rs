//! Compact authoring task over native Plane attachment properties.
use super::*;
use crate::viewport::DepthHandle;
use glam::Vec3;

impl Editor {
    pub(super) fn start_offset_plane(&mut self, support: &str) {
        if self.busy || self.draft.is_some() || self.unavailable()
            || self.sketch_ui.session.is_some() { return; }
        let Some(source) = objects(&self.state).iter().find(|o| o["name"] == support).cloned() else { return; };
        if !matches!(source["type"].as_str(), Some("App::Plane" | "PartDesign::Plane")) { return; }
        let Some(body) = crate::body_commands::body_of(&self.state, support) else { return; };
        let Some(body_id) = objects(&self.state).iter().find(|o| o["name"] == body).map(|o|o["native_id"].clone()) else { return; };
        self.start_native_feature_for_profile("PartDesign::Plane", Some(support));
        if let Some(d) = &mut self.draft {
            d.name = "Offset plane".into();
            d.parameters["body"] = json!(body);
            d.parameters["properties"] = json!({"AttachmentSupport":[{"link":support,"subelements":[""]}],"MapMode":"FlatFace"});
            d.parameters["offset_plane"] = json!({"support":support,"support_id":source["native_id"],
                "document_uid":self.state["document_uid"],"body_id":body_id,"magnitude":0.,"flipped":false});
        }
    }

    pub(super) fn edit_offset_plane(&mut self, name: &str) {
        if self.busy || self.draft.is_some() || self.unavailable() { return; }
        let Some(obj) = objects(&self.state).iter().find(|o|o["name"]==name).cloned() else { return; };
        let desc = &obj["plane_offset"];
        if !desc.is_object() || desc["revision"] != self.state["revision"] { return; }
        self.edit_native_properties(name);
        let body_id = self.draft.as_ref().and_then(|d| objects(&self.state).iter()
            .find(|o| o["name"] == d.parameters["body"])).map(|o|o["native_id"].clone());
        if let Some(d) = &mut self.draft {
            d.name = "Offset plane".into();
            d.parameters["offset_plane"] = json!({"support":desc["support"],"support_id":desc["support_id"],
                "document_uid":self.state["document_uid"],"body_id":body_id,"feature_id":desc["feature_id"],"magnitude":desc["length"],"flipped":desc["flipped"]});
            d.changed = Some(Instant::now());
        }
    }

    pub(super) fn offset_plane_fields(&mut self, ui: &mut egui::Ui) {
        let d = self.draft.as_mut().unwrap();
        let task = &mut d.parameters["offset_plane"];
        ui.label("Offset plane");
        ui.add_enabled_ui(!self.busy, |ui| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Offset");
                changed |= super::units::quantity_field(ui, &mut task["magnitude"],
                    &self.property_ui.schema["property_metadata"]["AttachmentOffset"]["quantity"]["position"][2]);
            });
            let mut flip = task["flipped"].as_bool().unwrap_or(false);
            if ui.checkbox(&mut flip, "Flip direction").changed() { task["flipped"] = json!(flip); changed = true; }
            if changed { d.valid = false; d.changed = Some(Instant::now()); }
        });
        if let Some(error) = &self.error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
    }

    pub(super) fn offset_plane_descriptor(&self, allowed: bool) -> Option<DepthHandle> {
        let d = self.draft.as_ref()?;
        let task = d.parameters.get("offset_plane")?;
        if !allowed || d.deferred || d.committing || self.state["preview"]["token"] != d.token
            || self.state["document_uid"].as_str().is_none_or(str::is_empty) { return None; }
        let obj = objects(&self.state).iter().find(|o| o["name"] == d.parameters["name"])?;
        let desc = &obj["plane_offset"];
        let source = objects(&self.state).iter().find(|o|o["name"] == task["support"])?;
        let body = objects(&self.state).iter().find(|o|o["name"] == d.parameters["body"])?;
        if body["native_id"] != task["body_id"] || obj["body"] != d.parameters["body"]
            || task["document_uid"] != self.state["document_uid"]
            || task.get("feature_id").is_some_and(|id| !id.is_null() && *id != obj["native_id"]) { return None; }
        if desc["revision"] != self.state["revision"] || desc["feature_id"] != obj["native_id"]
            || desc["feature"] != obj["name"] || desc["support"] != task["support"]
            || desc["support_id"] != task["support_id"] || source["native_id"] != task["support_id"]
            || ["AttachmentOffset", "MapReversed", "MapMode", "AttachmentSupport"].iter().any(|p|
                !crate::property_commands::writable(obj, p) || obj["expressions"].as_object().is_some_and(|e|
                    e.keys().any(|k| k.trim_start_matches('.').split('.').next() == Some(*p)))) { return None; }
        let length = task["magnitude"].as_f64()?;
        if !length.is_finite() || length < 0. || task["flipped"] != desc["flipped"] { return None; }
        let vector = |v: &Value| -> Option<Vec3> {
            let v = Vec3::new(v[0].as_f64()? as f32,v[1].as_f64()? as f32,v[2].as_f64()? as f32);
            v.is_finite().then_some(v)
        };
        let axis = vector(&desc["axis"])?;
        if (axis.length()-1.).abs() > 0.001 { return None; }
        Some(DepthHandle { owner: format!("{}:{}:{}:{}:{}:{}", self.state["document_uid"],
            d.token, self.state["revision"], desc["feature_id"], desc["support_id"], desc["frame"]),
            origin: vector(&desc["origin"])?, axis, factor:1., length, ready: !self.busy && d.changed.is_none() })
    }

    pub(super) fn offset_plane_edit(&mut self, owner: &str, length: f64) {
        let Some(h) = self.offset_plane_descriptor(true) else { return; };
        if h.owner != owner || !length.is_finite() || h.length == length { return; }
        let d = self.draft.as_mut().unwrap();
        d.parameters["offset_plane"]["magnitude"] = json!(length.abs());
        if length < 0. {
            let flip = d.parameters["offset_plane"]["flipped"] != true;
            d.parameters["offset_plane"]["flipped"] = json!(flip);
        }
        d.valid = false; d.changed = Some(Instant::now());
        let q = &mut self.property_ui.schema["property_metadata"]["AttachmentOffset"]["quantity"]["position"][2];
        q["value"] = json!(length.abs());
        q["input"] = json!(format!("{} mm", length.abs()));
    }
}

#[cfg(test)]
mod tests;
