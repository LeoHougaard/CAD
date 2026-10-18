//! Original failed workflows: preserve precise rejection or prove a shared fix.
use cad::{engine::Runtime, model::Document, plugins::Registry};
use std::path::Path;

fn rejected(name: &str, message: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/project-rejections")
        .join(format!("{name}.cadpart"));
    let document = Document::load(&path).unwrap();
    assert!(document.features.iter().all(|f| !f.suppressed));
    let result = Runtime::new(Registry::builtin().unwrap())
        .evaluate(&document)
        .unwrap();
    let error = result.scene.error.as_ref().expect(
        "Known incomplete workflow unexpectedly succeeded; update its regression and documentation",
    );
    assert!(error.contains(message), "{name}: {error}");
    assert!(
        !result.geometry.parts.is_empty(),
        "The prior precise solid must remain available"
    );
    for part in &result.geometry.parts {
        let validity =
            brepkit_operations::validate::validate_solid(&result.geometry.topo, part.solid)
                .unwrap();
        assert!(validity.is_valid(), "{name}: {:?}", validity.issues);
    }
}
#[test]
fn drilling_boss_centers_after_fusing_rings_preserves_the_exact_enclosure() {
    let mut document = Document::load(&Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/project-rejections/instrument-enclosure.cadpart")).unwrap();
    let directory = Path::new("target/test-output/original-enclosure-order");
    std::fs::create_dir_all(directory).unwrap();
    for height in [25., 30.] {
        document.variables.insert("height".into(), height.to_string());
        let path = directory.join(format!("height-{height}.cadpart"));
        document.save(&path).unwrap();
        let reopened = Document::load(&path).unwrap();
        assert_eq!(document, reopened);
        let result = Runtime::new(Registry::builtin().unwrap()).evaluate(&reopened).unwrap();
        assert!(result.scene.error.is_none(), "{:?}", result.scene.error);
        assert_eq!(result.geometry.parts.len(), 1);
        let expected = 80. * 50. * height - 76. * 46. * (height - 2.)
            + 4. * std::f64::consts::PI * 12. * 8.
            - 4. * std::f64::consts::PI * 4. * 2. - 16. * 8. * 2.;
        assert!((result.scene.meshes[0].volume - expected).abs() < 1e-6);
        let validity = brepkit_operations::validate::validate_solid(
            &result.geometry.topo, result.geometry.parts[0].solid).unwrap();
        assert!(validity.is_valid(), "{:?}", validity.issues);
        let mut edges = std::collections::BTreeMap::new();
        for triangle in result.scene.meshes[0].indices.as_chunks::<3>().0 {
            for i in 0..3 {
                let (a, b) = (triangle[i], triangle[(i + 1) % 3]);
                let entry = edges.entry((a.min(b), a.max(b))).or_insert((0, 0i32));
                entry.0 += 1;
                entry.1 += if a < b { 1 } else { -1 };
            }
        }
        assert!(edges.values().all(|&(count, orientation)| count == 2 && orientation == 0));
        #[cfg(feature = "step")]
        result.geometry.export("step", &directory.join(format!("height-{height}.step"))).unwrap();
        let spec = serde_json::json!({"case": format!("height-{height}"),
            "step_exported": cfg!(feature="step"),
            "expected_solids": [{"volume_mm3": expected,
                "bounds_mm": [[-40.,-25.,0.],[40.,25.,height]]}]});
        std::fs::write(directory.join(format!("height-{height}.json")),
            serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
    }
}
#[test]
fn through_bore_in_fused_loft_reducer_is_rejected_exactly() {
    rejected("circular-reducer", "mesh approximation")
}
#[test]
fn chamfer_after_comb_cutouts_preserves_a_closed_exact_solid() {
    let mut document = Document::load(&Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/project-rejections/cable-comb.cadpart")).unwrap();
    for width in [120., 130.] {
        document.variables.insert("width".into(), width.to_string());
        let result = Runtime::new(Registry::builtin().unwrap()).evaluate(&document).unwrap();
        assert!(result.scene.error.is_none(), "{:?}", result.scene.error);
        assert_eq!(result.geometry.parts.len(), 1);
        let expected = width * 24. * 12. - 5. * 12. * 16. * 12.
            - 2. * std::f64::consts::PI * 6.25 * 12. - 0.5 * 0.5 * width / 2.;
        assert!((result.scene.meshes[0].volume - expected).abs() < 1e-6);
        let validity = brepkit_operations::validate::validate_solid(
            &result.geometry.topo, result.geometry.parts[0].solid).unwrap();
        assert!(validity.is_valid(), "{:?}", validity.issues);
    }
}
