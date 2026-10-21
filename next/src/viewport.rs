//! Presentation of native display data. This module never owns modeling state.
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use glam::{Quat, Vec3};
use serde_json::Value;
use std::sync::Arc;
mod gl_renderer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub object: String,
    pub subelement: String,
}

struct Triangle {
    points: [Vec3; 3],
    normals: [Vec3; 3],
    face: String,
}
struct Edge {
    points: Vec<Vec3>,
    name: String,
}
struct SketchCurve {
    points: Vec<Vec3>,
    construction: bool,
    index: usize,
}
struct ProjectedTriangle {
    points: [Pos2; 3],
    depths: [f32; 3],
    face: usize,
    colors: [Color32; 3],
}

pub struct Viewport {
    object: String,
    renderer: Option<Arc<gl_renderer::Renderer>>,
    revision: Option<u64>,
    triangles: Vec<Triangle>,
    edges: Vec<Edge>,
    sketch: Option<Value>,
    curves: Vec<SketchCurve>,
    sketch_origin: Vec3,
    sketch_rotation: Quat,
    center: Vec3,
    rotation: Quat,
    scale: f32,
    rect: Rect,
    pub response: Option<egui::Response>,
    pub selected: Option<Selection>,
}

impl Default for Viewport {
    fn default() -> Self {
        let mut result = Self {
            object: String::new(),
            renderer: None,
            revision: None,
            triangles: vec![],
            edges: vec![],
            sketch: None,
            curves: vec![],
            sketch_origin: Vec3::ZERO,
            sketch_rotation: Quat::IDENTITY,
            center: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: 8.,
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(600.)),
            response: None,
            selected: None,
        };
        result.iso();
        result
    }
}

fn vector(value: &Value) -> Vec3 {
    Vec3::new(
        value[0].as_f64().unwrap_or(0.) as f32,
        value[1].as_f64().unwrap_or(0.) as f32,
        value[2].as_f64().unwrap_or(0.) as f32,
    )
}
fn placement(value: &Value) -> (Vec3, Quat) {
    let r = &value["rotation"];
    let q = Quat::from_xyzw(
        r[0].as_f64().unwrap_or(0.) as f32,
        r[1].as_f64().unwrap_or(0.) as f32,
        r[2].as_f64().unwrap_or(0.) as f32,
        r[3].as_f64().unwrap_or(1.) as f32,
    );
    (
        vector(&value["position"]),
        if q.is_finite() && q.length_squared() > 0. {
            q.normalize()
        } else {
            Quat::IDENTITY
        },
    )
}

impl Viewport {
    pub fn set_gl(&mut self, gl: Arc<eframe::glow::Context>) -> Result<(), String> {
        let renderer = gl_renderer::Renderer::new(gl)?;
        self.destroy_gl();
        self.renderer = Some(renderer);
        Ok(())
    }
    pub fn destroy_gl(&mut self) {
        if let Some(renderer) = self.renderer.take() {
            renderer.destroy();
        }
    }
    pub fn set_scene(&mut self, mesh: Value, topology: Value) {
        let object = mesh["object"].as_str().unwrap_or("").to_owned();
        let revision = mesh["revision"].as_u64();
        if object != self.object || revision != self.revision {
            self.selected = None;
        }
        self.object = object;
        self.revision = revision;
        let (origin, rotation) = placement(&mesh["world_from_shape"]);
        let positions: Vec<Vec3> = mesh["positions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| origin + rotation * vector(p))
            .collect();
        // Display normals are native surface normals, not averaged triangle normals.
        // Rotate directions into world space without applying placement translation.
        let normals: Vec<Vec3> = mesh["normals"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|normal| rotation * vector(normal).normalize_or_zero())
            .collect();
        let native_triangles = mesh["triangles"].as_array();
        self.triangles.clear();
        for face in mesh["faces"].as_array().into_iter().flatten() {
            let first = face["first_triangle"].as_u64().unwrap_or(0) as usize;
            let count = face["triangle_count"].as_u64().unwrap_or(0) as usize;
            let name = face["subelement"].as_str().unwrap_or("");
            for tri in native_triangles
                .into_iter()
                .flatten()
                .skip(first)
                .take(count)
            {
                let indices = [tri[0].as_u64(), tri[1].as_u64(), tri[2].as_u64()];
                if let [Some(a), Some(b), Some(c)] = indices
                    && let (Some(a), Some(b), Some(c)) = (
                        positions.get(a as usize),
                        positions.get(b as usize),
                        positions.get(c as usize),
                    )
                    && a.is_finite()
                    && b.is_finite()
                    && c.is_finite()
                {
                    let geometric_normal = (*b - *a).cross(*c - *a).normalize_or_zero();
                    let triangle_normals = indices.map(|index| {
                        index
                            .and_then(|index| normals.get(index as usize))
                            .copied()
                            .filter(|normal| normal.is_finite() && normal.length_squared() > 0.)
                            .unwrap_or(geometric_normal)
                    });
                    self.triangles.push(Triangle {
                        points: [*a, *b, *c],
                        normals: triangle_normals,
                        face: name.to_owned(),
                    });
                }
            }
        }
        let (origin, rotation) = placement(&topology["world_from_shape"]);
        self.edges = topology["edges"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|edge| {
                // An endpoint chord misrepresents a curved edge. Only fall back for lines.
                let points = edge["points"].as_array().or_else(|| {
                    (edge["geometry_type"].as_str() == Some("Part::GeomLine"))
                        .then(|| edge["endpoints"].as_array())
                        .flatten()
                })?;
                Some(Edge {
                    name: edge["subelement"].as_str()?.to_owned(),
                    points: points
                        .iter()
                        .map(|p| origin + rotation * vector(p))
                        .collect(),
                })
            })
            .collect();
    }

    pub fn set_sketch(&mut self, sketch: Option<Value>) {
        self.curves.clear();
        if let Some(entry) = &sketch {
            let p = if entry["global_placement"].is_object() {
                &entry["global_placement"]
            } else {
                &entry["properties"]["Placement"]
            };
            (self.sketch_origin, self.sketch_rotation) = placement(p);
            for geo in entry["sketch"]["geometry"].as_array().into_iter().flatten() {
                let mut points = vec![];
                match geo["kind"].as_str() {
                    Some("line") => points.extend([vector(&geo["start"]), vector(&geo["end"])]),
                    Some(kind @ ("circle" | "arc")) => {
                        let center = vector(&geo["center"]);
                        let radius = geo["radius"].as_f64().unwrap_or(0.) as f32;
                        let (start, sweep) = if kind == "circle" {
                            (0., std::f32::consts::TAU)
                        } else {
                            let a = vector(&geo["start"]) - center;
                            let b = vector(&geo["end"]) - center;
                            let m = vector(&geo["mid"]) - center;
                            let start = a.y.atan2(a.x);
                            let end = (b.y.atan2(b.x) - start).rem_euclid(std::f32::consts::TAU);
                            let mid = (m.y.atan2(m.x) - start).rem_euclid(std::f32::consts::TAU);
                            (
                                start,
                                if mid <= end {
                                    end
                                } else {
                                    end - std::f32::consts::TAU
                                },
                            )
                        };
                        let steps = (sweep.abs() * 20.).ceil().max(8.) as usize;
                        for i in 0..=steps {
                            let a = start + sweep * i as f32 / steps as f32;
                            points.push(center + Vec3::new(a.cos(), a.sin(), 0.) * radius);
                        }
                    }
                    _ => {}
                }
                self.curves.push(SketchCurve {
                    points: points
                        .into_iter()
                        .map(|p| self.sketch_origin + self.sketch_rotation * p)
                        .collect(),
                    construction: geo["construction"].as_bool().unwrap_or(false),
                    index: geo["index"].as_u64().unwrap_or(0) as usize,
                });
            }
        }
        self.sketch = sketch;
    }

    pub fn top(&mut self) {
        self.rotation = Quat::IDENTITY;
    }
    pub fn iso(&mut self) {
        self.rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)
            * Quat::from_rotation_x(54.73561_f32.to_radians());
    }
    pub fn align_sketch(&mut self) {
        self.rotation = self.sketch_rotation;
        self.center = self.sketch_origin;
    }
    pub fn fit(&mut self) {
        let points: Vec<Vec3> = self
            .triangles
            .iter()
            .flat_map(|t| t.points)
            .chain(self.curves.iter().flat_map(|c| c.points.iter().copied()))
            .collect();
        if points.is_empty() {
            self.center = self.sketch_origin;
            self.scale = 8.;
            return;
        }
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for p in &points {
            min = min.min(*p);
            max = max.max(*p);
        }
        self.center = (min + max) * 0.5;
        let mut extent = Vec3::ZERO;
        for p in points {
            extent = extent.max((self.rotation.conjugate() * (p - self.center)).abs());
        }
        self.scale = (self.rect.width() / (extent.x * 2.).max(1.))
            .min(self.rect.height() / (extent.y * 2.).max(1.))
            * 0.78;
        self.scale = self.scale.clamp(0.0001, 100000.);
    }
    fn project(&self, p: Vec3) -> (Pos2, f32) {
        let p = self.rotation.conjugate() * (p - self.center);
        (self.rect.center() + Vec2::new(p.x, -p.y) * self.scale, p.z)
    }
    pub fn sketch_screen_point(&self, point: [f64; 3]) -> Pos2 {
        self.project(
            self.sketch_origin
                + self.sketch_rotation
                    * Vec3::new(point[0] as f32, point[1] as f32, point[2] as f32),
        )
        .0
    }
    /// Intersect the cursor ray with the native sketch plane, returning sketch-local coordinates.
    pub fn sketch_point(&self, pos: Pos2) -> Option<[f64; 3]> {
        let p = (pos - self.rect.center()) / self.scale;
        let origin = self.center + self.rotation * Vec3::new(p.x, -p.y, 0.);
        let direction = self.rotation * Vec3::Z;
        let normal = self.sketch_rotation * Vec3::Z;
        let denominator = direction.dot(normal);
        if denominator.abs() < 1e-6 {
            return None;
        }
        let hit = origin + direction * ((self.sketch_origin - origin).dot(normal) / denominator);
        let local = self.sketch_rotation.conjugate() * (hit - self.sketch_origin);
        Some([local.x as f64, local.y as f64, 0.])
    }
    fn projected(&self) -> Vec<ProjectedTriangle> {
        self.triangles
            .iter()
            .enumerate()
            .map(|(index, t)| {
                let p = t.points.map(|p| self.project(p));
                let selected = self
                    .selected
                    .as_ref()
                    .is_some_and(|s| s.object == self.object && s.subelement == t.face);
                let light = Vec3::new(-0.3, -0.5, 0.8).normalize();
                let colors = t.normals.map(|normal| {
                    if selected {
                        Color32::from_rgb(47, 147, 143)
                    } else {
                        let brightness = 0.48 + 0.52 * normal.dot(light).max(0.);
                        Color32::from_rgb(
                            (119. * brightness) as u8,
                            (132. * brightness) as u8,
                            (143. * brightness) as u8,
                        )
                    }
                });
                ProjectedTriangle {
                    points: p.map(|p| p.0),
                    depths: p.map(|p| p.1),
                    face: index,
                    colors,
                }
            })
            .collect()
    }
    fn pick(&self, pos: Pos2, triangles: &[ProjectedTriangle]) -> Option<Selection> {
        let front = front_triangle(pos, triangles);
        let front_depth = front.map(|(_, z)| z).unwrap_or(f32::NEG_INFINITY);
        let tolerance = 0.8 / self.scale;
        let mut edge_hit: Option<(String, f32)> = None;
        for edge in &self.edges {
            for line in edge.points.windows(2) {
                let a = self.project(line[0]);
                let b = self.project(line[1]);
                let (distance, t) = segment_distance(pos, a.0, b.0);
                let depth = a.1 + t * (b.1 - a.1);
                if distance < 6.
                    && depth >= front_depth - tolerance
                    && edge_hit.as_ref().is_none_or(|(_, d)| distance < *d)
                {
                    edge_hit = Some((edge.name.clone(), distance));
                }
            }
        }
        edge_hit
            .map(|(subelement, _)| Selection {
                object: self.object.clone(),
                subelement,
            })
            .or_else(|| {
                front.map(|(i, _)| Selection {
                    object: self.object.clone(),
                    subelement: self.triangles[triangles[i].face].face.clone(),
                })
            })
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Selection> {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        self.rect = rect;
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0., Color32::from_rgb(8, 10, 12));
        let delta = ui.input(|i| i.pointer.delta());
        if response.dragged_by(egui::PointerButton::Secondary) {
            let yaw = Quat::from_axis_angle(Vec3::Z, -delta.x * 0.008);
            let pitch = Quat::from_axis_angle(self.rotation * Vec3::X, -delta.y * 0.008);
            self.rotation = (yaw * pitch * self.rotation).normalize();
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            self.center -= self.rotation * Vec3::new(delta.x, -delta.y, 0.) / self.scale;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0. {
                let anchor = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
                let p = anchor - rect.center();
                let before = self.rotation * Vec3::new(p.x, -p.y, 0.) / self.scale;
                self.scale = (self.scale * (scroll * 0.002).exp()).clamp(0.0001, 100000.);
                let after = self.rotation * Vec3::new(p.x, -p.y, 0.) / self.scale;
                self.center += before - after;
            }
        }
        let triangles = self.projected();
        if let Some(renderer) = &self.renderer {
            let depth_extent = triangles
                .iter()
                .flat_map(|t| t.depths)
                .map(f32::abs)
                .fold(1_f32, f32::max)
                * 1.1;
            let mut vertices = Vec::with_capacity(triangles.len() * 18);
            for triangle in &triangles {
                for ((point, depth), color) in triangle
                    .points
                    .iter()
                    .zip(triangle.depths)
                    .zip(triangle.colors)
                {
                    vertices.extend_from_slice(&[
                        (point.x - rect.center().x) * 2. / rect.width(),
                        -(point.y - rect.center().y) * 2. / rect.height(),
                        -depth / depth_extent,
                        color.r() as f32 / 255.,
                        color.g() as f32 / 255.,
                        color.b() as f32 / 255.,
                    ]);
                }
            }
            painter.add(renderer.callback(rect, vertices));
        }
        let depth_index = DepthIndex::new(rect, &triangles);
        // Test each small edge segment against the visible surface.
        for edge in &self.edges {
            let selected = self
                .selected
                .as_ref()
                .is_some_and(|s| s.object == self.object && s.subelement == edge.name);
            let stroke = Stroke::new(
                if selected { 2.5_f32 } else { 1.1_f32 },
                if selected {
                    Color32::from_rgb(95, 221, 207)
                } else {
                    Color32::from_rgb(32, 42, 48)
                },
            );
            for line in edge.points.windows(2) {
                let a = self.project(line[0]);
                let b = self.project(line[1]);
                let steps = (a.0.distance(b.0) / 8.).ceil().clamp(1., 1000.) as usize;
                for i in 0..steps {
                    let t0 = i as f32 / steps as f32;
                    let t1 = (i + 1) as f32 / steps as f32;
                    let mid = (t0 + t1) * 0.5;
                    let pos = a.0.lerp(b.0, mid);
                    let z = a.1 + (b.1 - a.1) * mid;
                    if depth_index
                        .front_depth(pos, &triangles)
                        .is_none_or(|front| z >= front - 0.8 / self.scale)
                    {
                        painter.line_segment([a.0.lerp(b.0, t0), a.0.lerp(b.0, t1)], stroke);
                    }
                }
            }
        }
        for curve in &self.curves {
            let color = if curve.construction {
                Color32::from_rgb(129, 106, 172)
            } else {
                Color32::from_rgb(83, 198, 185)
            };
            let points: Vec<Pos2> = curve.points.iter().map(|p| self.project(*p).0).collect();
            let stroke = Stroke::new(1.8_f32, color);
            if curve.construction {
                painter.extend(Shape::dashed_line(&points, stroke, 7., 4.));
            } else {
                painter.add(Shape::line(points, stroke));
            }
            if let (Some(first), Some(last)) = (curve.points.first(), curve.points.last()) {
                for p in [first, last] {
                    painter.circle_filled(self.project(*p).0, 2.5, color);
                }
            }
        }
        self.draw_dimensions(&painter);
        let origin = rect.left_bottom() + Vec2::new(36., -36.);
        for (axis, label, color) in [
            (Vec3::X, "X", Color32::from_rgb(184, 94, 104)),
            (Vec3::Y, "Y", Color32::from_rgb(101, 173, 142)),
            (Vec3::Z, "Z", Color32::from_rgb(123, 133, 207)),
        ] {
            let p = self.rotation.conjugate() * axis;
            let tip = origin + Vec2::new(p.x, -p.y) * 22.;
            painter.line_segment([origin, tip], Stroke::new(1.5_f32, color));
            painter.text(
                tip,
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::monospace(10.),
                color,
            );
        }
        let mut selection = None;
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            // Sketch entities are intentionally selectable in their active edit plane.
            let sketch_hit = self.curves.iter().find(|curve| {
                curve.points.windows(2).any(|line| {
                    segment_distance(pos, self.project(line[0]).0, self.project(line[1]).0).0 < 6.
                })
            });
            selection = sketch_hit
                .map(|curve| Selection {
                    object: self
                        .sketch
                        .as_ref()
                        .and_then(|s| s["name"].as_str())
                        .unwrap_or("")
                        .to_owned(),
                    subelement: format!("Geometry{}", curve.index),
                })
                .or_else(|| {
                    self.sketch
                        .is_none()
                        .then(|| self.pick(pos, &triangles))
                        .flatten()
                });
            self.selected = selection.clone();
        }
        self.response = Some(response);
        selection
    }
    fn draw_dimensions(&self, painter: &egui::Painter) {
        let Some(sketch) = &self.sketch else {
            return;
        };
        let points: Vec<Pos2> = self
            .curves
            .iter()
            .flat_map(|c| c.points.iter())
            .map(|p| self.project(*p).0)
            .collect();
        let sketch_center = if points.is_empty() {
            self.rect.center()
        } else {
            Rect::from_points(&points).center()
        };
        let color = Color32::from_rgb(184, 159, 211);
        let stroke = Stroke::new(1_f32, color);
        for constraint in sketch["sketch"]["constraints"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let kind = constraint["type"].as_str().unwrap_or("");
            let Some(index) = constraint["first"].as_u64() else {
                continue;
            };
            let Some(curve) = self.curves.iter().find(|c| c.index == index as usize) else {
                continue;
            };
            let value = constraint["value"].as_f64().unwrap_or(0.);
            if kind == "Distance"
                && curve.points.len() == 2
                && constraint["first_pos"].as_i64() == Some(0)
            {
                let a = self.project(curve.points[0]).0;
                let b = self.project(curve.points[1]).0;
                let direction = (b - a).normalized();
                if a.distance(b) < 1. {
                    continue;
                }
                let mut normal = Vec2::new(-direction.y, direction.x);
                if normal.dot(a.lerp(b, 0.5) - sketch_center) < 0. {
                    normal = -normal;
                }
                let offset = normal * 26.;
                for endpoint in [a, b] {
                    painter.line_segment(
                        [endpoint + normal * 4., endpoint + offset + normal * 6.],
                        stroke,
                    );
                }
                painter.line_segment([a + offset, b + offset], stroke);
                let tick = (direction + normal).normalized() * 4.;
                for endpoint in [a + offset, b + offset] {
                    painter.line_segment([endpoint - tick, endpoint + tick], stroke);
                }
                dimension_label(
                    painter,
                    a.lerp(b, 0.5) + offset,
                    format!("{value:.2} mm"),
                    color,
                );
            } else if matches!(kind, "Radius" | "Diameter") {
                let Some(geo) = sketch["sketch"]["geometry"]
                    .as_array()
                    .and_then(|geometry| {
                        geometry.iter().find(|g| g["index"].as_u64() == Some(index))
                    })
                else {
                    continue;
                };
                let center = vector(&geo["center"]);
                let radius = geo["radius"].as_f64().unwrap_or(0.) as f32;
                if radius <= 0. {
                    continue;
                }
                let radial = Vec3::new(1., 1., 0.).normalize() * radius;
                let world = |p: Vec3| {
                    self.project(self.sketch_origin + self.sketch_rotation * p)
                        .0
                };
                let a = world(if kind == "Diameter" {
                    center - radial
                } else {
                    center
                });
                let b = world(center + radial);
                let direction = (b - a).normalized();
                let leader = b + direction * 24.;
                painter.line_segment([a, leader], stroke);
                let normal = Vec2::new(-direction.y, direction.x);
                let tick = (direction + normal).normalized() * 4.;
                painter.line_segment([b - tick, b + tick], stroke);
                if kind == "Diameter" {
                    painter.line_segment([a - tick, a + tick], stroke);
                }
                dimension_label(
                    painter,
                    leader + direction * 28.,
                    format!(
                        "{}{value:.2} mm",
                        if kind == "Diameter" { "Ø" } else { "R" }
                    ),
                    color,
                );
            }
            // Coordinate and angular datums remain editable in the dimension panel.
            // They need native dimension placement before being added to the canvas.
        }
    }
}

fn dimension_label(painter: &egui::Painter, position: Pos2, text: String, color: Color32) {
    let galley = painter.layout_no_wrap(text, egui::FontId::monospace(12.), color);
    let rect = Rect::from_center_size(position, galley.size() + Vec2::new(8., 4.));
    painter.rect_filled(rect, 2., Color32::from_rgb(8, 10, 12));
    painter.galley(position - galley.size() * 0.5, galley, color);
}

fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> (f32, f32) {
    let d = b - a;
    let t = if d.length_sq() > 0. {
        (p - a).dot(d) / d.length_sq()
    } else {
        0.
    }
    .clamp(0., 1.);
    (p.distance(a + d * t), t)
}
fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}
fn triangle_depth(p: Pos2, tri: &ProjectedTriangle) -> Option<f32> {
    let [a, b, c] = tri.points;
    let denominator = cross(b - a, c - a);
    if denominator.abs() < 1e-6 {
        return None;
    }
    let u = cross(p - a, c - a) / denominator;
    let v = cross(b - a, p - a) / denominator;
    if u >= -1e-5 && v >= -1e-5 && u + v <= 1.00001 {
        Some(tri.depths[0] * (1. - u - v) + tri.depths[1] * u + tri.depths[2] * v)
    } else {
        None
    }
}
fn front_triangle(p: Pos2, triangles: &[ProjectedTriangle]) -> Option<(usize, f32)> {
    triangles
        .iter()
        .enumerate()
        .filter_map(|(i, t)| triangle_depth(p, t).map(|z| (i, z)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

// A screen-space index keeps edge visibility checks local even on a detailed mesh.
struct DepthIndex {
    rect: Rect,
    columns: usize,
    rows: usize,
    cells: Vec<Vec<usize>>,
}
impl DepthIndex {
    const CELL: f32 = 32.;
    fn new(rect: Rect, triangles: &[ProjectedTriangle]) -> Self {
        let columns = (rect.width() / Self::CELL).ceil().max(1.) as usize;
        let rows = (rect.height() / Self::CELL).ceil().max(1.) as usize;
        let mut index = Self {
            rect,
            columns,
            rows,
            cells: vec![vec![]; columns * rows],
        };
        for (i, t) in triangles.iter().enumerate() {
            let bounds = Rect::from_points(&t.points).intersect(rect);
            if !bounds.is_positive() {
                continue;
            }
            let (left, top) = index.cell(bounds.min);
            let (right, bottom) = index.cell(bounds.max);
            for y in top..=bottom {
                for x in left..=right {
                    index.cells[y * columns + x].push(i);
                }
            }
        }
        index
    }
    fn cell(&self, p: Pos2) -> (usize, usize) {
        let p = p - self.rect.min;
        (
            ((p.x / Self::CELL).max(0.) as usize).min(self.columns - 1),
            ((p.y / Self::CELL).max(0.) as usize).min(self.rows - 1),
        )
    }
    fn front_depth(&self, p: Pos2, triangles: &[ProjectedTriangle]) -> Option<f32> {
        if !self.rect.contains(p) {
            return None;
        }
        let (x, y) = self.cell(p);
        self.cells[y * self.columns + x]
            .iter()
            .filter_map(|&i| triangle_depth(p, &triangles[i]))
            .max_by(f32::total_cmp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn pick_uses_interpolated_front_depth_and_rejects_hidden_edges() {
        let mut viewport = Viewport::default();
        viewport.top();
        viewport.scale = 10.;
        viewport.set_scene(json!({"object":"Pad","positions":[[0,0,0],[10,0,0],[0,10,0],[0,0,5],[10,0,5],[0,10,5]],"triangles":[[0,1,2],[3,4,5]],"faces":[{"subelement":"Face1","first_triangle":0,"triangle_count":1},{"subelement":"Face2","first_triangle":1,"triangle_count":1}]}),json!({"edges":[{"subelement":"Edge1","points":[[0,1,0],[8,1,0]]}]}));
        let pos = viewport.project(Vec3::new(2., 1., 0.)).0;
        assert_eq!(
            viewport
                .pick(pos, &viewport.projected())
                .unwrap()
                .subelement,
            "Face2"
        );
        viewport.edges[0].points.iter_mut().for_each(|p| p.z = 5.);
        assert_eq!(
            viewport
                .pick(pos, &viewport.projected())
                .unwrap()
                .subelement,
            "Edge1"
        );
    }
    #[test]
    fn sketch_ray_round_trip_respects_global_attachment() {
        let mut v = Viewport::default();
        let q = Quat::from_rotation_y(0.8);
        v.set_sketch(Some(json!({"global_placement":{"position":[20,30,40],"rotation":q.to_array()},"sketch":{"geometry":[]}})));
        v.align_sketch();
        let position = v.sketch_screen_point([12., -7., 0.]);
        let point = v.sketch_point(position).unwrap();
        assert!((point[0] - 12.).abs() < 1e-4 && (point[1] + 7.).abs() < 1e-4);
        v.rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * q;
        assert!(v.sketch_point(position).is_none());
    }
    #[test]
    fn same_object_new_revision_invalidates_native_element_selection() {
        let mut viewport = Viewport::default();
        viewport.set_scene(json!({"object":"Pad","revision":3}), json!({}));
        viewport.selected = Some(Selection {
            object: "Pad".into(),
            subelement: "Face6".into(),
        });
        viewport.set_scene(json!({"object":"Pad","revision":3}), json!({}));
        assert!(viewport.selected.is_some());
        viewport.set_scene(json!({"object":"Pad","revision":4}), json!({}));
        assert!(viewport.selected.is_none());
    }
    #[test]
    fn native_vertex_normals_rotate_with_shape_without_translation() {
        let mut viewport = Viewport::default();
        let q = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        viewport.set_scene(json!({"object":"Pad","world_from_shape":{"position":[10,20,30],"rotation":q.to_array()},
            "positions":[[0,0,0],[1,0,0],[0,1,0]],"normals":[[1,0,0],[0,1,0],[0,0,1]],
            "triangles":[[0,1,2]],"faces":[{"subelement":"Face1","first_triangle":0,"triangle_count":1}]}),json!({}));
        let triangle = &viewport.triangles[0];
        assert!(triangle.points[1].abs_diff_eq(Vec3::new(10., 21., 30.), 1e-5));
        assert!(triangle.normals[0].abs_diff_eq(Vec3::Y, 1e-5));
        assert!(triangle.normals[1].abs_diff_eq(-Vec3::X, 1e-5));
        assert!(triangle.normals[2].abs_diff_eq(Vec3::Z, 1e-5));
        assert_ne!(
            viewport.projected()[0].colors[0],
            viewport.projected()[0].colors[1]
        );
    }
    #[test]
    fn triangle_depth_interpolates_instead_of_using_centroid() {
        let t = ProjectedTriangle {
            points: [Pos2::new(0., 0.), Pos2::new(10., 0.), Pos2::new(0., 10.)],
            depths: [0., 10., 0.],
            face: 0,
            colors: [Color32::WHITE; 3],
        };
        assert!((triangle_depth(Pos2::new(8., 1.), &t).unwrap() - 8.).abs() < 1e-5);
        assert!(triangle_depth(Pos2::new(8., 8.), &t).is_none());
    }
}
