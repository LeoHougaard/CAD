//! Proposed sourced tutorial-prefix attempt. Not compiled or executed by its author.
//! Register as a child of editor::sketch::lifecycle_tests after Root freezes source.
use super::*;
use std::time::Duration;

const SOURCE: &str = "OS-WR3-BLOCK";

fn settle(n: &mut Native) {
    for _ in 0..3 {
        n.frame(vec![]);
    }
}

fn record_note(n: &mut Native, step: &str, phase: &str, detail: Value) {
    let event = json!({"tutorial":SOURCE,"step":step,"phase":phase,"detail":detail,
        "state":n.editor.state,"ui":n.ui_state(),"full_tutorial_credit":0});
    n.events.push(event);
    n.record();
}

macro_rules! note {
    ($n:expr, $step:expr, $phase:expr, $detail:expr) => {{
        let detail = $detail;
        record_note($n, $step, $phase, detail);
    }};
}

fn stop(n: &mut Native, step: &str, reason: &str) -> ! {
    note!(
        n,
        step,
        "blocked",
        json!({"reason":reason,"untested_tail":"all later source steps"})
    );
    fs::write(
        n.folder.join("tutorial-attempt.json"),
        serde_json::to_vec_pretty(&json!({
        "tutorial":SOURCE,"status":"blocked","first_blocking_step":step,"reason":reason,
        "full_tutorial_credit":0,"full_40_page_tutorial_attempted":false}))
        .unwrap(),
    )
    .unwrap();
    panic!("{SOURCE} stopped at {step}: {reason}");
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

fn texts(n: &mut Native) -> Vec<(String, egui::Rect)> {
    settle(n);
    n.frame(vec![])
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) => Some((
                t.galley.text().to_owned(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            _ => None,
        })
        .collect()
}

fn pointer(n: &mut Native, step: &str, pos: egui::Pos2, role: &str) {
    note!(
        n,
        step,
        "pointer",
        json!({"role":role,"screen":[pos.x,pos.y]})
    );
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
}

fn click(n: &mut Native, step: &str, label: &str, scope: Option<egui::Rect>) {
    if n.editor.busy || n.editor.draft.as_ref().is_some_and(|d| d.changed.is_some()) {
        drain(n, step);
    }
    let all = texts(n);
    if n.editor.busy || n.editor.draft.as_ref().is_some_and(|d| d.changed.is_some()) {
        drain(n, step);
        return click(n, step, label, scope);
    }
    let matches: Vec<_> = all
        .iter()
        .filter(|(t, r)| t == label && scope.is_none_or(|a| a.contains(r.center())))
        .collect();
    note!(
        n,
        step,
        "visible-control",
        json!({"label":label,"matches":matches.len(),
        "visible_text":all.iter().map(|(t,_)|t).collect::<Vec<_>>()})
    );
    if matches.len() != 1 {
        stop(
            n,
            step,
            &format!(
                "required scoped UI action {label:?} has {} visible matches",
                matches.len()
            ),
        );
    }
    pointer(n, step, matches[0].1.center(), label);
}

// Observe all real replies produced by one user action. A name flush followed
// by native Finish is allowed; no synthetic reply or assumed one-reply shortcut.
fn drain(n: &mut Native, step: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        while n.editor.busy {
            let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) else {
                stop(n, step, "native action deadline expired");
            };
            let reply = n
                .raw
                .recv_timeout(left)
                .unwrap_or_else(|_| stop(n, step, "native reply timed out"));
            let accepted = reply.accepted;
            note!(
                n,
                step,
                "native-reply",
                json!({"op":reply.op,"accepted":reply.accepted,
            "error":reply.error,"state":reply.state,"scenes":reply.scenes})
            );
            n.forward.send(reply).unwrap();
            n.editor.poll();
            if !accepted {
                stop(
                    n,
                    step,
                    "a required action was rejected; raw reply and unchanged untested tail retained",
                );
            }
        }

        settle(n);
        if !n.editor.busy && n.editor.draft.as_ref().is_none_or(|d| d.changed.is_none()) {
            break;
        }
        if std::time::Instant::now() >= deadline {
            stop(
                n,
                step,
                "automatic native preview did not settle before input",
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn replace_text(n: &mut Native, step: &str, value: &str) {
    check!(
        n,
        step,
        n.ctx.wants_keyboard_input(),
        "required text field does not own keyboard input"
    );
    tutorial_key(n, Key::A, egui::Modifiers::COMMAND);
    n.frame(vec![egui::Event::Text(value.into())]);
    note!(n, step, "typed", json!({"literal":value}));
}

fn sketch(n: &Native, name: &str) -> Value {
    object(&n.editor.state, name)["sketch"].clone()
}

fn whole_edge(n: &mut Native, step: &str, name: &str, index: i32) {
    let s = sketch(n, name);
    let g = s["geometry"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["index"] == index)
        .unwrap();
    settle(n);
    let expected = SketchElement { index, point: 0 };
    let area = n.editor.viewport.response.as_ref().unwrap().rect;
    let target = [0.4, 0.6, 0.25, 0.75].into_iter().find_map(|t| {
        let p = std::array::from_fn(|i| {
            g["start"][i].as_f64().unwrap() * (1. - t) + g["end"][i].as_f64().unwrap() * t
        });
        let pos = n.editor.viewport.sketch_screen_point(p);
        (area.contains(pos)
            && n.editor.viewport.sketch_hit(pos) == Some(expected)
            && n.editor.viewport.dimension_hit(pos).is_none())
        .then_some(pos)
    });
    let Some(pos) = target else {
        stop(
            n,
            step,
            "required whole native edge is not visibly pickable; no hidden Fit or endpoint replacement was inserted",
        );
    };
    pointer(n, step, pos, "whole native edge");
    check!(
        n,
        step,
        n.editor.viewport.sketch_selection.contains(&expected),
        "actual selection did not retain the intended whole edge"
    );
    check!(
        n,
        step,
        n.editor.sketch_ui.drag.is_none(),
        "dimension selection accidentally started a drag"
    );
}

fn size_pair(n: &mut Native, step: &str, name: &str, pair: [i32; 2]) {
    if n.editor.sketch_ui.dimension_tool.is_none() {
        tutorial_key(n, Key::D, egui::Modifiers::NONE);
    }
    check!(
        n,
        step,
        n.editor.sketch_ui.dimension_tool.is_some(),
        "D did not enter Dimension before selection"
    );
    check!(
        n,
        step,
        n.editor.viewport.sketch_selection.is_empty(),
        "previous selection remains; no direct clearing was substituted"
    );
    whole_edge(n, step, name, pair[0]);
    whole_edge(n, step, name, pair[1]);
    check!(
        n,
        step,
        n.editor.viewport.sketch_selection.len() == 2,
        "two whole edges were not selected"
    );
    let area = n.editor.viewport.response.as_ref().unwrap().rect;
    let p = [
        egui::pos2(area.right() - 35., area.bottom() - 45.),
        egui::pos2(area.left() + 45., area.bottom() - 45.),
    ]
    .into_iter()
    .find(|p| {
        n.editor.viewport.sketch_hit(*p).is_none() && n.editor.viewport.dimension_hit(*p).is_none()
    });
    let Some(pos) = p else {
        stop(n, step, "no unoccluded empty dimension placement position");
    };
    pointer(
        n,
        step,
        pos,
        "dimension annotation placement, source-unspecified location",
    );
    drain(n, step);
    check!(
        n,
        step,
        n.editor
            .sketch_ui
            .dimension
            .as_ref()
            .is_some_and(|d| d.kind == "Distance"),
        "whole opposite edges did not create a distance dimension"
    );
    replace_text(n, step, "12 in");
    tutorial_key(n, Key::Enter, egui::Modifiers::NONE);
    drain(n, step);
}

// Read-only native parser authority, explicitly not a tutorial user action.
// Preserve exact native F64 representation instead of comparing an invented
// decimal-millimeter literal against the parser's actual 12-inch value.
fn native_length_oracle(n: &mut Native, literal: &str) -> Value {
    let before = n.editor.state.clone();
    n.editor.send(
        "quantities",
        json!({"items":[{"text":literal,"unit":"mm"}]}),
    );
    assert!(n.editor.busy);
    let reply = n.raw.recv_timeout(Duration::from_secs(25)).unwrap();
    assert!(reply.accepted && reply.op == "quantities");
    let value = reply.data["items"][0]["base_value"].clone();
    assert!(value.as_f64().is_some_and(|v| v.is_finite() && v > 0.));
    note!(
        n,
        "native-oracle",
        "readonly-not-user-action",
        json!({"literal":literal,"data":reply.data})
    );
    n.forward.send(reply).unwrap();
    n.editor.poll();
    assert_eq!(
        n.editor.state, before,
        "parser query must not edit native state"
    );
    value
}

#[test]
#[ignore = "requires exact Root source/runtime grant and CAD_LIFECYCLE_OUTPUT"]
fn sourced_block_tutorial_through_first_cube_actual_editor_attempt() {
    let mut n = Native::new("sourced-block-first-cube");
    n.send("new", json!({"name":"Assignment1ExtrusionsAttempt"}), true);
    check!(
        &mut n,
        "fixture",
        objects(&n.editor.state).is_empty(),
        "adapted assignment document is not genuinely empty"
    );
    note!(
        &mut n,
        "fixture",
        "declared-adaptation",
        json!({"source":"classroom empty assignment document unavailable",
        "replacement":"new empty local document only; no seeded Body, Sketch, profile or dimensions"})
    );

    // Same sourced document-menu, length-dropdown and checkmark sequence.
    // UI labels are bound to the actual composition; no native-schema shortcut.
    click(&mut n, "A03", "Document", None);
    click(&mut n, "A03", "Workspace units...", None);
    click(&mut n, "A03", "Millimeter", None);
    click(&mut n, "A03", "Inch", None);
    click(&mut n, "A03", "\u{2713}", None);
    drain(&mut n, "A03");
    note!(
        &mut n,
        "A03",
        "accepted",
        json!({"native_units":n.editor.state["units"]})
    );

    let native_12_in = native_length_oracle(&mut n, "12 in");
    settle(&mut n);
    let top = n
        .editor
        .viewport
        .standard_guide_probe(crate::viewport::StandardPlane::Top);
    let Some(top) = top else {
        stop(
            &mut n,
            "A04",
            "visible Top plane guide has no actual pick target",
        );
    };
    pointer(&mut n, "A04", top, "viewport Top plane");
    check!(
        &mut n,
        "A04",
        n.editor.viewport.selected_standard_plane() == Some(crate::viewport::StandardPlane::Top),
        "Top was not selected before Sketch"
    );
    click(&mut n, "A04", "Sketch", None);
    drain(&mut n, "A04");
    let Some(name) = n.editor.sketch.clone() else {
        stop(&mut n, "A04", "Sketch did not begin");
    };
    let token = n.editor.sketch_ui.session.clone();
    check!(
        &mut n,
        "A04",
        token.is_some(),
        "native sketch session is absent"
    );
    let support = object(&n.editor.state, &name)["properties"]["AttachmentSupport"][0]["link"]
        .as_str()
        .unwrap()
        .to_owned();
    check!(
        &mut n,
        "A04",
        object(&n.editor.state, &support)["properties"]["Role"] == "XY_Plane",
        "new sketch is not actually Top-supported"
    );

    settle(&mut n);
    let field = n.ctx.read_response(egui::Id::new("native_sketch_name"));
    let Some(field) = field else {
        stop(
            &mut n,
            "A05",
            "active sketch title/name edit control is absent",
        );
    };
    pointer(
        &mut n,
        "A05",
        field.rect.center(),
        "active sketch name, source title-edit confirmation inferred",
    );
    replace_text(&mut n, "A05", "Base");
    let area = n.editor.viewport.response.as_ref().unwrap().rect;
    let cube = egui::Rect::from_min_max(
        egui::pos2(area.right() - 160., area.top()),
        egui::pos2(area.right(), area.top() + 150.),
    );
    click(&mut n, "A05", "Top", Some(cube));
    drain(&mut n, "A05");
    check!(
        &mut n,
        "A05",
        object(&n.editor.state, &name)["label"] == "Base",
        "the next prescribed Top cube click lost the active Base name"
    );
    check!(
        &mut n,
        "A05",
        n.editor.sketch_ui.session == token,
        "name confirmation prematurely finished or replaced the sketch"
    );

    click(&mut n, "A06", "Corner rectangle (G)", None);
    check!(
        &mut n,
        "A06",
        n.editor.sketch_ui.tool == Tool::Rectangle,
        "declared Corner rectangle variant is not active"
    );
    for p in [[-5., -4., 0.], [5., 4., 0.]] {
        settle(&mut n);
        let pos = n.editor.viewport.sketch_screen_point(p);
        check!(
            &mut n,
            "A06",
            n.editor
                .viewport
                .response
                .as_ref()
                .unwrap()
                .rect
                .contains(pos),
            "declared rough rectangle corner is offscreen"
        );
        pointer(
            &mut n,
            "A06",
            pos,
            "source-unspecified diagonal rectangle corner",
        );
    }
    drain(&mut n, "A06");
    let baseline = sketch(&n, &name);
    let ids = baseline["native_xml"]["persistent_ids"].clone();
    check!(
        &mut n,
        "A06",
        baseline["geometry_count"] == 4,
        "rectangle did not create exactly four native curves"
    );
    check!(
        &mut n,
        "A06",
        baseline["geometry"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["kind"] == "line" && g["construction"] == false),
        "rectangle contains unexpected geometry"
    );

    size_pair(&mut n, "A07-width", &name, [3, 1]);
    size_pair(&mut n, "A07-height", &name, [0, 2]);
    let sized = sketch(&n, &name);
    check!(
        &mut n,
        "A07",
        sized["native_xml"]["persistent_ids"] == ids,
        "native rectangle IDs changed during dimensioning"
    );
    let dims: Vec<_> = sized["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "Distance" && c["value"] == native_12_in)
        .collect();
    check!(
        &mut n,
        "A07",
        dims.len() == 2,
        "two required native12in distances were not retained"
    );
    note!(
        &mut n,
        "A07",
        "native-model",
        json!({"sketch":sized,"source_requires_dof0":false})
    );
    click(&mut n, "A07", "Finish", None);
    drain(&mut n, "A07");
    check!(
        &mut n,
        "A07",
        n.editor.sketch_ui.session.is_none(),
        "Finish did not close Base"
    );

    let before_view = n.editor.state.clone();
    click(&mut n, "A08", "View", None);
    click(&mut n, "A08", "Trimetric", None);
    check!(
        &mut n,
        "A08",
        n.editor.state == before_view && !n.editor.busy,
        "Trimetric changed native state or submitted a modeling request"
    );

    // No profile/selection assignment or Shift+E. Product defaults are recorded,
    // but the source-required region click still follows the typed depth.
    note!(
        &mut n,
        "A09",
        "before-extrude",
        json!({"viewport_selection":n.editor.viewport.selected.as_ref().map(|s|json!({"object":s.object,"subelement":s.subelement})),"tree_selection":n.editor.selected_object})
    );
    click(&mut n, "A09", "Extrude", None);
    drain(&mut n, "A09");
    check!(
        &mut n,
        "A09",
        n.editor.draft.is_some(),
        "feature-first Extrude did not open its task"
    );
    note!(
        &mut n,
        "A09",
        "initial-feature-defaults",
        json!({"parameters":n.editor.draft.as_ref().unwrap().parameters})
    );
    let value = n.editor.property_ui.schema["property_metadata"]["Length"]["quantity"]["input"]
        .as_str()
        .map(str::to_owned);
    let Some(value) = value else {
        stop(
            &mut n,
            "A09",
            "visible native Depth/Length input cannot be bound without inventing a widget target",
        );
    };
    let feature = n.ctx.memory(|m| m.area_rect(egui::Id::new("Feature")));
    let Some(feature) = feature else {
        stop(&mut n, "A09", "Extrude feature dialog is not observed");
    };
    click(&mut n, "A09", &value, Some(feature));
    replace_text(&mut n, "A09", "12 in");
    let g = sketch(&n, &name);
    let points = g["geometry"].as_array().unwrap();
    let center: [f64; 3] = std::array::from_fn(|i| {
        points
            .iter()
            .map(|p| p["start"][i].as_f64().unwrap())
            .sum::<f64>()
            / points.len() as f64
    });
    let pos = n.editor.viewport.sketch_screen_point(center);
    check!(
        &mut n,
        "A09",
        n.editor
            .viewport
            .response
            .as_ref()
            .unwrap()
            .rect
            .contains(pos),
        "source-required square region is not visible; no camera mutation inserted"
    );
    pointer(&mut n, "A09", pos, "square region after typed12in depth");
    settle(&mut n);
    check!(
        &mut n,
        "A09",
        n.editor
            .draft
            .as_ref()
            .is_some_and(|d| d.parameters["properties"]["Profile"]["link"] == name),
        "region click did not bind Base; no tree pick or Use-selection replacement was inserted"
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        drain(&mut n, "A09");
        if n.editor
            .draft
            .as_ref()
            .is_some_and(|d| d.valid && d.changed.is_none())
        {
            break;
        }
        if std::time::Instant::now() >= deadline {
            stop(
                &mut n,
                "A09",
                "typed depth/region did not reach an accepted native preview",
            );
        }
        std::thread::sleep(Duration::from_millis(10));
        n.frame(vec![]);
    }
    let result_name = n.editor.draft.as_ref().unwrap().parameters["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let feature = n.ctx.memory(|m| m.area_rect(egui::Id::new("Feature")));
    let Some(feature) = feature else {
        stop(
            &mut n,
            "A09",
            "observed Extrude dialog disappeared before confirmation",
        );
    };
    click(&mut n, "A09", "Apply", Some(feature));
    drain(&mut n, "A09");
    let result = object(&n.editor.state, &result_name).clone();
    check!(
        &mut n,
        "A09",
        result["type"] == "PartDesign::Pad" && result["properties"]["Profile"]["link"] == name,
        "result is not a real extrusion of Base"
    );
    check!(
        &mut n,
        "A09",
        result["properties"]["Length"] == native_12_in,
        "native depth is not12in"
    );
    check!(
        &mut n,
        "A09",
        result["properties"]["Type"] == "Length"
            && result["properties"]["SideType"] == "One side"
            && result["properties"]["Reversed"] == false,
        "extrusion is not forward Blind/one-side as sourced"
    );
    check!(
        &mut n,
        "A09",
        result["shape"]["valid"] == true
            && result["shape"]["null"] == false
            && result["shape"]["solids"] == 1
            && result["shape"]["faces"] == 6
            && result["shape"]["edges"] == 12
            && result["shape"]["vertices"] == 8,
        "result is not a valid one-solid cube topology"
    );
    check!(
        &mut n,
        "A09",
        result["shape"]["volume"]
            .as_f64()
            .is_some_and(|v| v.is_finite() && v > 0.),
        "native solid volume is not finite and positive"
    );
    note!(
        &mut n,
        "A09",
        "prefix-complete-pending-independent-geometry",
        json!({"result":result,"analytic_dimensions_mm":["304.8","304.8","304.8"],"analytic_volume_mm3":"28316846.592","volume_bit_equality_asserted":false,"independent_stock_still_required":true})
    );
    note!(
        &mut n,
        "native-evidence-save",
        "not-a-tutorial-user-action",
        json!({"reason":"save the completed prefix for independent stock readback","user_action_credit":false})
    );
    let evidence_path = n.save("first-cube-native-evidence.FCStd");
    note!(
        &mut n,
        "native-evidence-save",
        "native-file-saved",
        json!({"path":evidence_path,"result_object":result_name})
    );
    fs::write(n.folder.join("tutorial-attempt.json"),serde_json::to_vec_pretty(&json!({"tutorial":SOURCE,
        "status":"first_cube_prefix_completed_pending_independent_geometry","full_tutorial_credit":0,
        "unexecuted":"PDF27 extrusion rename and all later stud, fillet and shell steps"})).unwrap()).unwrap();
}
