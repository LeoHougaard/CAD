//! Camera and display choices are presentation state; native visibility lives in the document.
use super::Viewport;
use glam::{Mat3, Quat, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardView {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    Isometric,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMode {
    Shaded,
    Edges,
    Wireframe,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    All,
    Faces,
    Edges,
    Vertices,
}

impl Viewport {
    pub fn set_view(&mut self, view: StandardView) {
        let (right, up) = match view {
            StandardView::Front => (Vec3::X, Vec3::Z),
            StandardView::Back => (-Vec3::X, Vec3::Z),
            StandardView::Left => (-Vec3::Y, Vec3::Z),
            StandardView::Right => (Vec3::Y, Vec3::Z),
            StandardView::Top => (Vec3::X, Vec3::Y),
            StandardView::Bottom => (Vec3::X, -Vec3::Y),
            StandardView::Isometric => {
                self.rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)
                    * Quat::from_rotation_x(54.73561_f32.to_radians());
                return;
            }
        };
        self.rotation = Quat::from_mat3(&Mat3::from_cols(right, up, right.cross(up))).normalize();
    }
    pub fn top(&mut self) {
        self.set_view(StandardView::Top);
    }
    pub fn iso(&mut self) {
        self.set_view(StandardView::Isometric);
    }
    /// Revision-scoped names from native display topology. No mesh-derived references are invented.
    pub fn topology_references(&self, object: &str) -> &[String] {
        self.topology_references
            .get(object)
            .map(Vec::as_slice)
            .or_else(|| self.reference_subelements(object))
            .unwrap_or_default()
    }
    pub fn has_vertices(&self) -> bool {
        !self.vertices.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewport::Selection;
    use serde_json::json;
    #[test]
    fn orthographic_views_preserve_expected_front_and_up_axes() {
        let mut viewport = Viewport::default();
        let center = viewport.center;
        let scale = viewport.scale;
        for (view, toward, up) in [
            (StandardView::Front, -Vec3::Y, Vec3::Z),
            (StandardView::Back, Vec3::Y, Vec3::Z),
            (StandardView::Left, -Vec3::X, Vec3::Z),
            (StandardView::Right, Vec3::X, Vec3::Z),
            (StandardView::Top, Vec3::Z, Vec3::Y),
            (StandardView::Bottom, -Vec3::Z, -Vec3::Y),
        ] {
            viewport.set_view(view);
            assert!((viewport.rotation * Vec3::Z).abs_diff_eq(toward, 1e-5));
            assert!((viewport.rotation * Vec3::Y).abs_diff_eq(up, 1e-5));
            assert_eq!(viewport.center, center);
            assert_eq!(viewport.scale, scale);
        }
    }
    #[test]
    fn selection_filters_and_wireframe_respect_native_scene_identity() {
        let mut viewport = Viewport::default();
        viewport.top();
        viewport.scale = 10.;
        let scene = |object: &str, z: f32| {
            (
                json!({"object":object,"revision":1,"world_from_shape":{"position":[0,0,z]},
            "positions":[[0,0,0],[10,0,0],[0,10,0]],"triangles":[[0,1,2]],"faces":[{"subelement":"Face1","first_triangle":0,"triangle_count":1}]}),
                json!({"world_from_shape":{"position":[0,0,z]},"edges":[{"subelement":"Edge1","points":[[0,1,0],[8,1,0]]}],"vertices":[{"subelement":"Vertex1","point":[2,2,0]}]}),
            )
        };
        viewport.set_scenes(vec![scene("Rear", 4.95), scene("Front", 5.)]);
        viewport.edges.retain(|edge| edge.object.as_ref() == "Rear");
        viewport
            .vertices
            .retain(|vertex| vertex.object.as_ref() == "Rear");
        let edge_position = viewport.project(Vec3::new(2., 1., 5.)).0;
        let triangles = viewport.projected();
        assert_eq!(
            viewport.pick(edge_position, &triangles).unwrap().object,
            "Front"
        );
        viewport.selection_mode = SelectionMode::Edges;
        assert!(viewport.pick(edge_position, &triangles).is_none());
        viewport.display_mode = DisplayMode::Wireframe;
        assert_eq!(
            viewport.pick(edge_position, &triangles),
            Some(Selection {
                object: "Rear".into(),
                subelement: "Edge1".into()
            })
        );
        viewport.selection_mode = SelectionMode::Faces;
        assert_eq!(
            viewport.pick(edge_position, &triangles),
            Some(Selection {
                object: "Front".into(),
                subelement: "Face1".into()
            })
        );
        viewport.selection_mode = SelectionMode::Vertices;
        let vertex_position = viewport.project(Vec3::new(2., 2., 5.)).0;
        assert_eq!(
            viewport.pick(vertex_position, &triangles),
            Some(Selection {
                object: "Rear".into(),
                subelement: "Vertex1".into()
            })
        );
        viewport.display_mode = DisplayMode::Edges;
        assert!(viewport.pick(vertex_position, &triangles).is_none());
        assert_eq!(viewport.triangles.len(), 2);
        assert_eq!(viewport.scene_revisions.len(), 2);
    }
    #[test]
    fn native_vertices_and_reference_names_keep_object_identity_and_placement() {
        let mut viewport = Viewport::default();
        viewport.top();
        viewport.set_scene(json!({"object":"Pad","revision":1}),json!({"world_from_shape":{"position":[10,20,30],"rotation":[0,0,0,1]},
            "faces":[{"subelement":"Face2"}],"edges":[{"subelement":"Edge4","points":[[0,0,0],[1,0,0]]}],
            "vertices":[{"subelement":"Vertex3","point":[2,3,4]}]}));
        assert_eq!(
            viewport.topology_references("Pad"),
            ["Face2", "Edge4", "Vertex3"]
        );
        assert!(viewport.topology_references("Missing").is_empty());
        viewport.selection_mode = SelectionMode::Vertices;
        let point = viewport.project(Vec3::new(12., 23., 34.)).0;
        assert_eq!(
            viewport.pick(point, &[]),
            Some(Selection {
                object: "Pad".into(),
                subelement: "Vertex3".into()
            })
        );
        viewport.selection_mode = SelectionMode::Faces;
        assert!(viewport.pick(point, &[]).is_none());
        viewport.set_scenes(vec![]);
        assert!(viewport.topology_references("Pad").is_empty() && !viewport.has_vertices());
    }
}
