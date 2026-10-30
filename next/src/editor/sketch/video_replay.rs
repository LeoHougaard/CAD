//! Include this module from editor/sketch/lifecycle_tests.rs in a private test copy.
//! Run the ignored test with CAD_TUTORIAL_PLAN, CAD_LIFECYCLE_RUNTIME and OUTPUT.
//! This drives real Editor frames and native replies, not a physical desktop.
use super::*;

fn frame(n: &mut Native, events: Vec<egui::Event>, modifiers: egui::Modifiers) -> egui::FullOutput {
    n.time += 0.1;
    let mut frame = eframe::Frame::_new_kittest();
    n.ctx.run(egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.,850.))),
        time:Some(n.time), events, modifiers, ..Default::default()
    }, |ctx| eframe::App::update(&mut n.editor,ctx,&mut frame))
}

fn settle(n: &mut Native, modifiers: egui::Modifiers) {
    for _ in 0..3 {
        frame(n, vec![], modifiers);
    }
}

fn point(n: &mut Native, target: &Value, mods: egui::Modifiers) -> egui::Pos2 {
    settle(n, mods);
    let xy = |v: &Value| egui::pos2(v[0].as_f64().unwrap() as f32, v[1].as_f64().unwrap() as f32);
    match target["kind"].as_str().unwrap() {
        "pixel" => xy(&target["xy"]),
        "sketch" => {
            let p = &target["xy"];
            let pos = n.editor.viewport.sketch_screen_point([p[0].as_f64().unwrap(), p[1].as_f64().unwrap(), 0.]);
            let response = n.editor.viewport.response.as_ref().unwrap();
            assert!(response.rect.contains(pos), "mapped sketch point outside canvas");
            assert_eq!(n.ctx.layer_id_at(pos), Some(response.layer_id), "mapped sketch point occluded");
            pos
        }
        "sketch-element" => {
            let name = n.editor.sketch.as_ref().expect("active native sketch");
            let sketch = object(&n.editor.state,name);
            let geometry: Vec<_> = sketch["sketch"]["geometry"].as_array().unwrap().iter()
                .filter(|g| g["persistent_id"] == target["persistent_id"]).collect();
            assert_eq!(geometry.len(),1,"unique current native persistent ID");
            let geometry = geometry[0];
            assert_eq!(geometry["kind"],"line","this binding only supports native lines");
            let fraction = match target["point"].as_u64().unwrap_or(0) {
                0 => target["fraction"].as_f64().unwrap_or(0.5),
                1 => 0., 2 => 1., _ => panic!("unknown native line point"),
            };
            let local = std::array::from_fn(|i| {
                let start=geometry["start"][i].as_f64().unwrap();
                start+(geometry["end"][i].as_f64().unwrap()-start)*fraction
            });
            let pos=n.editor.viewport.sketch_screen_point(local);
            let response=n.editor.viewport.response.as_ref().unwrap();
            assert!(response.rect.contains(pos),"native entity point outside canvas");
            assert_eq!(n.ctx.layer_id_at(pos),Some(response.layer_id),"native entity point occluded");
            let hit=n.editor.viewport.sketch_hit(pos).expect("native entity point must hit sketch geometry");
            assert_eq!(i64::from(hit.index),geometry["index"].as_i64().unwrap(),"native entity point hit another curve");
            assert_eq!(hit.point,target["point"].as_u64().unwrap_or(0) as i32,"native entity point hit another handle");
            pos
        }
        "plane" => {
            let role = match target["role"].as_str().unwrap() {
                "front" => crate::viewport::StandardPlane::Front,
                "top" => crate::viewport::StandardPlane::Top,
                "right" => crate::viewport::StandardPlane::Right,
                other => panic!("unknown plane {other}"),
            };
            n.editor.viewport.standard_guide_probe(role).expect("visible native plane guide")
        }
        "label" => {
            let frame = frame(n, vec![], mods);
            let scope = match target["scope"].as_str() {
                Some("shortcut") => Some(n.ctx.memory(|m| m.area_rect(egui::Id::new("tool_shortcut_menu"))).expect("shortcut menu open")),
                None => None,
                other => panic!("unsupported label scope {other:?}"),
            };
            let found: Vec<_> = frame.shapes.iter().filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == target["text"].as_str().unwrap() => {
                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                    scope.is_none_or(|area| area.contains(rect.center())).then_some(rect.center())
                }
                _ => None,
            }).collect();
            assert_eq!(found.len(), 1, "target label must be unique in its scope: {target}");
            found[0]
        }
        other => panic!("unsupported point mapping {other}"),
    }
}

fn is_modifier(name: &str) -> bool {
    matches!(name,"ShiftLeft"|"ShiftRight"|"CtrlLeft"|"CtrlRight"|"AltLeft"|"AltRight")
}

fn modifiers(action: &Value, held: &std::collections::HashSet<String>) -> egui::Modifiers {
    let mut result = egui::Modifiers::NONE;
    let names = action["modifiers"].as_array().into_iter().flatten().map(|s|s.as_str().unwrap())
        .chain(held.iter().filter(|s|is_modifier(s)).map(String::as_str));
    for name in names {
        match name {
            "ShiftLeft" | "ShiftRight" => result.shift = true,
            "CtrlLeft" | "CtrlRight" => result.ctrl = true,
            "AltLeft" | "AltRight" => result.alt = true,
            other => panic!("unsupported modifier {other}"),
        }
    }
    result
}

fn keyboard(n: &mut Native, action: &Value, pressed: bool, mut mods: egui::Modifiers) {
    let name = action["key"].as_str().unwrap();
    if is_modifier(name) {
        match name {
            "ShiftLeft"|"ShiftRight" => mods.shift=pressed,
            "CtrlLeft"|"CtrlRight" => mods.ctrl=pressed,
            "AltLeft"|"AltRight" => mods.alt=pressed,
            _ => unreachable!(),
        }
        frame(n,vec![],mods);
        return;
    }
    // egui represents keypad digits/Enter with logical number-row/Enter keys.
    let normalized = match name {
        "NumEnter" => "Enter",
        "Num0" => "0", "Num1" => "1", "Num2" => "2", "Num3" => "3", "Num4" => "4",
        "Num5" => "5", "Num6" => "6", "Num7" => "7", "Num8" => "8", "Num9" => "9",
        other => other,
    };
    let key = Key::from_name(normalized).unwrap_or_else(|| panic!("unsupported key {name}"));
    let mut events = vec![egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: mods }];
    if pressed && let Some(text) = action["text"].as_str() {
        events.push(egui::Event::Text(text.to_owned()));
    }
    frame(n,events,mods);
}

fn camera(n: &Native) -> Value {
    if n.editor.sketch.is_none() { return Value::Null; }
    let rect = n.editor.viewport.response.as_ref().expect("current canvas").rect;
    let projected: Vec<_> = [[0.,0.,0.],[10.,0.,0.],[0.,10.,0.]].into_iter().map(|world| {
        let point = n.editor.viewport.sketch_screen_point(world);
        json!({"world":world,"pixel":[point.x,point.y]})
    }).collect();
    let panel = n.ctx.memory(|m| m.area_rect(egui::Id::new("native_sketch_controls")))
        .map(|r| [r.min.x,r.min.y,r.max.x,r.max.y]);
    json!({"canvas":[rect.min.x,rect.min.y,rect.width(),rect.height()],"sketch_controls_rect":panel,"anchors":projected})
}

fn expected(n: &Native, action: &Value) {
    if action.get("expect").is_none() { return; }
    let expectations = action["expect"].as_object().expect("expect must be an object");
    assert!(!expectations.is_empty());
    let sketch = n.editor.sketch.as_ref().map(|name| object(&n.editor.state, name)["sketch"].clone()).unwrap_or(Value::Null);
    let state = json!({"native":n.editor.state,"sketch":sketch,"ui":n.ui_state(),"camera":camera(n),
        "input":n.ctx.input(|i|json!({"ctrl":i.modifiers.ctrl,"shift":i.modifiers.shift,"alt":i.modifiers.alt,
            "pointer":i.pointer.latest_pos().map(|p|[p.x,p.y]),"buttons_down":i.pointer.any_down()}))});
    for (path, value) in expectations {
        assert_eq!(state.pointer(path).expect("expected JSON pointer exists"), value, "checkpoint {path}");
    }
}

#[test]
#[ignore = "mapped video input replay through actual Editor/native worker"]
fn tutorial_replay_from_json() {
    let plan: Value = serde_json::from_slice(&fs::read(std::env::var_os("CAD_TUTORIAL_PLAN").expect("CAD_TUTORIAL_PLAN")).unwrap()).unwrap();
    assert_eq!(plan["version"], 1);
    let mut n = Native::new("tutorial-replay");
    n.send("new", json!({"name":"TutorialReplay"}), true);
    assert!(objects(&n.editor.state).is_empty());
    let mut held = std::collections::HashSet::<String>::new();
    for action in plan["actions"].as_array().unwrap() {
        let kind = action["kind"].as_str().unwrap();
        if kind == "key_down" { assert!(held.insert(action["key"].as_str().unwrap().to_owned())); }
        if kind == "key_up" { assert!(held.remove(action["key"].as_str().unwrap())); }
        let mods = modifiers(action,&held);
        settle(&mut n,mods);
        let mut resolved = Value::Null;
        match kind {
            "click" => {
                let pos = point(&mut n, &action["target"],mods);
                resolved = json!([pos.x,pos.y]);
                let button = match action["button"].as_str().unwrap() {
                    "left" => egui::PointerButton::Primary,
                    "right" => egui::PointerButton::Secondary,
                    "middle" => egui::PointerButton::Middle,
                    other => panic!("unsupported button {other}"),
                };
                for pressed in [true, false] {
                    frame(&mut n,vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton { pos, button, pressed, modifiers: mods }],mods);
                }
            }
            "move" => {
                if action["target"]["kind"] == "path" {
                    for sample in action["target"]["points"].as_array().unwrap() {
                        let pos = egui::pos2(sample["xy"][0].as_f64().unwrap() as f32, sample["xy"][1].as_f64().unwrap() as f32);
                        frame(&mut n,vec![egui::Event::PointerMoved(pos)],mods);
                    }
                    resolved = action["target"].clone();
                } else {
                    let pos = point(&mut n, &action["target"],mods);
                    resolved = json!([pos.x,pos.y]);
                    frame(&mut n,vec![egui::Event::PointerMoved(pos)],mods);
                }
            }
            "drag" => {
                assert_eq!(action["target"]["kind"], "path");
                let points = action["target"]["points"].as_array().unwrap();
                assert!(points.len() >= 2);
                let button = match action["button"].as_str().unwrap() {
                    "left" => egui::PointerButton::Primary,
                    "right" => egui::PointerButton::Secondary,
                    "middle" => egui::PointerButton::Middle,
                    other => panic!("unsupported button {other}"),
                };
                for (i, sample) in points.iter().enumerate() {
                    let pos = egui::pos2(sample["xy"][0].as_f64().unwrap() as f32, sample["xy"][1].as_f64().unwrap() as f32);
                    let mut events = vec![egui::Event::PointerMoved(pos)];
                    if i == 0 { events.push(egui::Event::PointerButton {pos, button, pressed:true, modifiers:mods}); }
                    frame(&mut n,events,mods);
                }
                let last = points.last().unwrap();
                let pos = egui::pos2(last["xy"][0].as_f64().unwrap() as f32, last["xy"][1].as_f64().unwrap() as f32);
                // The final motion reaches a held-button frame before release.
                // Combining them would lose the last camera delta in egui.
                frame(&mut n,vec![egui::Event::PointerButton {pos, button, pressed:false, modifiers:mods}],mods);
                resolved = action["target"].clone();
            }
            "scroll" => {
                let pos = point(&mut n, &action["target"],mods);
                resolved = json!([pos.x,pos.y]);
                frame(&mut n,vec![egui::Event::PointerMoved(pos), egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(action["delta"][0].as_f64().unwrap() as f32, action["delta"][1].as_f64().unwrap() as f32),
                    modifiers:mods,
                }],mods);
            }
            "key" => { keyboard(&mut n, action, true,mods); keyboard(&mut n, action, false,mods); }
            "key_down"|"key_up" => {
                if is_modifier(action["key"].as_str().unwrap()) { frame(&mut n,vec![],mods); }
                else { keyboard(&mut n,action,kind=="key_down",mods); }
            }
            "text" => { frame(&mut n,vec![egui::Event::Text(action["text"].as_str().unwrap().to_owned())],mods); }
            "checkpoint" => (),
            other => panic!("egui adapter does not implement {other}; no substitution"),
        }
        for op in action["wait_for"].as_array().into_iter().flatten() {
            n.receive(op.as_str().unwrap(), true);
        }
        settle(&mut n,mods);
        expected(&n, action);
        n.events.push(json!({"replay_action":action,"resolved_target":resolved,"ui":n.ui_state(),"camera":camera(&n),"state":n.editor.state,"physical_desktop":false}));
        n.record();
    }
    assert!(held.is_empty(),"replay left keys held");
}
