//! Transient sketch tool feedback. This draws local points without touching the native document.
use crate::{sketch_commands::Tool, viewport::Viewport};
use eframe::egui::{self, Color32, Pos2, Shape, Stroke, Vec2};
use std::f64::consts::TAU;

pub(super) fn preview(
    painter: &egui::Painter,
    viewport: &Viewport,
    tool: Tool,
    points: &[[f64; 3]],
    cursor: Option<[f64; 3]>,
) {
    if tool == Tool::Select {
        return;
    }
    let color = Color32::from_rgb(107, 215, 208);
    let stroke = Stroke::new(1.5_f32, color);
    let project = |point| viewport.sketch_screen_point(point);
    let cursor = cursor.filter(|point| point.iter().all(|value| value.is_finite()));
    if let Some(cursor) = cursor {
        let point = project(cursor);
        painter.line_segment(
            [point - Vec2::new(5., 0.), point + Vec2::new(5., 0.)],
            stroke,
        );
        painter.line_segment(
            [point - Vec2::new(0., 5.), point + Vec2::new(0., 5.)],
            stroke,
        );
    }
    if tool == Tool::Point {
        return;
    }
    let Some(&first) = points.first() else {
        return;
    };
    if points
        .iter()
        .any(|point| point.iter().any(|value| !value.is_finite()))
    {
        return;
    }
    let mut path = vec![];
    match tool {
        Tool::Line | Tool::Polyline => {
            path.extend(points.iter().copied());
            if let Some(cursor) = cursor {
                path.push(cursor);
            }
        }
        Tool::Rectangle => {
            if let Some(end) = cursor.or_else(|| points.get(1).copied()) {
                path.extend([
                    first,
                    [end[0], first[1], 0.],
                    end,
                    [first[0], end[1], 0.],
                    first,
                ]);
            }
        }
        Tool::Circle => {
            if let Some(end) = cursor.or_else(|| points.get(1).copied()) {
                let radius = (end[0] - first[0]).hypot(end[1] - first[1]);
                if radius > 0. {
                    let screen_radius = projected_radius(viewport, first, radius);
                    let count = segment_count(radius, screen_radius, TAU);
                    for step in 0..=count {
                        let angle = TAU * step as f64 / count as f64;
                        path.push([
                            first[0] + radius * angle.cos(),
                            first[1] + radius * angle.sin(),
                            0.,
                        ]);
                    }
                    painter.line_segment(
                        [project(first), project(end)],
                        Stroke::new(0.8_f32, color.gamma_multiply(0.5)),
                    );
                }
            }
        }
        Tool::Arc => {
            if let Some(&middle) = points.get(1) {
                if let Some(end) = cursor.or_else(|| points.get(2).copied()) {
                    if let Some(arc) = Arc::through(first, middle, end) {
                        let screen_radius = projected_radius(
                            viewport,
                            [arc.center[0], arc.center[1], 0.],
                            arc.radius,
                        );
                        let count = segment_count(arc.radius, screen_radius, arc.sweep);
                        path.extend((0..=count).map(|step| arc.point(step as f64 / count as f64)));
                        path[0] = first;
                        *path.last_mut().unwrap() = end;
                    } else {
                        path.extend([first, middle, end]);
                    }
                } else {
                    path.extend([first, middle]);
                }
            } else if let Some(cursor) = cursor {
                path.extend([first, cursor]);
            }
        }
        Tool::Select | Tool::Point => {}
    }
    if path.len() > 1 {
        painter.add(Shape::line(path.into_iter().map(project).collect(), stroke));
    }
    for point in points {
        let position: Pos2 = project(*point);
        painter.circle_filled(position, 3., color);
    }
}

fn projected_radius(viewport: &Viewport, center: [f64; 3], radius: f64) -> f64 {
    let origin = viewport.sketch_screen_point(center);
    let x = viewport.sketch_screen_point([center[0] + radius, center[1], 0.]);
    let y = viewport.sketch_screen_point([center[0], center[1] + radius, 0.]);
    origin.distance(x).max(origin.distance(y)) as f64
}

fn segment_count(radius: f64, screen_radius: f64, sweep: f64) -> usize {
    if !radius.is_finite() || !screen_radius.is_finite() || screen_radius <= 0.35 {
        return 8;
    }
    // Bound the chord error to about a third of a screen point.
    let max_step = 2. * (1. - 0.35 / screen_radius).clamp(-1., 1.).acos();
    (sweep.abs() / max_step).ceil().clamp(8., 4096.) as usize
}

struct Arc {
    center: [f64; 2],
    radius: f64,
    start: f64,
    sweep: f64,
}
impl Arc {
    fn through(start: [f64; 3], middle: [f64; 3], end: [f64; 3]) -> Option<Self> {
        if [start, middle, end]
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return None;
        }
        let b = [middle[0] - start[0], middle[1] - start[1]];
        let c = [end[0] - start[0], end[1] - start[1]];
        let determinant = 2. * (b[0] * c[1] - b[1] * c[0]);
        let scale = b[0].hypot(b[1]) * c[0].hypot(c[1]);
        if scale == 0. || determinant.abs() <= 16. * f64::EPSILON * scale {
            return None;
        }
        let b_squared = b[0] * b[0] + b[1] * b[1];
        let c_squared = c[0] * c[0] + c[1] * c[1];
        let offset = [
            (b_squared * c[1] - c_squared * b[1]) / determinant,
            (b[0] * c_squared - c[0] * b_squared) / determinant,
        ];
        let center = [start[0] + offset[0], start[1] + offset[1]];
        let radius = offset[0].hypot(offset[1]);
        if !radius.is_finite() || radius == 0. {
            return None;
        }
        let angle = |point: [f64; 3]| (point[1] - center[1]).atan2(point[0] - center[0]);
        let start_angle = angle(start);
        let end_angle = (angle(end) - start_angle).rem_euclid(TAU);
        let mid_angle = (angle(middle) - start_angle).rem_euclid(TAU);
        let sweep = if mid_angle <= end_angle {
            end_angle
        } else {
            end_angle - TAU
        };
        Some(Self {
            center,
            radius,
            start: start_angle,
            sweep,
        })
    }
    fn point(&self, fraction: f64) -> [f64; 3] {
        let angle = self.start + self.sweep * fraction;
        [
            self.center[0] + self.radius * angle.cos(),
            self.center[1] + self.radius * angle.sin(),
            0.,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: [f64; 3], b: [f64; 3]) {
        assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 1e-8, "{a:?} != {b:?}");
    }
    #[test]
    fn arc_sweep_contains_middle_for_clockwise_and_major_arcs() {
        for (first, middle, end, expected_sweep) in [
            (
                [1., 0., 0.],
                [0., 1., 0.],
                [-1., 0., 0.],
                std::f64::consts::PI,
            ),
            (
                [1., 0., 0.],
                [0., -1., 0.],
                [-1., 0., 0.],
                -std::f64::consts::PI,
            ),
            (
                [1., 0., 0.],
                [-1., 0., 0.],
                [0., 1., 0.],
                -3. * std::f64::consts::FRAC_PI_2,
            ),
        ] {
            let arc = Arc::through(first, middle, end).unwrap();
            assert!((arc.sweep - expected_sweep).abs() < 1e-10);
            close(arc.point(0.), first);
            close(arc.point(1.), end);
            let mid_angle = (middle[1] - arc.center[1]).atan2(middle[0] - arc.center[0]);
            let delta = if arc.sweep > 0. {
                (mid_angle - arc.start).rem_euclid(TAU)
            } else {
                -(arc.start - mid_angle).rem_euclid(TAU)
            };
            let fraction = delta / arc.sweep;
            assert!((0. ..=1.).contains(&fraction));
            close(arc.point(fraction), middle);
        }
    }
    #[test]
    fn translated_arc_and_degenerate_points_are_handled() {
        let arc = Arc::through([101., 200., 0.], [100., 201., 0.], [99., 200., 0.]).unwrap();
        close(arc.point(0.5), [100., 201., 0.]);
        assert!(Arc::through([0.; 3], [1., 1., 0.], [2., 2., 0.]).is_none());
        assert!(Arc::through([0.; 3], [0.; 3], [1., 0., 0.]).is_none());
        assert!(Arc::through([f64::NAN, 0., 0.], [1., 1., 0.], [2., 0., 0.]).is_none());
    }
    #[test]
    fn tessellation_adapts_to_screen_radius_with_bounded_chord_error() {
        let count = segment_count(100., 1000., TAU);
        let error = 1000. * (1. - (TAU / count as f64 * 0.5).cos());
        assert!(error <= 0.35);
        assert!(count > segment_count(100., 10., TAU));
        assert!(count <= 4096);
    }
}
