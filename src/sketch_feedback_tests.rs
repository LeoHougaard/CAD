use super::*;
use egui::epaint::{ColorMode, Shape};

fn draw(
    sketch: &Sketch,
    selected: &[Selection],
    hover: Option<Selection>,
    readonly: bool,
) -> Vec<Shape> {
    let resolved = crate::sketch::resolve(
        sketch,
        Plane::named("Top", 0.),
        &Numbers::new(&BTreeMap::new()).unwrap(),
    )
    .unwrap();
    let ctx = egui::Context::default();
    ctx.style_mut(|s| {
        s.visuals.selection.stroke.color = Color32::from_rgb(112, 184, 190);
        s.visuals.widgets.hovered.bg_stroke.color = HOVER;
    });
    let mut camera = Camera {
        span: 100.,
        ..Default::default()
    };
    camera.align(&resolved.plane);
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(700., 500.))),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let rect = ui.available_rect_before_wrap();
                    let projector = camera.projector(rect);
                    if readonly {
                        paint_readonly(ui, &resolved, &projector, rect);
                    } else {
                        paint_geometry(ui, &resolved, &projector, rect, selected, hover, false);
                    }
                });
        },
    );
    output.shapes.into_iter().map(|s| s.shape).collect()
}
fn overlapping_lines() -> Sketch {
    let mut sketch = Sketch::default();
    sketch.add_line([-20., 0.], [20., 0.]);
    sketch.add_line([-20., 0.], [20., 0.]);
    sketch
}
#[test]
fn selected_curve_stays_above_later_coincident_and_hovered_curves() {
    let shapes = draw(
        &overlapping_lines(),
        &[Selection::edge(1, 0)],
        Some(Selection::edge(2, 0)),
        false,
    );
    let last = shapes
        .iter()
        .filter_map(|s| {
            if let Shape::Path(p) = s {
                Some(p)
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    assert_eq!(
        last.stroke.color,
        ColorMode::Solid(Color32::from_rgb(112, 184, 190))
    );
    assert!(last.stroke.width >= 3.);
}
#[test]
fn selected_point_ring_stays_above_later_coincident_points() {
    let shapes = draw(&overlapping_lines(), &[Selection::point(1, 0)], None, false);
    let last = shapes
        .iter()
        .filter_map(|s| {
            if let Shape::Circle(c) = s {
                Some(c)
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    assert_eq!(last.stroke.color, Color32::from_rgb(112, 184, 190));
    assert_eq!(last.fill, Color32::TRANSPARENT);
    assert!(last.radius >= 4.);
}
#[test]
fn construction_circle_keeps_visible_gaps_across_short_tessellation_segments() {
    let mut sketch = Sketch::default();
    sketch.add_circle([0., 0.], 10.);
    sketch.entities[0].construction = true;
    let shapes = draw(&sketch, &[], None, true);
    let length: f32 = shapes
        .iter()
        .map(|s| match s {
            Shape::Path(p) if p.stroke.color == ColorMode::Solid(BLUE) => {
                p.points.windows(2).map(|p| p[0].distance(p[1])).sum()
            }
            Shape::LineSegment { points, stroke } if stroke.color == BLUE => {
                points[0].distance(points[1])
            }
            _ => 0.,
        })
        .sum();
    // A span of 100 mm projects this circle to a 100 px diameter in a 500 px view.
    let circumference = std::f32::consts::PI * 100.;
    assert!(
        length > circumference * 0.5 && length < circumference * 0.7,
        "Drawn length{length}, full circumference{circumference}"
    );
    assert!(
        !shapes.iter().any(|s| matches!(s, Shape::Circle(_))),
        "Read-only sketches do not show editable control-point dots"
    );
}

#[test]
fn construction_dash_work_is_bounded_by_viewport_at_extreme_zoom() {
    let mut sketch = Sketch::default();
    sketch.add_line([-100000000., 0.], [100000000., 0.]);
    sketch.entities[0].construction = true;
    let shapes = draw(&sketch, &[], None, true);
    let segments: Vec<_> = shapes
        .iter()
        .filter_map(|s| {
            if let Shape::LineSegment { points, stroke } = s {
                (stroke.color == BLUE).then_some(points)
            } else {
                None
            }
        })
        .collect();
    assert!(
        segments.len() > 50 && segments.len() < 100,
        "Only visible dashes are emitted: {}",
        segments.len()
    );
    for p in segments {
        assert!(
            p.iter()
                .all(|p| p.x >= -2. && p.x <= 702. && p.y >= -2. && p.y <= 502.)
        );
    }
}
