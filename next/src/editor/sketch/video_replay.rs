//! Include this module from editor/sketch/lifecycle_tests.rs in a private test copy.
//! Run the ignored test with CAD_TUTORIAL_PLAN, CAD_LIFECYCLE_RUNTIME and OUTPUT.
//! This drives real Editor frames and native replies, not a physical desktop.
use super::*;

fn rect_value(rect: Option<egui::Rect>) -> Value {
    rect.map(|r| json!([r.min.x, r.min.y, r.max.x, r.max.y]))
        .unwrap_or(Value::Null)
}

fn frame(n: &mut Native, events: Vec<egui::Event>, modifiers: egui::Modifiers) -> egui::FullOutput {
    use std::io::Write as _;
    n.time += 0.1;
    let time = n.time;
    let mut frame = eframe::Frame::_new_kittest();
    let mut trace = Vec::new();
    let output = n.ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 850.),
            )),
            time: Some(time),
            events,
            modifiers,
            ..Default::default()
        },
        |ctx| {
            let before_features = ctx.memory(|m| m.area_rect(egui::Id::new("features")));
            let before_canvas = n.editor.viewport.response.as_ref().map(|r| r.rect);
            let before_session = n.editor.sketch_ui.session.clone();
            let before_busy = n.editor.busy;
            eframe::App::update(&mut n.editor, ctx, &mut frame);
            let after_features = ctx.memory(|m| m.area_rect(egui::Id::new("features")));
            let after_canvas = n.editor.viewport.response.as_ref().map(|r| r.rect);
            trace.push(json!({"time":time,"features_before":rect_value(before_features),
                "features_after":rect_value(after_features),"canvas_before":rect_value(before_canvas),
                "canvas_after":rect_value(after_canvas),"available_after":rect_value(Some(ctx.available_rect())),
                "session_before":before_session,"session_after":n.editor.sketch_ui.session,
                "sketch_after":n.editor.sketch,"busy_before":before_busy,"busy_after":n.editor.busy}));
        },
    );
    let toolbar_text = |label: &str| {
        output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == label)
        })
    };
    let mut pending_check = None;
    if let Some(last) = trace.last_mut() {
        last["select_present"] = json!(toolbar_text("Select"));
        last["finish_present"] = json!(toolbar_text("Finish"));
        if last["busy_after"] == true
            && last["session_after"].is_null()
            && !last["sketch_after"].is_null()
        {
            pending_check = Some((
                toolbar_text("Select"),
                toolbar_text("Finish"),
                last["features_before"][0] == last["features_after"][0]
                    && last["features_before"][1] == last["features_after"][1],
            ));
        }
    }
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(n.folder.join("frame-motion.jsonl"))
        .unwrap();
    for entry in trace {
        writeln!(log, "{entry}").unwrap();
    }
    if let Some((select, finish, features_unchanged)) = pending_check {
        assert!(!select, "pending sketch has no sketch toolbar");
        assert!(!finish, "pending sketch has no Finish tool");
        assert!(features_unchanged, "pending sketch must not move Features");
    }
    output
}

fn settle(n: &mut Native, modifiers: egui::Modifiers) {
    for _ in 0..3 {
        frame(n, vec![], modifiers);
    }
}

fn transform_point(placement: &Value, point: &Value) -> [f64; 3] {
    let vec =
        |value: &Value, index: usize| value[index].as_f64().expect("native placement coordinate");
    let xyz = glam::Vec3::new(
        vec(point, 0) as f32,
        vec(point, 1) as f32,
        vec(point, 2) as f32,
    );
    let p = &placement["position"];
    let q = &placement["rotation"];
    let rotation = glam::Quat::from_xyzw(
        vec(q, 0) as f32,
        vec(q, 1) as f32,
        vec(q, 2) as f32,
        vec(q, 3) as f32,
    );
    assert!(rotation.is_finite() && rotation.length_squared() > 0. && xyz.is_finite());
    let origin = glam::Vec3::new(vec(p, 0) as f32, vec(p, 1) as f32, vec(p, 2) as f32);
    assert!(origin.is_finite());
    let world = origin + rotation.normalize() * xyz;
    [world.x as f64, world.y as f64, world.z as f64]
}

fn bound_annotation<'a>(
    sketch: &'a Value,
    target: &Value,
) -> Result<(i64, &'a Value), &'static str> {
    let geometry: Vec<_> = sketch["sketch"]["geometry"]
        .as_array()
        .ok_or("geometry missing")?
        .iter()
        .filter(|g| g["persistent_id"] == target["persistent_id"])
        .collect();
    let [curve] = geometry.as_slice() else {
        return Err("unique native geometry required");
    };
    if let Some(kind) = target["geometry_kind"].as_str()
        && curve["kind"] != kind
    {
        return Err("native geometry kind changed");
    }
    let index = curve["index"]
        .as_i64()
        .ok_or("native geometry index missing")?;
    let constraints: Vec<_> = sketch["sketch"]["constraints"]
        .as_array()
        .ok_or("constraints missing")?
        .iter()
        .filter(|c| {
            c["type"] == target["constraint_kind"]
                && ["first", "second", "third"]
                    .iter()
                    .any(|key| c[*key].as_i64() == Some(index))
        })
        .collect();
    let [constraint] = constraints.as_slice() else {
        return Err("unique native constraint required");
    };
    if constraint["active"] == false {
        return Err("native constraint inactive");
    }
    if !constraint["value"].as_f64().is_some_and(f64::is_finite) {
        return Err("native dimension value missing");
    }
    Ok((index, *constraint))
}

fn is_constraint_label(
    text: &str,
    canonical: &str,
    text_rect: egui::Rect,
    label: egui::Rect,
    clip: egui::Rect,
    hit: Option<usize>,
    constraint: usize,
) -> bool {
    text == canonical
        && label.contains(text_rect.min)
        && label.contains(text_rect.max)
        && clip.contains(text_rect.center())
        && hit == Some(constraint)
}

#[test]
fn annotation_binding_uses_current_geometry_value_and_rejects_wrong_glyph() {
    let mut sketch = json!({"sketch":{"geometry":[
        {"persistent_id":17,"index":4,"kind":"arc"},
        {"persistent_id":18,"index":5,"kind":"line"}],
        "constraints":[{"index":9,"type":"Diameter","first":4,"value":13.5,"active":true}]}});
    let target = json!({"persistent_id":17,"geometry_kind":"arc","constraint_kind":"Diameter"});
    let (index, dimension) = bound_annotation(&sketch, &target).unwrap();
    assert_eq!(index, 4);
    assert_eq!(dimension["value"], 13.5);
    sketch["sketch"]["constraints"][0]["value"] = json!(7.25);
    assert_eq!(bound_annotation(&sketch, &target).unwrap().1["value"], 7.25);
    sketch["sketch"]["constraints"][0]["value"] = Value::Null;
    assert!(bound_annotation(&sketch, &target).is_err());
    sketch["sketch"]["constraints"][0]["value"] = json!(7.25);
    assert!(
        bound_annotation(
            &sketch,
            &json!({"persistent_id":18,"geometry_kind":"line","constraint_kind":"Diameter"})
        )
        .is_err()
    );
    assert!(
        bound_annotation(
            &sketch,
            &json!({"persistent_id":17,"geometry_kind":"line","constraint_kind":"Diameter"})
        )
        .is_err()
    );
    assert!(
        bound_annotation(
            &sketch,
            &json!({"persistent_id":17,"geometry_kind":"arc","constraint_kind":"Radius"})
        )
        .is_err()
    );
    let label = egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(80., 20.));
    let text = egui::Rect::from_min_max(egui::pos2(4., 4.), egui::pos2(72., 16.));
    let clip = label.expand(10.);
    let expected = "\u{2300}7.25 mm";
    assert!(is_constraint_label(
        expected,
        expected,
        text,
        label,
        clip,
        Some(9),
        9
    ));
    assert!(!is_constraint_label(
        "·",
        expected,
        text,
        label,
        clip,
        Some(9),
        9
    ));
    assert!(!is_constraint_label(
        expected,
        expected,
        text,
        label,
        clip,
        Some(8),
        9
    ));
    assert!(!is_constraint_label(
        expected,
        expected,
        text.translate(egui::vec2(100., 0.)),
        label,
        clip,
        Some(9),
        9
    ));
}

fn click_sequence(count: u64, mut emit: impl FnMut(bool)) {
    assert!(
        matches!(count, 1 | 2),
        "one click or one double-click required"
    );
    for _ in 0..count {
        for pressed in [true, false] {
            emit(pressed);
        }
    }
}

#[test]
fn double_click_sequence_is_two_real_pointer_clicks_in_consecutive_frames() {
    let ctx = egui::Context::default();
    let pos = egui::pos2(40., 40.);
    let rect = egui::Rect::from_min_max(egui::pos2(10., 10.), egui::pos2(100., 70.));
    let render = |events, time| {
        let mut observed = (false, false);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(200., 100.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = ui.allocate_rect(rect, egui::Sense::click());
                    observed = (response.clicked(), response.double_clicked());
                });
            },
        );
        observed
    };
    render(vec![], 0.);
    let mut time = 0.;
    let mut transitions = Vec::new();
    click_sequence(2, |pressed| {
        time += 0.1;
        let observed = render(
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            time,
        );
        transitions.push((pressed, observed));
    });
    assert_eq!(transitions.len(), 4);
    assert_eq!(
        transitions
            .iter()
            .map(|(pressed, _)| *pressed)
            .collect::<Vec<_>>(),
        vec![true, false, true, false]
    );
    assert!(
        !transitions[1].1.1,
        "first click must not be a double click"
    );
    assert!(
        transitions[3].1.0 && transitions[3].1.1,
        "second ordinary click must be detected as double click"
    );
}

fn native_origin_plane(editor: &Editor, target: &Value) -> String {
    let role = match target["role"].as_str().unwrap() {
        "top" => "XY_Plane",
        "front" => "XZ_Plane",
        "right" => "YZ_Plane",
        other => panic!("unsupported origin plane {other}"),
    };
    let body = editor
        .body_ui
        .active
        .as_deref()
        .expect("active native Body");
    let bodies: Vec<_> = objects(&editor.state)
        .iter()
        .filter(|o| o["name"] == body && o["type"] == "PartDesign::Body")
        .collect();
    assert_eq!(bodies.len(), 1, "unique current active Body");
    assert_ne!(
        bodies[0]["properties"]["Visibility"], false,
        "active Body hidden"
    );
    let planes: Vec<_> = objects(&editor.state)
        .iter()
        .filter(|o| {
            o["type"] == "App::Plane" && o["body"] == body && o["properties"]["Role"] == role
        })
        .collect();
    assert_eq!(planes.len(), 1, "unique active Body origin plane role");
    let plane = planes[0];
    assert_ne!(
        plane["properties"]["Visibility"], false,
        "origin plane hidden"
    );
    assert_eq!(plane["reference_display"]["kind"], "plane");
    assert_ne!(
        plane["reference_display"]["visible"], false,
        "origin plane display hidden"
    );
    let name = plane["name"].as_str().unwrap();
    assert!(
        editor.state["revision"].as_u64().is_some(),
        "missing native revision"
    );
    assert_eq!(
        editor.viewport.test_reference_revision(name),
        editor.state["revision"].as_u64(),
        "origin plane reference must be current"
    );
    name.to_owned()
}

// Probe the exact input owner inside the current pass, after Editor::update.
// The pointer move lets egui compute its interaction hit from the preceding
// settled pass; that hit must agree with the freshly registered widget.
fn current_target(
    n: &mut Native,
    pos: egui::Pos2,
    target: &Value,
    mods: egui::Modifiers,
    datum_aliases: &std::collections::HashMap<String, String>,
) {
    frame(n, vec![egui::Event::PointerMoved(pos)], mods);
    let expected_revision = n.editor.state["revision"].as_u64();
    n.time += 0.1;
    let mut app_frame = eframe::Frame::_new_kittest();
    let mut captured = None;
    let mut current_annotation: Option<(usize, egui::Rect, String)> = None;
    let output = n.ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 850.),
            )),
            time: Some(n.time),
            events: vec![egui::Event::PointerMoved(pos)],
            modifiers: mods,
            ..Default::default()
        },
        |ctx| {
            eframe::App::update(&mut n.editor, ctx, &mut app_frame);
            let layer = ctx.layer_id_at(pos).expect("current target has no layer");
            let hit = ctx
                .viewport(|v| v.hits.click)
                .expect("current target has no input owner");
            let current = ctx
                .viewport(|v| v.this_pass.widgets.get(hit.id).copied())
                .expect("target owner absent from current pass");
            let response = ctx
                .read_response(hit.id)
                .expect("current owner response absent");
            assert!(
                current.enabled && response.enabled(),
                "current input owner disabled"
            );
            assert!(
                current.sense.senses_click() && current.interact_rect.contains(pos),
                "current input owner cannot receive click"
            );
            assert_eq!(
                current.layer_id, layer,
                "current target is covered by another layer"
            );
            assert_eq!(response.layer_id, layer, "current response layer changed");
            assert_eq!(
                n.editor.state["revision"].as_u64(),
                expected_revision,
                "native revision changed while resolving target"
            );
            assert!(!n.editor.busy, "native operation owns current input");
            let canvas_target = matches!(
                target["kind"].as_str(),
                Some(
                    "scene-vertex"
                        | "datum-plane"
                        | "native-origin-plane"
                        | "constraint-annotation"
                        | "sketch"
                        | "sketch-element"
                        | "plane"
                )
            );
            if canvas_target {
                let canvas = n
                    .editor
                    .viewport
                    .response
                    .as_ref()
                    .expect("current canvas response");
                assert!(
                    canvas.enabled() && canvas.rect.contains(pos),
                    "current canvas disabled or target outside it"
                );
                assert_eq!(
                    canvas.id, current.id,
                    "another widget owns the current pointer"
                );
                assert_eq!(canvas.layer_id, layer, "current canvas layer covered");
            }
            if target["kind"] == "constraint-annotation" {
                let name = n.editor.sketch.as_ref().expect("active sketch");
                let sketch = object(&n.editor.state, name);
                assert!(
                    n.editor.viewport.sketch_matches(&Some(sketch.clone())),
                    "stale current sketch"
                );
                let (_, native_constraint) =
                    bound_annotation(sketch, target).expect("current native annotation binding");
                let constraint = native_constraint["index"].as_u64().unwrap() as usize;
                let (label, text) = n
                    .editor
                    .viewport
                    .test_dimension_label(constraint)
                    .expect("one current rendered label for the native constraint");
                assert!(
                    label.contains(pos),
                    "current target is not inside the native dimension label"
                );
                assert_eq!(
                    n.editor.viewport.dimension_hit(pos),
                    Some(constraint),
                    "annotation changed in current pass"
                );
                current_annotation = Some((constraint, label, text));
            }
            if target["kind"] == "scene-vertex" {
                let owners: Vec<_> = objects(&n.editor.state)
                    .iter()
                    .filter(|o| {
                        o["label"] == target["object_label"]
                            && o["type"] == "Sketcher::SketchObject"
                            && o["effective_visibility"] == true
                    })
                    .collect();
                assert_eq!(owners.len(), 1);
                let owner = owners[0];
                let curves: Vec<_> = owner["sketch"]["geometry"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|g| g["persistent_id"] == target["persistent_id"])
                    .collect();
                assert_eq!(curves.len(), 1);
                let handles: Vec<_> = curves[0]["handles"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|h| h["point"] == target["endpoint"])
                    .collect();
                assert_eq!(handles.len(), 1);
                let world = transform_point(&owner["global_placement"], &handles[0]["position"]);
                assert!(
                    n.editor
                        .viewport
                        .test_world_screen_point(world)
                        .distance(pos)
                        < 0.25,
                    "current vertex moved"
                );
                let hits: Vec<_> = n
                    .editor
                    .viewport
                    .scene_vertices_at(pos)
                    .into_iter()
                    .filter(|h| {
                        h.revision == expected_revision
                            && h.mapped_name.as_ref().is_some_and(|m| !m.is_empty())
                            && objects(&n.editor.state).iter().any(|o| {
                                o["name"] == h.object
                                    && o["type"] == "Sketcher::SketchObject"
                                    && o["effective_visibility"] == true
                            })
                    })
                    .collect();
                assert_eq!(hits.len(), 1);
                assert_eq!(hits[0].object, owner["name"].as_str().unwrap());
                assert_eq!(
                    owner["sketch"]["native_xml"]["element_map"][&hits[0].subelement].as_str(),
                    hits[0].mapped_name.as_deref()
                );
                let actual = n
                    .editor
                    .viewport
                    .test_scene_vertex_world(&hits[0].object, &hits[0].subelement)
                    .unwrap();
                assert!(
                    world
                        .iter()
                        .zip(actual)
                        .map(|(a, b)| (a - b).powi(2))
                        .sum::<f64>()
                        .sqrt()
                        <= 1e-4
                );
            }
            if target["kind"] == "native-origin-plane" {
                let name = native_origin_plane(&n.editor, target);
                let hit = n
                    .editor
                    .viewport
                    .test_scene_pick(pos)
                    .expect("current origin plane hit");
                assert_eq!(hit.object, name, "current native origin plane hit changed");
                let plane = object(&n.editor.state, &name);
                assert_eq!(
                    Some(hit.subelement.as_str()),
                    plane["reference_display"]["subelement"].as_str(),
                    "current native plane subelement changed"
                );
            }
            if target["kind"] == "datum-plane" {
                let hit = n
                    .editor
                    .viewport
                    .test_scene_pick(pos)
                    .expect("current datum has no scene hit");
                if let Some(key) = target["alias"].as_str() {
                    assert_eq!(
                        &hit.object,
                        datum_aliases.get(key).expect("committed datum alias")
                    );
                }
                assert_eq!(
                    n.editor.viewport.test_reference_revision(&hit.object),
                    expected_revision,
                    "current datum display stale"
                );
            }
            captured = Some((current.id, current.interact_rect, layer));
        },
    );
    let (_, rect, _) = captured.expect("current pass did not capture an input owner");
    if let Some((constraint, label, canonical)) = current_annotation {
        let matches: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) => {
                    let text_rect = text.galley.rect.translate(text.pos.to_vec2());
                    (text_rect.contains(pos)
                        && shape.clip_rect.contains(pos)
                        && is_constraint_label(
                            text.galley.text(),
                            &canonical,
                            text_rect,
                            label,
                            shape.clip_rect,
                            Some(constraint),
                            constraint,
                        ))
                    .then_some(text_rect)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "current pass must render one exact native constraint label {constraint}"
        );
    }
    if target["kind"] == "label" {
        let label = target["text"].as_str().unwrap();
        assert!(
            output.shapes.iter().any(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) =>
                    text.galley.text() == label
                        && shape.clip_rect.contains(pos)
                        && rect.contains(pos)
                        && text.galley.rect.translate(text.pos.to_vec2()).contains(pos),
                _ => false,
            }),
            "current input owner no longer contains the intended label"
        );
    }
}

fn point(
    n: &mut Native,
    target: &Value,
    mods: egui::Modifiers,
    datum_aliases: &std::collections::HashMap<String, String>,
) -> egui::Pos2 {
    settle(n, mods);
    let xy = |v: &Value| egui::pos2(v[0].as_f64().unwrap() as f32, v[1].as_f64().unwrap() as f32);
    let pos = match target["kind"].as_str().unwrap() {
        "pixel" => xy(&target["xy"]),
        "sketch" => {
            let p = &target["xy"];
            let pos = n.editor.viewport.sketch_screen_point([
                p[0].as_f64().unwrap(),
                p[1].as_f64().unwrap(),
                0.,
            ]);
            let response = n.editor.viewport.response.as_ref().unwrap();
            assert!(
                response.rect.contains(pos),
                "mapped sketch point outside canvas"
            );
            assert_eq!(
                n.ctx.layer_id_at(pos),
                Some(response.layer_id),
                "mapped sketch point occluded"
            );
            pos
        }
        "sketch-element" => {
            let active = n
                .editor
                .sketch
                .as_ref()
                .or(n.editor.property_ui.axis_profile.as_ref())
                .expect("active sketch or axis profile");
            let scene_label = target["object_label"].as_str();
            let name = if let Some(label) = scene_label {
                let sources: Vec<_> = objects(&n.editor.state)
                    .iter()
                    .filter(|o| o["label"] == label && o["type"] == "Sketcher::SketchObject")
                    .collect();
                assert_eq!(sources.len(), 1, "unique scene sketch label");
                assert_eq!(
                    sources[0]["effective_visibility"], true,
                    "scene sketch must be visible"
                );
                sources[0]["name"].as_str().unwrap()
            } else {
                active
            };
            let sketch = object(&n.editor.state, name);
            if scene_label.is_some() {
                assert_eq!(
                    sketch["global_placement"],
                    object(&n.editor.state, active)["global_placement"],
                    "scene mapping currently requires the same native frame"
                );
            }
            let geometry: Vec<_> = sketch["sketch"]["geometry"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|g| g["persistent_id"] == target["persistent_id"])
                .collect();
            assert_eq!(geometry.len(), 1, "unique current native persistent ID");
            let geometry = geometry[0];
            assert_eq!(
                geometry["kind"], "line",
                "this binding only supports native lines"
            );
            let fraction = match target["point"].as_u64().unwrap_or(0) {
                0 => target["fraction"].as_f64().unwrap_or(0.5),
                1 => 0.,
                2 => 1.,
                _ => panic!("unknown native line point"),
            };
            let local = std::array::from_fn(|i| {
                let start = geometry["start"][i].as_f64().unwrap();
                start + (geometry["end"][i].as_f64().unwrap() - start) * fraction
            });
            let pos = n.editor.viewport.sketch_screen_point(local);
            let response = n.editor.viewport.response.as_ref().unwrap();
            assert!(
                response.rect.contains(pos),
                "native entity point outside canvas"
            );
            assert_eq!(
                n.ctx.layer_id_at(pos),
                Some(response.layer_id),
                "native entity point occluded"
            );
            if scene_label.is_some() {
                if target["construction_axis"] == true {
                    assert_eq!(
                        n.editor
                            .viewport
                            .construction_axis_hit(&n.editor.state, active, pos)
                            .unwrap(),
                        Some(json!({"master":name,"geometry_id":target["persistent_id"]})),
                        "scene axis must hit the bound native construction line"
                    );
                } else {
                    let hits = n.editor.viewport.scene_vertices_at(pos);
                    assert!(
                        hits.iter().any(|h| h.object == name
                            && h.mapped_name.as_ref().is_some_and(|m| !m.is_empty())),
                        "scene point must hit an actual mapped vertex of its native owner"
                    );
                }
            } else {
                let hit = n
                    .editor
                    .viewport
                    .sketch_hit(pos)
                    .expect("native entity point must hit sketch geometry");
                assert_eq!(
                    i64::from(hit.index),
                    geometry["index"].as_i64().unwrap(),
                    "native entity point hit another curve"
                );
                assert_eq!(
                    hit.point,
                    target["point"].as_u64().unwrap_or(0) as i32,
                    "native entity point hit another handle"
                );
            }
            pos
        }
        "scene-vertex" => {
            let revision = n.editor.state["revision"]
                .as_u64()
                .expect("native revision");
            let label = target["object_label"].as_str().expect("sketch label");
            let matches: Vec<_> = objects(&n.editor.state)
                .iter()
                .filter(|o| o["label"] == label && o["type"] == "Sketcher::SketchObject")
                .collect();
            assert_eq!(matches.len(), 1, "unique native sketch label");
            let owner = matches[0];
            assert_eq!(owner["effective_visibility"], true, "native sketch hidden");
            let name = owner["name"].as_str().unwrap();
            let geometry: Vec<_> = owner["sketch"]["geometry"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|g| g["persistent_id"] == target["persistent_id"])
                .collect();
            assert_eq!(geometry.len(), 1, "unique native geometry identity");
            let endpoint = target["endpoint"].as_u64().unwrap();
            assert!(endpoint == 1 || endpoint == 2);
            let handles: Vec<_> = geometry[0]["handles"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|h| h["point"].as_u64() == Some(endpoint))
                .collect();
            assert_eq!(handles.len(), 1, "unique native geometry endpoint");
            let world = transform_point(&owner["global_placement"], &handles[0]["position"]);
            let pos = n.editor.viewport.test_world_screen_point(world);
            let response = n.editor.viewport.response.as_ref().unwrap();
            assert!(response.rect.contains(pos), "scene vertex outside canvas");
            assert_eq!(
                n.ctx.layer_id_at(pos),
                Some(response.layer_id),
                "scene vertex occluded"
            );
            let hits: Vec<_> = n
                .editor
                .viewport
                .scene_vertices_at(pos)
                .into_iter()
                .filter(|h| {
                    h.revision == Some(revision)
                        && h.mapped_name.as_ref().is_some_and(|m| !m.is_empty())
                        && objects(&n.editor.state).iter().any(|o| {
                            o["name"] == h.object
                                && o["type"] == "Sketcher::SketchObject"
                                && o["effective_visibility"] == true
                        })
                })
                .collect();
            assert_eq!(
                hits.len(),
                1,
                "expected one current visible native vertex at endpoint"
            );
            assert_eq!(
                hits[0].object, name,
                "vertex belongs to another native object"
            );
            let native_world = n
                .editor
                .viewport
                .test_scene_vertex_world(name, &hits[0].subelement)
                .unwrap();
            let distance = world
                .iter()
                .zip(native_world)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!(
                distance <= 1e-4,
                "mapped vertex is not the specified native geometry endpoint"
            );
            assert_eq!(
                owner["sketch"]["native_xml"]["element_map"][&hits[0].subelement].as_str(),
                hits[0].mapped_name.as_deref(),
                "vertex element map changed"
            );
            pos
        }
        "native-origin-plane" => {
            let name = native_origin_plane(&n.editor, target);
            let plane = object(&n.editor.state, &name);
            let descriptor = &plane["reference_display"];
            let extent = objects(&n.editor.state)
                .iter()
                .flat_map(|o| {
                    (0..3).filter_map(move |axis| {
                        let min = o["shape"]["bounds"]["min"][axis].as_f64()?;
                        let max = o["shape"]["bounds"]["max"][axis].as_f64()?;
                        let size = (max - min) as f32;
                        (size.is_finite() && size > 0.).then_some(size)
                    })
                })
                .fold(0_f32, f32::max) as f64;
            let w = descriptor["size"][0].as_f64().unwrap().max(extent * 1.25);
            let h = descriptor["size"][1].as_f64().unwrap().max(extent * 1.25);
            assert!(w.is_finite() && h.is_finite() && w > 0. && h > 0.);
            let response = n
                .editor
                .viewport
                .response
                .as_ref()
                .expect("canvas response");
            let mut pos = None;
            'search_origin: for x in -4..=4 {
                for y in -4..=4 {
                    let world = transform_point(
                        &descriptor["placement"],
                        &json!([w * x as f64 / 10., h * y as f64 / 10., 0.]),
                    );
                    let p = n.editor.viewport.test_world_screen_point(world);
                    if response.interact_rect.contains(p)
                        && n.ctx.layer_id_at(p) == Some(response.layer_id)
                        && n.editor.viewport.test_scene_pick(p).is_some_and(|hit| {
                            hit.object == name
                                && Some(hit.subelement.as_str())
                                    == descriptor["subelement"].as_str()
                        })
                    {
                        pos = Some(p);
                        break 'search_origin;
                    }
                }
            }
            pos.expect("visible native origin plane reference hit")
        }
        "datum-plane" => {
            let revision = n.editor.state["revision"]
                .as_u64()
                .expect("native revision");
            let support = target["support_object_label"].as_str().map(|label| {
                let objects: Vec<_> = objects(&n.editor.state)
                    .iter()
                    .filter(|o| o["label"] == label)
                    .collect();
                assert_eq!(objects.len(), 1, "unique support label");
                objects[0]["name"].as_str().unwrap().to_owned()
            });
            let label = target["object_label"].as_str();
            let alias = target["alias"]
                .as_str()
                .map(|key| datum_aliases.get(key).expect("committed datum alias"));
            assert!(
                label.is_some() || support.is_some() || alias.is_some(),
                "datum selector needs an identity"
            );
            let matches: Vec<_> =
                objects(&n.editor.state)
                    .iter()
                    .filter(|o| {
                        o["type"] == "PartDesign::Plane"
                            && alias.is_none_or(|name| o["name"] == name.as_str())
                            && label.is_none_or(|v| o["label"] == v)
                            && support.as_ref().is_none_or(|name| {
                                o["properties"]["AttachmentSupport"].as_array().is_some_and(
                                    |links| links.iter().any(|link| link["link"] == name.as_str()),
                                )
                            })
                    })
                    .collect();
            assert_eq!(matches.len(), 1, "unique live native datum plane");
            let plane = matches[0];
            assert_ne!(plane["properties"]["Visibility"], false);
            assert_eq!(plane["reference_display"]["kind"], "plane");
            assert_ne!(plane["reference_display"]["visible"], false);
            let name = plane["name"].as_str().unwrap();
            assert_eq!(
                n.editor.viewport.test_reference_revision(name),
                Some(revision),
                "stale native datum display"
            );
            let descriptor = &plane["reference_display"];
            let w = descriptor["size"][0].as_f64().unwrap();
            let h = descriptor["size"][1].as_f64().unwrap();
            assert!(w.is_finite() && h.is_finite() && w > 0. && h > 0.);
            let rect = n.editor.viewport.response.as_ref().unwrap().rect;
            let layer = n.editor.viewport.response.as_ref().map(|r| r.layer_id);
            let mut pos = None;
            'search: for x in -4..=4 {
                for y in -4..=4 {
                    let world = transform_point(
                        &descriptor["placement"],
                        &json!([w * x as f64 / 10., h * y as f64 / 10., 0.]),
                    );
                    let p = n.editor.viewport.test_world_screen_point(world);
                    if rect.contains(p)
                        && n.ctx.layer_id_at(p) == layer
                        && n.editor
                            .viewport
                            .test_scene_pick(p)
                            .is_some_and(|hit| hit.object == name)
                    {
                        pos = Some(p);
                        break 'search;
                    }
                }
            }
            pos.expect("visible actual datum plane hit")
        }
        "constraint-annotation" => {
            let revision = n.editor.state["revision"]
                .as_u64()
                .expect("native revision");
            let active = n
                .editor
                .sketch
                .as_ref()
                .expect("active native sketch")
                .clone();
            let sketch = object(&n.editor.state, &active);
            assert!(
                n.editor.viewport.sketch_matches(&Some(sketch.clone())),
                "stale sketch display"
            );
            let (index, native_constraint) =
                bound_annotation(sketch, target).expect("unique current native annotation binding");
            let geometry_kind = sketch["sketch"]["geometry"]
                .as_array()
                .unwrap()
                .iter()
                .find(|g| g["persistent_id"] == target["persistent_id"])
                .unwrap()["kind"]
                .clone();
            let native_constraint = (*native_constraint).clone();
            let constraint = native_constraint["index"].as_u64().unwrap() as usize;
            let rendered = frame(n, vec![], mods);
            assert_eq!(
                n.editor.state["revision"].as_u64(),
                Some(revision),
                "native revision changed"
            );
            let label = n.editor.viewport.test_dimension_label(constraint);
            let mut candidates = Vec::new();
            let mut positions = Vec::new();
            for shape in &rendered.shapes {
                if let egui::epaint::Shape::Text(text) = &shape.shape {
                    let text_rect = text.galley.rect.translate(text.pos.to_vec2());
                    let pos = text_rect.center();
                    let hit = n.editor.viewport.dimension_hit(pos);
                    let text_match = label
                        .as_ref()
                        .is_some_and(|(_, canonical)| text.galley.text() == canonical);
                    let in_label = label.as_ref().is_some_and(|(rect, _)| {
                        rect.contains(text_rect.min) && rect.contains(text_rect.max)
                    });
                    let overlaps = label
                        .as_ref()
                        .is_some_and(|(rect, _)| rect.intersects(text_rect));
                    let clipped = shape.clip_rect.contains(pos);
                    if hit == Some(constraint) || text_match || overlaps {
                        candidates.push(json!({"text":text.galley.text(),
                            "text_rect":[text_rect.min.x,text_rect.min.y,text_rect.max.x,text_rect.max.y],
                            "clip_rect":[shape.clip_rect.min.x,shape.clip_rect.min.y,shape.clip_rect.max.x,shape.clip_rect.max.y],
                            "center":[pos.x,pos.y],"dimension_hit":hit,"exact_text":text_match,
                            "inside_label":in_label,"center_in_clip":clipped}));
                    }
                    if label.as_ref().is_some_and(|(rect, canonical)| {
                        is_constraint_label(
                            text.galley.text(),
                            canonical,
                            text_rect,
                            *rect,
                            shape.clip_rect,
                            hit,
                            constraint,
                        )
                    }) {
                        positions.push(pos);
                    }
                }
            }
            fs::write(n.folder.join(format!("annotation-candidates-{revision}-{constraint}.json")),
                serde_json::to_vec_pretty(&json!({"revision":revision,"sketch":active,
                    "geometry_persistent_id":target["persistent_id"],"geometry_index":index,
                    "geometry_kind":geometry_kind,"constraint_index":constraint,"constraint_kind":target["constraint_kind"],
                    "native_value":native_constraint["value"],"native_quantity":native_constraint["quantity"],
                    "canonical_text":label.as_ref().map(|(_,text)|text),
                    "label_rect":label.as_ref().map(|(r,_)|[r.min.x,r.min.y,r.max.x,r.max.y]),
                    "candidates":candidates})).unwrap()).unwrap();
            assert!(
                label.is_some(),
                "one rendered label for the native constraint"
            );
            assert_eq!(
                positions.len(),
                1,
                "unique exact current native constraint label"
            );
            positions[0]
        }
        "plane" => {
            let role = match target["role"].as_str().unwrap() {
                "front" => crate::viewport::StandardPlane::Front,
                "top" => crate::viewport::StandardPlane::Top,
                "right" => crate::viewport::StandardPlane::Right,
                other => panic!("unknown plane {other}"),
            };
            n.editor
                .viewport
                .standard_guide_probe(role)
                .expect("visible native plane guide")
        }
        "tree-eye" => {
            let label = target["object_label"]
                .as_str()
                .expect("tree-eye object label");
            let objects: Vec<_> = objects(&n.editor.state)
                .iter()
                .filter(|o| o["label"] == label)
                .collect();
            assert_eq!(objects.len(), 1, "tree-eye owner label must be unique");
            let name = objects[0]["name"].as_str().unwrap().to_owned();
            let (_, response) = n.frame_widget(egui::Id::new(("feature_visibility", name)), mods);
            let response = response.expect("native feature eye must be in the visible tree");
            assert!(response.enabled(), "native feature eye must be enabled");
            assert_eq!(
                n.ctx.layer_id_at(response.rect.center()),
                Some(response.layer_id),
                "native feature eye must not be occluded"
            );
            response.rect.center()
        }
        "label" => {
            let frame = frame(n, vec![], mods);
            let scope = match target["scope"].as_str() {
                Some("shortcut") => Some(
                    n.ctx
                        .memory(|m| m.area_rect(egui::Id::new("tool_shortcut_menu")))
                        .expect("shortcut menu open"),
                ),
                None => None,
                other => panic!("unsupported label scope {other:?}"),
            };
            let found: Vec<_> = frame
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text)
                        if text.galley.text() == target["text"].as_str().unwrap() =>
                    {
                        let rect = text.galley.rect.translate(text.pos.to_vec2());
                        scope
                            .is_none_or(|area| area.contains(rect.center()))
                            .then_some(rect.center())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                found.len(),
                1,
                "target label must be unique in its scope: {target}"
            );
            if target["text"] == "Revolve" {
                let point = found[0];
                let feature_rect = n.ctx.memory(|m| m.area_rect(egui::Id::new("features")));
                let canvas_rect = n.editor.viewport.response.as_ref().unwrap().rect;
                let mut text_shapes = Vec::new();
                for shape in &frame.shapes {
                    if let egui::epaint::Shape::Text(text) = &shape.shape {
                        let rect = text.galley.rect.translate(text.pos.to_vec2());
                        text_shapes.push(json!({"text":text.galley.text(),
                            "rect":[rect.min.x,rect.min.y,rect.max.x,rect.max.y],
                            "clip":[shape.clip_rect.min.x,shape.clip_rect.min.y,shape.clip_rect.max.x,shape.clip_rect.max.y]}));
                    }
                }
                fs::write(n.folder.join("revolve-target-diagnostic.json"),serde_json::to_vec_pretty(&json!({
                    "target":[point.x,point.y],"layer_at_target":format!("{:?}",n.ctx.layer_id_at(point)),
                    "features_rect":feature_rect.map(|r|json!([r.min.x,r.min.y,r.max.x,r.max.y])),
                    "canvas_rect":[canvas_rect.min.x,canvas_rect.min.y,canvas_rect.max.x,canvas_rect.max.y],
                    "features_contains_target":feature_rect.is_some_and(|r|r.contains(point)),
                    "busy":n.editor.busy,"draft":n.editor.draft.is_some(),"session":n.editor.sketch_ui.session,
                    "sketch_creation":n.editor.sketch_creation.is_some(),"drag":n.editor.sketch_ui.drag.is_some(),
                    "dimension":n.editor.sketch_ui.dimension.is_some(),"task":n.editor.sketch_ui.task.is_some(),
                    "pending_file":n.editor.pending_file.is_some(),"file_dialog":n.editor.file_dialog.is_some(),
                    "overwrite":n.editor.overwrite,"units_active":n.editor.units_ui.active(),
                    "popup":egui::Popup::is_any_open(&n.ctx),"texts":text_shapes
                })).unwrap()).unwrap();
                let features = feature_rect.expect("Features window must be visible");
                assert!(!features.contains(point), "Features must not cover Revolve");
                assert!(
                    features.top() >= canvas_rect.top(),
                    "Features must stay below the toolbar"
                );
            }
            found[0]
        }
        other => panic!("unsupported point mapping {other}"),
    };
    if matches!(
        target["kind"].as_str(),
        Some(
            "scene-vertex"
                | "datum-plane"
                | "native-origin-plane"
                | "constraint-annotation"
                | "sketch"
                | "sketch-element"
                | "plane"
                | "label"
        )
    ) {
        current_target(n, pos, target, mods, datum_aliases);
    }
    pos
}

fn is_modifier(name: &str) -> bool {
    matches!(
        name,
        "ShiftLeft" | "ShiftRight" | "CtrlLeft" | "CtrlRight" | "AltLeft" | "AltRight"
    )
}

fn modifiers(action: &Value, held: &std::collections::HashSet<String>) -> egui::Modifiers {
    let mut result = egui::Modifiers::NONE;
    let names = action["modifiers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| s.as_str().unwrap())
        .chain(held.iter().filter(|s| is_modifier(s)).map(String::as_str));
    for name in names {
        match name {
            "ShiftLeft" | "ShiftRight" => result.shift = true,
            "CtrlLeft" | "CtrlRight" => {
                result.ctrl = true;
                result.command = true;
            }
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
            "ShiftLeft" | "ShiftRight" => mods.shift = pressed,
            "CtrlLeft" | "CtrlRight" => {
                mods.ctrl = pressed;
                mods.command = pressed;
            }
            "AltLeft" | "AltRight" => mods.alt = pressed,
            _ => unreachable!(),
        }
        frame(n, vec![], mods);
        return;
    }
    // egui represents keypad digits/Enter with logical number-row/Enter keys.
    let normalized = match name {
        "NumEnter" => "Enter",
        "Num0" => "0",
        "Num1" => "1",
        "Num2" => "2",
        "Num3" => "3",
        "Num4" => "4",
        "Num5" => "5",
        "Num6" => "6",
        "Num7" => "7",
        "Num8" => "8",
        "Num9" => "9",
        other => other,
    };
    let key = Key::from_name(normalized).unwrap_or_else(|| panic!("unsupported key {name}"));
    let mut events = vec![egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: mods,
    }];
    if pressed && let Some(text) = action["text"].as_str() {
        events.push(egui::Event::Text(text.to_owned()));
    }
    frame(n, events, mods);
}

fn camera(n: &Native) -> Value {
    if n.editor.sketch.is_none() {
        return Value::Null;
    }
    let rect = n
        .editor
        .viewport
        .response
        .as_ref()
        .expect("current canvas")
        .rect;
    let projected: Vec<_> = [[0., 0., 0.], [10., 0., 0.], [0., 10., 0.]]
        .into_iter()
        .map(|world| {
            let point = n.editor.viewport.sketch_screen_point(world);
            json!({"world":world,"pixel":[point.x,point.y]})
        })
        .collect();
    let panel = n
        .ctx
        .memory(|m| m.area_rect(egui::Id::new("native_sketch_controls")))
        .map(|r| [r.min.x, r.min.y, r.max.x, r.max.y]);
    json!({"canvas":[rect.min.x,rect.min.y,rect.width(),rect.height()],"sketch_controls_rect":panel,"anchors":projected})
}

fn expected(n: &Native, action: &Value) {
    if action.get("expect").is_none() {
        return;
    }
    let expectations = action["expect"]
        .as_object()
        .expect("expect must be an object");
    assert!(!expectations.is_empty());
    let sketch = n
        .editor
        .sketch
        .as_ref()
        .map(|name| object(&n.editor.state, name)["sketch"].clone())
        .unwrap_or(Value::Null);
    let state = json!({"native":n.editor.state,"sketch":sketch,"ui":n.ui_state(),"camera":camera(n),
        "input":n.ctx.input(|i|json!({"ctrl":i.modifiers.ctrl,"shift":i.modifiers.shift,"alt":i.modifiers.alt,
            "pointer":i.pointer.latest_pos().map(|p|[p.x,p.y]),"buttons_down":i.pointer.any_down()}))});
    for (path, value) in expectations {
        assert_eq!(
            state.pointer(path).expect("expected JSON pointer exists"),
            value,
            "checkpoint {path}"
        );
    }
}

#[test]
#[ignore = "mapped video input replay through actual Editor/native worker"]
fn tutorial_replay_from_json() {
    let plan: Value = serde_json::from_slice(
        &fs::read(std::env::var_os("CAD_TUTORIAL_PLAN").expect("CAD_TUTORIAL_PLAN")).unwrap(),
    )
    .unwrap();
    assert_eq!(plan["version"], 1);
    let mut n = Native::new("tutorial-replay");
    n.send("new", json!({"name":"TutorialReplay"}), true);
    assert!(objects(&n.editor.state).is_empty());
    let mut held = std::collections::HashSet::<String>::new();
    let mut datum_aliases = std::collections::HashMap::<String, String>::new();
    let mut geometry_aliases = std::collections::HashMap::<String, i64>::new();
    let mut plane_baseline: Option<(
        std::collections::BTreeSet<String>,
        String,
        String,
        String,
        String,
    )> = None;
    for source_action in plan["actions"].as_array().unwrap() {
        let mut action = source_action.clone();
        if let Some(key) = action["target"]["persistent_id_alias"].as_str() {
            let id = *geometry_aliases
                .get(key)
                .expect("resolved native geometry alias");
            action["target"]["persistent_id"] = json!(id);
        }
        let action = &action;
        let kind = action["kind"].as_str().unwrap();
        let before_geometry = if action["capture_line_alias"].is_string() {
            let active = n.editor.sketch.as_ref().expect("active sketch before line");
            Some(
                object(&n.editor.state, active)["sketch"]["geometry"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|g| g["persistent_id"].as_i64())
                    .collect::<std::collections::BTreeSet<_>>(),
            )
        } else {
            None
        };
        if action["capture_plane_baseline"] == true {
            assert_eq!(
                n.editor.viewport.selections.len(),
                2,
                "plane and point must be selected"
            );
            let plane = &n.editor.viewport.selections[0];
            let vertex = &n.editor.viewport.selections[1];
            let top = object(&n.editor.state, &plane.object);
            assert_eq!(top["type"], "App::Plane");
            assert_eq!(
                top["properties"]["Role"], "XY_Plane",
                "actual Top support required"
            );
            let owner = object(&n.editor.state, &vertex.object);
            assert_eq!(owner["type"], "Sketcher::SketchObject");
            assert_eq!(owner["effective_visibility"], true);
            let (mapped, revision) = n
                .editor
                .viewport
                .scene_vertex_identity(vertex)
                .expect("native selected vertex identity");
            assert_eq!(Some(revision), n.editor.state["revision"].as_u64());
            let names = objects(&n.editor.state)
                .iter()
                .filter_map(|o| o["name"].as_str().map(str::to_owned))
                .collect();
            plane_baseline = Some((
                names,
                plane.object.clone(),
                vertex.object.clone(),
                vertex.subelement.clone(),
                mapped,
            ));
        }
        if kind == "key_down" {
            assert!(held.insert(action["key"].as_str().unwrap().to_owned()));
        }
        if kind == "key_up" {
            assert!(held.remove(action["key"].as_str().unwrap()));
        }
        let mods = modifiers(action, &held);
        settle(&mut n, mods);
        let mut resolved = Value::Null;
        match kind {
            "click" => {
                let pos = point(&mut n, &action["target"], mods, &datum_aliases);
                resolved = json!([pos.x, pos.y]);
                let button = match action["button"].as_str().unwrap() {
                    "left" => egui::PointerButton::Primary,
                    "right" => egui::PointerButton::Secondary,
                    "middle" => egui::PointerButton::Middle,
                    other => panic!("unsupported button {other}"),
                };
                let count = action["click_count"].as_u64().unwrap_or(1);
                click_sequence(count, |pressed| {
                    frame(
                        &mut n,
                        vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button,
                                pressed,
                                modifiers: mods,
                            },
                        ],
                        mods,
                    );
                    if count > 1 {
                        use std::io::Write as _;
                        let mut log = fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(n.folder.join("multi-click-events.jsonl"))
                            .unwrap();
                        writeln!(
                            log,
                            "{}",
                            json!({"action_id":action["id"],"button":action["button"],
                            "position":[pos.x,pos.y],"pressed":pressed,"frame_time":n.time})
                        )
                        .unwrap();
                    }
                });
                if action["target"]["kind"] == "scene-vertex" {
                    let label = action["target"]["object_label"].as_str().unwrap();
                    let owner = objects(&n.editor.state)
                        .iter()
                        .find(|o| o["label"] == label)
                        .unwrap();
                    let name = owner["name"].as_str().unwrap();
                    let revision = n.editor.state["revision"].as_u64().unwrap();
                    let hits = n.editor.viewport.scene_vertices_at(pos);
                    let vertex = hits
                        .iter()
                        .find(|h| h.object == name && h.revision == Some(revision))
                        .unwrap();
                    assert!(
                        n.editor
                            .viewport
                            .selections
                            .iter()
                            .any(|s| s.object == name && s.subelement == vertex.subelement),
                        "ordinary click did not select the intended current vertex"
                    );
                }
                if action["target"]["kind"] == "native-origin-plane" {
                    let name = native_origin_plane(&n.editor, &action["target"]);
                    let hit = n
                        .editor
                        .viewport
                        .test_scene_pick(pos)
                        .expect("native origin plane after click");
                    assert_eq!(hit.object, name);
                    assert!(
                        n.editor
                            .viewport
                            .selections
                            .iter()
                            .any(|s| s.object == hit.object && s.subelement == hit.subelement),
                        "ordinary click did not select native origin plane"
                    );
                }
                if action["target"]["kind"] == "datum-plane" {
                    let hit = n
                        .editor
                        .viewport
                        .test_scene_pick(pos)
                        .expect("datum hit after click");
                    assert!(
                        n.editor
                            .viewport
                            .selections
                            .iter()
                            .any(|s| s.object == hit.object && s.subelement == hit.subelement),
                        "ordinary click did not select the intended datum"
                    );
                }
            }
            "move" => {
                if action["target"]["kind"] == "path" {
                    for sample in action["target"]["points"].as_array().unwrap() {
                        let pos = egui::pos2(
                            sample["xy"][0].as_f64().unwrap() as f32,
                            sample["xy"][1].as_f64().unwrap() as f32,
                        );
                        frame(&mut n, vec![egui::Event::PointerMoved(pos)], mods);
                    }
                    resolved = action["target"].clone();
                } else {
                    let pos = point(&mut n, &action["target"], mods, &datum_aliases);
                    resolved = json!([pos.x, pos.y]);
                    frame(&mut n, vec![egui::Event::PointerMoved(pos)], mods);
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
                    let pos = egui::pos2(
                        sample["xy"][0].as_f64().unwrap() as f32,
                        sample["xy"][1].as_f64().unwrap() as f32,
                    );
                    let mut events = vec![egui::Event::PointerMoved(pos)];
                    if i == 0 {
                        events.push(egui::Event::PointerButton {
                            pos,
                            button,
                            pressed: true,
                            modifiers: mods,
                        });
                    }
                    frame(&mut n, events, mods);
                }
                let last = points.last().unwrap();
                let pos = egui::pos2(
                    last["xy"][0].as_f64().unwrap() as f32,
                    last["xy"][1].as_f64().unwrap() as f32,
                );
                // The final motion reaches a held-button frame before release.
                // Combining them would lose the last camera delta in egui.
                frame(
                    &mut n,
                    vec![egui::Event::PointerButton {
                        pos,
                        button,
                        pressed: false,
                        modifiers: mods,
                    }],
                    mods,
                );
                resolved = action["target"].clone();
            }
            "scroll" => {
                let pos = point(&mut n, &action["target"], mods, &datum_aliases);
                resolved = json!([pos.x, pos.y]);
                frame(
                    &mut n,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(
                                action["delta"][0].as_f64().unwrap() as f32,
                                action["delta"][1].as_f64().unwrap() as f32,
                            ),
                            modifiers: mods,
                        },
                    ],
                    mods,
                );
            }
            "key" => {
                keyboard(&mut n, action, true, mods);
                keyboard(&mut n, action, false, mods);
            }
            "key_down" | "key_up" => {
                if is_modifier(action["key"].as_str().unwrap()) {
                    frame(&mut n, vec![], mods);
                } else {
                    keyboard(&mut n, action, kind == "key_down", mods);
                }
            }
            "text" => {
                frame(
                    &mut n,
                    vec![egui::Event::Text(
                        action["text"].as_str().unwrap().to_owned(),
                    )],
                    mods,
                );
            }
            "checkpoint" => (),
            other => panic!("egui adapter does not implement {other}; no substitution"),
        }
        for op in action["wait_for"].as_array().into_iter().flatten() {
            // Deferred previews are queued by ordinary Editor frames after their
            // debounce interval. Keep rendering before waiting for that reply.
            let deadline = Instant::now() + Duration::from_secs(5);
            while !n.editor.busy {
                assert!(
                    Instant::now() < deadline,
                    "mapped action did not queue its expected native reply: {op}; {:?}",
                    n.ui_state()
                );
                frame(&mut n, vec![], mods);
                std::thread::sleep(Duration::from_millis(5));
            }
            n.receive(op.as_str().unwrap(), true);
        }
        if let Some(alias) = action["capture_plane_alias"].as_str() {
            let (before, top, master, vertex, mapped) =
                plane_baseline.take().expect("captured pre-preview state");
            let planes: Vec<_> = objects(&n.editor.state)
                .iter()
                .filter(|o| {
                    o["type"] == "PartDesign::Plane"
                        && o["name"]
                            .as_str()
                            .is_some_and(|name| !before.contains(name))
                })
                .collect();
            assert_eq!(
                planes.len(),
                1,
                "preview commit must add exactly one native datum plane"
            );
            let plane = planes[0];
            assert_eq!(plane["properties"]["MapMode"], "ParallelPlane");
            let support = plane["properties"]["AttachmentSupport"].as_array().unwrap();
            assert!(support.iter().any(|s| s["link"] == top));
            assert!(support.iter().any(|s| {
                s["link"] == master
                    && s["subelements"]
                        .as_array()
                        .is_some_and(|v| v.iter().any(|v| v == &vertex))
            }));
            assert!(
                plane["test_support_xml"]
                    .as_str()
                    .is_some_and(|xml| xml.contains(&mapped)),
                "persisted mapped vertex identity missing"
            );
            assert!(
                datum_aliases
                    .insert(alias.to_owned(), plane["name"].as_str().unwrap().to_owned())
                    .is_none()
            );
        }
        if let Some(alias) = action["capture_line_alias"].as_str() {
            let previous = before_geometry.as_ref().unwrap();
            let active = n.editor.sketch.as_ref().expect("active sketch after line");
            let sketch = object(&n.editor.state, active);
            let added: Vec<_> = sketch["sketch"]["geometry"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|g| {
                    g["persistent_id"]
                        .as_i64()
                        .is_some_and(|id| !previous.contains(&id))
                })
                .collect();
            assert_eq!(added.len(), 1, "one newly drawn native line");
            assert_eq!(added[0]["kind"], "line");
            assert!(
                geometry_aliases
                    .insert(
                        alias.to_owned(),
                        added[0]["persistent_id"].as_i64().unwrap()
                    )
                    .is_none()
            );
        }
        if let Some(alias) = action["capture_diameter_alias"].as_str() {
            let active = n.editor.sketch.as_ref().expect("active slot sketch");
            let sketch = object(&n.editor.state, active);
            let constraints: Vec<_> = sketch["sketch"]["constraints"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|c| c["type"] == "Diameter")
                .collect();
            assert_eq!(constraints.len(), 1, "unique native slot diameter");
            let index = constraints[0]["first"].as_i64().unwrap();
            let geometry: Vec<_> = sketch["sketch"]["geometry"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|g| g["index"].as_i64() == Some(index))
                .collect();
            assert_eq!(geometry.len(), 1);
            assert!(
                geometry_aliases
                    .insert(
                        alias.to_owned(),
                        geometry[0]["persistent_id"].as_i64().unwrap()
                    )
                    .is_none()
            );
        }
        settle(&mut n, mods);
        expected(&n, action);
        if action["id"] == "M09" {
            let frame = frame(&mut n, vec![], mods);
            for label in ["Select", "Finish"] {
                assert!(
                    frame.shapes.iter().any(|shape| {
                        matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == label)
                    }),
                    "accepted sketch must expose {label}"
                );
            }
            let before = n
                .events
                .iter()
                .find(|event| event["replay_action"]["id"] == "M08")
                .unwrap();
            let top_before = before["features_rect"][1].as_f64().unwrap() as f32;
            let canvas = n.editor.viewport.response.as_ref().unwrap().rect;
            let features = n
                .ctx
                .memory(|m| m.area_rect(egui::Id::new("features")))
                .unwrap();
            assert_eq!(
                features.top(),
                top_before.max(canvas.top()),
                "Features stays at its prior position, clamped below the toolbar"
            );
        }
        let features = n.ctx.memory(|m| m.area_rect(egui::Id::new("features")));
        let canvas = n.editor.viewport.response.as_ref().map(|r| r.rect);
        if action["id"] == "M16-enter" {
            let previous = n
                .events
                .iter()
                .find(|event| event["replay_action"]["id"] == "M16-2")
                .unwrap();
            let top_before = previous["features_rect"][1].as_f64().unwrap() as f32;
            assert_eq!(
                features.unwrap().top(),
                top_before,
                "busy line entry must not move Features"
            );
        }
        n.events.push(json!({"replay_action":action,"resolved_target":resolved,"ui":n.ui_state(),"camera":camera(&n),"state":n.editor.state,"physical_desktop":false,
            "features_rect":features.map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]),
            "canvas_rect":canvas.map(|r|[r.min.x,r.min.y,r.max.x,r.max.y])}));
        n.record();
    }
    assert!(held.is_empty(), "replay left keys held");
    if let Some(name) = plan["verification_save"].as_str() {
        assert!(name.len() > 6 && name.ends_with(".FCStd"));
        assert!(
            name.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c)),
            "verification filename must stay within owned output"
        );
        assert!(
            n.editor.sketch_ui.session.is_none() && n.editor.draft.is_none(),
            "verification save requires a completed edit"
        );
        let before = n.editor.state.clone();
        let path = n.save(name);
        n.events.push(json!({"verification_control":{"op":"save","path":path,"source_video_input":false},"before":before,"state":n.editor.state,"physical_desktop":false}));
        n.record();
    }
}
