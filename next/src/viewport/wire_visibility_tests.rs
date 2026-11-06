//! Painter checks for finished native wires alongside solid scene edges.
use super::*;
use serde_json::json;

fn wire() -> (Value, Value) {
    (
        json!({"object":"Sketch1","revision":7,"positions":[],"triangles":[],"faces":[]}),
        json!({"edges":[
            {"subelement":"Edge1","points":[[0,0,0],[-36.2223472595,0,0]]},
            {"subelement":"Edge2","points":[[-36.2223472595,0,0],[-36.2223472595,0,62]]}
        ],"vertices":[
            {"subelement":"Vertex1","point":[0,0,0]},
            {"subelement":"Vertex2","point":[-36.2223472595,0,0]},
            {"subelement":"Vertex3","point":[-36.2223472595,0,62]}
        ]}),
    )
}

fn solid() -> (Value, Value) {
    (
        json!({"object":"Pad","revision":7,"positions":[[10,0,0],[20,0,0],[10,0,10]],
            "triangles":[[0,1,2]],"faces":[{"subelement":"Face1","first_triangle":0,"triangle_count":1}]}),
        json!({"edges":[{"subelement":"Edge1","points":[[11,0,2],[18,0,2]]}]}),
    )
}

fn frame(viewport: &mut Viewport) -> egui::FullOutput {
    let context = egui::Context::default();
    context.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewport.ui(ui);
            });
        },
    )
}

fn strokes_at(output: &egui::FullOutput, target: Pos2) -> Vec<Stroke> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::LineSegment { points, stroke } => {
                let delta = points[1] - points[0];
                let t = ((target - points[0]).dot(delta) / delta.length_sq()).clamp(0., 1.);
                (target.distance(points[0] + delta * t) < 0.5).then_some(*stroke)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn finished_wire_and_mixed_scene_paint_readable_edges_without_changing_identity() {
    let mut viewport = Viewport::default();
    viewport.set_view(StandardView::Front);
    viewport.scale = 4.;
    viewport.set_scene(wire().0, wire().1);
    let output = frame(&mut viewport);
    let light = Color32::from_rgb(174, 187, 193);
    let dark = Color32::from_rgb(32, 42, 48);
    let selected = Color32::from_rgb(95, 221, 207);
    for point in [Vec3::new(-18., 0., 0.), Vec3::new(-36.222347, 0., 31.)] {
        assert!(
            strokes_at(&output, viewport.project(point).0)
                .iter()
                .any(|s| s.color == light)
        );
    }
    assert_eq!(viewport.scene_revisions.get("Sketch1"), Some(&Some(7)));
    assert_eq!(viewport.edges.len(), 2);
    assert_eq!(viewport.edges[0].name, "Edge1");
    assert_eq!(viewport.edges[1].name, "Edge2");
    assert!(viewport.edges[1].points[1].abs_diff_eq(Vec3::new(-36.222347, 0., 62.), 1e-4));

    viewport.set_scenes(vec![wire(), solid()]);
    let output = frame(&mut viewport);
    let wire_point = viewport.project(Vec3::new(-18., 0., 0.)).0;
    let solid_point = viewport.project(Vec3::new(14., 0., 2.)).0;
    assert!(
        strokes_at(&output, wire_point)
            .iter()
            .any(|s| s.color == light)
    );
    assert!(
        strokes_at(&output, solid_point)
            .iter()
            .any(|s| s.color == dark)
    );

    viewport.display_mode = DisplayMode::Shaded;
    let output = frame(&mut viewport);
    assert!(
        strokes_at(&output, wire_point)
            .iter()
            .any(|s| s.color == light)
    );
    assert!(
        !strokes_at(&output, solid_point)
            .iter()
            .any(|s| s.color == dark)
    );

    viewport.display_mode = DisplayMode::Wireframe;
    let output = frame(&mut viewport);
    assert!(
        strokes_at(&output, solid_point)
            .iter()
            .any(|s| s.color == light)
    );
    assert!(
        strokes_at(&output, wire_point)
            .iter()
            .any(|s| s.color == light)
    );

    viewport.display_mode = DisplayMode::Edges;
    viewport.selected = Some(Selection {
        object: "Sketch1".into(),
        subelement: "Edge1".into(),
    });
    let output = frame(&mut viewport);
    assert!(
        strokes_at(&output, wire_point)
            .iter()
            .any(|s| s.color == selected && s.width == 2.5)
    );
    assert!(
        strokes_at(&output, solid_point)
            .iter()
            .any(|s| s.color == dark)
    );
    assert_eq!(viewport.selected.as_ref().unwrap().subelement, "Edge1");
    assert_eq!(viewport.scene_revisions.get("Sketch1"), Some(&Some(7)));
    assert_eq!(viewport.edges.len(), 3);
    assert_eq!(viewport.edges[0].object.as_ref(), "Sketch1");
    assert_eq!(viewport.edges[1].name, "Edge2");
    assert_eq!(viewport.edges[2].object.as_ref(), "Pad");
}
