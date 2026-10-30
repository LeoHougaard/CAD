//! Bounded reference graphics come from native placements and display sizes.
//! They never create BRep faces, alter the model, or invent selectable subelements.
use super::{
    Color32, DepthIndex, Pos2, ProjectedTriangle, Quat, Selection, SelectionMode, Stroke, Vec2,
    Vec3, Viewport, egui, placement, segment_distance, triangle_depth,
};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

pub(super) struct Reference {
    object: Arc<str>,
    kind: String,
    origin: Vec3,
    rotation: Quat,
    size: Vec2,
    line_axis: Vec3,
    pub(super) native_subelements: Vec<String>,
}
impl Reference {
    fn world(&self, point: Vec3) -> Vec3 {
        self.origin + self.rotation * point
    }
    fn plane_corners(&self) -> [Vec3; 4] {
        let x = self.size.x * 0.5;
        let y = self.size.y * 0.5;
        [
            Vec3::new(-x, -y, 0.),
            Vec3::new(x, -y, 0.),
            Vec3::new(x, y, 0.),
            Vec3::new(-x, y, 0.),
        ]
        .map(|point| self.world(point))
    }
    fn lines(&self) -> Vec<[Vec3; 2]> {
        match self.kind.as_str() {
            "plane" => {
                let corners = self.plane_corners();
                (0..4).map(|i| [corners[i], corners[(i + 1) % 4]]).collect()
            }
            "line" => vec![[
                self.world(-self.line_axis * self.size.x * 0.5),
                self.world(self.line_axis * self.size.x * 0.5),
            ]],
            "coordinate_system" | "origin" => [Vec3::X, Vec3::Y, Vec3::Z]
                .into_iter()
                .map(|axis| [self.origin, self.world(axis * self.size.x * 0.5)])
                .collect(),
            _ => vec![],
        }
    }
    pub(super) fn bounds_points(&self) -> Vec<Vec3> {
        if self.kind == "point" {
            vec![self.origin]
        } else {
            self.lines().into_iter().flatten().collect()
        }
    }
    pub(super) fn selected_bounds(&self, selections: &[Selection]) -> Vec<Vec3> {
        if selections.iter().any(|selection| {
            selection.object.as_str() == self.object.as_ref()
                && (selection.subelement.is_empty()
                    || self.native_subelements.contains(&selection.subelement))
        }) {
            self.bounds_points()
        } else {
            vec![]
        }
    }
    fn allowed(&self, mode: SelectionMode) -> bool {
        match mode {
            SelectionMode::All => true,
            SelectionMode::Faces => self.kind == "plane",
            SelectionMode::Edges => {
                matches!(self.kind.as_str(), "line" | "coordinate_system" | "origin")
            }
            SelectionMode::Vertices => self.kind == "point",
        }
    }
}
impl Viewport {
    pub fn planes_visible(&self) -> bool {
        self.planes_visible
    }

    /// Hide display planes without changing native object visibility or history.
    pub fn toggle_planes(&mut self, state: &Value) {
        self.planes_visible = !self.planes_visible;
        if !self.planes_visible {
            self.clear_standard_plane();
        }
        self.set_references(state);
    }

    /// Refresh presentation-only native datums directly from the solved snapshot.
    /// Infinite reference Shapes are deliberately never tessellated for display.
    pub fn set_references(&mut self, state: &Value) {
        self.cancel_other_picker();
        let revision = state["revision"].as_u64();
        let mut revisions = BTreeMap::new();
        // Origin planes are infinite. Give their display a useful extent in
        // an existing model so their selectable area is not buried in a solid.
        let model_extent = state["objects"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|object| {
                (0..3).filter_map(move |axis| {
                    let min = object["shape"]["bounds"]["min"][axis].as_f64()?;
                    let max = object["shape"]["bounds"]["max"][axis].as_f64()?;
                    let extent = (max - min) as f32;
                    (extent.is_finite() && extent > 0.).then_some(extent)
                })
            })
            .fold(0_f32, f32::max);
        let references = state["objects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|object| {
                if object["properties"]["Visibility"] == false {
                    return None;
                }
                let descriptor = object.get("reference_display")?;
                if descriptor["visible"] == false {
                    return None;
                }
                if let Some(owner) = object["body"].as_str()
                    && state["objects"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|parent| {
                            parent["name"] == owner && parent["properties"]["Visibility"] == false
                        })
                {
                    return None;
                }
                let kind = descriptor["kind"].as_str()?;
                if kind == "plane" && !self.planes_visible {
                    return None;
                }
                if !matches!(
                    kind,
                    "plane" | "line" | "point" | "coordinate_system" | "origin"
                ) {
                    return None;
                }
                let name = object["name"].as_str()?;
                let (origin, rotation) = placement(&descriptor["placement"]);
                let mut size = Vec2::new(
                    descriptor["size"][0].as_f64().unwrap_or(20.) as f32,
                    descriptor["size"][1].as_f64().unwrap_or(20.) as f32,
                );
                if kind == "plane" && object["type"] == "App::Plane" {
                    size.x = size.x.max(model_extent * 1.25);
                    size.y = size.y.max(model_extent * 1.25);
                }
                if !origin.is_finite() || !size.is_finite() || size.x <= 0. || size.y <= 0. {
                    return None;
                }
                revisions.insert(name.to_owned(), revision);
                Some(Reference {
                    object: Arc::from(name),
                    kind: kind.to_owned(),
                    origin,
                    rotation,
                    size,
                    line_axis: if descriptor["line_axis"] == "x" {
                        Vec3::X
                    } else {
                        Vec3::Z
                    },
                    native_subelements: descriptor["subelement"]
                        .as_str()
                        .filter(|name| !name.is_empty())
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                })
            })
            .collect();
        let valid = |selection: &Selection| {
            self.scene_revisions
                .get(&selection.object)
                .or_else(|| revisions.get(&selection.object))
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
        self.reference_revisions = revisions;
        self.references = references;
    }
    pub(super) fn reference_subelements(&self, object: &str) -> Option<&[String]> {
        self.references
            .iter()
            .find(|reference| reference.object.as_ref() == object)
            .map(|reference| reference.native_subelements.as_slice())
    }
    pub(super) fn pick_reference(
        &self,
        pos: Pos2,
        front: Option<(usize, f32)>,
        triangles: &[ProjectedTriangle],
    ) -> Option<Selection> {
        self.references
            .iter()
            .filter(|reference| reference.allowed(self.selection_mode))
            .filter_map(|reference| {
                let mut candidate: Option<(f32, f32)> = None;
                for line in reference.lines() {
                    let a = self.project(line[0]);
                    let b = self.project(line[1]);
                    let (distance, fraction) = segment_distance(pos, a.0, b.0);
                    let depth = a.1 + (b.1 - a.1) * fraction;
                    if distance <= 6. && candidate.is_none_or(|(d, _)| distance < d) {
                        candidate = Some((distance, depth));
                    }
                }
                if reference.kind == "plane" {
                    let corners = reference.plane_corners().map(|point| self.project(point));
                    for indices in [[0, 1, 2], [0, 2, 3]] {
                        let triangle = ProjectedTriangle {
                            points: indices.map(|i| corners[i].0),
                            depths: indices.map(|i| corners[i].1),
                            face: 0,
                            colors: [Color32::TRANSPARENT; 3],
                        };
                        if let Some(depth) = triangle_depth(pos, &triangle) {
                            candidate = Some((6., depth));
                        }
                    }
                } else if reference.kind == "point" {
                    let (point, depth) = self.project(reference.origin);
                    let distance = point.distance(pos);
                    if distance <= 7. {
                        candidate = Some((distance, depth));
                    }
                }
                let (distance, depth) = candidate?;
                self.point_visible(&reference.object, depth, front, triangles)
                    .then_some((reference, distance, depth))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.2.total_cmp(&a.2)))
            .map(|(reference, _, _)| Selection {
                object: reference.object.to_string(),
                subelement: reference
                    .native_subelements
                    .first()
                    .cloned()
                    .unwrap_or_default(),
            })
    }
    pub(super) fn draw_references(
        &self,
        painter: &egui::Painter,
        triangles: &[ProjectedTriangle],
        depth_index: &DepthIndex,
    ) {
        for reference in &self.references {
            let selected = self
                .selections
                .iter()
                .chain(self.selected.iter())
                .any(|selection| selection.object == reference.object.as_ref());
            let color = if selected {
                Color32::from_rgb(95, 221, 207)
            } else {
                Color32::from_rgb(143, 122, 176)
            };
            let stroke = Stroke::new(if selected { 2.2_f32 } else { 1.2_f32 }, color);
            for (axis, line) in reference.lines().into_iter().enumerate() {
                let a = self.project(line[0]);
                let b = self.project(line[1]);
                let stroke = if !selected
                    && matches!(reference.kind.as_str(), "origin" | "coordinate_system")
                {
                    Stroke::new(
                        1.2_f32,
                        [
                            Color32::from_rgb(184, 94, 104),
                            Color32::from_rgb(101, 173, 142),
                            Color32::from_rgb(123, 133, 207),
                        ][axis],
                    )
                } else {
                    stroke
                };
                let steps = (a.0.distance(b.0) / 8.).ceil().clamp(1., 1000.) as usize;
                for step in 0..steps {
                    let t0 = step as f32 / steps as f32;
                    let t1 = (step + 1) as f32 / steps as f32;
                    let middle = (t0 + t1) * 0.5;
                    let point = a.0.lerp(b.0, middle);
                    let depth = a.1 + (b.1 - a.1) * middle;
                    if self.point_visible(
                        &reference.object,
                        depth,
                        depth_index.front(point, triangles),
                        triangles,
                    ) {
                        painter.line_segment([a.0.lerp(b.0, t0), a.0.lerp(b.0, t1)], stroke);
                    }
                }
            }
            if reference.kind == "point" {
                let (point, depth) = self.project(reference.origin);
                if self.point_visible(
                    &reference.object,
                    depth,
                    depth_index.front(point, triangles),
                    triangles,
                ) {
                    painter.circle_filled(point, if selected { 4. } else { 3. }, color);
                    painter.line_segment(
                        [point - Vec2::new(6., 0.), point + Vec2::new(6., 0.)],
                        stroke,
                    );
                    painter.line_segment(
                        [point - Vec2::new(0., 6.), point + Vec2::new(0., 6.)],
                        stroke,
                    );
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn plane_display_toggle_preserves_axes_and_native_visibility() {
        let mut viewport = Viewport::default();
        viewport.top();
        let state = json!({"revision":1,"objects":[
            {"name":"Plane","properties":{"Visibility":true},"reference_display":{"kind":"plane","size":[20,20],"placement":{"position":[100,0,0]},"subelement":"Face1"}},
            {"name":"Axis","properties":{"Visibility":true},"reference_display":{"kind":"line","size":[20,20],"line_axis":"x","subelement":"Edge1"}}
        ]});
        viewport.set_references(&state);
        viewport.select_native_element(
            Some(Selection {
                object: "Plane".into(),
                subelement: "Face1".into(),
            }),
            false,
        );
        viewport.toggle_planes(&state);
        assert!(!viewport.planes_visible());
        assert!(viewport.selected.is_none() && viewport.selections.is_empty());
        assert_eq!(viewport.references.len(), 1);
        assert_eq!(viewport.references[0].object.as_ref(), "Axis");
        assert_eq!(state["objects"][0]["properties"]["Visibility"], true);
        assert!(
            viewport
                .pick(viewport.project(Vec3::new(100., 0., 0.)).0, &[])
                .is_none()
        );
        assert_eq!(
            viewport
                .pick(viewport.project(Vec3::new(5., 0., 0.)).0, &[])
                .unwrap()
                .object,
            "Axis"
        );
        viewport.toggle_planes(&state);
        assert!(viewport.planes_visible());
        assert_eq!(viewport.references.len(), 2);
        assert_eq!(
            viewport
                .pick(viewport.project(Vec3::new(100., 0., 0.)).0, &[])
                .unwrap()
                .object,
            "Plane"
        );
    }

    #[test]
    fn infinite_native_reference_uses_finite_native_display_size_and_actual_native_face() {
        let mut viewport = Viewport::default();
        viewport.top();
        let state = json!({"revision":1,"objects":[{"name":"Datum","properties":{"Visibility":true},"shape":{"bounds":{"min":[-1e100,-1e100,0],"max":[1e100,1e100,0]}},
            "reference_display":{"kind":"plane","placement":{"position":[100,20,0]},"size":[20,30],"subelement":"Face1"}}]});
        viewport.set_references(&state);
        viewport.fit();
        assert!(viewport.center.abs_diff_eq(Vec3::new(100., 20., 0.), 1e-5));
        assert!(viewport.scale.is_finite() && viewport.scale > 1.);
        assert_eq!(
            viewport.pick(viewport.project(Vec3::new(100., 20., 0.)).0, &[]),
            Some(Selection {
                object: "Datum".into(),
                subelement: "Face1".into()
            })
        );
        assert_eq!(viewport.topology_references("Datum"), ["Face1"]);
    }
    #[test]
    fn reference_identity_survives_same_revision_and_hidden_or_changed_reference_clears_selection()
    {
        let mut viewport = Viewport::default();
        viewport.top();
        let mut state = json!({"revision":1,"objects":[{"name":"Axis","properties":{"Visibility":true},"reference_display":{"kind":"line","placement":{"rotation":[0,0,std::f64::consts::FRAC_1_SQRT_2,std::f64::consts::FRAC_1_SQRT_2]},"size":[20,20],"line_axis":"x","subelement":"Edge1"}}]});
        viewport.set_references(&state);
        let reference = &viewport.references[0];
        let points = reference.bounds_points();
        assert!(
            (points[1] - points[0])
                .normalize()
                .abs_diff_eq(Vec3::Y, 1e-5)
        );
        viewport.select_native_element(
            Some(Selection {
                object: "Axis".into(),
                subelement: String::new(),
            }),
            false,
        );
        viewport.set_scenes(vec![]);
        viewport.set_references(&state);
        assert!(viewport.selected.is_some());
        state["revision"] = json!(2);
        viewport.set_references(&state);
        assert!(viewport.selected.is_none());
        viewport.select_native_element(
            Some(Selection {
                object: "Axis".into(),
                subelement: String::new(),
            }),
            false,
        );
        state["objects"][0]["properties"]["Visibility"] = json!(false);
        viewport.set_references(&state);
        assert!(viewport.references.is_empty() && viewport.selected.is_none());
    }
    #[test]
    fn reference_visibility_respects_native_effective_flag_and_owner_body() {
        let mut viewport = Viewport::default();
        let mut state = json!({"revision":1,"objects":[
            {"name":"Body","properties":{"Visibility":false}},
            {"name":"Plane","body":"Body","properties":{"Visibility":true},"reference_display":{"kind":"plane","size":[20,20],"visible":true}}
        ]});
        viewport.set_references(&state);
        assert!(viewport.references.is_empty());
        state["objects"][0]["properties"]["Visibility"] = json!(true);
        state["objects"][1]["reference_display"]["visible"] = json!(false);
        viewport.set_references(&state);
        assert!(viewport.references.is_empty());
        state["objects"][1]["reference_display"]["visible"] = json!(true);
        viewport.set_references(&state);
        assert_eq!(viewport.references.len(), 1);
    }
    #[test]
    fn isolated_native_points_are_visible_and_pickable_in_default_selection_mode() {
        let mut viewport = Viewport::default();
        viewport.top();
        viewport.set_scene(json!({"object":"Point","revision":1,"triangles":[]}),json!({"world_from_shape":{"position":[100,20,30]},"edges":[],"vertices":[{"subelement":"Vertex1","point":[0,0,0]}]}));
        assert_eq!(viewport.selection_mode, SelectionMode::All);
        assert!(viewport.vertices[0].isolated);
        viewport.fit();
        assert!(viewport.center.abs_diff_eq(Vec3::new(100., 20., 30.), 1e-5));
        assert_eq!(
            viewport.pick(viewport.rect.center(), &[]),
            Some(Selection {
                object: "Point".into(),
                subelement: "Vertex1".into()
            })
        );
        viewport.selection_mode = SelectionMode::Faces;
        assert!(viewport.pick(viewport.rect.center(), &[]).is_none());
    }
    #[test]
    fn wire_and_point_only_native_scenes_fit_without_surface_triangles() {
        let mut viewport = Viewport::default();
        viewport.top();
        viewport.set_scene(json!({"object":"Wire"}),json!({"edges":[{"subelement":"Edge1","points":[[100,20,0],[140,20,0]]}],"vertices":[{"subelement":"Vertex1","point":[100,20,0]}]}));
        viewport.fit();
        assert!(viewport.center.abs_diff_eq(Vec3::new(120., 20., 0.), 1e-5));
        assert_eq!(
            viewport
                .pick(viewport.project(Vec3::new(120., 20., 0.)).0, &[])
                .unwrap()
                .object,
            "Wire"
        );
    }
}
