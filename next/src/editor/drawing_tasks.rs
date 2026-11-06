//! Confirm-only drawing tasks share the ordinary Draft/controller lifetime.
use super::{Draft, Editor, objects};
use eframe::egui;
use serde_json::{Value, json};

fn owner(state: &Value) -> Value {
    json!([
        state["document"],
        state["revision"],
        state["drawing"]["document_uid"],
        state["available"],
        state["preview"],
        state["sketch_edit"]
    ])
}
fn reference(state: &Value, name: &str, kind: &str) -> Option<Value> {
    let o = objects(state)
        .iter()
        .find(|o| o["name"] == name && o["type"] == kind)?;
    let uid = state["drawing"]["document_uid"].as_str()?;
    Some(json!({"object":name,"native_id":o["native_id"].as_i64()?,"document_uid":uid}))
}
fn rows(state: &Value) -> &[Value] {
    state["drawing"]["objects"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}
fn supported(state: &Value) -> bool {
    let c = &state["drawing_capabilities"];
    c["available"] == true
        && c["version"] == 3
        && c["projection"] == "Third angle"
        && c["templates"].as_array().is_some_and(|t| {
            t.iter().any(|t| {
                t["id"] == "ANSI-A-ASME-landscape-v1" && t["paper_mm"] == json!([279.4, 215.9])
            })
        })
}
fn edit(state: &Value, p: &Value) -> Result<Value, String> {
    if p["owner"] != owner(state)
        || state["available"] != true
        || !state["preview"].is_null()
        || !state["sketch_edit"].is_null()
    {
        return Err("The document or task owner changed. Cancel and start again.".into());
    }
    if !supported(state) {
        return Err("Drawing tools are unavailable in this session.".into());
    }
    let mut e = p["edit"].clone();
    let source = e["source"]["object"]
        .as_str()
        .ok_or("Choose a local Assembly")?;
    if reference(state, source, "Assembly::AssemblyObject").as_ref() != Some(&e["source"]) {
        return Err("The selected Assembly changed. Start again.".into());
    }
    if e["op"] != "drawing_create_page" {
        let page = rows(state)
            .iter()
            .find(|r| r["identity"] == e["page"] && r["type"] == "TechDraw::DrawPage")
            .ok_or("The drawing page changed. Start again.")?;
        if page["source"] != e["source"] {
            return Err("Choose the page's Assembly".into());
        }
        if e["op"] == "drawing_layout" {
            for v in e["views"].as_array().ok_or("Choose drawing views")? {
                if !page["views"]
                    .as_array()
                    .is_some_and(|vs| vs.contains(&v["view"]))
                {
                    return Err("The drawing views changed. Start again.".into());
                }
            }
        }
    }
    if e["op"] != "drawing_insert_view" {
        let scale = e["scale"].as_f64().ok_or("Enter a scale")?;
        if !scale.is_finite() || !(1e-6..=1000.).contains(&scale) {
            return Err("Scale must be between 0.000001 and 1000".into());
        }
    }
    let positions: Vec<&Value> = if e["op"] == "drawing_insert_view" {
        vec![&e["position_mm"]]
    } else {
        e["views"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| &v["position_mm"])
            .collect()
    };
    for xy in positions {
        for (axis, max) in [279.4, 215.9].iter().enumerate() {
            if !xy[axis]
                .as_f64()
                .is_some_and(|x| x.is_finite() && (0. ..=*max).contains(&x))
            {
                return Err("Position must be inside the ANSI A page, in millimeters".into());
            }
        }
    }
    // Task/controller metadata never enters the native edit.
    e.as_object_mut().ok_or("Drawing edit unavailable")?;
    Ok(e)
}
impl Editor {
    fn drawing_idle(&self) -> bool {
        !self.busy
            && !self.unavailable()
            && self.state["available"] == true
            && self.state["preview"].is_null()
            && self.state["sketch_edit"].is_null()
            && self.draft.is_none()
            && self.sketch_creation.is_none()
            && self.sketch_ui.session.is_none()
            && self.sketch_ui.task.is_none()
            && self.sketch_ui.drag.is_none()
            && self.sketch_ui.dimension.is_none()
            && !self.units_ui.active()
            && !self.measurement_is_open()
            && self.pending_file.is_none()
            && self.file_dialog.is_none()
            && !self.overwrite
            && !self.file_browser.active()
            && !self.exchange_ui.active()
    }
    fn start_drawing(&mut self, op: &str) {
        if !self.drawing_idle() || !supported(&self.state) {
            return;
        }
        let selected = self.selected_object.as_deref().unwrap_or_default();
        let mut e = if op == "drawing_create_page" {
            let Some(source) = reference(&self.state, selected, "Assembly::AssemblyObject") else {
                self.error = Some("Select a local Assembly in the tree first.".into());
                return;
            };
            let name = self.unique("Drawing");
            json!({"op":op,"name":name,"template_id":"ANSI-A-ASME-landscape-v1","source":source,"scale":0.5,"projection":"Third angle"})
        } else {
            let Some(page) = rows(&self.state)
                .iter()
                .find(|r| r["identity"]["object"] == selected && r["type"] == "TechDraw::DrawPage")
                .cloned()
            else {
                self.error = Some("Select a drawing page in the tree first.".into());
                return;
            };
            json!({"op":op,"page":page["identity"],"source":page["source"],"scale":page["scale"],"projection":"Third angle"})
        };
        if op == "drawing_insert_view" {
            e.as_object_mut().unwrap().remove("scale");
            e.as_object_mut().unwrap().remove("projection");
            e["name"] = json!(self.unique("DrawingView"));
            e["orientation"] = json!("Front");
            e["position_mm"] = json!([75., 65.]);
        } else if op == "drawing_layout" {
            let page = rows(&self.state)
                .iter()
                .find(|r| r["identity"] == e["page"])
                .unwrap();
            let views:Vec<_>=rows(&self.state).iter().filter(|r| page["views"].as_array().is_some_and(|v| v.contains(&r["identity"])))
                .map(|r| json!({"view":r["identity"],"position_mm":[r["x"]["value"],r["y"]["value"]]})).collect();
            if views.is_empty() {
                self.error = Some("Insert a view before editing the layout.".into());
                return;
            }
            e["views"] = json!(views);
        }
        self.sequence += 1;
        let token = format!("drawing-{}-{}", std::process::id(), self.sequence);
        self.draft = Some(Draft {
            id: "native.drawing".into(),
            name: match op {
                "drawing_create_page" => "Create drawing",
                "drawing_insert_view" => "Insert drawing view",
                _ => "Drawing layout",
            }
            .into(),
            token: token.clone(),
            parameters: json!({"owner":owner(&self.state),"task_token":token,"edit":e}),
            changed: None,
            valid: true,
            all_parameters: false,
            deferred: true,
            committing: false,
        });
    }
    pub(super) fn drawing_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(self.drawing_idle(), |ui| {
            ui.menu_button("Drawing", |ui| {
                if !supported(&self.state) {
                    ui.label("Drawing tools are unavailable in this session.");
                    return;
                }
                for (label, op) in [
                    ("Create drawing...", "drawing_create_page"),
                    ("Insert view...", "drawing_insert_view"),
                    ("Layout...", "drawing_layout"),
                ] {
                    if ui.button(label).clicked() {
                        self.start_drawing(op);
                        ui.close();
                    }
                }
            });
        });
    }
    pub(super) fn finish_drawing(&mut self, commit: bool) {
        if self.busy {
            return;
        }
        if !commit {
            self.draft = None;
            self.error = None;
            return;
        }
        let Some(d) = self.draft.as_ref() else {
            return;
        };
        if d.parameters["task_token"].as_str() != Some(d.token.as_str()) {
            self.error = Some("The drawing task owner changed. Cancel and start again.".into());
            return;
        }
        match edit(&self.state, &d.parameters) {
            Ok(e) => {
                let label = d.name.clone();
                self.draft.as_mut().unwrap().committing = true;
                self.transaction(&label, vec![e]);
                if !self.busy {
                    self.draft.as_mut().unwrap().committing = false;
                }
            }
            Err(error) => self.error = Some(error),
        }
    }
    pub(super) fn drawing_panel(&mut self, ctx: &egui::Context) {
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Drawing task").resizable(false).collapsible(false).show(ctx,|ui| {
            let d=self.draft.as_mut().unwrap();
            ui.label(&d.name);ui.label("ANSI A landscape · 11 × 8.5 in · Third angle");
            ui.weak("Paper display and shaded views are not available yet.");
            ui.add_enabled_ui(!self.busy && !d.committing,|ui| {
                let e=&mut d.parameters["edit"];
                if e["op"] == "drawing_insert_view" {
                    let mut orientation=e["orientation"].as_str().unwrap_or("Front").to_owned();
                    egui::ComboBox::from_id_salt("drawing_orientation").selected_text(&orientation).show_ui(ui,|ui| {
                        for v in ["Front","Right","Top","Iso"] { ui.selectable_value(&mut orientation,v.to_owned(),v); }
                    });e["orientation"]=json!(orientation);position_fields(ui,&mut e["position_mm"],"View");
                } else {
                    let mut scale=e["scale"].as_f64().unwrap_or(0.5);
                    ui.horizontal(|ui| {ui.label("Scale, paper : model");ui.add(egui::DragValue::new(&mut scale).speed(0.01).range(1e-6..=1000.));});
                    ui.weak(format!("1 : {}",1./scale));e["scale"]=json!(scale);
                    if let Some(views)=e.get_mut("views").and_then(Value::as_array_mut) {
                        for (i,v) in views.iter_mut().enumerate() { position_fields(ui,&mut v["position_mm"],&format!("View {}",i+1)); }
                    }
                }
                ui.weak("Positions are millimeters from the lower-left paper corner. Apply commits once.");
                apply=ui.button("Apply").clicked();cancel=ui.button("Cancel").clicked();
            });
            if let Some(error)=&self.error {ui.colored_label(egui::Color32::LIGHT_RED,error);}
        });
        if cancel {
            self.finish_drawing(false);
        } else if apply {
            self.finish_drawing(true);
        }
    }
}
fn position_fields(ui: &mut egui::Ui, xy: &mut Value, label: &str) {
    ui.push_id(label, |ui| {
        ui.horizontal(|ui| {
            ui.label(label);
            for (axis, max) in [279.4, 215.9].iter().enumerate() {
                ui.label(if axis == 0 { "X (mm)" } else { "Y (mm)" });
                let mut value = xy[axis].as_f64().unwrap_or(0.);
                ui.add(egui::DragValue::new(&mut value).range(0. ..=*max));
                xy[axis] = json!(value);
            }
        });
    });
}
#[cfg(test)]
#[path = "drawing_tasks_tests.rs"]
mod tests;
