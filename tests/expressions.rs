use cad::{
    engine::Runtime,
    model::{Document, Sketch},
    plugins::{Numbers, Registry},
};
use serde_json::json;
use std::collections::BTreeMap;

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-10 * expected.abs().max(1.),
        "{actual} != {expected}"
    );
}

#[test]
fn calculator_syntax_and_degree_functions_match_numeric_fields() {
    let n = Numbers::new(&BTreeMap::from([
        ("width".into(), "20 mm".into()),
        ("x".into(), "6".into()),
    ]))
    .unwrap();
    for (text, expected) in [
        ("= #width/2 + 3", 13.),
        ("2^3^2", 512.),
        ("(-2)^2", 4.),
        ("-2^2", -4.),
        ("2^-2", 0.25),
        ("1/2 inch", 12.7),
        ("2 x 3", 6.),
        ("#x x #x", 36.),
        ("2×3÷4", 1.5),
        ("2(#width+5)", 50.),
        ("(2+3)(4+1)", 25.),
        ("1e-3*1000", 1.),
        ("sin(30)", 0.5),
        ("cos(60°)", 0.5),
        ("tan(45 deg)", 1.),
        ("asin(0.5)", 30.),
        ("acos(0.5)", 60.),
        ("atan(1)", 45.),
        ("atan2(1,-1)", 135.),
        ("sin(pi/2 rad)", 1.),
        ("sqrt(25)", 5.),
        ("√(25)", 5.),
        ("root(81,4)", 3.),
        ("root(-27,3)", -3.),
        ("cbrt(-8)", -2.),
        ("log(e)", 1.),
        ("log10(100)", 2.),
        ("max(2,min(4,3))", 3.),
        ("2π", std::f64::consts::TAU),
    ] {
        close(
            n.eval(text).unwrap_or_else(|e| panic!("{text}: {e}")),
            expected,
        );
    }
}

#[test]
fn domains_malformed_expressions_and_variable_cycles_are_rejected() {
    let n = Numbers::new(&BTreeMap::new()).unwrap();
    for text in [
        "1/0",
        "sqrt(-1)",
        "root(-4,2)",
        "root(9,0)",
        "asin(2)",
        "log(0)",
        "min(sqrt(-1),4)",
        "min(1/0,4)",
        "exp(1000)",
        "sin(",
        "#",
        "unknown+1",
        "1e100",
        "2;;3",
    ] {
        assert!(n.eval(text).is_err(), "Must reject {text}");
    }
    assert!(
        Numbers::new(&BTreeMap::from([
            ("a".into(), "b+1".into()),
            ("b".into(), "a-1".into())
        ]))
        .is_err()
    );
}

#[test]
fn old_documents_keep_radian_semantics_and_new_documents_use_degrees() {
    let mut d = Document::default();
    assert_eq!(d.format, 2);
    close(
        Numbers::for_document(&d).unwrap().eval("sin(30)").unwrap(),
        0.5,
    );
    d.format = 1;
    d.variables.insert("pi".into(), "7".into());
    let legacy = Numbers::for_document(&d).unwrap();
    close(legacy.eval("sin(PI/2)").unwrap(), 1.);
    close(legacy.eval("pi").unwrap(), 7.);
    assert!(legacy.eval("2^3").is_err());
}

#[test]
fn expressions_remain_parametric_through_features_variables_and_reopen() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    d.variables.insert("width".into(), "2*sqrt(25)".into());
    let mut sketch = Sketch::default();
    sketch.add_rect([0., 0.], [10., 10.]);
    sketch.entities[0].values = vec!["0".into(), "0".into(), "#width".into(), "10".into()];
    let mut f = r.create("sketch", &mut d).unwrap();
    f.set_sketch(&sketch);
    d.features.push(f);
    let mut f = r.create("extrude", &mut d).unwrap();
    f.params["sketch"] = json!("f1");
    f.params["depth"] = json!("20*sin(30)");
    d.features.push(f);
    let mut runtime = Runtime::new(r);
    let first = runtime.evaluate(&d).unwrap();
    assert!(first.scene.error.is_none(), "{:?}", first.scene.error);
    close(first.scene.meshes[0].volume, 1000.);
    d.variables.insert("width".into(), "2*root(81,4)".into());
    let reopened: Document = serde_json::from_slice(&serde_json::to_vec(&d).unwrap()).unwrap();
    assert_eq!(reopened.features[1].params["depth"], "20*sin(30)");
    close(
        runtime.evaluate(&reopened).unwrap().scene.meshes[0].volume,
        600.,
    );
}
