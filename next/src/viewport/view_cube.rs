//! Camera-only navigation. Cube gestures never become model-space input.
use super::{Pos2, Quat, Rect, Shape, StandardView, Stroke, Vec2, Vec3, Viewport, egui};

#[derive(Clone)]
struct Face {
    view: StandardView,
    label: &'static str,
    points: [Pos2; 4],
    depth: f32,
    corners: [(Vec3, f32); 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CubeView {
    Face(StandardView),
    Corner(Vec3),
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
        let corners = points.map(|v| {
            let world = Vec3::from_array(v);
            (world, (camera * world).z)
        });
        let points = points.map(|v| {
            let p = camera * Vec3::from_array(v);
            center + Vec2::new(p.x, -p.y) * scale
        });
        result.push(Face {
            view,
            label,
            points,
            depth: toward.z,
            corners,
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

fn contains(face: &Face, pos: Pos2) -> bool {
    let crosses = [0, 1, 2, 3].map(|i| {
        let a = face.points[i];
        let b = face.points[(i + 1) % 4];
        (b.x - a.x) * (pos.y - a.y) - (b.y - a.y) * (pos.x - a.x)
    });
    crosses.iter().all(|c| *c >= 0.) || crosses.iter().all(|c| *c <= 0.)
}

fn hit(faces: &[Face], pos: Pos2) -> Option<CubeView> {
    // Keep hits inside the visible cube silhouette, including its boundary.
    if !faces.iter().any(|face| contains(face, pos)) {
        return None;
    }
    if let Some((_, corner)) = faces
        .iter()
        .flat_map(|face| face.points.iter().zip(face.corners))
        .filter(|(point, _)| point.distance(pos) < 7.)
        .min_by(|(a, (_, az)), (b, (_, bz))| {
            a.distance(pos)
                .total_cmp(&b.distance(pos))
                .then_with(|| bz.total_cmp(az))
        })
    {
        return Some(CubeView::Corner(corner.0));
    }
    faces.iter().rev().find_map(|face| {
        // Edge navigation remains unsupported; do not choose an adjacent face.
        if (0..4)
            .any(|i| super::segment_distance(pos, face.points[i], face.points[(i + 1) % 4]).0 < 3.)
        {
            return None;
        }
        contains(face, pos).then_some(CubeView::Face(face.view))
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
            match view {
                CubeView::Face(view) => self.set_view(view),
                CubeView::Corner(corner) => {
                    let toward = corner.normalize();
                    let right = Vec3::Z.cross(toward).normalize();
                    let up = toward.cross(right);
                    self.rotation =
                        Quat::from_mat3(&glam::Mat3::from_cols(right, up, toward)).normalize();
                }
            }
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
            let visuals = if enabled && hover == Some(CubeView::Face(face.view)) {
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
            assert_eq!(hit(&projected, center(face)), Some(CubeView::Face(view)));
            assert_eq!(projected.len(), 1);
        }
    }
    #[test]
    fn cube_projection_tracks_camera_and_rejects_edges_and_outside() {
        let mut v = Viewport::default();
        v.set_view(StandardView::Trimetric);
        let rect = v.view_cube_rect().unwrap();
        let projected = faces(v.rotation, rect);
        assert_eq!(projected.len(), 3);
        for face in &projected {
            assert!(face.points.iter().all(|p| rect.contains(*p)));
            assert_eq!(
                hit(&projected, center(face)),
                Some(CubeView::Face(face.view))
            );
        }
        let top = projected
            .iter()
            .find(|f| f.view == StandardView::Top)
            .unwrap();
        assert_eq!(
            hit(&projected, top.points[0]),
            Some(CubeView::Corner(top.corners[0].0))
        );
        assert_eq!(
            hit(&projected, top.points[0].lerp(top.points[1], 0.5)),
            None
        );
        assert_eq!(hit(&projected, rect.min), None);
        assert_eq!(hit(&projected, rect.max), None);
    }

    fn frame(v: &mut Viewport, ctx: &egui::Context, events: Vec<egui::Event>) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                events,
                ..Default::default()
            },
            |ctx| {
                let _ = egui::CentralPanel::default().show(ctx, |ui| v.ui(ui));
            },
        );
    }

    #[test]
    fn all_eight_projected_corners_pointer_to_upright_isometric_camera() {
        for x in [-1., 1.] {
            for y in [-1., 1.] {
                for z in [-1., 1.] {
                    let corner = Vec3::new(x, y, z);
                    let mut v = Viewport::default();
                    v.set_view(if z > 0. {
                        StandardView::Top
                    } else {
                        StandardView::Bottom
                    });
                    let ctx = egui::Context::default();
                    frame(&mut v, &ctx, vec![]);
                    frame(&mut v, &ctx, vec![]);
                    let rect = v.view_cube_rect().unwrap();
                    let projected = faces(v.rotation, rect);
                    let face = &projected[0];
                    let index = face.corners.iter().position(|(p, _)| *p == corner).unwrap();
                    let pos = face.points[index];
                    assert_eq!(hit(&projected, pos), Some(CubeView::Corner(corner)));
                    let before = (v.center, v.scale, v.sketch_selection.clone());
                    for pressed in [true, false] {
                        frame(
                            &mut v,
                            &ctx,
                            vec![
                                egui::Event::PointerMoved(pos),
                                egui::Event::PointerButton {
                                    pos,
                                    button: egui::PointerButton::Primary,
                                    pressed,
                                    modifiers: egui::Modifiers::NONE,
                                },
                            ],
                        );
                    }
                    // Fixed f32 camera arithmetic bound, frozen before execution.
                    assert!((v.rotation * Vec3::Z).abs_diff_eq(corner.normalize(), 1e-6));
                    assert!((v.rotation * Vec3::Y).z > 0., "world Z stays upright");
                    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                        let projected_axis = v.rotation.conjugate() * axis;
                        assert!(
                            (projected_axis.truncate().length_squared() - 2. / 3.).abs() < 1e-6
                        );
                    }
                    assert_eq!((v.center, v.scale, v.sketch_selection.clone()), before);
                }
            }
        }
    }

    #[test]
    fn corner_regions_reject_occluded_corners_and_silhouette_exterior() {
        let mut v = Viewport::default();
        for view in [
            StandardView::Top,
            StandardView::Bottom,
            StandardView::Front,
            StandardView::Back,
            StandardView::Left,
            StandardView::Right,
        ] {
            v.set_view(view);
            let projected = faces(v.rotation, v.view_cube_rect().unwrap());
            let face = &projected[0];
            for (point, (corner, _)) in face.points.iter().zip(face.corners) {
                assert_eq!(hit(&projected, *point), Some(CubeView::Corner(corner)));
                let outside = *point + (*point - center(face)).normalized();
                assert_eq!(hit(&projected, outside), None);
            }
        }
    }
}
