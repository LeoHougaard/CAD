//! Orthographic view cube. Face, edge and corner hits share the same geometry
//! as the painted cube, so picking stays correct at every camera orientation.
use crate::{render::Camera, spacemouse::SpaceMouse};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use glam::Vec3;

pub fn bounds(viewport: Rect) -> Rect {
    Rect::from_min_size(
        egui::pos2(viewport.right() - 170., viewport.top() + 10.),
        Vec2::new(158., 182.),
    )
}

#[derive(Clone, Copy)]
struct Face {
    normal: Vec3,
    u: Vec3,
    v: Vec3,
    label: &'static str,
}
const FACES: [Face; 6] = [
    Face {
        normal: Vec3::Z,
        u: Vec3::X,
        v: Vec3::Y,
        label: "Top",
    },
    Face {
        normal: Vec3::NEG_Z,
        u: Vec3::NEG_X,
        v: Vec3::Y,
        label: "Bottom",
    },
    Face {
        normal: Vec3::NEG_Y,
        u: Vec3::X,
        v: Vec3::Z,
        label: "Front",
    },
    Face {
        normal: Vec3::Y,
        u: Vec3::NEG_X,
        v: Vec3::Z,
        label: "Back",
    },
    Face {
        normal: Vec3::X,
        u: Vec3::Y,
        v: Vec3::Z,
        label: "Right",
    },
    Face {
        normal: Vec3::NEG_X,
        u: Vec3::NEG_Y,
        v: Vec3::Z,
        label: "Left",
    },
];
fn project(camera: &Camera, center: Pos2, p: Vec3) -> Pos2 {
    let q = camera.rotation.conjugate() * p;
    center + Vec2::new(q.x, -q.y) * 35.
}
fn polygon(camera: &Camera, center: Pos2, f: Face, a: [f32; 2], b: [f32; 2]) -> Vec<Pos2> {
    [[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]]
        .into_iter()
        .map(|p| project(camera, center, f.normal + f.u * p[0] + f.v * p[1]))
        .collect()
}
fn contains(points: &[Pos2], p: Pos2) -> bool {
    let mut sign = 0.;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if cross.abs() < 0.001 {
            continue;
        }
        if sign == 0. {
            sign = cross.signum();
        } else if sign * cross < 0. {
            return false;
        }
    }
    sign != 0.
}
fn zone_direction(f: Face, x: usize, y: usize) -> Vec3 {
    f.normal + f.u * (x as f32 - 1.) + f.v * (y as f32 - 1.)
}

fn face_label(ui: &egui::Ui, camera: &Camera, center: Pos2, face: Face) {
    let galley = ui.painter().layout_no_wrap(
        face.label.into(),
        egui::FontId::proportional(11.),
        Color32::from_rgb(230, 236, 243),
    );
    let atlas = ui.fonts(|f| f.font_image_size());
    let mut mesh = egui::epaint::Mesh::default();
    for row in &galley.rows {
        let offset = mesh.vertices.len() as u32;
        for vertex in &row.visuals.mesh.vertices {
            let p = row.pos.to_vec2() + vertex.pos.to_vec2() - galley.rect.center().to_vec2();
            mesh.vertices.push(egui::epaint::Vertex {
                pos: project(
                    camera,
                    center,
                    face.normal + face.u * (p.x / 35.) - face.v * (p.y / 35.),
                ),
                uv: egui::pos2(vertex.uv.x / atlas[0] as f32, vertex.uv.y / atlas[1] as f32),
                color: vertex.color,
            });
        }
        mesh.indices
            .extend(row.visuals.mesh.indices.iter().map(|i| i + offset));
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// Returns whether the model should be fitted. Camera orientation changes here.
pub fn show(
    ui: &mut egui::Ui,
    viewport: Rect,
    camera: &mut Camera,
    edges: &mut bool,
    space: &mut SpaceMouse,
    model_pivot: Vec3,
) -> bool {
    let bounds = bounds(viewport);
    let cube = Rect::from_min_size(bounds.min, Vec2::new(158., 139.));
    let response = ui.interact(cube, ui.id().with("view_cube"), Sense::click_and_drag());
    let center = cube.center();
    let inverse = camera.rotation.conjugate();
    let mut visible: Vec<_> = FACES
        .iter()
        .copied()
        .filter(|f| (inverse * f.normal).z > 0.001)
        .collect();
    visible.sort_by(|a, b| (inverse * a.normal).z.total_cmp(&(inverse * b.normal).z));
    let pointer = response.hover_pos();
    let mut hit = None;
    let mut highlight = None;
    for f in &visible {
        let points = polygon(camera, center, *f, [-1., -1.], [1., 1.]);
        let light = (inverse * f.normal).z;
        let gray = (65. + light * 22.) as u8;
        ui.painter().add(egui::Shape::convex_polygon(
            points,
            Color32::from_rgb(gray, gray + 9, gray + 18),
            Stroke::new(1_f32, Color32::from_rgb(145, 158, 174)),
        ));
        let steps = [-1., -0.54, 0.54, 1.];
        for y in 0..3 {
            for x in 0..3 {
                let points = polygon(
                    camera,
                    center,
                    *f,
                    [steps[x], steps[y]],
                    [steps[x + 1], steps[y + 1]],
                );
                if pointer.is_some_and(|p| contains(&points, p)) {
                    hit = Some(zone_direction(*f, x, y));
                    highlight = Some(points);
                }
            }
        }
    }
    if let Some(points) = highlight {
        ui.painter().add(egui::Shape::convex_polygon(
            points,
            Color32::from_rgb(49, 77, 82),
            Stroke::new(1_f32, Color32::from_rgb(112, 184, 190)),
        ));
    }
    for f in &visible {
        if (inverse * f.normal).z > 0.24 {
            face_label(ui, camera, center, *f);
        }
    }
    if response.dragged() {
        if response.drag_started() {
            camera.pivot = model_pivot;
        }
        camera.orbit(ui.input(|i| i.pointer.delta()));
    } else if response.clicked()
        && let Some(direction) = hit
    {
        camera.pivot = model_pivot;
        camera.set_view(direction);
    }
    response.on_hover_text("Click a face, edge or corner. Drag to rotate.");
    let mut fit = false;
    let step = if ui.input(|i| i.modifiers.shift) {
        90_f32
    } else if ui.input(|i| i.modifiers.ctrl) {
        5.
    } else {
        15.
    };
    for (offset, sign) in [(0., 1.), (134., -1.)] {
        let r = Rect::from_min_size(bounds.min + Vec2::new(offset, 0.), Vec2::splat(24.));
        let response = ui.put(r, egui::Button::new(""));
        let c = r.center();
        let points: Vec<_> = (0..=12)
            .map(|i| {
                let t = (40. + i as f32 * 23.) * std::f32::consts::PI / 180.;
                c + Vec2::new(t.cos() * sign, t.sin()) * 7.
            })
            .collect();
        let end = *points.last().unwrap();
        ui.painter().add(egui::Shape::line(
            points,
            Stroke::new(1.3_f32, Color32::LIGHT_GRAY),
        ));
        ui.painter().line_segment(
            [end, end + Vec2::new(-4. * sign, -1.)],
            Stroke::new(1.3_f32, Color32::LIGHT_GRAY),
        );
        if response
            .on_hover_text("Roll view. Shift: 90 degrees. Ctrl: 5 degrees.")
            .clicked()
        {
            camera.pivot = model_pivot;
            camera
                .rotate_to(camera.rotation * glam::Quat::from_rotation_z(sign * step.to_radians()));
        }
    }
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(Rect::from_min_size(
            bounds.min + Vec2::new(0., 146.),
            Vec2::new(158., 34.),
        )),
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.;
            ui.spacing_mut().button_padding = Vec2::new(6., 4.);
            ui.horizontal(|ui| {
                if ui.button("Home").on_hover_text("Isometric view").clicked() {
                    camera.set_view(Vec3::new(1., -1., 1.));
                    fit = true;
                }
                if ui.button("Fit").clicked() {
                    fit = true;
                }
                ui.menu_button("View", |ui| {
                    ui.selectable_label(true, "Orthographic")
                        .on_hover_text("Parallel projection at every zoom level.");
                    ui.checkbox(edges, "Shaded edges");
                    ui.separator();
                    ui.label("SpaceMouse");
                    ui.label(egui::RichText::new(space.status()).small())
                        .on_hover_text(format!("{} HID reports received", space.reports()));
                    ui.checkbox(&mut space.options.enabled, "Enabled");
                    ui.add(egui::Slider::new(&mut space.options.speed, 0.2..=3.).text("Speed"));
                    ui.checkbox(&mut space.options.reverse_zoom, "Reverse zoom");
                    ui.checkbox(&mut space.options.reverse_rotation, "Reverse rotation");
                });
            });
        },
    );
    fit
}

pub fn axes(ui: &egui::Ui, viewport: Rect, camera: &Camera) {
    let origin = viewport.left_bottom() + Vec2::new(43., -43.);
    for (axis, label, color) in [
        (Vec3::X, "X", Color32::from_rgb(231, 112, 117)),
        (Vec3::Y, "Y", Color32::from_rgb(130, 198, 126)),
        (Vec3::Z, "Z", Color32::from_rgb(116, 165, 232)),
    ] {
        let q = camera.rotation.conjugate() * axis;
        let end = origin + Vec2::new(q.x, -q.y) * 25.;
        ui.painter()
            .line_segment([origin, end], Stroke::new(1.5_f32, color));
        ui.painter().text(
            end + (end - origin).normalized() * 8.,
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(11.),
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_cube_zone_targets_its_face_edge_or_corner() {
        for f in FACES {
            for x in 0..3 {
                for y in 0..3 {
                    let d = zone_direction(f, x, y);
                    let r = crate::render::view_rotation(d);
                    assert!((r * Vec3::Z - d.normalize()).length() < 1e-6);
                    assert!((r * Vec3::X).dot(r * Vec3::Y).abs() < 1e-6);
                }
            }
        }
    }
    #[test]
    fn visible_cube_polygons_pick_face_centers_at_any_orientation() {
        let center = egui::pos2(80., 80.);
        for direction in [
            Vec3::Z,
            Vec3::NEG_Y,
            Vec3::X,
            Vec3::new(1., -1., 1.),
            Vec3::new(-1., 1., -1.),
        ] {
            let mut c = Camera::default();
            c.set_view(direction);
            for f in FACES {
                if (c.rotation.conjugate() * f.normal).z > 0.01 {
                    assert!(contains(
                        &polygon(&c, center, f, [-0.54, -0.54], [0.54, 0.54]),
                        project(&c, center, f.normal)
                    ));
                }
            }
        }
    }
}
