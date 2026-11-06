//! Draft only. Source L01-L04, 01:25-02:13. No native execution by its author.
//! Child module of editor::sketch::lifecycle_tests. See REGISTRATION.md.
use super::*;
use std::time::Duration;

const TUTORIAL: &str = "TPD-LOFT-GUIDES-ZxXFKepkuMU";
const STEPS: &[&str] = &[
    "fixture-empty-local-document",
    "L01-Sketch-first",
    "L01-Top-plane",
    "L01-normal",
    "L02-Ellipse-menu",
    "L02-center",
    "L02-major-click",
    "L02-minor-click",
    "L02-inline-major-200",
    "L02-inline-minor-100",
    "L03-minor-100-to-80",
    "L03-major-200-to-150",
    "L04-name",
    "L04-Finish",
];

fn note(n: &mut Native, step: &str, phase: &str, detail: Value) {
    n.events
        .push(json!({"tutorial":TUTORIAL,"step":step,"phase":phase,
        "detail":detail,"ui":n.ui_state(),"state":n.editor.state,
        "tutorial_credit":0}));
    n.record();
}

fn stop(n: &mut Native, step: &str, reason: &str) -> ! {
    let later: Vec<_> = STEPS.iter().skip_while(|s| **s != step).skip(1).collect();
    let result = json!({"tutorial":TUTORIAL,"status":"blocked",
        "first_unsupported_step":step,"reason":reason,"later_prefix_steps_untested":later,
        "L05_through_L32":"untested","full_tutorial_pass":false,"tutorial_credit":0});
    note(n, step, "blocked", result.clone());
    fs::write(
        n.folder.join("tutorial-attempt.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    panic!("{TUTORIAL} stopped at {step}: {reason}");
}

fn require(n: &mut Native, step: &str, ok: bool, reason: &str) {
    if !ok {
        stop(n, step, reason);
    }
}

macro_rules! check {
    ($n:expr, $step:expr, $condition:expr, $reason:expr) => {{
        let ok = $condition;
        require($n, $step, ok, $reason);
    }};
}

fn settle(n: &mut Native) {
    for _ in 0..3 {
        n.frame(vec![]);
    }
}

fn input(n: &mut Native, step: &str, description: Value, events: Vec<egui::Event>) {
    note(n, step, "input", description);
    n.frame(events);
}

fn pointer(n: &mut Native, step: &str, pos: egui::Pos2, role: &str) {
    for pressed in [true, false] {
        input(
            n,
            step,
            json!({"kind":"pointer","role":role,"screen":[pos.x,pos.y],
            "pressed":pressed,"button":"primary","modifiers":"none"}),
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

fn key(n: &mut Native, step: &str, key: Key, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        input(
            n,
            step,
            json!({"kind":"key","key":format!("{key:?}"),
            "modifiers":format!("{modifiers:?}"),"pressed":pressed}),
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            }],
        );
    }
}

fn texts(n: &mut Native) -> Vec<(String, egui::Rect)> {
    settle(n);
    let list: Vec<_> = n
        .frame(vec![])
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) => Some((
                t.galley.text().to_owned(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            _ => None,
        })
        .collect();
    list
}

fn click(n: &mut Native, step: &str, label: &str) {
    let all = texts(n);
    let matches: Vec<_> = all.iter().filter(|(text, _)| text == label).collect();
    note(
        n,
        step,
        "visible-controls",
        json!({"required":label,"matches":matches.len(),
        "all":all.iter().map(|(text,r)|json!({"text":text,"rect":[r.min.x,r.min.y,r.max.x,r.max.y]})).collect::<Vec<_>>()}),
    );
    if matches.len() != 1 {
        stop(
            n,
            step,
            &format!(
                "required visible control {label:?} has {} matches",
                matches.len()
            ),
        );
    }
    pointer(n, step, matches[0].1.center(), label);
}

fn drain(n: &mut Native, step: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while n.editor.busy {
        let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) else {
            stop(n, step, "native action deadline expired");
        };
        let reply = n
            .raw
            .recv_timeout(left)
            .unwrap_or_else(|_| stop(n, step, "native reply timed out"));
        let accepted = reply.accepted;
        note(
            n,
            step,
            "native-reply",
            json!({"op":reply.op,"accepted":reply.accepted,
            "error":reply.error,"state":reply.state,"scenes":reply.scenes}),
        );
        n.forward.send(reply).unwrap();
        n.editor.poll();
        if !accepted {
            stop(
                n,
                step,
                "required source action was rejected; no alternative action substituted",
            );
        }
    }
    settle(n);
}

fn snapshot(n: &Native, name: &str) -> Value {
    object(&n.editor.state, name)["sketch"].clone()
}

fn ellipse(n: &mut Native, step: &str, name: &str) -> Value {
    let sketch = snapshot(n, name);
    let geometry = sketch["geometry"].as_array();
    check!(
        n,
        step,
        geometry.is_some_and(|g| g.len() == 1
            && g[0]["kind"] == "ellipse"
            && g[0]["construction"] == false),
        "expected one genuine nonconstruction native ellipse, with no seeded helper geometry"
    );
    geometry.unwrap()[0].clone()
}

fn size(n: &mut Native, step: &str, name: &str, major: f64, minor: f64) {
    let g = ellipse(n, step, name);
    note(
        n,
        step,
        "native-size",
        json!({"geometry":g,"sketch":snapshot(n,name),
        "expected_full_axes_mm":[major,minor]}),
    );
    check!(
        n,
        step,
        g["major_radius"].as_f64() == Some(major / 2.)
            && g["minor_radius"].as_f64() == Some(minor / 2.),
        "native ellipse semiaxes do not equal the source full-axis dimensions exactly"
    );
    check!(
        n,
        step,
        g["center"] == json!([0., 0., 0.]),
        "source ellipse center left the native origin"
    );
    check!(
        n,
        step,
        g["angle"].as_f64() == Some(0.),
        "source ellipse major axis is not horizontal in its native Top frame"
    );
}

// The source shows auto-focused inline sizing after the third ellipse click.
// It does not show D or a separate Dimension toolbar action at this point.
fn inline_size(n: &mut Native, step: &str, literal: &str) {
    settle(n);
    let visible: Vec<_> = texts(n).into_iter().map(|(t, _)| t).collect();
    note(
        n,
        step,
        "required-input-owner",
        json!({"keyboard":n.ctx.wants_keyboard_input(),
        "dimension_editor":n.editor.sketch_ui.dimension.is_some(),
        "sketch_name_focused":n.editor.sketch_name_focused(&n.ctx),
        "visible_text":visible}),
    );
    check!(
        n,
        step,
        n.ctx.wants_keyboard_input()
            && n.editor.sketch_ui.dimension.is_some()
            && !n.editor.sketch_name_focused(&n.ctx),
        "source-required ellipse size prompt is not focused after the real creation click; D, constraint menus, exposed geometry and backend edits were not substituted"
    );
    key(n, step, Key::A, egui::Modifiers::COMMAND);
    input(
        n,
        step,
        json!({"kind":"text","literal":literal}),
        vec![egui::Event::Text(literal.into())],
    );
    key(n, step, Key::Enter, egui::Modifiers::NONE);
    drain(n, step);
}

fn edit_dimension(n: &mut Native, step: &str, name: &str, old: f64, literal: &str) {
    let s = snapshot(n, name);
    let candidates: Vec<_> = s["constraints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["value"].as_f64() == Some(old) && c["driving"] != false)
        .filter_map(|c| c["index"].as_u64().map(|i| i as usize))
        .collect();
    check!(
        n,
        step,
        candidates.len() == 1,
        "source annotation cannot be bound to one actual driving native dimension"
    );
    let index = candidates[0];
    let labels: Vec<_> = texts(n)
        .into_iter()
        .filter(|(_, r)| n.editor.viewport.dimension_hit(r.center()) == Some(index))
        .collect();
    note(
        n,
        step,
        "annotation-hit-guard",
        json!({"native_constraint":index,
        "labels":labels.iter().map(|(t,r)|json!({"text":t,"screen":[r.center().x,r.center().y]})).collect::<Vec<_>>()}),
    );
    check!(
        n,
        step,
        labels.len() == 1,
        "source double-click annotation has no unique visible native dimension hit"
    );
    let pos = labels[0].1.center();
    for _ in 0..2 {
        pointer(
            n,
            step,
            pos,
            "source existing dimensional annotation double-click",
        );
    }
    settle(n);
    check!(
        n,
        step,
        n.editor
            .sketch_ui
            .dimension
            .as_ref()
            .is_some_and(|d| d.index == Some(index)),
        "actual double-click did not open the intended native dimensional annotation"
    );
    inline_size(n, step, literal);
}

#[test]
#[ignore = "requires Root frozen composition/runtime/actor grant; source prefix only"]
fn sourced_loft_base_profile_inline_sizes_and_corrections_actual_editor_attempt() {
    let mut n = Native::new("sourced-loft-base-profile-prefix");
    note(
        &mut n,
        "fixture-empty-local-document",
        "setup-only",
        json!({"adaptation":"source starts in an existing empty Tutorial tab; use one new empty local document, no seeded Body/Sketch/ellipse/constraints"}),
    );
    n.editor
        .send("new", json!({"name":"SilentLoftTutorialPrefix"}));
    check!(
        &mut n,
        "fixture-empty-local-document",
        n.editor.busy,
        "new empty document setup was not submitted"
    );
    drain(&mut n, "fixture-empty-local-document");
    check!(
        &mut n,
        "fixture-empty-local-document",
        n.editor.state.is_object()
            && !n.editor.state["document"].is_null()
            && objects(&n.editor.state).is_empty(),
        "new document is not genuinely empty"
    );
    check!(
        &mut n,
        "fixture-empty-local-document",
        n.editor.state["units"]["defaults"]["mm"]["default_unit"] == "mm",
        "source uses millimeters; this private fixture has a different default and no extra units-menu action was inserted"
    );

    click(&mut n, "L01-Sketch-first", "Sketch");
    check!(
        &mut n,
        "L01-Sketch-first",
        n.editor.awaiting_standard_plane(),
        "Sketch-first did not wait for an actual support-plane choice"
    );
    settle(&mut n);
    let Some(top) = n
        .editor
        .viewport
        .standard_guide_probe(crate::viewport::StandardPlane::Top)
    else {
        stop(
            &mut n,
            "L01-Top-plane",
            "Top guide has no actual native viewport target",
        );
    };
    pointer(&mut n, "L01-Top-plane", top, "source Top plane");
    drain(&mut n, "L01-Top-plane");
    let Some(name) = n.editor.sketch.clone() else {
        stop(
            &mut n,
            "L01-Top-plane",
            "Top plane click did not begin a sketch",
        );
    };
    check!(
        &mut n,
        "L01-Top-plane",
        n.editor.sketch_ui.session.is_some(),
        "real native Sketch Begin did not yield an active session"
    );
    let support = object(&n.editor.state, &name)["properties"]["AttachmentSupport"][0]["link"]
        .as_str()
        .map(str::to_owned);
    check!(
        &mut n,
        "L01-Top-plane",
        support
            .as_ref()
            .is_some_and(|s| object(&n.editor.state, s)["properties"]["Role"] == "XY_Plane"),
        "actual native support is not the Top origin plane"
    );
    note(
        &mut n,
        "L01-normal",
        "source-event-limit",
        json!({"choice":"N normal-view shortcut","evidence":"normal view visible at90s; physical N is supplementary ASR, no key overlay"}),
    );
    key(&mut n, "L01-normal", Key::N, egui::Modifiers::NONE);
    settle(&mut n);
    let o = n.editor.viewport.sketch_screen_point([0.; 3]);
    let x = n.editor.viewport.sketch_screen_point([1., 0., 0.]);
    let y = n.editor.viewport.sketch_screen_point([0., 1., 0.]);
    note(
        &mut n,
        "L01-normal",
        "actual-projection",
        json!({"origin":[o.x,o.y],"x":[x.x,x.y],"y":[y.x,y.y]}),
    );
    check!(
        &mut n,
        "L01-normal",
        x.x > o.x && x.y == o.y && y.x == o.x && y.y < o.y,
        "N did not produce the observed normal Top projection"
    );

    // Appearance alias only: source circle-tool dropdown -> Ellipse.
    click(&mut n, "L02-Ellipse-menu", "Curves");
    click(&mut n, "L02-Ellipse-menu", "Ellipse");
    check!(
        &mut n,
        "L02-Ellipse-menu",
        n.editor.sketch_ui.tool == Tool::Ellipse,
        "actual menu did not activate Ellipse"
    );
    settle(&mut n);
    let origin = n.editor.viewport.sketch_screen_point([0.; 3]);
    let origin_hit = n.editor.viewport.sketch_hit(origin);
    note(
        &mut n,
        "L02-center",
        "native-hit-guard",
        json!({"screen":[origin.x,origin.y],"native_hit":origin_hit.map(|h|json!([h.index,h.point]))}),
    );
    check!(
        &mut n,
        "L02-center",
        origin_hit
            == Some(SketchElement {
                index: -1,
                point: 1
            }),
        "source center click does not hit the real native origin"
    );
    pointer(
        &mut n,
        "L02-center",
        origin,
        "ellipse center at native origin",
    );
    // Rough size is not a source geometry oracle. Source frames show free pointer
    // placement before the 200/100 inputs. Keep horizontal major / positive minor.
    for (step, world) in [
        ("L02-major-click", [7., 0., 0.]),
        ("L02-minor-click", [4., 2.5, 0.]),
    ] {
        settle(&mut n);
        let pos = n.editor.viewport.sketch_screen_point(world);
        let hit = n.editor.viewport.sketch_hit(pos);
        note(
            &mut n,
            step,
            "declared-rough-pointer",
            json!({"world":world,"screen":[pos.x,pos.y],
            "native_hit":hit.map(|h|json!([h.index,h.point])),"source_exact_rough_coordinates":"not observed; declared trial placement only"}),
        );
        check!(
            &mut n,
            step,
            n.editor
                .viewport
                .response
                .as_ref()
                .is_some_and(|r| r.rect.contains(pos)),
            "source rough ellipse click is offscreen; no camera mutation substituted"
        );
        check!(
            &mut n,
            step,
            hit.is_none_or(|h| h.index < 0 && h.point == 0),
            "rough ellipse click would snap to an unintended native handle"
        );
        pointer(
            &mut n,
            step,
            pos,
            "source major endpoint / minor height click",
        );
    }
    drain(&mut n, "L02-minor-click");
    let first = ellipse(&mut n, "L02-minor-click", &name);
    let initial = snapshot(&n, &name);
    let ids = initial["native_xml"]["persistent_ids"].clone();
    check!(
        &mut n,
        "L02-minor-click",
        ids.as_array().is_some_and(|a| a.len() == 1),
        "single real native ellipse persistent ID was not observed"
    );
    check!(
        &mut n,
        "L02-minor-click",
        first["center"] == json!([0., 0., 0.]),
        "native ellipse is not centered at the origin"
    );
    check!(
        &mut n,
        "L02-minor-click",
        initial["constraints"]
            .as_array()
            .is_some_and(|cs| cs.iter().any(|c| c["type"] == "Coincident"
                && ((c["first"] == 0
                    && c["first_pos"] == 3
                    && c["second"] == -1
                    && c["second_pos"] == 1)
                    || (c["second"] == 0
                        && c["second_pos"] == 3
                        && c["first"] == -1
                        && c["first_pos"] == 1)))),
        "origin click did not create a genuine native ellipse-center coincidence"
    );

    inline_size(&mut n, "L02-inline-major-200", "200");
    inline_size(&mut n, "L02-inline-minor-100", "100");
    size(&mut n, "L02-inline-minor-100", &name, 200., 100.);
    // The separate reference tab at111-115s is inspection, not a state switch in
    // this local document. Continue Tutorial's intentional wrong-size correction.
    edit_dimension(&mut n, "L03-minor-100-to-80", &name, 100., "80");
    size(&mut n, "L03-minor-100-to-80", &name, 200., 80.);
    edit_dimension(&mut n, "L03-major-200-to-150", &name, 200., "150");
    size(&mut n, "L03-major-200-to-150", &name, 150., 80.);
    check!(
        &mut n,
        "L03-major-200-to-150",
        snapshot(&n, &name)["native_xml"]["persistent_ids"] == ids,
        "native ellipse identity changed during dimension corrections"
    );

    settle(&mut n);
    let Some(field) = n.ctx.read_response(egui::Id::new("native_sketch_name")) else {
        stop(
            &mut n,
            "L04-name",
            "actual editable sketch name is not visible",
        );
    };
    pointer(
        &mut n,
        "L04-name",
        field.rect.center(),
        "source sketch title",
    );
    check!(
        &mut n,
        "L04-name",
        n.editor.sketch_name_focused(&n.ctx),
        "actual sketch name does not own the keyboard"
    );
    key(&mut n, "L04-name", Key::A, egui::Modifiers::COMMAND);
    input(
        &mut n,
        "L04-name",
        json!({"kind":"text","literal":"BASE PROFILE"}),
        vec![egui::Event::Text("BASE PROFILE".into())],
    );
    // Match source: type name then green confirmation, not an added name Enter.
    click(&mut n, "L04-Finish", "Finish");
    drain(&mut n, "L04-Finish");
    check!(
        &mut n,
        "L04-Finish",
        n.editor.sketch_ui.session.is_none(),
        "Finish did not close the native sketch session"
    );
    check!(
        &mut n,
        "L04-Finish",
        object(&n.editor.state, &name)["label"] == "BASE PROFILE",
        "confirmation lost the required sketch name"
    );
    size(&mut n, "L04-Finish", &name, 150., 80.);
    let result = json!({"tutorial":TUTORIAL,"status":"bounded-prefix-completed",
        "source_actions_completed":["L01","L02","L03","L04"],"final_full_axes_mm":[150,80],
        "native_sketch":object(&n.editor.state,&name),"L05_through_L32":"untested",
        "physical_computer_use":false,"independent_stock_geometry":"not performed",
        "full_tutorial_pass":false,"tutorial_credit":0});
    note(&mut n, "L04-Finish", "bounded-prefix", result.clone());
    fs::write(
        n.folder.join("tutorial-attempt.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
}
