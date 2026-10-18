use cad::{
    dimensions::Selection,
    model::{Document, Plane, Sketch, SketchConstraint},
    plugins::{Numbers, Registry},
    sketch,
};
use std::collections::BTreeMap;

#[test]
fn rectangle_side_construction_preserves_dimensions_expressions_and_other_curves() {
    let mut vars = BTreeMap::new();
    vars.insert("width".into(), "40".into());
    let numbers = Numbers::new(&vars).unwrap();
    let mut original = Sketch::default();
    original.add_rect([-20., -10.], [20., 10.]);
    original.entities[0].values[2] = "width".into();
    original.constraints.push(SketchConstraint {
        kind: "Height".into(),
        a: 1,
        value: "20".into(),
        ..Default::default()
    });
    original.add_circle([70., 0.], 5.);
    original.entities[1].construction = true;
    let plane = Plane::named("Top", 0.);
    let solved = sketch::resolve(&original, plane.clone(), &numbers).unwrap();
    let chosen = [Selection::edge(1, 2), Selection::edge(2, 0)];
    let (changed, solved, chosen) =
        sketch::set_construction(&original, &solved, &chosen, None, &numbers).unwrap();
    assert_eq!(changed.entities.len(), 5);
    assert_eq!(
        solved.dof, 5,
        "Replacing a rectangle must preserve its mobility"
    );
    for (i, line) in changed.entities[..4].iter().enumerate() {
        assert_eq!(line.construction, i == 2);
    }
    assert!(!changed.entities[4].construction);
    assert!(changed.constraints.iter().any(|c| c.value == "width"));
    assert!(
        changed
            .constraints
            .iter()
            .any(|c| c.kind == "Length" && c.value == "20")
    );
    let (back, solved, _) =
        sketch::set_construction(&changed, &solved, &chosen, None, &numbers).unwrap();
    assert!(back.entities[..4].iter().all(|e| !e.construction));
    assert!(
        back.entities[4].construction,
        "Q twice restores a mixed selection"
    );
    let (all, _, _) =
        sketch::set_construction(&back, &solved, &chosen, Some(true), &numbers).unwrap();
    assert!(
        all.entities
            .iter()
            .filter(|e| chosen.iter().any(|s| s.entity == e.id))
            .all(|e| e.construction)
    );
    let serialized = serde_json::to_string(&changed).unwrap();
    let reopened: Sketch = serde_json::from_str(&serialized).unwrap();
    assert_eq!(reopened, changed);
    let restored = sketch::resolve(&original, plane, &numbers).unwrap();
    assert_eq!(restored.entities[0].values, vec![-20., -10., 40., 20.]);
    // Explicit undo/redo document snapshots need no new schema or side channel.
    let registry = Registry::builtin().unwrap();
    let mut doc = Document::default();
    let mut feature = registry.create("sketch", &mut doc).unwrap();
    feature.set_sketch(&original);
    doc.features.push(feature);
    let before = doc.clone();
    doc.features[0].set_sketch(&changed);
    let after = doc.clone();
    doc = before.clone();
    assert_eq!(doc.features[0].sketch().unwrap(), original);
    doc = after;
    assert_eq!(doc.features[0].sketch().unwrap(), changed);
}
