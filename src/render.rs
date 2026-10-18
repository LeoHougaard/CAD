use crate::{engine::Scene, model::Plane};
use eframe::{
    egui,
    glow::{self, HasContext},
};
use glam::{Mat3, Mat4, Quat, Vec3};
use std::{
    collections::BTreeSet,
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
    let (o, d) = camera.ray(pos, rect);
    let mut result = None;
    let mut best = f32::INFINITY;
    for mesh in &scene.meshes {
        if hidden.contains(&mesh.part) {
            continue;
        }
        for face in &mesh.faces {
            for tri in mesh.indices[face.start..face.end].as_chunks::<3>().0 {
                let a = Vec3::from_array(mesh.positions[tri[0] as usize]);
                let b = Vec3::from_array(mesh.positions[tri[1] as usize]);
                let c = Vec3::from_array(mesh.positions[tri[2] as usize]);
                if let Some(t) = ray_triangle(o, d, a, b, c)
                    && t < best
                {
                    best = t;
                    result = Some(Pick {
                        point: o + d * t,
                        part: mesh.part.clone(),
                        face: face.key.clone(),
                        global_face: face.global_id,
                        plane: face.plane.clone(),
                        distance: t,
                    });
                }
            }
        }
    }
    result
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

#[derive(Clone, Copy)]
pub struct View {
    pub matrix: Mat4,
    pub selected: usize,
    pub shaded_edges: bool,
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
}
impl Renderer {
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
uniform float selected;
uniform bool lines;
out vec4 color;
void main(){
 if(lines){color=vec4(v_color,1.0);return;}
 vec3 n=normalize(v_normal);
 float shade=0.48+0.39*abs(dot(n,normalize(vec3(0.35,-0.5,0.9))))+0.13*abs(n.z);
 vec3 base=v_color;
 if(abs(v_face-selected)<0.2)base=vec3(0.26,0.77,0.88);
 color=vec4(base*shade,1.0);
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
            for face in &mesh.faces {
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
            for edge in mesh.edges.iter().filter(|e| !e.seam) {
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
        if revision != self.revision {
            self.upload(gl, scene, hidden);
            self.revision = revision;
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
            gl.uniform_1_f32(
                gl.get_uniform_location(self.program, "selected").as_ref(),
                view.selected as f32,
            );
            gl.uniform_1_i32(gl.get_uniform_location(self.program, "lines").as_ref(), 0);
            gl.enable(glow::POLYGON_OFFSET_FILL);
            gl.polygon_offset(1., 1.);
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, self.tri_count);
            gl.disable(glow::POLYGON_OFFSET_FILL);
            gl.uniform_1_i32(gl.get_uniform_location(self.program, "lines").as_ref(), 1);
            gl.bind_vertex_array(Some(self.line_vao));
            if view.shaded_edges {
                gl.draw_arrays(glow::LINES, 0, self.line_count);
            }
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
                render.paint(painter.gl(), &scene, &hidden, revision, view);
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
