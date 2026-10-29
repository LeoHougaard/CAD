//! Camera-only navigation. Cube gestures never become model-space input.
use super::{Pos2, Quat, Rect, Shape, StandardView, Stroke, Vec2, Vec3, Viewport, egui};

#[derive(Clone)]
struct Face {
    view: StandardView,
    label: &'static str,
    points: [Pos2; 4],
    depth: f32,
}

fn faces(rotation: Quat, rect: Rect) -> Vec<Face> {
    let camera = rotation.conjugate();
    let scale = rect.width() * 0.255;
    let center = rect.center();
    let mut result = Vec::new();
    for (view, label, normal, points) in [
        (
            StandardView::Top,
            "Top",
            Vec3::Z,
            [[-1., -1., 1.], [1., -1., 1.], [1., 1., 1.], [-1., 1., 1.]],
        ),
        (
            StandardView::Bottom,
            "Bottom",
            -Vec3::Z,
            [
                [-1., 1., -1.],
                [1., 1., -1.],
                [1., -1., -1.],
                [-1., -1., -1.],
            ],
        ),
        (
            StandardView::Front,
            "Front",
            -Vec3::Y,
            [
                [-1., -1., -1.],
                [1., -1., -1.],
                [1., -1., 1.],
                [-1., -1., 1.],
            ],
        ),
        (
            StandardView::Back,
            "Back",
            Vec3::Y,
            [[1., 1., -1.], [-1., 1., -1.], [-1., 1., 1.], [1., 1., 1.]],
        ),
        (
            StandardView::Right,
            "Right",
            Vec3::X,
            [[1., -1., -1.], [1., 1., -1.], [1., 1., 1.], [1., -1., 1.]],
        ),
        (
            StandardView::Left,
            "Left",
            -Vec3::X,
            [
                [-1., 1., -1.],
                [-1., -1., -1.],
                [-1., -1., 1.],
                [-1., 1., 1.],
            ],
        ),
    ] {
        let toward = camera * normal;
        if toward.z <= 0.001 {
            continue;
        }
        let points = points.map(|v| {
            let p = camera * Vec3::from_array(v);
            center + Vec2::new(p.x, -p.y) * scale
        });
        result.push(Face {
            view,
            label,
            points,
            depth: toward.z,
        });
    }
    result.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    result
}

fn center(face: &Face) -> Pos2 {
    Pos2::new(
        face.points.iter().map(|p| p.x).sum::<f32>() / 4.,
        face.points.iter().map(|p| p.y).sum::<f32>() / 4.,
    )
}

fn hit(faces: &[Face], pos: Pos2) -> Option<StandardView> {
    faces.iter().rev().find_map(|face| {
        // Edge and corner navigation are not implemented. Refuse those small
        // zones instead of incorrectly choosing an adjacent face.
        if face.points.iter().any(|p| p.distance(pos) < 7.)
            || (0..4).any(|i| {
                super::segment_distance(pos, face.points[i], face.points[(i + 1) % 4]).0 < 3.
            })
        {
            return None;
        }
        let crosses = (0..4)
            .map(|i| {
                let a = face.points[i];
                let b = face.points[(i + 1) % 4];
                (b.x - a.x) * (pos.y - a.y) - (b.y - a.y) * (pos.x - a.x)
            })
            .collect::<Vec<_>>();
        (crosses.iter().all(|c| *c >= 0.) || crosses.iter().all(|c| *c <= 0.)).then_some(face.view)
    })
}

impl Viewport {
    pub fn view_cube_rect(&self) -> Option<Rect> {
        let side = (self.rect.width().min(self.rect.height()) - 24.).min(128.);
        if !self.rect.is_finite() || side < 88. {
            return None;
        }
        let min = Pos2::new(self.rect.max.x - side - 12., self.rect.min.y + 12.);
        Some(Rect::from_min_size(min, Vec2::splat(side)))
    }

    /// True only for a cube-started gesture or idle hover over its footprint.
    /// A model drag that started elsewhere retains its release even over here.
    pub fn view_cube_owns_pointer(&self) -> bool {
        self.cube_consumed
    }

    pub(super) fn view_cube_input(&mut self, ui: &mut egui::Ui) {
        self.cube_consumed = false;
        let Some(rect) = self.view_cube_rect() else {
            return;
        };
        let response = ui.interact(
            rect,
            egui::Id::new("native_view_cube"),
            egui::Sense::click_and_drag(),
        );
        let (pos, pressed, released, down) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.any_pressed(),
                i.pointer.any_released(),
                i.pointer.any_down(),
            )
        });
        let inside = pos.is_some_and(|p| rect.contains(p));
        let enabled =
            self.view_cube_input_enabled && ui.is_enabled() && !egui::Popup::is_any_open(ui.ctx());
        if pressed && inside {
            self.cube_pointer_owned = true;
            self.cube_action_owned = enabled && response.hovered();
        }
        self.cube_consumed = self.cube_pointer_owned || inside && !down && !released;
        if enabled && self.cube_action_owned && response.dragged_by(egui::PointerButton::Primary) {
            let delta = ui.input(|i| i.pointer.delta());
            let yaw = Quat::from_axis_angle(Vec3::Z, -delta.x * 0.008);
            let pitch = Quat::from_axis_angle(self.rotation * Vec3::X, -delta.y * 0.008);
            self.rotation = (yaw * pitch * self.rotation).normalize();
            self.cancel_other_picker();
        }
        if enabled
            && self.cube_action_owned
            && response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
            && let Some(view) = hit(&faces(self.rotation, rect), pos)
        {
            self.set_view(view);
            self.cancel_other_picker();
        }
        if !down {
            self.cube_pointer_owned = false;
            self.cube_action_owned = false;
        }
    }

    pub(super) fn draw_view_cube(&self, ui: &egui::Ui) {
        let Some(rect) = self.view_cube_rect() else {
            return;
        };
        let faces = faces(self.rotation, rect);
        let hover = ui
            .input(|i| i.pointer.hover_pos())
            .and_then(|p| hit(&faces, p));
        let enabled =
            self.view_cube_input_enabled && ui.is_enabled() && !egui::Popup::is_any_open(ui.ctx());
        for face in faces {
            let visuals = if enabled && hover == Some(face.view) {
                &ui.visuals().widgets.hovered
            } else {
                &ui.visuals().widgets.inactive
            };
            ui.painter().add(Shape::convex_polygon(
                face.points.to_vec(),
                visuals.bg_fill,
                Stroke::new(1_f32, visuals.fg_stroke.color),
            ));
            if face.depth > 0.16 {
                ui.painter().text(
                    center(&face),
                    egui::Align2::CENTER_CENTER,
                    face.label,
                    egui::FontId::proportional(12.),
                    if enabled {
                        ui.visuals().text_color()
                    } else {
                        ui.visuals().weak_text_color()
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visible_cube_face_center_selects_each_canonical_view() {
        let mut v = Viewport::default();
        let rect = v.view_cube_rect().unwrap();
        for view in [
            StandardView::Top,
            StandardView::Bottom,
            StandardView::Front,
            StandardView::Back,
            StandardView::Left,
            StandardView::Right,
        ] {
            v.set_view(view);
            let projected = faces(v.rotation, rect);
            let face = projected.iter().find(|f| f.view == view).unwrap();
            assert_eq!(hit(&projected, center(face)), Some(view));
            assert_eq!(projected.len(), 1);
        }
    }
    #[test]
    fn cube_projection_tracks_camera_and_rejects_unimplemented_edges_and_corners() {
        let mut v = Viewport::default();
        v.set_view(StandardView::Trimetric);
        let rect = v.view_cube_rect().unwrap();
        let projected = faces(v.rotation, rect);
        assert_eq!(projected.len(), 3);
        for face in &projected {
            assert!(face.points.iter().all(|p| rect.contains(*p)));
            assert_eq!(hit(&projected, center(face)), Some(face.view));
        }
        let top = projected
            .iter()
            .find(|f| f.view == StandardView::Top)
            .unwrap();
        assert_eq!(hit(&projected, top.points[0]), None);
        assert_eq!(
            hit(&projected, top.points[0].lerp(top.points[1], 0.5)),
            None
        );
        assert_eq!(hit(&projected, rect.min), None);
    }
}
