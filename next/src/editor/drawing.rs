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
        Tool::BSpline | Tool::PeriodicBSpline => {
            let mut controls = points.to_vec();
            controls.extend(cursor);
            if tool == Tool::PeriodicBSpline && controls.len() > 2 {
                controls.push(controls[0]);
            }
            let polygon: Vec<_> = controls.into_iter().map(project).collect();
            painter.extend(Shape::dashed_line(&polygon, stroke, 7., 4.));
        }
        Tool::Ellipse | Tool::EllipseArc | Tool::HyperbolaArc | Tool::ParabolaArc => {
            let mut controls = points.to_vec();
            controls.extend(cursor);
            controls.truncate(tool.required_points());
            path = conic_preview(painter, viewport, tool, &controls, color);
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

fn conic_preview(
    painter: &egui::Painter,
    viewport: &Viewport,
    tool: Tool,
    controls: &[[f64; 3]],
    color: Color32,
) -> Vec<[f64; 3]> {
    let project = |point| viewport.sketch_screen_point(point);
    let guide = Stroke::new(0.9_f32, color.gamma_multiply(0.55));
    if controls.len() < 2 {
        return vec![];
    }
    painter.line_segment([project(controls[0]), project(controls[1])], guide);
    let sample = |eval: &dyn Fn(f64) -> [f64; 3], start, end| {
        sample_curve(eval, start, end, |point| {
            let p = project(point);
            [f64::from(p.x), f64::from(p.y)]
        })
    };
    if tool == Tool::ParabolaArc {
        let Some(frame) = ParabolaFrame::through(controls[0], controls[1]) else {
            return vec![];
        };
        if controls.len() >= 4 {
            let (start, end) = ordered(frame.parameter(controls[2]), frame.parameter(controls[3]));
            return sample(&|t| frame.point(t), start, end);
        }
        let extent = controls
            .get(2)
            .map(|p| frame.parameter(*p).abs())
            .unwrap_or(0.)
            .max(4. * frame.focal);
        let path = sample(&|t| frame.point(t), -extent, extent);
        painter.extend(Shape::dashed_line(
            &path.into_iter().map(project).collect::<Vec<_>>(),
            guide,
            7.,
            4.,
        ));
        return vec![];
    }
    let Some(&minor) = controls.get(2) else {
        return vec![];
    };
    let Some(frame) = ConicFrame::through(controls[0], controls[1], minor) else {
        return vec![];
    };
    painter.line_segment(
        [
            project(frame.at(-frame.major, 0.)),
            project(frame.at(frame.major, 0.)),
        ],
        guide,
    );
    painter.line_segment(
        [
            project(frame.at(0., -frame.minor)),
            project(frame.at(0., frame.minor)),
        ],
        guide,
    );
    if tool != Tool::HyperbolaArc && frame.minor > frame.major {
        return vec![];
    }
    if tool == Tool::Ellipse {
        return sample(&|t| frame.ellipse(t), 0., TAU);
    }
    let hyperbola = tool == Tool::HyperbolaArc;
    let parameter = |p| {
        if hyperbola {
            frame.hyperbola_parameter(p)
        } else {
            frame.ellipse_parameter(p)
        }
    };
    let eval = |t| {
        if hyperbola {
            frame.hyperbola(t)
        } else {
            frame.ellipse(t)
        }
    };
    if controls.len() >= 5 {
        let start = parameter(controls[3]);
        let end = parameter(controls[4]);
        let (start, end) = if hyperbola {
            ordered(start, end)
        } else {
            let sweep = (end - start).rem_euclid(TAU);
            (start, start + if sweep == 0. { TAU } else { sweep })
        };
        return sample(&eval, start, end);
    }
    let (start, end) = if hyperbola {
        let extent = controls
            .get(3)
            .map(|p| parameter(*p).abs())
            .unwrap_or(0.)
            .max(1.);
        (-extent, extent)
    } else {
        (0., TAU)
    };
    let path = sample(&eval, start, end);
    painter.extend(Shape::dashed_line(
        &path.into_iter().map(project).collect::<Vec<_>>(),
        guide,
        7.,
        4.,
    ));
    vec![]
}

fn ordered(a: f64, b: f64) -> (f64, f64) {
    (a.min(b), a.max(b))
}
fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

struct ConicFrame {
    center: [f64; 3],
    u: [f64; 2],
    v: [f64; 2],
    major: f64,
    minor: f64,
}
impl ConicFrame {
    fn through(center: [f64; 3], major_point: [f64; 3], minor_point: [f64; 3]) -> Option<Self> {
        let offset = [major_point[0] - center[0], major_point[1] - center[1]];
        let major = offset[0].hypot(offset[1]);
        if !major.is_finite() || major <= 1e-9 {
            return None;
        }
        let u = [offset[0] / major, offset[1] / major];
        let perpendicular = [-u[1], u[0]];
        let signed_minor = dot(
            [minor_point[0] - center[0], minor_point[1] - center[1]],
            perpendicular,
        );
        if !signed_minor.is_finite() || signed_minor.abs() <= 1e-9 {
            return None;
        }
        let v = perpendicular.map(|value| value * signed_minor.signum());
        Some(Self {
            center,
            u,
            v,
            major,
            minor: signed_minor.abs(),
        })
    }
    fn at(&self, x: f64, y: f64) -> [f64; 3] {
        [
            self.center[0] + x * self.u[0] + y * self.v[0],
            self.center[1] + x * self.u[1] + y * self.v[1],
            0.,
        ]
    }
    fn local(&self, point: [f64; 3]) -> [f64; 2] {
        let delta = [point[0] - self.center[0], point[1] - self.center[1]];
        [dot(delta, self.u), dot(delta, self.v)]
    }
    fn ellipse(&self, t: f64) -> [f64; 3] {
        self.at(self.major * t.cos(), self.minor * t.sin())
    }
    fn ellipse_parameter(&self, point: [f64; 3]) -> f64 {
        let [x, y] = self.local(point);
        (y / self.minor).atan2(x / self.major)
    }
    fn hyperbola(&self, t: f64) -> [f64; 3] {
        self.at(self.major * t.cosh(), self.minor * t.sinh())
    }
    fn hyperbola_parameter(&self, point: [f64; 3]) -> f64 {
        (self.local(point)[1] / self.minor).asinh()
    }
}
struct ParabolaFrame {
    vertex: [f64; 3],
    u: [f64; 2],
    v: [f64; 2],
    focal: f64,
}
impl ParabolaFrame {
    fn through(vertex: [f64; 3], focus: [f64; 3]) -> Option<Self> {
        let delta = [focus[0] - vertex[0], focus[1] - vertex[1]];
        let focal = delta[0].hypot(delta[1]);
        if !focal.is_finite() || focal <= 1e-9 {
            return None;
        }
        let u = delta.map(|v| v / focal);
        Some(Self {
            vertex,
            u,
            v: [-u[1], u[0]],
            focal,
        })
    }
    fn point(&self, t: f64) -> [f64; 3] {
        let x = t * t / (4. * self.focal);
        [
            self.vertex[0] + x * self.u[0] + t * self.v[0],
            self.vertex[1] + x * self.u[1] + t * self.v[1],
            0.,
        ]
    }
    fn parameter(&self, point: [f64; 3]) -> f64 {
        dot(
            [point[0] - self.vertex[0], point[1] - self.vertex[1]],
            self.v,
        )
    }
}

/// Subdivide in screen space, with bounded work even when a cursor is far away.
fn sample_curve(
    eval: &dyn Fn(f64) -> [f64; 3],
    start: f64,
    end: f64,
    project: impl Fn([f64; 3]) -> [f64; 2],
) -> Vec<[f64; 3]> {
    if !start.is_finite() || !end.is_finite() {
        return vec![];
    }
    let first = eval(start);
    let mut points = vec![first];
    let mut pending = vec![(start, first, end, eval(end), 0)];
    while let Some((a, first, b, last, depth)) = pending.pop() {
        let middle = (a + b) * 0.5;
        let mid = eval(middle);
        if [first, mid, last].iter().flatten().any(|v| !v.is_finite()) {
            return vec![];
        }
        let (p, q, m) = (project(first), project(last), project(mid));
        if [p, q, m].iter().flatten().any(|v| !v.is_finite()) {
            return vec![];
        }
        let direction = [q[0] - p[0], q[1] - p[1]];
        let length = dot(direction, direction);
        let delta = [m[0] - p[0], m[1] - p[1]];
        let t = if length > 0. {
            (dot(delta, direction) / length).clamp(0., 1.)
        } else {
            0.
        };
        let error = (delta[0] - direction[0] * t).hypot(delta[1] - direction[1] * t);
        if error > 0.35 && depth < 12 && points.len() + pending.len() < 4096 {
            pending.push((middle, mid, b, last, depth + 1));
            pending.push((a, first, middle, mid, depth + 1));
        } else {
            points.push(last);
        }
    }
    points
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
    #[test]
    fn conic_axes_match_native_rotated_and_negative_minor_frames() {
        let frame = ConicFrame::through([10., 20., 0.], [10., 25., 0.], [13., 22., 0.]).unwrap();
        assert_eq!((frame.major, frame.minor), (5., 3.));
        close(frame.ellipse(0.), [10., 25., 0.]);
        close(frame.ellipse(std::f64::consts::FRAC_PI_2), [13., 20., 0.]);
        for parameter in [-2., -0.7, 0., 0.8, 2.5] {
            assert!((frame.ellipse_parameter(frame.ellipse(parameter)) - parameter).abs() < 1e-12);
            assert!(
                (frame.hyperbola_parameter(frame.hyperbola(parameter)) - parameter).abs() < 1e-12
            );
            let [x, y] = frame.local(frame.hyperbola(parameter));
            assert!((x * x / 25. - y * y / 9. - 1.).abs() < 1e-12);
        }
        assert!(ConicFrame::through([0.; 3], [0.; 3], [1., 1., 0.]).is_none());
        assert!(ConicFrame::through([0.; 3], [1., 0., 0.], [3., 0., 0.]).is_none());
    }
    #[test]
    fn parabola_uses_vertex_focus_frame_and_native_distance_parameter() {
        let frame = ParabolaFrame::through([2., 3., 0.], [2., 5., 0.]).unwrap();
        close(frame.point(4.), [-2., 5., 0.]);
        for parameter in [-4., -1., 0., 1., 4.] {
            let point = frame.point(parameter);
            assert!((frame.parameter(point) - parameter).abs() < 1e-12);
            let distance_focus = (point[0] - 2.).hypot(point[1] - 5.);
            let distance_directrix = point[1] - 1.;
            assert!((distance_focus - distance_directrix).abs() < 1e-12);
        }
        assert!(ParabolaFrame::through([0.; 3], [0.; 3]).is_none());
    }
    #[test]
    fn adaptive_conic_preview_bounds_screen_error_and_work() {
        let frame = ConicFrame::through([0.; 3], [5., 0., 0.], [0., 3., 0.]).unwrap();
        let path = sample_curve(&|t| frame.ellipse(t), 0., TAU, |p| {
            [p[0] * 100., p[1] * 100.]
        });
        assert!(path.len() > 8 && path.len() <= 4097);
        close(path[0], *path.last().unwrap());
        for step in 0..1024 {
            let p = frame.ellipse(TAU * step as f64 / 1024.);
            let error = path
                .windows(2)
                .map(|segment| {
                    let [a, b] = [segment[0], segment[1]];
                    let direction = [b[0] - a[0], b[1] - a[1]];
                    let offset = [p[0] - a[0], p[1] - a[1]];
                    let fraction =
                        (dot(offset, direction) / dot(direction, direction)).clamp(0., 1.);
                    (offset[0] - direction[0] * fraction).hypot(offset[1] - direction[1] * fraction)
                        * 100.
                })
                .fold(f64::INFINITY, f64::min);
            assert!(error <= 0.36, "screen chord error {error}");
        }
        let huge = sample_curve(&|t| [t.cos() * 1e25, t.sin() * 1e25, 0.], 0., TAU, |p| {
            [p[0], p[1]]
        });
        assert!(huge.len() <= 4097);
        assert!(sample_curve(&|_| [f64::INFINITY, 0., 0.], 0., 1., |p| [p[0], p[1]]).is_empty());
    }
    #[test]
    fn conic_tool_previews_follow_native_projected_arc_parameters() {
        let context = egui::Context::default();
        let viewport = Viewport::default();
        let mut paths = vec![];
        let _ = context.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            for (tool, controls) in [
                (
                    Tool::EllipseArc,
                    vec![
                        [0.; 3],
                        [5., 0., 0.],
                        [2., -3., 0.],
                        [5., 0., 0.],
                        [0., -3., 0.],
                    ],
                ),
                (
                    Tool::HyperbolaArc,
                    vec![
                        [0.; 3],
                        [3., 0., 0.],
                        [0., 2., 0.],
                        [99., 2_f64.sinh() * 2., 0.],
                        [99., -1_f64.sinh() * 2., 0.],
                    ],
                ),
                (
                    Tool::ParabolaArc,
                    vec![[0.; 3], [2., 0., 0.], [99., 4., 0.], [-7., -2., 0.]],
                ),
            ] {
                paths.push(conic_preview(
                    &painter,
                    &viewport,
                    tool,
                    &controls,
                    Color32::WHITE,
                ));
            }
            assert!(
                conic_preview(
                    &painter,
                    &viewport,
                    Tool::Ellipse,
                    &[[0.; 3], [2., 0., 0.], [0., 3., 0.]],
                    Color32::WHITE
                )
                .is_empty()
            );
        });
        close(paths[0][0], [5., 0., 0.]);
        close(*paths[0].last().unwrap(), [0., -3., 0.]);
        assert!(paths[0].iter().all(|p| p[0] >= -1e-9 && p[1] <= 1e-9));
        close(paths[1][0], [3. * 1_f64.cosh(), -2. * 1_f64.sinh(), 0.]);
        close(
            *paths[1].last().unwrap(),
            [3. * 2_f64.cosh(), 2. * 2_f64.sinh(), 0.],
        );
        close(paths[2][0], [0.5, -2., 0.]);
        close(*paths[2].last().unwrap(), [2., 4., 0.]);
    }
}
