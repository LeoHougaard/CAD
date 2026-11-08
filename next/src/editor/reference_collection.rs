use super::{Editor, objects};
use crate::{property_commands, task_contract, viewport::{Selection, SelectionMode}};
use serde_json::{Value, json};
use std::time::Instant;

fn references(value: &Value) -> Vec<Selection> {
    let rows: Vec<&Value> = value.as_array().map(|v|v.iter().collect()).unwrap_or_else(||vec![value]);
    rows.into_iter().flat_map(|v| {
        let Some(name)=v["link"].as_str() else {return Vec::new()};
        let elements:Vec<_>=v["subelements"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        if elements.is_empty() {vec![Selection{object:name.into(),subelement:String::new()}]}
        else {elements.into_iter().map(|s|Selection{object:name.into(),subelement:s.into()}).collect()}
    }).collect()
}

impl Editor {
    pub(super) fn activate_reference_field(&mut self, property: String) {
        if self.activate_special_reference(property.clone()) {return;}
        let Some(draft)=&self.draft else{return};
        let value=draft.parameters["properties"].get(&property).unwrap_or(&self.property_ui.schema["properties"][&property]);
        let profile=draft.parameters["properties"]["Profile"]["link"].as_str()
            .or_else(||self.property_ui.schema["properties"]["Profile"]["link"].as_str()).map(str::to_owned);
        self.viewport.clear_viewport_selection();
        self.viewport.selections=references(value);
        self.viewport.selected=self.viewport.selections.last().cloned();
        self.property_ui.region_pick=property=="Profile" && draft.parameters["type"]!="PartDesign::Hole";
        self.property_ui.axis_pick=property=="ReferenceAxis" && matches!(draft.parameters["type"].as_str(),Some("PartDesign::Revolution"|"PartDesign::Groove"));
        if self.property_ui.axis_pick {self.property_ui.axis_profile=profile;}
        self.property_ui.active_reference=Some(property);
        self.error=None;
    }

    pub(super) fn reference_selection_mode(&self) -> Option<SelectionMode> {
        use task_contract::EntityKind::*;
        let property=self.property_ui.active_reference.as_deref()?;
        let property=if property.starts_with("Sections:"){"Sections"}else{property};
        let rule=task_contract::reference_kind(self.draft.as_ref()?.parameters["type"].as_str()?,property)?;
        if rule.allowed.iter().all(|k|matches!(k,Edge|LinearEdge|SketchLine|LinearCurve|Arc|Circle|CylindricalEdge|Curve)) {Some(SelectionMode::Edges)}
        else if rule.allowed.iter().all(|k|matches!(k,Face|PlanarFace|CylindricalFace|ConicalFace)) {Some(SelectionMode::Faces)}
        else if rule.allowed.iter().all(|k|matches!(k,Vertex|SketchPoint|CircleCenter)) {Some(SelectionMode::Vertices)}
        else {Some(SelectionMode::All)}
    }

    pub(super) fn collect_reference_field(&mut self) {
        if self.collect_special_reference() {return;}
        use task_contract::{EntityKind as K, Pick};
        let Some(property)=self.property_ui.active_reference.clone() else{return};
        let Some(draft)=&self.draft else{return};
        let kind=draft.parameters["type"].as_str().unwrap_or_default();
        let Some(rule)=task_contract::reference_kind(kind,&property) else{return};
        let native_type=self.property_ui.schema["property_types"][&property].as_str().unwrap_or_default();
        let mut selected=self.viewport.selections.clone();
        // Validate all identities before filtering or replacing a singular pick.
        let pairs:Vec<_>=selected.iter().map(|s|(s.object.clone(),s.subelement.clone())).collect();
        if let Err(error)=property_commands::validate_selected_objects(objects(&self.state),&pairs) {
            self.error=Some(error);return;
        }
        let own=draft.parameters["name"].as_str().unwrap_or_default();
        selected.retain(|s|s.object!=own);
        if !pairs.is_empty() && selected.is_empty() {
            self.error=Some("Choose geometry other than the feature being edited".into());return;
        }
        if rule.max==Some(1) && selected.len()>1 {selected=selected.last().cloned().into_iter().collect();}
        if selected.is_empty() {
            let value=if native_type.contains("List"){json!([])}else{Value::Null};
            self.set_reference_value(&property,value);return;
        }
        let mut picks=Vec::new();
        for selection in &selected {
            let Some(object)=objects(&self.state).iter().find(|o|o["name"]==selection.object) else{return};
            let native=object["type"].as_str().unwrap_or_default();
            let geometry=self.viewport.scene_geometry_type(selection).unwrap_or_default();
            let entity=if selection.subelement.starts_with("Edge") {
                if geometry.contains("Line") {if native=="Sketcher::SketchObject"{K::SketchLine}else{K::LinearEdge}}
                else if geometry.contains("Circle") {K::Circle}else{K::Edge}
            } else if selection.subelement.starts_with("Face") {
                if geometry.contains("Plane"){K::PlanarFace}else if geometry.contains("Cylinder"){K::CylindricalFace}else{K::Face}
            } else if selection.subelement.starts_with("Vertex") {if native=="Sketcher::SketchObject"{K::SketchPoint}else{K::Vertex}}
            else if native=="Sketcher::SketchObject" {K::Sketch}
            else if matches!(native,"App::Plane"|"PartDesign::Plane"){K::Plane}
            else if object["shape"]["solids"].as_u64().unwrap_or(0)>0 {K::Part}
            else {K::Feature};
            let current=object["native_id"].as_i64().is_some() && (selection.subelement.is_empty() || self.viewport.scene_subelement_identity(selection)
                .is_some_and(|(mapped,rev)|!mapped.is_empty() && Some(rev)==self.state["revision"].as_u64()));
            let public=!matches!(native,"App::Origin"|"App::Line"|"PartDesign::CoordinateSystem")
                && (native!="PartDesign::Body" || entity==K::Part);
            picks.push(Pick{kind:entity,public_geometry:public,identity_current:current});
        }
        let pairs:Vec<_>=selected.iter().map(|s|(s.object.clone(),s.subelement.clone())).collect();
        let converted=property_commands::selected_reference(native_type,objects(&self.state),&pairs);
        if task_contract::validate_picks(kind,&property,&picks, |_| converted.is_ok()).is_err() {
            self.error=Some(format!("Choose an eligible entity for {}",task_contract::field_label(kind,&property).unwrap_or("this field")));
            return;
        }
        self.viewport.selections=selected;self.viewport.selected=self.viewport.selections.last().cloned();
        self.set_reference_value(&property,converted.unwrap());
    }

    pub(super) fn set_reference_value(&mut self, property:&str,value:Value) {
        if let Some(draft)=&mut self.draft {
            if draft.parameters["properties"][property]==value {return}
            if property=="Profile" {self.property_ui.reference.bind(&mut draft.parameters,&value);}
            draft.parameters["properties"][property]=value.clone();
            if property=="Profile" {draft.parameters.as_object_mut().unwrap().remove("region_profile");}
            if property=="ReferenceAxis" {draft.parameters.as_object_mut().unwrap().remove("construction_axis");}
            draft.changed=Some(Instant::now());draft.valid=false;self.error=None;
        }
        if self.property_ui.active_reference.as_deref()==Some(property) {
            self.viewport.clear_viewport_selection();
            self.viewport.selections=references(&value);
            self.viewport.selected=self.viewport.selections.last().cloned();
        }
    }
}
