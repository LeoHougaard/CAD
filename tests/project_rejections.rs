//! Rejected first attempts retained separately from the ten completed projects.
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
fn chamfer_after_comb_cutouts_rejects_an_open_shell() {
    rejected("cable-comb", "shell is not closed")
}
