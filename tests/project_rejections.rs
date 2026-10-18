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
fn drilling_boss_centers_after_fusing_rings_is_rejected_exactly() {
    rejected("instrument-enclosure", "mesh approximation")
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
