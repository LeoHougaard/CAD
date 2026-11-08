//! Headless egui interactions; authored for Root's serial desktop-feature test run.
use super::*;
use serde_json::json;

fn frame(v: &mut Viewport, ctx: &egui::Context, events: Vec<egui::Event>, enabled: bool) {
    let modifiers = events.iter().find_map(|event| match event {
        egui::Event::PointerButton { modifiers, .. } | egui::Event::Key { modifiers, .. } => Some(*modifiers),
        _ => None,
    }).unwrap_or_default();
    let _ = ctx.run(egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
        events, modifiers, ..Default::default()
    }, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(enabled, |ui| { v.ui(ui); });
        });
    });
}
fn button(pos: Pos2, button: egui::PointerButton, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::PointerButton { pos, button, pressed, modifiers }
}
fn click(v: &mut Viewport, ctx: &egui::Context, pos: Pos2, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        frame(v, ctx, vec![egui::Event::PointerMoved(pos),
            button(pos, egui::PointerButton::Primary, pressed, modifiers)], true);
    }
}
fn space(v: &mut Viewport, ctx: &egui::Context, pos: Pos2, enabled: bool) {
    for pressed in [true, false] {
        frame(v, ctx, vec![egui::Event::PointerMoved(pos), egui::Event::Key {
            key: egui::Key::Space, physical_key: None, pressed, repeat: false,
            modifiers: egui::Modifiers::NONE,
        }], enabled);
    }
}
fn solid(mode: SelectionMode) -> Viewport {
    let mut v = Viewport::default(); v.top(); v.scale = 8.; v.selection_mode = mode;
    v.set_scenes([[-18., -18.], [8., -18.], [-18., 8.], [8., 8.]].into_iter()
        .enumerate().map(|(i, [x,y])| {
            let (mut mesh, mut topology) = super::select_other_ui_tests::scene(&format!("Part{i}"), 0.);
            let placement = json!({"position":[x,y,0.]});
            mesh["world_from_shape"] = placement.clone();
            topology["world_from_shape"] = placement;
            (mesh, topology)
        }).collect());
    v
}

#[test]
fn ordinary_four_entity_clicks_add_then_remove_exact_native_picks_in_every_mode() {
    for (mode, name, local) in [
        (SelectionMode::Edges, "Edge4", [4.,2.]),
        (SelectionMode::Vertices, "Vertex3", [2.,2.]),
        (SelectionMode::Faces, "Face7", [5.,5.]),
    ] {
        let mut v = solid(mode); let ctx = egui::Context::default();
        frame(&mut v, &ctx, vec![], true);
        let positions: Vec<_> = [[-18.,-18.],[8.,-18.],[-18.,8.],[8.,8.]].into_iter()
            .map(|[x,y]| v.project(Vec3::new(x+local[0], y+local[1], 0.)).0).collect();
        let expected: Vec<_> = (0..4).map(|i| Selection { object: format!("Part{i}"), subelement: name.into() }).collect();
        for (i, pos) in positions.iter().enumerate() {
            click(&mut v, &ctx, *pos, egui::Modifiers::NONE);
            assert_eq!(v.selections, expected[..=i]);
            assert_eq!(v.selected.as_ref(), expected.get(i));
        }
        click(&mut v, &ctx, positions[1], egui::Modifiers::NONE);
        assert_eq!(v.selections, vec![expected[0].clone(), expected[2].clone(), expected[3].clone()]);
        assert_eq!(v.selected, Some(expected[3].clone()));
        // Modifiers never become a requirement for addition or removal.
        click(&mut v, &ctx, positions[1], egui::Modifiers::SHIFT);
        assert_eq!(v.selections.len(), 4);
        let blank = v.rect.left_top() + Vec2::splat(25.);
        click(&mut v, &ctx, blank, egui::Modifiers::NONE);
        assert!(v.selections.is_empty() && v.selected.is_none());
        click(&mut v, &ctx, positions[0], egui::Modifiers::NONE);
        space(&mut v, &ctx, blank, true);
        assert!(v.selections.is_empty() && v.selected.is_none());
    }
}

fn sketch() -> Viewport {
    let mut v = Viewport::default(); v.top(); v.scale = 10.;
    v.set_sketch(Some(json!({"name":"Sketch","sketch":{"geometry":[
        {"index":0,"kind":"line","start":[-8,-8,0],"end":[8,-8,0]},
        {"index":1,"kind":"line","start":[8,-8,0],"end":[8,8,0]},
        {"index":2,"kind":"line","start":[8,8,0],"end":[-8,8,0]},
        {"index":3,"kind":"line","start":[-8,8,0],"end":[-8,-8,0]}]}})));
    v
}
#[test]
fn ordinary_sketch_four_edge_clicks_toggle_and_blank_or_space_clears_pick_positions() {
    let mut v = sketch(); let ctx = egui::Context::default();
    frame(&mut v, &ctx, vec![], true);
    let positions: Vec<_> = [[0.,-8.,0.],[8.,0.,0.],[0.,8.,0.],[-8.,0.,0.]].into_iter()
        .map(|p| v.sketch_screen_point(p)).collect();
    let elements: Vec<_> = (0..4).map(|index| SketchElement { index, point: 0 }).collect();
    for (i, pos) in positions.iter().enumerate() {
        click(&mut v, &ctx, *pos, egui::Modifiers::NONE);
        assert_eq!(v.sketch_selection, elements[..=i]);
        assert!(v.sketch_pick_position(elements[i]).is_some());
    }
    click(&mut v, &ctx, positions[3], egui::Modifiers::NONE);
    assert_eq!(v.sketch_selection, elements[..3]);
    assert_eq!(v.selected.as_ref().map(|s| s.subelement.as_str()), Some("Geometry2"));
    assert!(v.sketch_pick_position(elements[3]).is_none());
    let blank = v.rect.left_top() + Vec2::splat(25.);
    space(&mut v, &ctx, blank, true);
    assert!(v.sketch_selection.is_empty() && v.selected.is_none());
    assert!(v.sketch_pick_positions.is_empty());
    click(&mut v, &ctx, positions[0], egui::Modifiers::NONE);
    click(&mut v, &ctx, blank, egui::Modifiers::NONE);
    assert!(v.sketch_selection.is_empty() && v.sketch_pick_positions.is_empty());
}

#[test]
fn navigation_drags_disabled_ui_and_sketch_tools_do_not_toggle_or_clear() {
    let mut v = sketch(); let ctx = egui::Context::default();
    frame(&mut v, &ctx, vec![], true);
    let pos = v.sketch_screen_point([0.,-8.,0.]);
    click(&mut v, &ctx, pos, egui::Modifiers::NONE);
    let before = v.sketch_selection.clone();
    let primary = v.selected.clone();
    v.sketch_select_enabled = false;
    click(&mut v, &ctx, pos, egui::Modifiers::NONE);
    space(&mut v, &ctx, pos, true);
    assert_eq!(v.sketch_selection, before);
    assert_eq!(v.selected, primary);
    v.sketch_select_enabled = true;
    space(&mut v, &ctx, pos, false);
    assert_eq!(v.sketch_selection, before);
    for b in [egui::PointerButton::Primary, egui::PointerButton::Middle, egui::PointerButton::Secondary] {
        frame(&mut v, &ctx, vec![egui::Event::PointerMoved(pos), button(pos,b,true,egui::Modifiers::NONE)], true);
        let end = pos + Vec2::new(35.,20.);
        frame(&mut v, &ctx, vec![egui::Event::PointerMoved(end)], true);
        space(&mut v, &ctx, end, true);
        frame(&mut v, &ctx, vec![button(end,b,false,egui::Modifiers::NONE)], true);
        assert_eq!(v.sketch_selection, before);
    }
}

#[test]
fn ordinary_coincident_sketch_handle_clicks_preserve_ordered_native_cycle() {
    let mut v = Viewport::default(); v.top(); v.scale = 10.;
    v.set_sketch(Some(json!({"name":"Sketch","sketch":{"geometry":[
        {"index":0,"kind":"line","start":[-5,-5,0],"end":[2,3,0]},
        {"index":1,"kind":"line","start":[2,3,0],"end":[5,5,0]}]}})));
    let ctx = egui::Context::default(); frame(&mut v, &ctx, vec![], true);
    let pos = v.sketch_screen_point([2.,3.,0.]);
    let a = SketchElement { index: 0, point: 2 };
    let b = SketchElement { index: 1, point: 1 };
    for expected in [vec![a], vec![a,b], vec![b], vec![]] {
        click(&mut v, &ctx, pos, egui::Modifiers::NONE);
        assert_eq!(v.sketch_selection, expected);
    }
}

#[test]
fn generic_collector_toggle_preserves_order_and_reports_membership() {
    let a = Selection { object: "A".into(), subelement: "Edge1".into() };
    let b = Selection { object: "B".into(), subelement: "Edge1".into() };
    let mut picks = vec![];
    assert!(toggle_selection(&mut picks, a.clone()));
    assert!(toggle_selection(&mut picks, b.clone()));
    assert!(!toggle_selection(&mut picks, a));
    assert_eq!(picks, vec![b]);
}

#[test]
fn four_native_edges_of_one_part_accumulate_without_modifiers_and_revision_prunes_them() {
    let mut v = Viewport::default(); v.top(); v.scale = 10.; v.selection_mode = SelectionMode::Edges;
    let mesh = json!({"object":"Pad","revision":7,"positions":[],"triangles":[]});
    let topology = json!({"edges":[
        {"subelement":"Edge1","mapped_name":"stable-a","points":[[-8,-8,0],[8,-8,0]]},
        {"subelement":"Edge2","mapped_name":"stable-b","points":[[8,-8,0],[8,8,0]]},
        {"subelement":"Edge3","mapped_name":"stable-c","points":[[8,8,0],[-8,8,0]]},
        {"subelement":"Edge4","mapped_name":"stable-d","points":[[-8,8,0],[-8,-8,0]]}]});
    v.set_scene(mesh.clone(), topology.clone());
    let ctx = egui::Context::default(); frame(&mut v, &ctx, vec![], true);
    for (i, point) in [[0.,-8.,0.],[8.,0.,0.],[0.,8.,0.],[-8.,0.,0.]].into_iter().enumerate() {
        let pos = v.project(Vec3::from_array(point)).0;
        click(&mut v, &ctx, pos, egui::Modifiers::NONE);
        assert_eq!(v.selections.len(), i+1);
        assert_eq!(v.selected.as_ref().unwrap().subelement, format!("Edge{}", i+1));
    }
    let picks = v.selections.clone();
    v.set_scene(mesh.clone(), topology.clone());
    assert_eq!(v.selections, picks);
    let mut revised = mesh; revised["revision"] = json!(8);
    v.set_scene(revised, topology);
    assert!(v.selections.is_empty() && v.selected.is_none());
}
