//! Presentation adapters keep public task controls separate from native storage.
use super::*;

fn effective<'a>(parameters:&'a Value,schema:&'a Value,key:&str)->&'a Value {
    parameters["properties"].get(key).unwrap_or(&schema["properties"][key])
}

fn full_revolve_expression(path:&str)->bool {
    matches!(path.trim_start_matches('.').split(['.','[']).next(),Some("Angle"|"Type"|"Midplane"))
}

pub(super) fn configure(ui:&mut egui::Ui,parameters:&mut Value,schema:&Value)->bool {
    if !matches!(parameters["type"].as_str(),Some("PartDesign::Revolution"|"PartDesign::Groove")) {return false;}
    let angle=effective(parameters,schema,"Angle").clone();
    let end=effective(parameters,schema,"Type").clone();
    let middle=effective(parameters,schema,"Midplane").clone();
    let mut full=parameters["revolve_full"].as_bool().unwrap_or(angle.as_f64()==Some(360.) && !middle.as_bool().unwrap_or(false)
        && matches!(end.as_str(),Some("Angle")|None));
    if !ui.checkbox(&mut full,"Full revolve").changed() {return false;}
    if full {
        // Preserve exact paths, including self-relative aliases and explicit
        // pending clears. Native expressions are cleared before literal writes.
        let mut bindings=serde_json::Map::new();
        for expressions in [&schema["expressions"],&parameters["expressions"]] {
            if let Some(expressions)=expressions.as_object() {
                for (path,binding) in expressions {
                    if full_revolve_expression(path) {bindings.insert(path.clone(),binding.clone());}
                }
            }
        }
        parameters["revolve_task"]=json!({"angle":angle,"end":end,"midplane":middle,"expressions":bindings});
        for path in bindings.keys() {parameters["expressions"][path]=Value::Null;}
        parameters["properties"]["Angle"]=json!(360.);
        parameters["properties"]["Type"]=json!("Angle");
        parameters["properties"]["Midplane"]=json!(false);
    } else {
        let saved=parameters["revolve_task"].clone();
        for (key,stored,fallback) in [("Angle","angle",json!(360.)),("Type","end",json!("Angle")),("Midplane","midplane",json!(false))] {
            parameters["properties"][key]=saved.get(stored).filter(|v|!v.is_null()).cloned().unwrap_or(fallback);
        }
        if let Some(bindings)=saved["expressions"].as_object() {
            for (path,binding) in bindings {parameters["expressions"][path]=binding.clone();}
        }
    }
    // The checkbox must remain unchecked while its restored angle is 360.
    parameters["revolve_full"]=json!(full);
    true
}

pub(super) fn fields(parameters:&Value,schema:&Value)->Vec<&'static str> {
    let kind=parameters["type"].as_str().unwrap_or_default();
    let end=crate::task_contract::enum_display(kind,"Type",effective(parameters,schema,"Type").as_str().unwrap_or_default()).unwrap_or("Blind");
    let second_end=crate::task_contract::enum_display(kind,"Type2",effective(parameters,schema,"Type2").as_str().unwrap_or_default()).unwrap_or("Blind");
    let full=parameters["revolve_full"].as_bool().unwrap_or_else(||effective(parameters,schema,"Angle").as_f64()==Some(360.) && !effective(parameters,schema,"Midplane").as_bool().unwrap_or(false));
    let visibility=crate::task_contract::Visibility {full_revolve:full,end_type:end,
        second:effective(parameters,schema,"SideType")=="Two sides",
        second_end_type:second_end,custom_direction:effective(parameters,schema,"UseCustomVector")==true};
    crate::task_contract::visible_fields(kind,&visibility).filter_map(|f|match f.binding {
        crate::task_contract::Binding::Native(key)=>Some(key),_=>None
    }).collect()
}

pub(super) fn enumeration(ui:&mut egui::Ui,kind:&str,property:&str,metadata:&Value,value:&mut Value)->Option<bool> {
    let options=crate::task_contract::enum_options(kind,property);
    if options.is_empty() {return None;}
    let original=value.clone();
    let label=crate::task_contract::enum_display(kind,property,value.as_str().unwrap_or_default()).unwrap_or("Select option");
    egui::ComboBox::from_id_salt("end_type").selected_text(label).show_ui(ui,|ui| {
        for option in options {
            let native=option.native.map(|v|json!(v));
            let supported=native.as_ref().is_some_and(|v|metadata["enumerations"].as_array().is_some_and(|values|values.contains(v)));
            if ui.add_enabled(supported,egui::Button::selectable(native.as_ref()==Some(value),option.display)).clicked() {
                *value=native.unwrap();ui.close();
            }
        }
    });
    Some(*value!=original)
}
