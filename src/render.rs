use crate::{engine::Scene, model::Plane};
use eframe::{
    egui,
    glow::{self, HasContext},
};
use glam::{Mat3, Mat4, Quat, Vec3};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct Camera {
    pub center: Vec3,
    pub pivot: Vec3,
    pub rotation: Quat,
    pub span: f32,
}
pub struct Projector {
    matrix: Mat4,
}

pub const ORIGIN_PLANE_HALF_SIZE: f64 = 50.;

#[derive(Clone, Copy)]
pub struct DragMotion {
    pub delta: egui::Vec2,
    pub start: Option<egui::Pos2>,
}

/// Integrate only movement while this button is held, in native event order.
/// Frame-wide pointer delta can include travel before a press or after release.
pub fn navigation_drag(input: &egui::InputState, button: egui::PointerButton) -> Option<DragMotion> {
    let first = input.events.iter().find_map(|event| match event {
        egui::Event::PointerButton { button: b, pressed, .. } if *b == button => Some(*pressed),
        _ => None,
    });
    let mut held = first.map_or_else(|| input.pointer.button_down(button), |pressed| !pressed);
    let mut active = held;
    let mut position = input.pointer.latest_pos().map(|p| p - input.pointer.delta());
    let mut motion = DragMotion { delta: egui::Vec2::ZERO, start: None };
    for event in &input.events {
        match event {
            egui::Event::PointerMoved(p) | egui::Event::PointerButton { pos: p, .. } => {
                if held && let Some(previous) = position { motion.delta += *p - previous; }
                position = Some(*p);
                if let egui::Event::PointerButton { button: b, pressed, .. } = event
                    && *b == button {
                    held = *pressed;
                    active |= held;
                    if held { motion.start = Some(*p); }
                }
            }
            egui::Event::PointerGone => position = None,
            _ => (),
        }
    }
    active.then_some(motion)
}

pub fn origin_plane_points(plane: &Plane) -> [[f64; 3]; 4] {
    let half = ORIGIN_PLANE_HALF_SIZE;
    [[-half, -half], [half, -half], [half, half], [-half, half]].map(|p| plane.at(p))
}

pub(crate) fn projected_text(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    project: impl Fn(egui::Vec2) -> egui::Pos2,
) -> egui::epaint::Mesh {
    let galley = ui.painter().layout_no_wrap(text.into(), font, color);
    let atlas = ui.fonts(|f| f.font_image_size());
    let mut mesh = egui::epaint::Mesh::default();
    for row in &galley.rows {
        let offset = mesh.vertices.len() as u32;
        for vertex in &row.visuals.mesh.vertices {
            let p = row.pos.to_vec2() + vertex.pos.to_vec2() - galley.rect.center().to_vec2();
            mesh.vertices.push(egui::epaint::Vertex {
                pos: project(p),
                uv: egui::pos2(vertex.uv.x / atlas[0] as f32, vertex.uv.y / atlas[1] as f32),
                color: vertex.color,
            });
        }
        mesh.indices.extend(row.visuals.mesh.indices.iter().map(|i| i + offset));
    }
    mesh
}

fn plane_label_axes(camera: &Camera, plane: &Plane) -> Option<(Vec3, Vec3)> {
    let facing = (camera.rotation * Vec3::Z).dot(Vec3::from_array(plane.normal.map(|v| v as f32)));
    (facing.abs() > 0.24).then(|| (
        Vec3::from_array(plane.u.map(|v| v as f32)) * facing.signum(),
        Vec3::from_array(plane.v.map(|v| v as f32)),
    ))
}

pub fn plane_label_mesh(ui: &egui::Ui, camera: &Camera, plane: &Plane, rect: egui::Rect, text: &str, color: egui::Color32) -> Option<egui::epaint::Mesh> {
    let (u, v) = plane_label_axes(camera, plane)?;
    let scale = camera.span / rect.height().max(1.);
    let anchor = Vec3::from_array(plane.at([25., 35.]).map(|v| v as f32));
    Some(projected_text(ui, text, egui::FontId::proportional(12.), color, |p| {
        let point = anchor + (u * p.x - v * p.y) * scale;
        camera.project(point.to_array().map(f64::from), rect)
    }))
}

/// Called once per preview rebuild; pointer movement never compares meshes.
pub fn edited_face_ids(scene: &Scene, source: Option<&Scene>, feature: &str) -> Vec<usize> {
    scene.meshes.iter().filter(|mesh| {
        let old = source.and_then(|s| s.meshes.iter().find(|m| m.part == mesh.part));
        old.map_or_else(
            || mesh.part.split(':').next() == Some(feature),
            |old| old.positions != mesh.positions || old.indices != mesh.indices || old.normals != mesh.normals,
        )
    }).flat_map(|m| m.faces.iter().map(|f| f.global_id)).collect()
}
impl Projector {
    pub fn project(&self, p: [f64; 3], rect: egui::Rect) -> egui::Pos2 {
        let q = self
            .matrix
            .project_point3(Vec3::from_array(p.map(|x| x as f32)));
        egui::pos2(
            rect.center().x + q.x * rect.width() / 2.,
            rect.center().y - q.y * rect.height() / 2.,
        )
    }
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            center: Vec3::ZERO,
            pivot: Vec3::ZERO,
            rotation: view_rotation(Vec3::new(1., -1., 1.)),
            span: 130.,
        }
    }
}
impl Camera {
    pub fn set_view(&mut self, direction: Vec3) {
        self.rotate_to(view_rotation(direction));
    }
    /// Preserve the pivot's view-space position, including after pan and zoom.
    pub fn rotate_to(&mut self, rotation: Quat) {
        let offset = self.rotation.conjugate() * (self.pivot - self.center);
        self.rotation = rotation.normalize();
        self.center = self.pivot - self.rotation * offset;
    }
    pub fn matrix(&self, rect: egui::Rect) -> Mat4 {
        let half = self.span / 2.;
        let aspect = rect.width() / rect.height().max(1.);
        let distance = self.span.max(10.) * 5.;
        let view = Mat4::look_at_rh(
            self.center + self.rotation * Vec3::Z * distance,
            self.center,
            self.rotation * Vec3::Y,
        );
        Mat4::orthographic_rh_gl(
            -half * aspect,
            half * aspect,
            -half,
            half,
            0.01,
            distance * 3.,
        ) * view
    }
    pub fn align(&mut self, p: &Plane) {
        self.rotation = Quat::from_mat3(&Mat3::from_cols(
            Vec3::from_array(p.u.map(|x| x as f32)),
            Vec3::from_array(p.v.map(|x| x as f32)),
            Vec3::from_array(p.normal.map(|x| x as f32)),
        ));
        self.center = Vec3::from_array(p.origin.map(|x| x as f32));
        self.pivot = self.center;
    }
    pub fn fit(&mut self, scene: &Scene) {
        self.fit_with_rect(
            scene,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(1.)),
        );
    }
    pub fn fit_with_rect(&mut self, scene: &Scene, rect: egui::Rect) {
        self.fit_scene_profile(scene, None, rect);
    }
    pub fn fit_with_profile(
        &mut self,
        scene: &Scene,
        sketch: &crate::sketch::ResolvedSketch,
        rect: egui::Rect,
    ) {
        self.fit_scene_profile(scene, Some(sketch), rect);
    }
    fn fit_scene_profile(
        &mut self,
        scene: &Scene,
        sketch: Option<&crate::sketch::ResolvedSketch>,
        rect: egui::Rect,
    ) {
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        let inverse = self.rotation.conjugate();
        for m in &scene.meshes {
            let [min, max] = m.bounds;
            for x in [min[0], max[0]] {
                for y in [min[1], max[1]] {
                    for z in [min[2], max[2]] {
                        let p = inverse * Vec3::new(x, y, z);
                        lo = lo.min(p);
                        hi = hi.max(p);
                    }
                }
            }
        }
        if let Some(sketch) = sketch {
            for entity in &sketch.entities {
                for p in entity.points() {
                    let world = sketch.plane.at(p);
                    let p = inverse * Vec3::from_array(world.map(|v| v as f32));
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
        }
        // Construction frames only determine Fit when no solid/profile exists.
        // Distant datum planes must not make a normal part unnecessarily small.
        if !lo.is_finite() {
            for plane in scene.planes.values() {
                for world in origin_plane_points(plane) {
                    let p = inverse * Vec3::from_array(world.map(|v| v as f32));
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
        }
        if self.fit_bounds(lo, hi, rect, 1.3) {
            self.pivot = model_center(scene, &BTreeSet::new()).unwrap_or(self.center);
        }
    }
    pub fn fit_sketch(&mut self, sketch: &crate::sketch::ResolvedSketch) {
        self.fit_sketch_with_rect(
            sketch,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(1.)),
        );
    }
    pub fn fit_sketch_with_rect(
        &mut self,
        sketch: &crate::sketch::ResolvedSketch,
        rect: egui::Rect,
    ) {
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        let inverse = self.rotation.conjugate();
        for e in &sketch.entities {
            for p in e.points() {
                let v = inverse * Vec3::from_array(sketch.plane.at(p).map(|x| x as f32));
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        if self.fit_bounds(lo, hi, rect, 1.5) {
            self.pivot = self.center;
        }
    }
    fn fit_bounds(&mut self, lo: Vec3, hi: Vec3, rect: egui::Rect, margin: f32) -> bool {
        if !lo.is_finite() || !hi.is_finite() {
            return false;
        }
        let aspect = if rect.is_finite() && rect.is_positive() {
            rect.width() / rect.height()
        } else {
            1.
        };
        let extent = hi - lo;
        self.center = self.rotation * ((lo + hi) / 2.);
        // Span is vertical; narrow viewports need additional room horizontally.
        // Keep deep geometry inside the existing orthographic near/far range too.
        self.span = (extent.y.max(extent.x / aspect).max(extent.z / 8.) * margin).max(0.02);
        true
    }
    pub fn project(&self, p: [f64; 3], rect: egui::Rect) -> egui::Pos2 {
        self.projector(rect).project(p, rect)
    }
    pub fn projector(&self, rect: egui::Rect) -> Projector {
        Projector {
            matrix: self.matrix(rect),
        }
    }
    pub fn ray(&self, pos: egui::Pos2, rect: egui::Rect) -> (Vec3, Vec3) {
        let x = (pos.x - rect.center().x) * 2. / rect.width();
        let y = (rect.center().y - pos.y) * 2. / rect.height();
        let inv = self.matrix(rect).inverse();
        let a = inv.project_point3(Vec3::new(x, y, -1.));
        let b = inv.project_point3(Vec3::new(x, y, 1.));
        (a, (b - a).normalize())
    }
    pub fn plane_point(
        &self,
        pos: egui::Pos2,
        rect: egui::Rect,
        plane: &Plane,
    ) -> Option<[f64; 2]> {
        let (o, d) = self.ray(pos, rect);
        let n = Vec3::from_array(plane.normal.map(|x| x as f32));
        let origin = Vec3::from_array(plane.origin.map(|x| x as f32));
        let dot = d.dot(n);
        if dot.abs() < 1e-6 {
            return None;
        }
        let q = o + d * ((origin - o).dot(n) / dot) - origin;
        Some([
            q.dot(Vec3::from_array(plane.u.map(|x| x as f32))) as f64,
            q.dot(Vec3::from_array(plane.v.map(|x| x as f32))) as f64,
        ])
    }
    pub fn pan(&mut self, delta: egui::Vec2, rect: egui::Rect) {
        let scale = self.span / rect.height().max(1.);
        self.center += self.rotation * Vec3::new(-delta.x * scale, delta.y * scale, 0.);
    }
    pub fn orbit(&mut self, delta: egui::Vec2) {
        self.rotate_to(
            self.rotation
                * Quat::from_scaled_axis(Vec3::new(-delta.y * 0.007, -delta.x * 0.007, 0.)),
        );
    }
    pub fn orbit_upright(&mut self, delta: egui::Vec2) {
        self.rotate_to(
            Quat::from_rotation_z(-delta.x * 0.007)
                * self.rotation
                * Quat::from_rotation_x(-delta.y * 0.007),
        );
    }
}

pub fn model_center(scene: &Scene, hidden: &BTreeSet<String>) -> Option<Vec3> {
    let mut sum = glam::DVec3::ZERO;
    let mut weight = 0.;
    for m in scene.meshes.iter().filter(|m| !hidden.contains(&m.part)) {
        let w = m.volume.abs().max(f64::EPSILON);
        sum += glam::DVec3::from_array(m.centroid) * w;
        weight += w;
    }
    (weight > 0.).then(|| (sum / weight).as_vec3())
}

pub fn automatic_pivot(
    scene: &Scene,
    camera: &Camera,
    rect: egui::Rect,
    hidden: &BTreeSet<String>,
) -> Vec3 {
    let center = model_center(scene, hidden).unwrap_or(camera.center);
    if !rect.is_positive() {
        return center;
    }
    let matrix = camera.matrix(rect);
    let fits = scene
        .meshes
        .iter()
        .filter(|m| !hidden.contains(&m.part))
        .all(|m| {
            let [lo, hi] = m.bounds.map(Vec3::from_array);
            [lo.x, hi.x].into_iter().all(|x| {
                [lo.y, hi.y].into_iter().all(|y| {
                    [lo.z, hi.z].into_iter().all(|z| {
                        let q = matrix.project_point3(Vec3::new(x, y, z));
                        q.x.abs() <= 1. && q.y.abs() <= 1.
                    })
                })
            })
        });
    if fits {
        center
    } else {
        pick(scene, camera, rect.center(), rect, hidden).map_or(center, |p| p.point)
    }
}

/// Camera basis for the six standard faces and edge/corner views, with Z up.
pub fn view_rotation(direction: Vec3) -> Quat {
    let normal = direction.normalize();
    let up = if normal.z.abs() > 0.999 {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let right = up.cross(normal).normalize();
    Quat::from_mat3(&Mat3::from_cols(right, normal.cross(right), normal)).normalize()
}

pub struct Pick {
    pub point: Vec3,
    pub part: String,
    pub face: String,
    pub global_face: usize,
    pub plane: Option<Plane>,
    pub distance: f32,
}
pub fn pick(
    scene: &Scene,
    camera: &Camera,
    pos: egui::Pos2,
    rect: egui::Rect,
    hidden: &BTreeSet<String>,
) -> Option<Pick> {
    picks(scene, camera, pos, rect, hidden).into_iter().next()
}

/// One hit per face, front to back, including obscured faces for Select other.
pub fn picks(
    scene: &Scene,
    camera: &Camera,
    pos: egui::Pos2,
    rect: egui::Rect,
    hidden: &BTreeSet<String>,
) -> Vec<Pick> {
    let (o, d) = camera.ray(pos, rect);
    let mut result = vec![];
    for mesh in &scene.meshes {
        if hidden.contains(&mesh.part) {
            continue;
        }
        for face in &mesh.faces {
            let mut best = f32::INFINITY;
            for tri in mesh.indices[face.start..face.end].as_chunks::<3>().0 {
                let a = Vec3::from_array(mesh.positions[tri[0] as usize]);
                let b = Vec3::from_array(mesh.positions[tri[1] as usize]);
                let c = Vec3::from_array(mesh.positions[tri[2] as usize]);
                if let Some(t) = ray_triangle(o, d, a, b, c)
                    && t < best
                {
                    best = t;
                }
            }
            if best.is_finite() {
                result.push(Pick {
                    point: o + d * best,
                    part: mesh.part.clone(),
                    face: face.key.clone(),
                    global_face: face.global_id,
                    plane: face.plane.clone(),
                    distance: best,
                });
            }
        }
    }
    result.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    result
}

pub struct EdgePick {
    pub part: String,
    pub key: String,
    pub distance: f32,
    pub pixels: f32,
}
pub fn pick_edges(
    scene: &Scene,
    camera: &Camera,
    pos: egui::Pos2,
    rect: egui::Rect,
    hidden: &BTreeSet<String>,
) -> Vec<EdgePick> {
    let projector = camera.projector(rect);
    let (origin, ray) = camera.ray(pos, rect);
    let mut result = vec![];
    for mesh in scene.meshes.iter().filter(|m| !hidden.contains(&m.part)) {
        for edge in mesh.edges.iter().filter(|e| !e.seam) {
            let mut best: Option<(f32, f32)> = None;
            for segment in edge.points.windows(2) {
                let a = projector.project(segment[0].map(f64::from), rect);
                let b = projector.project(segment[1].map(f64::from), rect);
                let pixels = point_segment_distance(pos, a, b);
                if pixels > 7. {
                    continue;
                }
                let v = b - a;
                let t = ((pos - a).dot(v) / v.length_sq().max(1e-8)).clamp(0., 1.);
                let point = Vec3::from_array(segment[0]).lerp(Vec3::from_array(segment[1]), t);
                let depth = (point - origin).dot(ray);
                if depth >= 0. && best.is_none_or(|(p, d)| pixels < p || (pixels == p && depth < d))
                {
                    best = Some((pixels, depth));
                }
            }
            if let Some((pixels, distance)) = best {
                result.push(EdgePick {
                    part: edge.part.clone(),
                    key: edge.key.clone(),
                    distance,
                    pixels,
                });
            }
        }
    }
    result.sort_by(|a, b| {
        a.pixels
            .total_cmp(&b.pixels)
            .then(a.distance.total_cmp(&b.distance))
    });
    result
}

fn segment_crosses_rect(a: egui::Pos2, b: egui::Pos2, rect: egui::Rect) -> bool {
    clip_segment(a, b, rect).is_some()
}
pub(crate) fn clip_segment(a: egui::Pos2, b: egui::Pos2, rect: egui::Rect) -> Option<(f64, f64)> {
    let mut low = 0.0_f64;
    let mut high = 1.0_f64;
    for (start, end, min, max) in [
        (a.x, b.x, rect.left(), rect.right()),
        (a.y, b.y, rect.top(), rect.bottom()),
    ] {
        let start = f64::from(start);
        let delta = f64::from(end) - start;
        if delta.abs() < 1e-8 {
            if start < f64::from(min) || start > f64::from(max) {
                return None;
            }
        } else {
            let p = (f64::from(min) - start) / delta;
            let q = (f64::from(max) - start) / delta;
            low = low.max(p.min(q));
            high = high.min(p.max(q));
            if low > high {
                return None;
            }
        }
    }
    Some((low, high))
}
pub(crate) fn triangle_crosses_rect(points: [egui::Pos2; 3], rect: egui::Rect) -> bool {
    if (0..3).any(|i| segment_crosses_rect(points[i], points[(i + 1) % 3], rect)) {
        return true;
    }
    let p = rect.center();
    let signs = std::array::from_fn::<_, 3, _>(|i| {
        let a = points[i];
        let b = points[(i + 1) % 3];
        (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
    });
    signs.iter().all(|s| *s >= 0.) || signs.iter().all(|s| *s <= 0.)
}
/// Left-to-right encloses; right-to-left crosses. Ctrl removes hits in the caller.
pub fn box_targets(
    scene: &Scene,
    camera: &Camera,
    viewport: egui::Rect,
    area: egui::Rect,
    crossing: bool,
    filter: crate::selection::Filter,
    hidden: &BTreeSet<String>,
) -> Vec<crate::selection::Target> {
    use crate::selection::{Filter, Target};
    let projector = camera.projector(viewport);
    let mut out = vec![];
    for mesh in scene.meshes.iter().filter(|m| !hidden.contains(&m.part)) {
        let projected: Vec<_> = mesh
            .positions
            .iter()
            .map(|p| projector.project(p.map(f64::from), viewport))
            .collect();
        if filter == Filter::Parts {
            let hit = if crossing {
                mesh.indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .any(|t| triangle_crosses_rect(t.map(|i| projected[i as usize]), area))
            } else {
                !projected.is_empty() && projected.iter().all(|p| area.contains(*p))
            };
            if hit {
                out.push(Target::Part(mesh.part.clone()));
            }
        } else if matches!(filter, Filter::All | Filter::Faces) {
            for face in &mesh.faces {
                let indices = &mesh.indices[face.start..face.end];
                let hit = if crossing {
                    indices
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .any(|t| triangle_crosses_rect(t.map(|i| projected[i as usize]), area))
                } else {
                    !indices.is_empty()
                        && indices
                            .iter()
                            .all(|i| area.contains(projected[*i as usize]))
                };
                if hit {
                    out.push(Target::Face {
                        part: mesh.part.clone(),
                        key: face.key.clone(),
                    });
                }
            }
        }
        if filter == Filter::Edges {
            for e in mesh.edges.iter().filter(|e| !e.seam) {
                let points: Vec<_> = e
                    .points
                    .iter()
                    .map(|p| projector.project(p.map(f64::from), viewport))
                    .collect();
                let hit = if crossing {
                    points
                        .windows(2)
                        .any(|p| segment_crosses_rect(p[0], p[1], area))
                } else {
                    !points.is_empty() && points.iter().all(|p| area.contains(*p))
                };
                if hit {
                    out.push(Target::Edge {
                        part: mesh.part.clone(),
                        key: e.key.clone(),
                    });
                }
            }
        }
    }
    out
}
fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let h = d.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-8 {
        return None;
    }
    let f = 1. / det;
    let s = o - a;
    let u = f * s.dot(h);
    if !(0.0..=1.).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = f * d.dot(q);
    if v < 0. || u + v > 1. {
        return None;
    }
    let t = f * e2.dot(q);
    (t > 0.).then_some(t)
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    face: f32,
    color: [f32; 3],
}

#[derive(Clone, Default)]
pub struct View {
    pub matrix: Mat4,
    pub edited: Vec<usize>,
    pub selected: Vec<usize>,
    pub hovered: Vec<usize>,
    pub selected_edges: Vec<(String, String)>,
    pub hovered_edges: Vec<(String, String)>,
    pub shaded_edges: bool,
    pub selection_color: [f32; 3],
    pub hover_color: [f32; 3],
    pub ghost_parts: BTreeSet<String>,
}

pub struct Renderer {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    line_vao: glow::VertexArray,
    line_vbo: glow::Buffer,
    tri_count: i32,
    line_count: i32,
    revision: u64,
    scene_identity: usize,
    face_ranges: BTreeMap<usize, (i32, i32)>,
    part_ranges: BTreeMap<String, (i32, i32)>,
    edge_ranges: BTreeMap<(String, String), (i32, i32)>,
}
impl Renderer {
    unsafe fn draw_ranges(gl: &glow::Context, mode: u32, ranges: impl Iterator<Item = (i32, i32)>) {
        let mut pending = None;
        for (start, count) in ranges {
            match pending {
                Some((first, len)) if first + len == start => pending = Some((first, len + count)),
                Some((first, len)) => {
                    unsafe { gl.draw_arrays(mode, first, len) };
                    pending = Some((start, count));
                }
                None => pending = Some((start, count)),
            }
        }
        if let Some((first, len)) = pending {
            unsafe { gl.draw_arrays(mode, first, len) };
        }
    }
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let vertex = gl.create_shader(glow::VERTEX_SHADER)?;
            gl.shader_source(
                vertex,
                r#"#version 330 core
layout(location=0) in vec3 position;
layout(location=1) in vec3 normal;
layout(location=2) in float face;
layout(location=3) in vec3 color;
uniform mat4 mvp;
out vec3 v_normal;
flat out float v_face;
out vec3 v_color;
void main(){gl_Position=mvp*vec4(position,1.0);v_normal=normal;v_face=face;v_color=color;}
"#,
            );
            gl.compile_shader(vertex);
            if !gl.get_shader_compile_status(vertex) {
                return Err(gl.get_shader_info_log(vertex));
            }
            let fragment = gl.create_shader(glow::FRAGMENT_SHADER)?;
            gl.shader_source(
                fragment,
                r#"#version 330 core
in vec3 v_normal;
flat in float v_face;
in vec3 v_color;
uniform int highlight;
uniform vec3 selection_color;
uniform vec3 hover_color;
uniform bool lines;
uniform float opacity;
uniform bool ghost;
out vec4 color;
void main(){
 vec3 base=ghost?vec3(0.55,0.57,0.62):v_color;
 if(highlight==1)base=selection_color;
 if(highlight==2)base=hover_color;
 if(highlight==3)base=mix(base,hover_color,0.45);
 if(lines){color=vec4(base,opacity);return;}
 vec3 n=normalize(v_normal);
 float shade=0.48+0.39*abs(dot(n,normalize(vec3(0.35,-0.5,0.9))))+0.13*abs(n.z);
 color=vec4(base*(highlight==0?shade:0.55+0.45*shade),opacity);
}
"#,
            );
            gl.compile_shader(fragment);
            if !gl.get_shader_compile_status(fragment) {
                return Err(gl.get_shader_info_log(fragment));
            }
            gl.attach_shader(program, vertex);
            gl.attach_shader(program, fragment);
            gl.link_program(program);
            gl.delete_shader(vertex);
            gl.delete_shader(fragment);
            if !gl.get_program_link_status(program) {
                return Err(gl.get_program_info_log(program));
            }
            let (vao, vbo) = Self::buffer(gl)?;
            let (line_vao, line_vbo) = Self::buffer(gl)?;
            Ok(Self {
                program,
                vao,
                vbo,
                line_vao,
                line_vbo,
                tri_count: 0,
                line_count: 0,
                revision: u64::MAX,
                scene_identity: 0,
                face_ranges: BTreeMap::new(),
                part_ranges: BTreeMap::new(),
                edge_ranges: BTreeMap::new(),
            })
        }
    }
    unsafe fn buffer(gl: &glow::Context) -> Result<(glow::VertexArray, glow::Buffer), String> {
        unsafe {
            let vao = gl.create_vertex_array()?;
            let vbo = gl.create_buffer()?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let stride = std::mem::size_of::<Vertex>() as i32;
            for (i, n, offset) in [(0, 3, 0), (1, 3, 12), (2, 1, 24), (3, 3, 28)] {
                gl.enable_vertex_attrib_array(i);
                gl.vertex_attrib_pointer_f32(i, n, glow::FLOAT, false, stride, offset);
            }
            gl.bind_vertex_array(None);
            Ok((vao, vbo))
        }
    }
    fn upload(&mut self, gl: &glow::Context, scene: &Scene, hidden: &BTreeSet<String>) {
        let mut triangles = vec![];
        let mut lines = vec![];
        self.face_ranges.clear();
        self.part_ranges.clear();
        self.edge_ranges.clear();
        let palette = [
            [0.68, 0.74, 0.81],
            [0.65, 0.73, 0.65],
            [0.72, 0.63, 0.52],
            [0.62, 0.57, 0.73],
        ];
        for (index, mesh) in scene.meshes.iter().enumerate() {
            if hidden.contains(&mesh.part) {
                continue;
            }
            let color = palette[index % palette.len()];
            let part_start = triangles.len() as i32;
            for face in &mesh.faces {
                self.face_ranges.insert(
                    face.global_id,
                    (triangles.len() as i32, (face.end - face.start) as i32),
                );
                for (triangle_index, tri) in mesh.indices[face.start..face.end]
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .enumerate()
                {
                    for (corner, &vertex) in tri.iter().enumerate() {
                        triangles.push(Vertex {
                            position: mesh.positions[vertex as usize],
                            normal: mesh.normals[face.start + triangle_index * 3 + corner],
                            face: face.global_id as f32,
                            color,
                        });
                    }
                }
            }
            self.part_ranges.insert(mesh.part.clone(), (part_start, triangles.len() as i32 - part_start));
            for edge in mesh.edges.iter().filter(|e| !e.seam) {
                self.edge_ranges.insert(
                    (edge.part.clone(), edge.key.clone()),
                    (
                        lines.len() as i32,
                        edge.points.len().saturating_sub(1) as i32 * 2,
                    ),
                );
                for points in edge.points.windows(2) {
                    for pos in points {
                        lines.push(Vertex {
                            position: *pos,
                            normal: [0.; 3],
                            face: -1.,
                            color: [0.10, 0.16, 0.21],
                        });
                    }
                }
            }
        }
        self.tri_count = triangles.len() as i32;
        self.line_count = lines.len() as i32;
        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&triangles),
                glow::STATIC_DRAW,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.line_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&lines),
                glow::STATIC_DRAW,
            );
        }
    }
    pub fn paint(
        &mut self,
        gl: &glow::Context,
        scene: &Scene,
        hidden: &BTreeSet<String>,
        revision: u64,
        view: View,
    ) {
        let identity = scene as *const Scene as usize;
        if revision != self.revision || identity != self.scene_identity {
            self.upload(gl, scene, hidden);
            self.revision = revision;
            self.scene_identity = identity;
        }
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.clear(glow::DEPTH_BUFFER_BIT);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(
                gl.get_uniform_location(self.program, "mvp").as_ref(),
                false,
                &view.matrix.to_cols_array(),
            );
            gl.uniform_3_f32_slice(gl.get_uniform_location(self.program, "selection_color").as_ref(), &view.selection_color);
            gl.uniform_3_f32_slice(gl.get_uniform_location(self.program, "hover_color").as_ref(), &view.hover_color);
            let opacity = gl.get_uniform_location(self.program, "opacity");
            let ghost = gl.get_uniform_location(self.program, "ghost");
            gl.uniform_1_f32(opacity.as_ref(), 1.);
            gl.uniform_1_i32(ghost.as_ref(), 0);
            let highlight = gl.get_uniform_location(self.program, "highlight");
            gl.uniform_1_i32(highlight.as_ref(), 0);
            gl.uniform_1_i32(gl.get_uniform_location(self.program, "lines").as_ref(), 0);
            gl.enable(glow::POLYGON_OFFSET_FILL);
            gl.polygon_offset(1., 1.);
            gl.bind_vertex_array(Some(self.vao));
            if view.ghost_parts.is_empty() {
                gl.draw_arrays(glow::TRIANGLES, 0, self.tri_count);
            } else {
                Self::draw_ranges(gl, glow::TRIANGLES, scene.meshes.iter()
                    .filter(|m| !view.ghost_parts.contains(&m.part))
                    .filter_map(|m| self.part_ranges.get(&m.part).copied()));
            }
            for (ids, color) in [(&view.edited, 3), (&view.hovered, 2), (&view.selected, 1)] {
                gl.uniform_1_i32(highlight.as_ref(), color);
                Self::draw_ranges(
                    gl,
                    glow::TRIANGLES,
                    ids.iter()
                        .filter_map(|id| self.face_ranges.get(id).copied()),
                );
            }
            if !view.ghost_parts.is_empty() {
                let mut ghosts: Vec<_> = scene.meshes.iter()
                    .filter(|m| view.ghost_parts.contains(&m.part)).collect();
                ghosts.sort_by(|a,b| {
                    let depth = |m: &crate::engine::MeshView| view.matrix.project_point3(Vec3::from_array(m.centroid.map(|v| v as f32))).z;
                    depth(b).total_cmp(&depth(a))
                });
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                gl.depth_mask(false);
                gl.uniform_1_i32(highlight.as_ref(), 0);
                gl.uniform_1_i32(ghost.as_ref(), 1);
                gl.uniform_1_f32(opacity.as_ref(), 0.18);
                Self::draw_ranges(gl, glow::TRIANGLES, ghosts.into_iter()
                    .filter_map(|m| self.part_ranges.get(&m.part).copied()));
                gl.uniform_1_f32(opacity.as_ref(), 1.);
                gl.uniform_1_i32(ghost.as_ref(), 0);
                gl.depth_mask(true);
                gl.disable(glow::BLEND);
            }
            gl.disable(glow::POLYGON_OFFSET_FILL);
            gl.uniform_1_i32(gl.get_uniform_location(self.program, "lines").as_ref(), 1);
            gl.bind_vertex_array(Some(self.line_vao));
            gl.uniform_1_i32(highlight.as_ref(), 0);
            if view.shaded_edges {
                gl.draw_arrays(glow::LINES, 0, self.line_count);
            }
            gl.line_width(2.5);
            for (keys, color) in [(&view.hovered_edges, 2), (&view.selected_edges, 1)] {
                gl.uniform_1_i32(highlight.as_ref(), color);
                Self::draw_ranges(
                    gl,
                    glow::LINES,
                    keys.iter()
                        .filter_map(|key| self.edge_ranges.get(key).copied()),
                );
            }
            gl.line_width(1.);
            gl.bind_vertex_array(None);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::BLEND);
        }
    }
    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.program);
            gl.delete_buffer(self.vbo);
            gl.delete_buffer(self.line_vbo);
            gl.delete_vertex_array(self.vao);
            gl.delete_vertex_array(self.line_vao);
        }
    }
}

pub fn paint_callback(
    rect: egui::Rect,
    renderer: Arc<Mutex<Renderer>>,
    scene: Arc<Scene>,
    hidden: BTreeSet<String>,
    revision: u64,
    view: View,
) -> egui::PaintCallback {
    egui::PaintCallback {
        rect,
        callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |_info, painter| {
            if let Ok(mut render) = renderer.lock() {
                render.paint(painter.gl(), &scene, &hidden, revision, view.clone());
            }
        })),
    }
}

pub fn point_segment_distance(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let v = b - a;
    let t = ((p - a).dot(v) / v.length_sq().max(1e-8)).clamp(0., 1.);
    p.distance(a + v * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_drag_uses_button_event_order_and_keeps_the_release_frame() {
        fn frame(ctx: &egui::Context, button: egui::PointerButton, events: Vec<egui::Event>) -> Option<DragMotion> {
            let mut motion = None;
            let _ = ctx.run(egui::RawInput { events, ..Default::default() }, |ctx| {
                motion = ctx.input(|i| navigation_drag(i, button));
            });
            motion
        }
        for button in [egui::PointerButton::Primary, egui::PointerButton::Secondary, egui::PointerButton::Middle] {
            let ctx = egui::Context::default();
            let point = egui::pos2;
            let moved = egui::Event::PointerMoved;
            let pressed = |pos, pressed| egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE };
            assert!(frame(&ctx, button, vec![moved(point(10.,20.))]).is_none());
            let drag = frame(&ctx, button, vec![moved(point(600.,700.)), pressed(point(600.,700.),true), moved(point(650.,675.))]).unwrap();
            assert_eq!(drag.start, Some(point(600.,700.)));
            assert_eq!(drag.delta, egui::vec2(50.,-25.), "Travel before pressing must not rotate or pan");
            let release = frame(&ctx, button, vec![moved(point(660.,650.)), pressed(point(660.,650.),false), moved(point(900.,900.))]).unwrap();
            assert_eq!(release.delta, egui::vec2(10.,-25.), "Keep the last drag segment; exclude travel after release");
            assert!(frame(&ctx, button, vec![moved(point(20.,30.))]).is_none());
            let click = frame(&ctx, button, vec![moved(point(800.,800.)), pressed(point(800.,800.),true), pressed(point(800.,800.),false)]).unwrap();
            assert_eq!(click.delta, egui::Vec2::ZERO);
        }
    }
    #[test]
    fn fit_datum_only_scene_and_keep_distant_datums_out_of_part_fit() {
        let mut scene = Scene::default();
        scene.planes.insert("f1".into(), Plane::named("Top", 250.));
        let mut camera = Camera::default();
        camera.align(&Plane::named("Top", 0.));
        camera.fit(&scene);
        assert!((camera.center.z - 250.).abs() < 1e-4);
        assert!((camera.span - 130.).abs() < 1e-4);
        let registry = crate::plugins::Registry::builtin().unwrap();
        let mut runtime = crate::engine::Runtime::new(registry);
        let part = runtime.evaluate(&crate::model::demo()).unwrap().scene;
        let mut distant = (*part).clone();
        distant.planes.insert("datum".into(), Plane::named("Top", 10000.));
        let mut expected = Camera::default(); expected.fit(&part);
        let mut actual = Camera::default(); actual.fit(&distant);
        assert_eq!(actual.center, expected.center);
        assert_eq!(actual.span, expected.span);
    }
    #[test]
    fn origin_planes_are_one_hundred_millimeters_at_every_zoom() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 800.));
        for name in ["Top", "Front", "Right"] {
            let plane = Plane::named(name, 0.);
            let corners = origin_plane_points(&plane);
            for i in 0..4 {
                let a = glam::DVec3::from_array(corners[i]);
                let b = glam::DVec3::from_array(corners[(i + 1) % 4]);
                assert!((a.distance(b) - 100.).abs() < 1e-10);
            }
            let mut camera = Camera::default();
            camera.align(&plane);
            for span in [20., 130., 1000.] {
                camera.span = span;
                let edge = camera.project(corners[0], rect).distance(camera.project(corners[1], rect));
                assert!((edge - 100. * rect.height() / span).abs() < 0.002);
                for (distance, inside) in [(49., true), (51., false)] {
                    let screen = camera.project(plane.at([distance, 0.]), rect);
                    let hit = camera.plane_point(screen, rect, &plane).unwrap();
                    assert_eq!(hit[0].abs() <= ORIGIN_PLANE_HALF_SIZE, inside);
                }
            }
        }
    }
    #[test]
    fn projected_plane_text_keeps_readable_winding_on_both_sides() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 800.));
                for name in ["Top", "Front", "Right"] {
                    let plane = Plane::named(name, 0.);
                    let normal = Vec3::from_array(plane.normal.map(|v| v as f32));
                    for sign in [-1., 1.] {
                        let mut camera = Camera::default();
                        camera.set_view(normal * sign);
                        for roll in [0., 0.65, 2.5] {
                            camera.rotate_to(camera.rotation * Quat::from_rotation_z(roll));
                            let (u, v) = plane_label_axes(&camera, &plane).unwrap();
                            let mesh = projected_text(ui, name, egui::FontId::proportional(12.), egui::Color32::WHITE, |p| {
                                camera.project((u * p.x - v * p.y).to_array().map(f64::from), rect)
                            });
                            assert!(!mesh.vertices.is_empty());
                            assert!(mesh.vertices.iter().all(|v| v.uv.x >= 0. && v.uv.x <= 1. && v.uv.y >= 0. && v.uv.y <= 1.));
                            for indices in mesh.indices.as_chunks::<3>().0 {
                                let p = indices.map(|i| mesh.vertices[i as usize].pos);
                                let a = p[1] - p[0];
                                let b = p[2] - p[0];
                                assert!(a.x * b.y - a.y * b.x >= -0.001, "Plane text was mirrored");
                            }
                        }
                    }
                }
            });
        });
    }
    #[test]
    fn preview_highlight_tracks_old_history_without_tinting_unchanged_parts() {
        let mut document = crate::model::demo();
        let registry = crate::plugins::Registry::builtin().unwrap();
        let mut runtime = crate::engine::Runtime::new(crate::plugins::Registry::builtin().unwrap());
        document.rollback = Some(3);
        let before_cut = runtime.evaluate(&document).unwrap().scene;
        document.rollback = None;
        let after_cut = runtime.evaluate(&document).unwrap().scene;
        let all_faces: Vec<_> = after_cut.meshes[0].faces.iter().map(|f| f.global_id).collect();
        assert_eq!(edited_face_ids(&after_cut, Some(&before_cut), "f4"), all_faces);
        assert!(edited_face_ids(&after_cut, Some(&after_cut), "f4").is_empty());
        let mut copy = registry.create("transform", &mut document).unwrap();
        let id = copy.id.clone();
        copy.params["copy"] = serde_json::json!(true);
        copy.params["x"] = serde_json::json!("100");
        document.features.push(copy);
        let preview = runtime.evaluate(&document).unwrap().scene;
        assert!(preview.error.is_none(), "{:?}", preview.error);
        let expected: Vec<_> = preview.meshes.iter().filter(|m| m.part.starts_with(&format!("{id}:")))
            .flat_map(|m| m.faces.iter().map(|f| f.global_id)).collect();
        assert!(!expected.is_empty());
        assert_eq!(edited_face_ids(&preview, Some(&after_cut), &id), expected);
    }
    #[test]
    fn face_hits_are_depth_ordered_and_hidden_parts_are_not_pickable() {
        let mut runtime = crate::engine::Runtime::new(crate::plugins::Registry::builtin().unwrap());
        let scene = runtime.evaluate(&crate::model::demo()).unwrap().scene;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 800.));
        let mut camera = Camera::default();
        camera.align(&Plane::named("Top", 0.));
        camera.fit_with_rect(&scene, rect);
        let pos = camera.project([0., 0., 8.], rect);
        let hits = picks(&scene, &camera, pos, rect, &BTreeSet::new());
        assert!(hits.len() >= 2);
        assert!(hits.windows(2).all(|h| h[0].distance <= h[1].distance));
        assert!((hits[0].point.z - 8.).abs() < 1e-3);
        let hidden = BTreeSet::from([hits[0].part.clone()]);
        assert!(picks(&scene, &camera, pos, rect, &hidden).is_empty());
        let mut display_only = (*scene).clone();
        let edge = display_only.meshes[0]
            .edges
            .iter_mut()
            .find(|e| !e.seam)
            .unwrap();
        edge.key = "untracked:curved:display-only".into();
        let point = edge.points[edge.points.len() / 2];
        let cursor = camera.project(point.map(f64::from), rect);
        assert!(
            pick_edges(&display_only, &camera, cursor, rect, &BTreeSet::new())
                .iter()
                .any(|e| e.key == "untracked:curved:display-only")
        );
        let area = egui::Rect::from_two_pos(
            camera.project([-41., -26., 0.], rect),
            camera.project([41., 26., 0.], rect),
        );
        let parts = box_targets(
            &scene,
            &camera,
            rect,
            area,
            false,
            crate::selection::Filter::Parts,
            &BTreeSet::new(),
        );
        assert_eq!(parts.len(), 1);
        assert!(
            box_targets(
                &scene,
                &camera,
                rect,
                area,
                true,
                crate::selection::Filter::Parts,
                &hidden
            )
            .is_empty()
        );
        let mut profile = scene.sketches.values().next().unwrap().clone();
        profile.plane.origin = [200., 0., 0.];
        camera.fit_with_profile(&scene, &profile, rect);
        for point in profile.entities.iter().flat_map(|e| e.points()) {
            assert!(
                rect.shrink(1.)
                    .contains(camera.project(profile.plane.at(point), rect))
            );
        }
        for mesh in &scene.meshes {
            for point in &mesh.positions {
                assert!(
                    rect.shrink(1.)
                        .contains(camera.project(point.map(f64::from), rect))
                );
            }
        }
    }
    #[test]
    fn crossing_box_finds_segments_and_triangles_without_vertices_inside() {
        let area = egui::Rect::from_min_max(egui::pos2(-1., -1.), egui::pos2(1., 1.));
        assert!(segment_crosses_rect(
            egui::pos2(-5., 0.),
            egui::pos2(5., 0.),
            area
        ));
        assert!(!segment_crosses_rect(
            egui::pos2(-5., 4.),
            egui::pos2(5., 4.),
            area
        ));
        assert!(triangle_crosses_rect(
            [
                egui::pos2(-8., -5.),
                egui::pos2(8., -5.),
                egui::pos2(0., 8.)
            ],
            area
        ));
    }
    #[test]
    fn choosing_a_pivot_does_not_jump_and_orbit_keeps_it_fixed_after_pan_and_zoom() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 800.));
        let mut c = Camera::default();
        c.pan(egui::vec2(190., -75.), rect);
        c.span *= 0.4;
        let point = [13., -7., 18.];
        let screen = c.project(point, rect);
        c.pivot = Vec3::from_array(point.map(|x| x as f32));
        assert!(c.project(point, rect).distance(screen) < 0.001);
        let radius = (c.center - c.pivot).length();
        for delta in [egui::vec2(110., 35.), egui::vec2(-48., 82.)] {
            c.orbit(delta);
            assert!(c.project(point, rect).distance(screen) < 0.002);
            assert!(((c.center - c.pivot).length() - radius).abs() < 0.001);
        }
        c.set_view(Vec3::NEG_Y);
        assert!(c.project(point, rect).distance(screen) < 0.002);
    }
    #[test]
    fn free_orbit_from_top_tilts_rather_than_rolling_about_world_z() {
        let mut c = Camera::default();
        c.set_view(Vec3::Z);
        c.orbit(egui::vec2(100., 0.));
        assert!((c.rotation * Vec3::Z).dot(Vec3::Z) < 0.8);
        assert!((c.rotation * Vec3::Y - Vec3::Y).length() < 1e-5);
    }
    #[test]
    fn orthographic_scale_and_parallel_pick_rays_do_not_depend_on_depth() {
        let camera = Camera::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 800.));
        let rotation = camera.rotation;
        let project = |v: Vec3| camera.project((rotation * v).to_array().map(f64::from), rect);
        for depth in [-400., 0., 400.] {
            let a = project(Vec3::new(-10., 0., depth));
            let b = project(Vec3::new(10., 0., depth));
            assert!((a.distance(b) - 20. * rect.height() / camera.span).abs() < 0.002);
        }
        let (a, ad) = camera.ray(egui::pos2(250., 230.), rect);
        let (b, bd) = camera.ray(egui::pos2(900., 650.), rect);
        assert!((ad - bd).length() < 1e-5);
        assert!((a - b).length() > 1.);
        let plane = Plane::named("Top", 0.);
        let mut top = camera.clone();
        top.align(&plane);
        let p = [17., -12.];
        let screen = top.project(plane.at(p), rect);
        let back = top.plane_point(screen, rect, &plane).unwrap();
        assert!((back[0] - p[0]).abs() < 1e-4 && (back[1] - p[1]).abs() < 1e-4);
    }
}
