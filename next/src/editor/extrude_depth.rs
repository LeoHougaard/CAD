//! The depth handle changes the same typed properties as the native task form.
use super::*;
use crate::viewport::DepthHandle;
use glam::Vec3;

impl Editor {
    pub(super) fn extrude_depth_descriptor(&self, allowed: bool) -> Option<DepthHandle> {
        let d = self.draft.as_ref()?;
        if !allowed || d.id != "native.properties" || d.parameters["type"] != "PartDesign::Pad"
            || d.committing || d.deferred || self.state["sketch_edit"].is_object()
            || self.state["preview"]["token"] != d.token { return None; }
        if self.state["document_uid"].as_str().is_none_or(str::is_empty) { return None; }
        let obj = objects(&self.state).iter().find(|o| o["name"] == d.parameters["name"])?;
        let desc = &obj["extrude_handle"];
        if desc["revision"] != self.state["revision"] || desc["feature_id"] != obj["native_id"]
            || desc["feature"] != obj["name"] || !desc.is_object()
            || !obj["expressions"]["Length"].is_null()
            || !obj["expressions"]["Reversed"].is_null()
            || !crate::property_commands::writable(obj, "Length")
            || !crate::property_commands::writable(obj, "Reversed") { return None; }
        let source = objects(&self.state).iter().find(|o| o["name"] == desc["profile"])?;
        if source["native_id"] != desc["profile_id"] { return None; }
        let edits = &d.parameters["properties"];
        // Unparsed text and changed direction/mode/reference values must first
        // pass the ordinary quantity parser/native preview. Do not overwrite it.
        if edits.as_object().is_some_and(|p| p.iter().any(|(key, value)|
            !matches!(key.as_str(), "Length" | "Reversed") && obj["properties"][key] != *value)) {
            return None;
        }
        let signed = desc["signed_length"].as_f64()?;
        let length = match edits.get("Length") {
            Some(value) => value.as_f64()?,
            None => signed.abs(),
        };
        let reverse = match edits.get("Reversed") {
            Some(value) => value.as_bool()?,
            None => signed < 0.,
        };
        let vector = |value: &Value| -> Option<Vec3> {
            let v = Vec3::new(value[0].as_f64()? as f32, value[1].as_f64()? as f32, value[2].as_f64()? as f32);
            v.is_finite().then_some(v)
        };
        let axis = vector(&desc["axis"])?;
        let factor = desc["factor"].as_f64()? as f32;
        if !length.is_finite() || length < 0. || (axis.length()-1.).abs() > 0.001
            || !factor.is_finite() || factor <= 0. { return None; }
        Some(DepthHandle {
            owner: format!("{}:{}:{}:{}", self.state["document_uid"], d.token, desc["feature_id"], desc["profile_id"]),
            origin: vector(&desc["origin"])?, axis, factor,
            length: if reverse { -length } else { length },
            ready: !self.busy && d.changed.is_none(),
        })
    }

    pub(super) fn extrude_depth_edit(&mut self, owner: &str, length: f64) {
        let Some(handle) = self.extrude_depth_descriptor(true) else { return; };
        if handle.owner != owner || !length.is_finite() || handle.length == length { return; }
        let d = self.draft.as_mut().unwrap();
        // Numbers are native storage millimeters, as with accepted quantity
        // values. The existing quantity field formats them using native units.
        d.parameters["properties"]["Length"] = json!(length.abs());
        d.parameters["properties"]["Reversed"] = json!(length < 0.);
        d.valid = false;
        d.changed = Some(Instant::now());
        // Until the solved native snapshot supplies its normal unit caption,
        // show an explicit unit. Never present storage mm as document inches.
        let quantity = &mut self.property_ui.schema["property_metadata"]["Length"]["quantity"];
        quantity["value"] = json!(length.abs());
        quantity["input"] = json!(format!("{} mm", length.abs()));
    }
}

#[cfg(test)]
mod tests;
