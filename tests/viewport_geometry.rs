use cad::{
    engine::Runtime,
    model::{Document, Sketch},
    plugins::Registry,
};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn circular_hole_mesh_is_closed_and_seams_have_continuous_surface_normals() {
    let registry = Registry::builtin().unwrap();
    let mut doc = Document::default();
    let mut sketch = Sketch::default();
    sketch.add_rect([0., 0.], [60., 30.]);
    sketch.add_circle([30., 15.], 5.);
    let mut f = registry.create("sketch", &mut doc).unwrap();
    f.set_sketch(&sketch);
    doc.features.push(f);
    let mut f = registry.create("extrude", &mut doc).unwrap();
    f.params["sketch"] = json!("f1");
    doc.features.push(f);
    let result = Runtime::new(registry).evaluate(&doc).unwrap();
    assert!(result.scene.error.is_none());
    let m = &result.scene.meshes[0];
    assert_eq!(m.bounds, [[0., 0., 0.], [60., 30., 10.]]);
    for (actual, expected) in m.centroid.into_iter().zip([30., 15., 5.]) {
        assert!((actual - expected).abs() < 1e-8);
    }
    assert_eq!(m.normals.len(), m.indices.len());
    let mut uses = HashMap::new();
    for triangle in m.indices.as_chunks::<3>().0 {
        for i in 0..3 {
            let a = triangle[i];
            let b = triangle[(i + 1) % 3];
            let key = if a < b { (a, b) } else { (b, a) };
            let entry = uses.entry(key).or_insert((0, 0_i32));
            entry.0 += 1;
            entry.1 += if a < b { 1 } else { -1 };
        }
    }
    assert!(
        uses.values()
            .all(|&(count, winding)| count == 2 && winding == 0),
        "Display mesh has a boundary or inconsistent winding"
    );
    assert_eq!(
        m.edges.iter().filter(|e| e.seam).count(),
        1,
        "Exactly one periodic cylindrical seam"
    );
    assert_eq!(
        m.edges
            .iter()
            .filter(|e| !e.seam && e.points.len() > 2)
            .count(),
        2,
        "Both circular rims stay visible"
    );
    let mut radial_corners = 0;
    for face in &m.faces {
        for corner in face.start..face.end {
            let p = m.positions[m.indices[corner] as usize];
            let n = m.normals[corner];
            let length = n.iter().map(|v| v * v).sum::<f32>().sqrt();
            assert!((length - 1.).abs() < 1e-5);
            if n[2].abs() < 0.01 && ((p[0] - 30.).hypot(p[1] - 15.) - 5.).abs() < 1e-4 {
                radial_corners += 1;
                assert!((n[0] + (p[0] - 30.) / 5.).abs() < 1e-5);
                assert!((n[1] + (p[1] - 15.) / 5.).abs() < 1e-5);
            }
            if n[2].abs() > 0.99 {
                assert!(
                    n[0].abs() < 1e-5 && n[1].abs() < 1e-5,
                    "Planar faces must stay flat-shaded"
                );
            }
        }
    }
    assert!(radial_corners > 200);
    let expected = (60. * 30. - std::f64::consts::PI * 25.) * 10.;
    assert!((m.volume - expected).abs() < 1e-6);
}

#[cfg(feature = "desktop")]
fn asymmetric_scene() -> std::sync::Arc<cad::engine::Scene> {
    let registry = Registry::builtin().unwrap();
    let mut doc = Document::default();
    let mut sketch = Sketch::default();
    sketch.add_rect([0., 0.], [60., 30.]);
    sketch.add_rect([40., 10.], [50., 20.]);
    let mut f = registry.create("sketch", &mut doc).unwrap();
    f.set_sketch(&sketch);
    doc.features.push(f);
    let mut f = registry.create("extrude", &mut doc).unwrap();
    f.params["sketch"] = json!("f1");
    doc.features.push(f);
    let result = Runtime::new(registry).evaluate(&doc).unwrap();
    assert!(result.scene.error.is_none(), "{:?}", result.scene.error);
    result.scene
}

#[cfg(feature = "desktop")]
#[test]
fn asymmetric_part_uses_volume_center_and_closeup_uses_visible_surface() {
    use cad::render::{Camera, automatic_pivot, model_center, pick};
    use eframe::egui;
    use glam::Vec3;
    use std::collections::BTreeSet;
    let scene = &asymmetric_scene();
    let hidden = BTreeSet::new();
    let expected = Vec3::new((1800. * 30. - 100. * 45.) / 1700., 15., 5.);
    let center = model_center(scene, &hidden).unwrap();
    assert!((center - expected).length() < 1e-4);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 800.));
    let mut camera = Camera::default();
    camera.set_view(Vec3::Z);
    camera.fit(scene);
    assert!((automatic_pivot(scene, &camera, rect, &hidden) - expected).length() < 1e-4);
    camera.center = Vec3::new(12., 10., 5.);
    camera.span = 10.;
    let surface = Vec3::new(12., 10., 10.);
    assert!(
        (pick(scene, &camera, rect.center(), rect, &hidden)
            .unwrap()
            .point
            - surface)
            .length()
            < 0.001
    );
    assert!((automatic_pivot(scene, &camera, rect, &hidden) - surface).length() < 0.001);
    let hidden = BTreeSet::from([scene.meshes[0].part.clone()]);
    assert!(model_center(scene, &hidden).is_none());
    assert!(pick(scene, &camera, rect.center(), rect, &hidden).is_none());
}

#[cfg(feature = "desktop")]
#[test]
fn fit_frames_all_geometry_in_narrow_portrait_and_wide_viewports() {
    use cad::render::{Camera, model_center};
    use eframe::egui;
    use glam::Vec3;
    use std::collections::BTreeSet;
    let scene = asymmetric_scene();
    let pivot = model_center(&scene, &BTreeSet::new()).unwrap();
    for direction in [Vec3::Z, Vec3::new(1., -1., 1.), Vec3::NEG_Y] {
        for size in [[622., 840.], [280., 900.], [1600., 450.], [800., 800.]] {
            let rect =
                egui::Rect::from_min_size(egui::pos2(188., 60.), egui::vec2(size[0], size[1]));
            let mut camera = Camera::default();
            camera.set_view(direction);
            let rotation = camera.rotation;
            camera.fit_with_rect(&scene, rect);
            assert_eq!(
                camera.rotation, rotation,
                "Fit must preserve the chosen view"
            );
            assert!(
                (camera.pivot - pivot).length() < 1e-4,
                "Framing must preserve the volume centroid pivot"
            );
            let matrix = camera.matrix(rect);
            for p in scene.meshes.iter().flat_map(|m| &m.positions) {
                let projected = matrix.project_point3(Vec3::from_array(*p));
                assert!(
                    projected.x.abs() <= 1. / 1.3 + 0.001 && projected.y.abs() <= 1. / 1.3 + 0.001,
                    "Part lost its framing margin: view={direction:?}, size={size:?}, point={projected:?}"
                );
                assert!(
                    projected.z.abs() < 1.,
                    "Part is outside the orthographic depth range"
                );
            }
            camera.fit_sketch_with_rect(&scene.sketches["f1"], rect);
            let projector = camera.projector(rect);
            for p in scene.sketches["f1"]
                .entities
                .iter()
                .flat_map(|e| e.points())
            {
                let point = projector.project(scene.sketches["f1"].plane.at(p), rect);
                assert!(
                    rect.shrink(1.).contains(point),
                    "Sketch clipped in {size:?}: {point:?}"
                );
            }
        }
    }
    let mut tiny = (*scene).clone();
    for mesh in &mut tiny.meshes {
        for p in &mut mesh.positions {
            *p = p.map(|v| v * 0.0001);
        }
        mesh.bounds = mesh.bounds.map(|p| p.map(|v| v * 0.0001));
        mesh.centroid = mesh.centroid.map(|v| v * 0.0001);
        mesh.volume *= 1e-12;
    }
    let mut camera = Camera::default();
    camera.fit_with_rect(
        &tiny,
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(622., 840.)),
    );
    assert!(
        camera.span >= 0.02 && camera.span < 0.1,
        "Small parts must not retain the old 20 mm minimum span"
    );

    let mut deep = (*scene).clone();
    for mesh in &mut deep.meshes {
        for p in &mut mesh.positions {
            p[2] *= 10000.;
        }
        for p in &mut mesh.bounds {
            p[2] *= 10000.;
        }
        mesh.centroid[2] *= 10000.;
    }
    camera.set_view(Vec3::Z);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(622., 840.));
    camera.fit_with_rect(&deep, rect);
    for p in deep.meshes.iter().flat_map(|m| &m.positions) {
        assert!(
            camera
                .matrix(rect)
                .project_point3(Vec3::from_array(*p))
                .z
                .abs()
                < 1.,
            "Fit must retain very deep geometry in the clipping range"
        );
    }
}
