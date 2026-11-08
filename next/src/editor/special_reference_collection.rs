//! Selection adapters for tasks whose references are not native property values.
use super::*;

pub(super) fn element_index(target: &str, prefix: &str) -> bool {
    target.strip_prefix(prefix).is_some_and(|s| !s.is_empty() && !s.starts_with('0')
        && s.bytes().all(|c| c.is_ascii_digit()) && s.parse::<usize>().is_ok())
}

pub(super) fn validate_identity(state: &Value, object: &Value, reference: &Value, id: &str) -> Result<(), String> {
    let valid_id = |v: &Value| v.as_u64().is_some();
    if !reference.is_object() || !valid_id(&object["native_id"])
        || reference[id] != object["native_id"] || reference["link"] != object["name"]
        || state["document_uid"].as_str().is_none_or(str::is_empty)
        || reference["document_uid"] != state["document_uid"]
        || reference.get("revision").is_some_and(|r| *r != state["revision"])
        || reference["frame"].as_array().is_none_or(|f| f.len() != 16 || f.iter().any(|v| v.as_f64().is_none_or(|v| !v.is_finite()))) {
        return Err("Native reference identity or frame is stale; select it again".into());
    }
    if reference["body"] != object["body"] { return Err("Source ownership changed; select it again".into()); }
    if let Some(body) = reference["body"].as_str() {
        if !objects(state).iter().any(|o| o["name"] == body && valid_id(&o["native_id"]) && o["native_id"] == reference["body_id"]) {
            return Err("Source body identity changed; select it again".into());
        }
    } else if !reference["body_id"].is_null() { return Err("Malformed source body identity".into()); }
    Ok(())
}

pub(super) fn loft_display(section: &Value) -> Value {
    let target = section["target"].as_str().unwrap_or_default();
    json!({"link":section["link"],"subelements":if target.is_empty() { vec![] } else { vec![target] }})
}

pub(super) fn sections_valid(state: &Value, sections: &[Value]) -> Result<(), String> {
    for (i, reference) in sections.iter().enumerate() {
        let name = reference["link"].as_str().ok_or("Malformed profile reference")?;
        let target = reference["target"].as_str().ok_or("Malformed profile target")?;
        if super::new_loft::section_source(state, name, target)? != *reference {
            return Err("Profile identity changed; click its field to select it again".into());
        }
        if !target.is_empty() && i != 0 && i + 1 != sections.len() {
            return Err("Point profiles must be the first or last profile".into());
        }
        if sections[..i].iter().any(|r| r["link"] == reference["link"] && r["target"] == reference["target"]) {
            return Err("Choose distinct ordered profiles".into());
        }
    }
    Ok(())
}

impl Editor {
    pub(super) fn activate_special_reference(&mut self, property: String) -> bool {
        let Some(d) = &self.draft else { return false; };
        let special = (d.parameters["type"] == "Part::Loft" && (property == "Sections" || property.starts_with("Sections:")))
            || (d.parameters["part_mirror"].is_object() && matches!(property.as_str(), "Source" | "MirrorPlane"))
            || (d.parameters["offset_plane"].is_object() && property == "AttachmentSupport");
        if !special { return false; }
        self.viewport.clear_viewport_selection();
        self.property_ui.region_pick = false;
        self.property_ui.axis_pick = false;
        // The viewport's marker hit returns (the sketch, Origin), which the native
        // loft serializer resolves to ObjectOrigin. Never synthesize a point ID.
        self.property_ui.loft_point_pick = d.parameters["type"] == "Part::Loft";
        self.property_ui.active_reference = Some(property);
        self.error = None;
        true
    }

    pub(super) fn collect_special_reference(&mut self) -> bool {
        let Some(key) = self.property_ui.active_reference.clone() else { return false; };
        let Some(d) = &self.draft else { return false; };
        let loft = d.parameters["type"] == "Part::Loft" && (key == "Sections" || key.starts_with("Sections:"));
        let mirror = d.parameters["part_mirror"].is_object() && matches!(key.as_str(), "Source" | "MirrorPlane");
        let plane = d.parameters["offset_plane"].is_object() && key == "AttachmentSupport";
        if !loft && !mirror && !plane { return false; }
        if self.busy || d.deferred || d.committing { return true; }
        // Fields own removals. A blank canvas click only clears the transient pick.
        let selected = self.viewport.selections.clone();
        if selected.is_empty() { return true; }
        let before = d.parameters.clone();
        let result = (|| -> Result<Value,String> {
            let mut parameters = before.clone();
            if self.state["revision"].as_u64().is_none() { return Err("Model revision is unavailable".into()); }
            let feature_id = if loft { &parameters["loft_feature_id"] }
                else if mirror { &parameters["part_mirror"]["feature_id"] }
                else { &parameters["offset_plane"]["feature_id"] };
            if !feature_id.is_null() && !objects(&self.state).iter().any(|o|
                o["name"] == parameters["name"] && o["native_id"] == *feature_id) {
                return Err("Feature identity changed; reopen the task".into());
            }
            // Validate every pick before writing any part of the draft.
            for pick in &selected {
                if !loft && pick.subelement == "Origin" { return Err("Choose a supported solid or plane".into()); }
                if pick.object == parameters["name"].as_str().unwrap_or_default() { return Err("A feature cannot reference itself".into()); }
                if !pick.subelement.is_empty() && pick.subelement != "Origin" {
                    let Some((mapped, revision)) = self.viewport.scene_subelement_identity(pick) else { return Err("Selected topology is stale; select it again".into()); };
                    if mapped.is_empty() || Some(revision) != self.state["revision"].as_u64() { return Err("Selected topology revision changed".into()); }
                    if loft && element_index(&pick.subelement, "Vertex") {
                        let source = objects(&self.state).iter().find(|o| o["name"] == pick.object).ok_or("Source is stale")?;
                        if source["loft_vertices"][&pick.subelement] != mapped { return Err("Selected vertex mapping changed".into()); }
                    }
                }
            }
            if loft {
                let picks = selected.iter().map(|s| super::new_loft::section_source(&self.state, &s.object, &s.subelement)).collect::<Result<Vec<_>,_>>()?;
                let sections = parameters["loft_sections"].as_array_mut().ok_or("Profiles are unavailable")?;
                if let Some(index) = key.strip_prefix("Sections:") {
                    let index = index.parse::<usize>().map_err(|_| "Invalid profile row")?;
                    if picks.len() != 1 || index >= sections.len() { return Err("Choose one replacement profile".into()); }
                    sections[index] = picks[0].clone();
                } else { sections.extend(picks); }
                sections_valid(&self.state, sections)?;
            } else if mirror {
                if selected.len() != 1 { return Err("Choose one entity for this field".into()); }
                let pick = &selected[0];
                // In part/plane tasks a current hit on the object's topology selects
                // that whole native entity, never a fabricated face-plane reference.
                let reference = super::part_mirror::reference(&self.state, &pick.object, "", key == "MirrorPlane")?;
                let task = &mut parameters["part_mirror"];
                // Reject malformed fallback axes even when the new pick is a source.
                if task["plane"].is_null() && !matches!(task["axis"].as_str(), Some("XY" | "XZ" | "YZ")) && key != "MirrorPlane" {
                    return Err("Select a mirror plane".into());
                }
                task[if key == "Source" { "source" } else { "plane" }] = reference;
                for (path, is_plane) in [("source",false),("plane",true)] {
                    let r = &task[path];
                    if r.is_null() { continue; }
                    let name = r["link"].as_str().ok_or("Malformed mirror reference")?;
                    if super::part_mirror::reference(&self.state,name,"",is_plane)? != *r { return Err("Mirror reference changed; select it again".into()); }
                }
            } else {
                if selected.len() != 1 { return Err("Choose one plane".into()); }
                let pick = &selected[0];
                let source = super::plane_offset::support_source(&self.state, &parameters, &pick.object, "")?;
                parameters["offset_plane"]["support"] = source["name"].clone();
                parameters["offset_plane"]["support_id"] = source["native_id"].clone();
                parameters["properties"]["AttachmentSupport"] = json!([{"link":source["name"],"subelements":[""]}]);
            }
            Ok(parameters)
        })();
        match result {
            Ok(parameters) => {
                let d = self.draft.as_mut().unwrap();
                if d.parameters != parameters { d.parameters = parameters; d.changed = Some(Instant::now()); d.valid = false; }
                self.error = None;
                self.viewport.clear_viewport_selection();
            }
            Err(error) => { self.error = Some(error); self.viewport.clear_viewport_selection(); }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> Value {
        let frame = json!([1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]);
        json!({"document_uid":"doc","revision":7,"objects":[
            {"name":"Sketch","type":"Sketcher::SketchObject","native_id":2,"effective_visibility":true,"body":null,
             "loft_source":{"link":"Sketch","target":"","source_id":2,"document_uid":"doc","body":null,"body_id":null,"frame":frame},
             "loft_vertices":{"Vertex1":"mapped"}},
            {"name":"Axis","type":"PartDesign::Line","native_id":3,"effective_visibility":true}]})
    }
    #[test]
    fn strict_element_names_and_real_origin_identity() {
        for bad in ["Vertex", "Vertex0", "Vertex01", "Vertex-1", "Vertex1tail", "Vertex99999999999999999999999999"] { assert!(!element_index(bad,"Vertex")); }
        let state = state();
        let origin = super::super::new_loft::section_source(&state,"Sketch","Origin").unwrap();
        assert_eq!(origin["source_id"],2); assert_eq!(origin["target"],"Origin");
        assert!(super::super::new_loft::section_source(&state,"Axis","Origin").is_err());
        assert!(super::super::new_loft::section_source(&state,"Sketch","Edge1").is_err());
    }
    #[test]
    fn identity_changes_and_interior_points_rejected_atomically() {
        let mut state = state();
        let origin = super::super::new_loft::section_source(&state,"Sketch","Origin").unwrap();
        let profile = super::super::new_loft::section_source(&state,"Sketch","").unwrap();
        assert!(sections_valid(&state,&[profile.clone(),origin.clone(),profile]).is_err());
        state["objects"][0]["native_id"] = json!(9);
        assert!(sections_valid(&state,&[origin]).is_err());
        state["objects"][0]["native_id"] = json!(2);
        state["objects"][0]["effective_visibility"] = json!(false);
        assert!(super::super::new_loft::section_source(&state,"Sketch","").is_err());
    }
    fn editor() -> Editor {
        let (mut e, _, _) = super::super::rectangular_extrude_tests::fixture();
        e.state = state();
        let mut other = e.state["objects"][0].clone();
        other["name"] = json!("Other"); other["native_id"] = json!(4);
        other["loft_source"]["link"] = json!("Other"); other["loft_source"]["source_id"] = json!(4);
        e.state["objects"].as_array_mut().unwrap().push(other);
        e.busy = false;
        e.draft = Some(Draft { id:"native.properties".into(), name:"Loft".into(), token:"test".into(),
            parameters:json!({"create":true,"name":"Loft","type":"Part::Loft","properties":{},"loft_sections":[]}),
            changed:None,valid:true,all_parameters:false,deferred:false,committing:false });
        e
    }
    fn pick(e: &mut Editor, name: &str, target: &str) {
        e.viewport.selections = vec![crate::viewport::Selection { object:name.into(), subelement:target.into() }];
        assert!(e.collect_special_reference());
    }
    #[test]
    fn collector_appends_then_field_replaces_actual_origin_without_mutating_model() {
        let mut e = editor(); let original = e.state.clone();
        assert!(e.activate_special_reference("Sections".into()));
        assert!(e.property_ui.loft_point_pick);
        pick(&mut e,"Sketch","Origin"); pick(&mut e,"Other","");
        let sections = &e.draft.as_ref().unwrap().parameters["loft_sections"];
        assert_eq!(sections[0]["source_id"],2); assert_eq!(sections[1]["source_id"],4);
        assert!(e.activate_special_reference("Sections:0".into()));
        pick(&mut e,"Sketch","");
        assert_eq!(e.draft.as_ref().unwrap().parameters["loft_sections"][0]["target"],"");
        assert_eq!(e.state,original);
        assert!(!e.draft.as_ref().unwrap().valid);
    }
    #[test]
    fn collector_validates_entire_batch_and_can_recover_after_rejection() {
        let mut e = editor(); e.activate_special_reference("Sections".into());
        let before = e.draft.as_ref().unwrap().parameters.clone();
        e.viewport.selections = vec![crate::viewport::Selection { object:"Sketch".into(), subelement:"Origin".into() },
            crate::viewport::Selection { object:"Axis".into(), subelement:String::new() }];
        e.collect_special_reference();
        assert_eq!(e.draft.as_ref().unwrap().parameters,before); assert!(e.error.is_some());
        pick(&mut e,"Other",""); assert!(e.error.is_none());
        assert_eq!(e.draft.as_ref().unwrap().parameters["loft_sections"].as_array().unwrap().len(),1);
    }
    #[test]
    fn plane_support_requires_current_body_and_whole_visible_plane() {
        let mut state = state();
        state["objects"].as_array_mut().unwrap().extend([
            json!({"name":"Body","native_id":10}),
            json!({"name":"Plane","native_id":11,"body":"Body","type":"PartDesign::Plane","effective_visibility":true})]);
        let mut parameters = json!({"name":"Offset","body":"Body","offset_plane":{"document_uid":"doc","body_id":10}});
        assert!(super::super::plane_offset::support_source(&state,&parameters,"Plane","").is_ok());
        assert!(super::super::plane_offset::support_source(&state,&parameters,"Plane","Face1").is_err());
        parameters["offset_plane"]["body_id"] = json!(20);
        assert!(super::super::plane_offset::support_source(&state,&parameters,"Plane","").is_err());
    }

    #[test]
    fn mirror_field_rejects_multiple_picks_and_stale_ids_before_assignment() {
        let mut e = editor();
        let mut solid = e.state["objects"][0].clone();
        solid["type"] = json!("Part::Feature");
        let mut reference = solid["loft_source"].clone();
        reference["native_id"] = reference["source_id"].clone();
        reference.as_object_mut().unwrap().remove("source_id");
        reference.as_object_mut().unwrap().remove("target");
        solid["mirror_source"] = reference;
        e.state["objects"][0] = solid;
        let d = e.draft.as_mut().unwrap();
        d.parameters = json!({"create":true,"name":"Mirror","type":"Part::Mirroring","properties":{},
            "part_mirror":{"source":null,"plane":null,"axis":"XY","offset":0.0,"feature_id":null}});
        e.activate_special_reference("Source".into());
        let before = e.draft.as_ref().unwrap().parameters.clone();
        e.viewport.selections = vec![crate::viewport::Selection { object:"Sketch".into(), subelement:String::new() },
            crate::viewport::Selection { object:"Other".into(), subelement:String::new() }];
        e.collect_special_reference(); assert_eq!(e.draft.as_ref().unwrap().parameters,before);
        e.state["objects"][0]["mirror_source"]["native_id"] = json!(999);
        pick(&mut e,"Sketch",""); assert_eq!(e.draft.as_ref().unwrap().parameters,before);
        e.state["objects"][0]["mirror_source"]["native_id"] = json!(2);
        pick(&mut e,"Sketch",""); assert_eq!(e.draft.as_ref().unwrap().parameters["part_mirror"]["source"]["native_id"],2);
        e.draft.as_mut().unwrap().parameters["part_mirror"]["axis"] = json!("XX");
        let before = e.draft.as_ref().unwrap().parameters.clone();
        pick(&mut e,"Sketch",""); assert_eq!(e.draft.as_ref().unwrap().parameters,before); assert!(e.error.is_some());
    }

}
