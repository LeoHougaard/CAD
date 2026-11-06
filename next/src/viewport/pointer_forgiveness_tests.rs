use super::*;
use serde_json::json;

fn sketch() -> Viewport {
    let mut viewport = Viewport::default();
    viewport.set_sketch(Some(json!({"name":"Sketch","sketch":{"geometry":[
        {"index":0,"kind":"line","start":[20,20,0],"end":[60,20,0]},
        {"index":1,"kind":"line","start":[20,30,0],"end":[60,30,0]}
    ],"constraints":[]}})));
    viewport.align_sketch();
    viewport
}

#[test]
fn offset_clicks_select_the_same_point_and_edge_at_different_zoom_levels() {
    for scale in [2., 8., 20.] {
        let mut viewport = sketch();
        viewport.scale = scale;
        let before = viewport.sketch.clone();
        let endpoint = viewport.sketch_screen_point([20., 20., 0.]);
        for delta in [Vec2::new(-9., 0.), Vec2::new(0., 9.), Vec2::new(-6., 6.)] {
            assert_eq!(viewport.sketch_hit(endpoint + delta), Some(SketchElement { index: 0, point: 1 }));
        }
        assert_eq!(viewport.sketch_hit(endpoint + Vec2::new(-11., 0.)), None);
        let middle = viewport.sketch_screen_point([40., 20., 0.]);
        for dy in [-7., 0., 7.] {
            assert_eq!(viewport.sketch_hit(middle + Vec2::new(0., dy)), Some(SketchElement { index: 0, point: 0 }));
        }
        assert_eq!(viewport.sketch_hit(middle + Vec2::new(0., 9.)), None);
        assert_eq!(viewport.sketch, before, "picking must not move or round geometry");
    }
}

#[test]
fn nearby_parallel_edges_choose_the_nearest_native_edge() {
    let mut viewport = sketch();
    viewport.scale = 1.;
    for (world, index) in [([40., 24., 0.], 0), ([40., 26., 0.], 1)] {
        assert_eq!(viewport.sketch_hit(viewport.sketch_screen_point(world)), Some(SketchElement { index, point: 0 }));
    }
}

#[test]
fn scene_reference_offsets_keep_all_owners_for_ambiguity_checks() {
    let mut viewport = sketch();
    viewport.set_scenes(vec![
        (json!({"object":"Master","revision":7}), json!({"vertices":[
            {"subelement":"Vertex2","mapped_name":"master-v2","point":[-40,0,0]}],
            "edges":[{"subelement":"Edge1","points":[[-50,20,0],[-10,20,0]]}]})),
        (json!({"object":"Other","revision":7}), json!({"vertices":[
            {"subelement":"Vertex1","mapped_name":"other-v1","point":[-40,0,0]}],
            "edges":[{"subelement":"Edge2","points":[[-50,20,0],[-10,20,0]]}]})),
    ]);
    let point = viewport.project(Vec3::new(-40., 0., 0.)).0;
    let vertices = viewport.scene_vertices_at(point + Vec2::new(9., 0.));
    assert_eq!(vertices.len(), 2);
    assert_ne!(vertices[0].mapped_name, vertices[1].mapped_name);
    assert!(vertices.iter().all(|vertex| vertex.revision == Some(7)));
    assert!(viewport.scene_vertices_at(point + Vec2::new(11., 0.)).is_empty());
    let edge = viewport.project(Vec3::new(-30., 20., 0.)).0;
    assert_eq!(viewport.scene_edges_at(edge + Vec2::new(0., 7.)).len(), 2);
    assert!(viewport.scene_edges_at(edge + Vec2::new(0., 9.)).is_empty());
}

#[test]
fn released_offset_click_uses_the_same_viewport_selection_path() {
    let mut viewport = sketch();
    viewport.view_cube_input_enabled = false;
    let ctx = egui::Context::default();
    let frame = |viewport: &mut Viewport, events| {
        let _ = ctx.run(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(900.))),
            events,
            ..Default::default()
        }, |ctx| { egui::CentralPanel::default().show(ctx, |ui| { viewport.ui(ui); }); });
    };
    frame(&mut viewport, vec![]);
    let target = viewport.sketch_screen_point([40., 20., 0.]) + Vec2::new(0., 7.);
    frame(&mut viewport, vec![egui::Event::PointerMoved(target)]);
    for pressed in [true, false] {
        frame(&mut viewport, vec![egui::Event::PointerButton {
            pos: target, button: egui::PointerButton::Primary, pressed,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    assert_eq!(viewport.sketch_selection, vec![SketchElement { index: 0, point: 0 }]);
}
