//! Transient choices for native origin-plane roles. These are never model objects.
use super::{
    Color32, Pos2, ProjectedTriangle, Shape, Stroke, Vec3, Viewport, egui, triangle_depth,
};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardPlane {
    Top,
    Front,
    Right,
}
impl StandardPlane {
    pub fn role(self) -> &'static str {
        match self {
            Self::Top => "XY_Plane",
            Self::Front => "XZ_Plane",
            Self::Right => "YZ_Plane",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Front => "Front",
            Self::Right => "Right",
        }
    }
    fn world(self, x: f32, y: f32) -> Vec3 {
        match self {
            Self::Top => Vec3::new(x, y, 0.),
            Self::Front => Vec3::new(x, 0., y),
            Self::Right => Vec3::new(0., x, y),
        }
    }
}
impl Viewport {
    fn standard_guide_hit(&self, pos: Pos2) -> Option<StandardPlane> {
        let mut hits = Vec::new();
        for plane in [
            StandardPlane::Top,
            StandardPlane::Front,
            StandardPlane::Right,
        ] {
            let corners = [(3., 3.), (27., 3.), (27., 27.), (3., 27.)]
                .map(|(x, y)| self.project(plane.world(x, y)));
            for indices in [[0, 1, 2], [0, 2, 3]] {
                let tri = ProjectedTriangle {
                    points: indices.map(|i| corners[i].0),
                    depths: indices.map(|i| corners[i].1),
                    face: 0,
                    colors: [Color32::TRANSPARENT; 3],
                };
                if let Some(depth) = triangle_depth(pos, &tri) {
                    hits.push((plane, depth));
                }
            }
        }
        hits.into_iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(plane, _)| plane)
    }
    #[cfg(test)]
    pub(crate) fn standard_guide_probe(&self, plane: StandardPlane) -> Option<Pos2> {
        let rect = self.rect.shrink(12.);
        for x in (rect.min.x as i32..rect.max.x as i32).step_by(7) {
            for y in (rect.min.y as i32..rect.max.y as i32).step_by(7) {
                let pos = Pos2::new(x as f32, y as f32);
                if pos.x < 500. || pos.y < 200. {
                    continue;
                }
                if self.standard_guide_hit(pos) == Some(plane) {
                    return Some(pos);
                }
            }
        }
        None
    }
    pub fn reconcile_standard_plane(&mut self, state: &Value) {
        let binding = state["document"]
            .as_str()
            .zip(state["revision"].as_u64())
            .map(|(document, revision)| (document.to_owned(), revision));
        if self.standard_plane_document != binding
            || state["available"] == false
            || state["recovery_only"] == true
            || state["objects"]
                .as_array()
                .is_none_or(|objects| !objects.is_empty())
        {
            self.clear_standard_plane();
        }
    }

    pub fn selected_standard_plane(&self) -> Option<StandardPlane> {
        self.standard_plane
    }
    pub fn clear_standard_plane(&mut self) {
        self.standard_plane = None;
        self.standard_plane_document = None;
    }
    pub fn standard_guides(
        &mut self,
        ui: &egui::Ui,
        state: &Value,
        available: bool,
        can_pick: bool,
    ) -> Option<StandardPlane> {
        let binding = state["document"]
            .as_str()
            .zip(state["revision"].as_u64())
            .map(|(d, r)| (d.to_owned(), r));
        if !available || binding.is_none() {
            self.clear_standard_plane();
            return None;
        }
        if self.standard_plane_document != binding {
            self.standard_plane = None;
            self.standard_plane_document = binding;
        }
        let response = self.response.as_ref()?;
        let pointer = response
            .interact_pointer_pos()
            .filter(|_| response.clicked());
        for plane in [
            StandardPlane::Top,
            StandardPlane::Front,
            StandardPlane::Right,
        ] {
            let corners = [(3., 3.), (27., 3.), (27., 27.), (3., 27.)]
                .map(|(x, y)| self.project(plane.world(x, y)));
            let vertices: Vec<Pos2> = corners.iter().map(|p| p.0).collect();
            let selected = self.standard_plane == Some(plane);
            let fill = if selected {
                Color32::from_rgba_unmultiplied(70, 200, 190, 45)
            } else {
                Color32::from_rgba_unmultiplied(135, 125, 175, 28)
            };
            ui.painter().add(Shape::convex_polygon(
                vertices.clone(),
                fill,
                Stroke::new(
                    if selected { 2_f32 } else { 1_f32 },
                    Color32::from_rgb(149, 143, 190),
                ),
            ));
            ui.painter().text(
                vertices[2],
                egui::Align2::CENTER_CENTER,
                plane.label(),
                egui::FontId::proportional(13.),
                Color32::from_rgb(220, 220, 230),
            );
        }
        if !ui.is_enabled() || !can_pick {
            return None;
        }
        let picked = pointer.and_then(|pos| self.standard_guide_hit(pos));
        if response.clicked() {
            self.standard_plane = picked;
            if picked.is_some() {
                self.clear_selection();
            }
        }
        picked
    }
}
