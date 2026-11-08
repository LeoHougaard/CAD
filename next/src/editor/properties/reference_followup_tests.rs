//! Real Editor widgets and pointer input; native state is an explicit double.
use super::*;

#[test]
fn remove_then_canvas_pick_does_not_reintroduce_the_removed_edge() {
    let (mut e,jobs)=profile_editor();
    let native=json!({"name":"Fillet","type":"PartDesign::Fillet","native_id":22,"body":"Body",
        "properties":{"Base":{"link":"Solid","subelements":["Edge1","Edge2"]}},
        "property_types":{"Base":"App::PropertyLinkSub"},"property_metadata":{"Base":{"read_only":false}}});
    e.state["objects"].as_array_mut().unwrap().extend([native,json!({"name":"Solid","label":"Solid","type":"PartDesign::Pad","native_id":23,"body":"Body","shape":{"solids":1}})]);
    e.edit_native_properties("Fillet");
    e.viewport.set_scene(json!({"object":"Solid","revision":1,"positions":[],"triangles":[]}),
        json!({"edges":(1..=3).map(|i|json!({"subelement":format!("Edge{i}"),"mapped_name":format!("mapped-{i}"),
            "geometry_type":"Part::GeomLine","points":[[-5,(i-2)*10,0],[5,(i-2)*10,0]]})).collect::<Vec<_>>()}));
    e.viewport.top();
    let mut ui=Ui::new();
    let mut draw=|ui:&mut egui::Ui| {e.native_property_fields(ui);e.canvas(ui,false,true);};
    ui.frame(vec![],&mut draw);ui.frame(vec![],&mut draw);
    ui.click("Edge of Solid",&mut draw);
    assert_eq!(e.property_ui.active_reference.as_deref(),Some("Base"));
    let mut draw=|ui:&mut egui::Ui| {e.native_property_fields(ui);e.canvas(ui,false,true);};
    ui.frame(vec![],&mut draw);ui.click("×",&mut draw);
    assert_eq!(e.draft.as_ref().unwrap().parameters["properties"]["Base"]["subelements"],json!(["Edge2"]));
    assert_eq!(e.viewport.selections.iter().map(|s|s.subelement.as_str()).collect::<Vec<_>>(),vec!["Edge2"]);
    let pos=e.viewport.test_world_screen_point([0.,10.,0.]);
    for pressed in [true,false] {
        ui.frame(vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}],
            |ui|{e.native_property_fields(ui);e.canvas(ui,false,true);});
    }
    let p=&e.draft.as_ref().unwrap().parameters;
    assert_eq!(p["properties"]["Base"]["subelements"],json!(["Edge2","Edge3"]));
    assert!(property_commands::build(p).unwrap().iter().any(|c|c["property"]=="Base" && c["value"]["subelements"]==json!(["Edge2","Edge3"])));
    assert!(jobs.try_recv().is_err());
}

#[test]
fn removing_region_profile_clears_helpers_before_command_composition() {
    let (mut e,jobs)=profile_editor();
    let p=&mut e.draft.as_mut().unwrap().parameters;
    p["properties"]["Profile"]=json!({"link":"Original"});
    p["region_profile"]=json!({"source":"Original","body":"Body","name":"Region"});
    p["reference_choice"]=json!({"request":{"source":"Original","body":"Body"}});
    click_reference(&mut e,"Original");
    click_reference(&mut e,"×");
    let p=&e.draft.as_ref().unwrap().parameters;
    assert!(p["properties"]["Profile"].is_null());
    assert!(p.get("region_profile").is_none());assert!(p.get("reference_choice").is_none());
    assert!(e.viewport.selections.is_empty());assert!(e.viewport.selected.is_none());
    assert!(!property_commands::build(p).unwrap().iter().any(|c|c["op"]=="create_planar_region_profile"));
    assert!(jobs.try_recv().is_err());
}

#[test]
fn full_revolve_suspends_and_restores_exact_expression_bindings_before_native_writes() {
    for path in ["Angle",".Angle","Angle.Value"] {
        for reopened in [false,true] {
            let mut ui=Ui::new();
            let mut p=json!({"create":false,"name":"Revolve","type":"PartDesign::Revolution","properties":{"Angle":90.,"Type":"Angle","Midplane":false},"expressions":{"Reversed":"0"}});
            let mut schema=json!({"properties":{},"expressions":{}});
            if reopened {schema["expressions"][path]=json!("90 deg");} else {p["expressions"][path]=json!("90 deg");}
            let mut draw=|ui:&mut egui::Ui| {super::super::super::task_fields::configure(ui,&mut p,&schema);};
            ui.frame(vec![],&mut draw);ui.frame(vec![],&mut draw);ui.click("Full revolve",&mut draw);
            let commands=property_commands::build(&p).unwrap();
            let clear=commands.iter().position(|c|c["op"]=="expression" && c["property"]==path && c["expression"].is_null()).unwrap();
            let angle=commands.iter().position(|c|c["op"]=="set" && c["property"]=="Angle" && c["value"]==360.).unwrap();
            assert!(clear<angle);assert!(!commands.iter().any(|c|c["op"]=="expression" && c["property"]==path && c["expression"].is_string()));
            assert_eq!(p["expressions"]["Reversed"],"0");
            let mut draw=|ui:&mut egui::Ui| {super::super::super::task_fields::configure(ui,&mut p,&schema);};
            ui.frame(vec![],&mut draw);ui.click("Full revolve",&mut draw);
            assert!(property_commands::build(&p).unwrap().iter().any(|c|c["op"]=="expression" && c["property"]==path && c["expression"]=="90 deg"));
            assert_eq!(p["properties"]["Angle"],90.);
            if path=="Angle" && reopened {
                let folder=std::path::PathBuf::from(std::env::var_os("CAD_CONVERGENCE_OUTPUT").unwrap());
                std::fs::write(folder.join("full-revolve-pointer-commands.json"),serde_json::to_vec_pretty(&json!({
                    "source":"actual Editor task widget pointer test; native follow-up required",
                    "full":commands,"restored":property_commands::build(&p).unwrap()})).unwrap()).unwrap();
            }
        }
    }
}

#[test]
fn full_revolve_pending_clear_overrides_reopened_expression_and_controls_are_restored() {
    let mut ui=Ui::new();
    let mut p=json!({"type":"PartDesign::Revolution","properties":{"Angle":90.,"Type":"Angle","Midplane":false},"expressions":{".Angle":null,".Type":"0","Midplane":"0"}});
    let schema=json!({"expressions":{".Angle":"45 deg"}});
    let mut draw=|ui:&mut egui::Ui| {super::super::super::task_fields::configure(ui,&mut p,&schema);};
    ui.frame(vec![],&mut draw);ui.frame(vec![],&mut draw);ui.click("Full revolve",&mut draw);
    for path in [".Angle",".Type","Midplane"] {assert!(p["expressions"][path].is_null());}
    let mut draw=|ui:&mut egui::Ui| {super::super::super::task_fields::configure(ui,&mut p,&schema);};
    ui.frame(vec![],&mut draw);ui.click("Full revolve",&mut draw);
    assert!(p["expressions"][".Angle"].is_null());
    assert_eq!(p["expressions"][".Type"],"0");assert_eq!(p["expressions"]["Midplane"],"0");
}
