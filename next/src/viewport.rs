//! Presentation of native display data. This module never owns modeling state.
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use glam::{Quat, Vec3};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
mod gl_renderer;
mod navigation;
mod references;
pub use navigation::{DisplayMode, SelectionMode, StandardView};
mod sketch;
mod sketch_curves;
pub use crate::sketch_commands::SketchElement;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub object: String,
    pub subelement: String,
}

struct Triangle {
    object: Arc<str>,
    points: [Vec3; 3],
    normals: [Vec3; 3],
    face: String,
}
struct Edge {
    object: Arc<str>,
    points: Vec<Vec3>,
    name: String,
}
struct Vertex {
    object: Arc<str>,
    isolated: bool,
    name: String,
    point: Vec3,
}
struct SketchCurve {
    native: sketch_curves::NativeCurve,
    handles: Vec<(SketchElement, Vec3)>,
    points: Vec<Vec3>,
}
struct ProjectedTriangle {
    points: [Pos2; 3],
    depths: [f32; 3],
    face: usize,
    colors: [Color32; 3],
}

pub struct Viewport {
    scene_revisions: BTreeMap<String, Option<u64>>,
    renderer: Option<Arc<gl_renderer::Renderer>>,
    triangles: Vec<Triangle>,
    edges: Vec<Edge>,
    vertices: Vec<Vertex>,
    topology_references: BTreeMap<String, Vec<String>>,
    references: Vec<references::Reference>,
    reference_revisions: BTreeMap<String, Option<u64>>,
    pub display_mode: DisplayMode,
    pub selection_mode: SelectionMode,
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
    pub selections: Vec<Selection>,
    pub sketch_selection: Vec<SketchElement>,
    pub sketch_select_enabled: bool,
    /// Pick native solid subelements during the external-reference tool.
    pub solid_select_in_sketch: bool,
    dimension_hits: Vec<sketch::DimensionHit>,
}

impl Default for Viewport {
    fn default() -> Self {
        let mut result = Self {
            scene_revisions: BTreeMap::new(),
            renderer: None,
            triangles: vec![],
            edges: vec![],
            vertices: vec![],
            topology_references: BTreeMap::new(),
            references: vec![],
            reference_revisions: BTreeMap::new(),
            display_mode: DisplayMode::Edges,
            selection_mode: SelectionMode::All,
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
            selections: vec![],
            sketch_selection: vec![],
            sketch_select_enabled: true,
            solid_select_in_sketch: false,
            dimension_hits: vec![],
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
    pub fn clear_selection(&mut self) {
        self.selected = None;
        self.selections.clear();
    }
    fn selected_element(&self, object: &str, subelement: &str) -> bool {
        self.selections
            .iter()
            .chain(self.selected.iter())
            .any(|selection| selection.object == object && selection.subelement == subelement)
    }
    fn select_native_element(&mut self, hit: Option<Selection>, toggle: bool) {
        let Some(hit) = hit else {
            self.clear_selection();
            return;
        };
        if toggle {
            if let Some(index) = self
                .selections
                .iter()
                .position(|selection| selection == &hit)
            {
                self.selections.remove(index);
            } else {
                self.selections.push(hit);
            }
        } else {
            self.selections = vec![hit];
        }
        self.selected = self.selections.last().cloned();
    }
    pub fn set_scene(&mut self, mesh: Value, topology: Value) {
        self.set_scenes(vec![(mesh, topology)]);
    }
    /// Display every supplied native object in one depth-tested scene. Omitted objects
    /// are neither displayed nor pickable. Revisions scope all native element names.
    pub fn set_scenes(&mut self, scenes: Vec<(Value, Value)>) {
        let revisions: BTreeMap<String, Option<u64>> = scenes
            .iter()
            .filter_map(|(mesh, _)| {
                let object = mesh["object"].as_str()?;
                (!object.is_empty()).then(|| (object.to_owned(), mesh["revision"].as_u64()))
            })
            .collect();
        let valid = |selection: &Selection| {
            revisions
                .get(&selection.object)
                .or_else(|| self.reference_revisions.get(&selection.object))
                .is_some_and(|revision| {
                    self.scene_revisions
                        .get(&selection.object)
                        .or_else(|| self.reference_revisions.get(&selection.object))
                        == Some(revision)
                })
        };
        self.selections.retain(&valid);
        if self
            .selected
            .as_ref()
            .is_some_and(|selection| !valid(selection))
        {
            self.selected = self.selections.last().cloned();
        }
        self.scene_revisions = revisions;
        self.triangles.clear();
        self.edges.clear();
        self.vertices.clear();
        self.topology_references.clear();
        for (mesh, topology) in scenes {
            self.append_scene(mesh, topology);
        }
    }
    fn append_scene(&mut self, mesh: Value, topology: Value) {
        let Some(object) = mesh["object"].as_str().filter(|object| !object.is_empty()) else {
            return;
        };
        let references = ["faces", "edges", "vertices"]
            .into_iter()
            .flat_map(|kind| {
                topology[kind]
                    .as_array()
                    .or_else(|| {
                        (kind == "faces")
                            .then(|| mesh["faces"].as_array())
                            .flatten()
                    })
                    .into_iter()
                    .flatten()
                    .filter_map(|element| element["subelement"].as_str())
                    .map(str::to_owned)
            })
            .collect();
        self.topology_references
            .insert(object.to_owned(), references);
        let object: Arc<str> = Arc::from(object);
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
                        object: Arc::clone(&object),
                        points: [*a, *b, *c],
                        normals: triangle_normals,
                        face: name.to_owned(),
                    });
                }
            }
        }
        let (origin, rotation) = placement(&topology["world_from_shape"]);
        let isolated = mesh["triangles"].as_array().is_none_or(Vec::is_empty)
            && topology["edges"].as_array().is_none_or(Vec::is_empty);
        self.vertices.extend(
            topology["vertices"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|vertex| {
                    let point = vertex.get("point").or_else(|| vertex.get("position"))?;
                    let point = origin + rotation * vector(point);
                    point.is_finite().then(|| Vertex {
                        object: Arc::clone(&object),
                        isolated,
                        name: vertex["subelement"].as_str().unwrap_or("").to_owned(),
                        point,
                    })
                }),
        );
        self.edges.extend(
            topology["edges"]
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
                        object: Arc::clone(&object),
                        name: edge["subelement"].as_str()?.to_owned(),
                        points: points
                            .iter()
                            .map(|p| origin + rotation * vector(p))
                            .collect(),
                    })
                }),
        );
    }

    pub fn set_sketch(&mut self, sketch: Option<Value>) {
        if self.sketch.as_ref().and_then(|s| s["name"].as_str())
            != sketch.as_ref().and_then(|s| s["name"].as_str())
        {
            self.clear_sketch_selection();
        }
        self.dimension_hits.clear();
        let external_references: BTreeMap<_, _> = self
            .curves
            .iter()
            .filter(|curve| curve.native.external)
            .map(|curve| (curve.native.index, curve.native.reference.clone()))
            .collect();
        self.curves.clear();
        if let Some(entry) = &sketch {
            let p = if entry["global_placement"].is_object() {
                &entry["global_placement"]
            } else {
                &entry["properties"]["Placement"]
            };
            (self.sketch_origin, self.sketch_rotation) = placement(p);
            let geometry = entry["sketch"]["geometry"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(sketch_curves::NativeCurve::from_snapshot);
            let external = entry["sketch"]["externals"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(sketch_curves::NativeCurve::from_external);
            for native in geometry.chain(external) {
                let world = |p: [f64; 3]| {
                    self.sketch_origin
                        + self.sketch_rotation * Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32)
                };
                self.curves.push(SketchCurve {
                    handles: native
                        .handles
                        .iter()
                        .map(|handle| (handle.id, world(handle.position)))
                        .collect(),
                    points: native.points.iter().map(|point| world(*point)).collect(),
                    native,
                });
            }
        }
        self.sketch = sketch;
        self.sketch_selection.retain(|element| {
            if element.index == -1 {
                return self.sketch.is_some() && element.point == 1;
            }
            if element.index <= -3
                && external_references
                    .get(&element.index)
                    .is_some_and(|previous| {
                        !self.curves.iter().any(|curve| {
                            curve.native.index == element.index
                                && &curve.native.reference == previous
                        })
                    })
            {
                return false;
            }
            self.curves.iter().any(|curve| {
                (curve.native.index == element.index && element.point == 0)
                    || curve.handles.iter().any(|(id, _)| *id == *element)
            })
        });
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
            .chain(
                self.edges
                    .iter()
                    .flat_map(|edge| edge.points.iter().copied()),
            )
            .chain(self.vertices.iter().map(|vertex| vertex.point))
            .chain(
                self.references
                    .iter()
                    .flat_map(|reference| reference.bounds_points()),
            )
            .chain(self.curves.iter().flat_map(|c| c.points.iter().copied()))
            .filter(|point| point.is_finite())
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
        if (max - min).length_squared() < 1e-12 {
            self.scale = 8.;
            return;
        }
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
                let selected = self.selected_element(&t.object, &t.face);
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
    fn edge_visible(
        &self,
        edge: &Edge,
        depth: f32,
        front: Option<(usize, f32)>,
        triangles: &[ProjectedTriangle],
    ) -> bool {
        self.point_visible(&edge.object, depth, front, triangles)
    }
    fn point_visible(
        &self,
        object: &str,
        depth: f32,
        front: Option<(usize, f32)>,
        triangles: &[ProjectedTriangle],
    ) -> bool {
        if self.display_mode == DisplayMode::Wireframe {
            return true;
        }

        let Some((index, front_depth)) = front else {
            return true;
        };
        let same_object = self.triangles[triangles[index].face].object.as_ref() == object;
        // Tessellation can put a native edge slightly behind its own display surface.
        // Applying that screen-space allowance across Bodies would expose hidden edges.
        let tolerance = if same_object {
            0.8 / self.scale
        } else {
            16. * f32::EPSILON * front_depth.abs().max(depth.abs()).max(1.)
        };
        depth >= front_depth - tolerance
    }
    fn pick(&self, pos: Pos2, triangles: &[ProjectedTriangle]) -> Option<Selection> {
        let front = front_triangle(pos, triangles);
        if matches!(
            self.selection_mode,
            SelectionMode::Vertices | SelectionMode::All
        ) {
            let hit = self
                .vertices
                .iter()
                .filter(|vertex| self.selection_mode == SelectionMode::Vertices || vertex.isolated)
                .filter_map(|vertex| {
                    let (point, depth) = self.project(vertex.point);
                    let distance = point.distance(pos);
                    (distance <= 6. && self.point_visible(&vertex.object, depth, front, triangles))
                        .then_some((vertex, distance, depth))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.2.total_cmp(&a.2)));
            if let Some((vertex, _, _)) = hit {
                return Some(Selection {
                    object: vertex.object.to_string(),
                    subelement: vertex.name.clone(),
                });
            }
        }
        let mut edge_hit: Option<(Selection, f32, f32)> = None;
        for edge in self.edges.iter().filter(|_| {
            matches!(
                self.selection_mode,
                SelectionMode::All | SelectionMode::Edges
            )
        }) {
            for line in edge.points.windows(2) {
                let a = self.project(line[0]);
                let b = self.project(line[1]);
                let (distance, t) = segment_distance(pos, a.0, b.0);
                let depth = a.1 + t * (b.1 - a.1);
                if distance < 6.
                    && self.edge_visible(edge, depth, front, triangles)
                    && edge_hit
                        .as_ref()
                        .is_none_or(|(_, d, z)| distance < *d || (distance == *d && depth > *z))
                {
                    edge_hit = Some((
                        Selection {
                            object: edge.object.to_string(),
                            subelement: edge.name.clone(),
                        },
                        distance,
                        depth,
                    ));
                }
            }
        }
        edge_hit
            .map(|(selection, _, _)| selection)
            .or_else(|| self.pick_reference(pos, front, triangles))
            .or_else(|| {
                if !matches!(
                    self.selection_mode,
                    SelectionMode::All | SelectionMode::Faces
                ) {
                    return None;
                }
                front.map(|(index, _)| {
                    let triangle = &self.triangles[triangles[index].face];
                    Selection {
                        object: triangle.object.to_string(),
                        subelement: triangle.face.clone(),
                    }
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
            for triangle in triangles.iter().filter(|triangle| {
                let source = &self.triangles[triangle.face];
                self.display_mode != DisplayMode::Wireframe
                    || self.selected_element(&source.object, &source.face)
            }) {
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
            let selected = self.selected_element(&edge.object, &edge.name);
            if self.display_mode == DisplayMode::Shaded && !selected {
                continue;
            }
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
                    if self.edge_visible(edge, z, depth_index.front(pos, &triangles), &triangles) {
                        painter.line_segment([a.0.lerp(b.0, t0), a.0.lerp(b.0, t1)], stroke);
                    }
                }
            }
        }
        for vertex in &self.vertices {
            let selected = self.selected_element(&vertex.object, &vertex.name);
            if self.selection_mode != SelectionMode::Vertices
                && !selected
                && !(self.selection_mode == SelectionMode::All && vertex.isolated)
            {
                continue;
            }
            let (point, depth) = self.project(vertex.point);
            if self.point_visible(
                &vertex.object,
                depth,
                depth_index.front(point, &triangles),
                &triangles,
            ) {
                painter.circle_filled(
                    point,
                    if selected { 4. } else { 2.5 },
                    if selected {
                        Color32::from_rgb(95, 221, 207)
                    } else {
                        Color32::from_rgb(174, 187, 193)
                    },
                );
            }
        }
        self.draw_references(&painter, &triangles, &depth_index);
        self.draw_sketch(&painter);
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
            if self.sketch.is_some() && !self.solid_select_in_sketch {
                if self.sketch_select_enabled && self.dimension_hit(pos).is_none() {
                    let hit = self.sketch_hit(pos);
                    let toggle =
                        ui.input(|i| i.modifiers.ctrl || i.modifiers.shift || i.modifiers.command);
                    self.select_sketch_element(hit, toggle);
                    selection = hit.map(|element| Selection {
                        object: self
                            .sketch
                            .as_ref()
                            .and_then(|s| s["name"].as_str())
                            .unwrap_or("")
                            .to_owned(),
                        subelement: format!("Geometry{}", element.index),
                    });
                }
                self.selected = selection.clone();
            } else {
                let hit = self.pick(pos, &triangles);
                let toggle = ui.input(|input| {
                    input.modifiers.ctrl || input.modifiers.shift || input.modifiers.command
                });
                self.select_native_element(hit, toggle);
                selection = self.selected.clone();
            }
        }
        self.response = Some(response);
        selection
    }
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
    fn front(&self, p: Pos2, triangles: &[ProjectedTriangle]) -> Option<(usize, f32)> {
        if !self.rect.contains(p) {
            return None;
        }
        let (x, y) = self.cell(p);
        self.cells[y * self.columns + x]
            .iter()
            .filter_map(|&i| triangle_depth(p, &triangles[i]).map(|depth| (i, depth)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
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
    fn external_tool_picks_native_solid_support_while_regular_sketch_click_picks_solver_geometry() {
        let mut viewport = Viewport::default();
        let (mesh, topology) = native_scene("Pad", 1, [0., 0., 5.], Quat::IDENTITY);
        viewport.set_scene(mesh, topology);
        viewport.set_sketch(Some(json!({"name":"Sketch","sketch":{"geometry":[
            {"index":0,"kind":"line","start":[0,2,0],"end":[6,2,0]}]}})));
        viewport.align_sketch();
        viewport.scale = 10.;
        let context = egui::Context::default();
        let frame = |viewport: &mut Viewport, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                events,
                ..Default::default()
            };
            let _ = context.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    viewport.ui(ui);
                });
            });
        };
        frame(&mut viewport, vec![]);
        let point = viewport.sketch_screen_point([2., 2., 0.]);
        for solid in [false, true] {
            viewport.solid_select_in_sketch = solid;
            frame(
                &mut viewport,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
            );
            frame(
                &mut viewport,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
            assert_eq!(
                viewport.selected,
                Some(Selection {
                    object: if solid { "Pad" } else { "Sketch" }.into(),
                    subelement: if solid { "Face1" } else { "Geometry0" }.into()
                })
            );
        }
        assert_eq!(
            viewport.sketch_selection,
            vec![SketchElement { index: 0, point: 0 }]
        );
    }
    fn native_scene(
        object: &str,
        revision: u64,
        position: [f32; 3],
        rotation: Quat,
    ) -> (Value, Value) {
        let transform = json!({"position":position,"rotation":rotation.to_array()});
        (
            json!({"object":object,"revision":revision,"world_from_shape":transform,
            "positions":[[0,0,0],[10,0,0],[0,10,0]],"normals":[[0,0,1],[0,0,1],[0,0,1]],
            "triangles":[[0,1,2]],"faces":[{"subelement":"Face1","first_triangle":0,"triangle_count":1}]}),
            json!({"object":object,"revision":revision,"world_from_shape":transform,
            "edges":[{"subelement":"Edge1","points":[[0,1,0],[8,1,0]]}]}),
        )
    }
    #[test]
    fn overlapping_objects_preserve_identity_and_occlude_faces_and_edges() {
        let mut viewport = Viewport::default();
        viewport.top();
        viewport.scale = 10.;
        viewport.set_scenes(vec![
            native_scene("Rear", 1, [0., 0., 0.], Quat::IDENTITY),
            native_scene("Front", 1, [0., 0., 5.], Quat::IDENTITY),
        ]);
        let face = viewport
            .pick(
                viewport.project(Vec3::new(2., 2., 0.)).0,
                &viewport.projected(),
            )
            .unwrap();
        assert_eq!(
            face,
            Selection {
                object: "Front".into(),
                subelement: "Face1".into()
            }
        );
        let edge = viewport
            .pick(
                viewport.project(Vec3::new(2., 1., 0.)).0,
                &viewport.projected(),
            )
            .unwrap();
        assert_eq!(
            edge,
            Selection {
                object: "Front".into(),
                subelement: "Edge1".into()
            }
        );
        // A rear edge remains hidden even when the nearer object has no matching edge.
        viewport.edges.retain(|edge| edge.object.as_ref() == "Rear");
        let hit = viewport
            .pick(
                viewport.project(Vec3::new(2., 1., 0.)).0,
                &viewport.projected(),
            )
            .unwrap();
        assert_eq!(hit, face);
    }
    #[test]
    fn a_nearby_hidden_body_edge_cannot_be_picked_through_another_body() {
        let mut viewport = Viewport::default();
        viewport.top();
        viewport.scale = 10.;
        viewport.set_scenes(vec![
            native_scene("Rear", 1, [0., 0., 4.95], Quat::IDENTITY),
            native_scene("Front", 1, [0., 0., 5.], Quat::IDENTITY),
        ]);
        viewport.edges.retain(|edge| edge.object.as_ref() == "Rear");
        let position = viewport.project(Vec3::new(2., 1., 5.)).0;
        assert_eq!(
            viewport.pick(position, &viewport.projected()),
            Some(Selection {
                object: "Front".into(),
                subelement: "Face1".into()
            })
        );
    }
    #[test]
    fn each_object_uses_its_own_placement_and_fit_includes_every_object() {
        let mut viewport = Viewport::default();
        viewport.top();
        let rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let mut first = native_scene("Turned", 1, [20., 0., 5.], rotation);
        first.0["normals"] = json!([[1, 0, 0], [1, 0, 0], [1, 0, 0]]);
        viewport.set_scenes(vec![
            first,
            native_scene("Other", 1, [-20., 0., 0.], Quat::IDENTITY),
        ]);
        assert!(viewport.triangles[0].points[1].abs_diff_eq(Vec3::new(20., 10., 5.), 1e-5));
        assert!(viewport.triangles[0].normals[0].abs_diff_eq(Vec3::Y, 1e-5));
        assert!(viewport.triangles[1].normals[0].abs_diff_eq(Vec3::Z, 1e-5));
        assert!(viewport.edges[0].points[0].abs_diff_eq(Vec3::new(19., 0., 5.), 1e-5));
        for (object, point) in [
            ("Turned", Vec3::new(18., 2., 5.)),
            ("Other", Vec3::new(-18., 2., 0.)),
        ] {
            assert_eq!(
                viewport
                    .pick(viewport.project(point).0, &viewport.projected())
                    .unwrap()
                    .object,
                object
            );
        }
        viewport.fit();
        assert!(
            viewport
                .triangles
                .iter()
                .flat_map(|triangle| triangle.points)
                .all(|point| viewport.rect.contains(viewport.project(point).0))
        );
    }
    #[test]
    fn multiselection_keeps_same_named_elements_separate_and_primary_recent() {
        let mut viewport = Viewport::default();
        viewport.set_scenes(vec![
            native_scene("A", 1, [0., 0., 0.], Quat::IDENTITY),
            native_scene("B", 1, [20., 0., 0.], Quat::IDENTITY),
        ]);
        let a = Selection {
            object: "A".into(),
            subelement: "Face1".into(),
        };
        let b = Selection {
            object: "B".into(),
            subelement: "Face1".into(),
        };
        viewport.select_native_element(Some(a.clone()), false);
        viewport.select_native_element(Some(b.clone()), true);
        assert_eq!(viewport.selections, vec![a.clone(), b.clone()]);
        assert_eq!(viewport.selected, Some(b.clone()));
        assert!(
            viewport
                .projected()
                .iter()
                .all(|triangle| triangle.colors == [Color32::from_rgb(47, 147, 143); 3])
        );
        viewport.select_native_element(Some(a), true);
        assert_eq!(viewport.selections, vec![b.clone()]);
        assert_eq!(viewport.selected, Some(b.clone()));
        viewport.select_native_element(Some(b.clone()), true);
        assert!(viewport.selections.is_empty() && viewport.selected.is_none());
        viewport.select_native_element(Some(b), false);
        viewport.select_native_element(None, true);
        assert!(viewport.selections.is_empty() && viewport.selected.is_none());
    }
    #[test]
    fn scene_revisions_and_hidden_objects_prune_only_their_own_selections() {
        let mut viewport = Viewport::default();
        viewport.top();
        viewport.set_scenes(vec![
            native_scene("A", 1, [0., 0., 0.], Quat::IDENTITY),
            native_scene("B", 1, [20., 0., 0.], Quat::IDENTITY),
        ]);
        viewport.select_native_element(
            Some(Selection {
                object: "A".into(),
                subelement: "Face1".into(),
            }),
            false,
        );
        let b = Selection {
            object: "B".into(),
            subelement: "Edge1".into(),
        };
        viewport.select_native_element(Some(b.clone()), true);
        viewport.set_scenes(vec![
            native_scene("B", 1, [20., 0., 0.], Quat::IDENTITY),
            native_scene("A", 2, [0., 0., 0.], Quat::IDENTITY),
        ]);
        assert_eq!(viewport.selections, vec![b.clone()]);
        assert_eq!(viewport.selected, Some(b));
        viewport.set_scenes(vec![native_scene("A", 2, [0., 0., 0.], Quat::IDENTITY)]);
        assert!(viewport.selections.is_empty() && viewport.selected.is_none());
        assert!(
            viewport
                .pick(
                    viewport.project(Vec3::new(22., 2., 0.)).0,
                    &viewport.projected()
                )
                .is_none()
        );
        viewport.set_scenes(vec![]);
        assert!(viewport.triangles.is_empty() && viewport.edges.is_empty());
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
