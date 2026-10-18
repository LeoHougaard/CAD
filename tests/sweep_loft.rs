//! Workflow reference: Onshape's official Sweep and Loft help. The dimensions
//! below are analytic verification fixtures, not dimensions copied from a video.
use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Sketch, SketchConstraint},
    plugins::Registry,
};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

const PI: f64 = std::f64::consts::PI;
const DIRECTORY: &str = "output/sweep-loft";

fn constraint(kind: &str, a: u64, value: &str) -> SketchConstraint {
    SketchConstraint {
        kind: kind.into(),
        a,
        value: value.into(),
        ..Default::default()
    }
}
fn document(name: &str, variables: &[(&str, &str)]) -> Document {
    Document {
        name: name.into(),
        variables: variables
            .iter()
            .map(|&(k, v)| (k.into(), v.into()))
            .collect(),
        ..Default::default()
    }
}
fn circles(x: &str, y: &str, radii: &[&str]) -> Sketch {
    let mut s = Sketch::default();
    for (i, &radius) in radii.iter().enumerate() {
        s.add_circle([0., 0.], 10. - i as f64 * 3.);
        let id = s.next_id;
        for (kind, value) in [("X", x), ("Y", y), ("Radius", radius)] {
            s.constraints.push(constraint(kind, id, value));
        }
    }
    s
}
fn rectangle(x: &str, y: &str, width: &str, height: &str) -> Sketch {
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [10., 8.]);
    for (kind, value) in [("X", x), ("Y", y), ("Width", width), ("Height", height)] {
        s.constraints.push(constraint(kind, 1, value));
    }
    s
}
fn path(bent: bool) -> Sketch {
    let mut s = Sketch::default();
    s.add_line([0., 0.], [0., 20.]);
    s.constraints.extend([
        constraint("Origin", 1, ""),
        constraint("Vertical", 1, ""),
        constraint("Length", 1, "#rise"),
    ]);
    if bent {
        s.add_line([0., 20.], [30., 20.]);
        s.constraints.extend([
            constraint("Horizontal", 2, ""),
            constraint("Length", 2, "#run"),
        ]);
    }
    s
}
fn sketch(doc: &mut Document, name: &str, plane: &str, offset: &str, geometry: Sketch) -> String {
    let mut f = Registry::builtin().unwrap().create("sketch", doc).unwrap();
    f.name = name.into();
    f.params["plane"] = json!(plane);
    f.params["offset"] = json!(offset);
    f.set_sketch(&geometry);
    let id = f.id.clone();
    doc.features.push(f);
    id
}
fn sweep(doc: &mut Document, profile: &str, path: &str, mode: &str, target: &str) {
    let mut f = Registry::builtin().unwrap().create("sweep", doc).unwrap();
    f.params["profile"] = json!(profile);
    f.params["path"] = json!(path);
    f.params["mode"] = json!(mode);
    f.params["target"] = json!(target);
    doc.features.push(f);
}
fn loft(doc: &mut Document, profiles: &[String], mode: &str, target: &str) {
    let mut f = Registry::builtin().unwrap().create("loft", doc).unwrap();
    f.params["profiles"] = json!(profiles);
    f.params["mode"] = json!(mode);
    f.params["target"] = json!(target);
    doc.features.push(f);
}
fn evaluate(runtime: &mut Runtime, doc: &Document) -> Evaluation {
    let result = runtime.evaluate(doc).unwrap();
    assert!(result.scene.error.is_none(), "{:?}", result.scene.statuses);
    result
}
fn verify(result: &Evaluation, expected: f64) {
    verify_with_mass_tolerance(result, expected, 1e-7);
}
fn verify_with_mass_tolerance(result: &Evaluation, expected: f64, mass_relative_tolerance: f64) {
    assert_eq!(result.scene.meshes.len(), 1);
    assert!(result.scene.sketches.values().all(|s| s.dof == 0));
    let mesh = &result.scene.meshes[0];
    let tolerance = (expected.abs() * mass_relative_tolerance).max(1e-5);
    assert!(
        (mesh.volume - expected).abs() < tolerance,
        "Volume {} != {expected}, tolerance {tolerance}",
        mesh.volume
    );
    let p = &result.geometry.parts[0];
    let validity =
        brepkit_operations::validate::validate_solid(&result.geometry.topo, p.solid).unwrap();
    assert!(validity.is_valid(), "{:?}", validity.issues);
    let mut uses = BTreeMap::new();
    for triangle in mesh.indices.as_chunks::<3>().0 {
        for i in 0..3 {
            let a = triangle[i];
            let b = triangle[(i + 1) % 3];
            let edge = uses.entry((a.min(b), a.max(b))).or_insert((0, 0_i32));
            edge.0 += 1;
            edge.1 += if a < b { 1 } else { -1 };
        }
    }
    assert!(
        uses.values().all(|&(n, w)| n == 2 && w == 0),
        "Display mesh has a boundary or inconsistent winding"
    );
}
fn artifact(doc: &Document, result: &Evaluation, name: &str, expected: f64) {
    artifact_with_mass_tolerance(doc, result, name, expected, 1e-7);
}
fn artifact_with_mass_tolerance(
    doc: &Document,
    result: &Evaluation,
    name: &str,
    expected: f64,
    mass_relative_tolerance: f64,
) {
    let directory = Path::new(DIRECTORY);
    std::fs::create_dir_all(directory).unwrap();
    let path = directory.join(format!("{name}.cadpart"));
    doc.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();
    assert_eq!(loaded, *doc);
    verify_with_mass_tolerance(
        &evaluate(&mut Runtime::new(Registry::builtin().unwrap()), &loaded),
        expected,
        mass_relative_tolerance,
    );
    #[cfg(feature = "step")]
    {
        let path = directory.join(format!("{name}.step"));
        result.geometry.export("step", &path).unwrap();
        let mut topology = brepkit_topology::Topology::new();
        let solids =
            brepkit_io::step::read_step(&std::fs::read_to_string(path).unwrap(), &mut topology)
                .unwrap();
        assert_eq!(solids.len(), 1);
        assert!(
            brepkit_operations::validate::validate_solid(&topology, solids[0])
                .unwrap()
                .is_valid()
        );
        let volume = cad::engine::solid_volume(&topology, solids[0]).unwrap();
        assert!((volume - expected).abs() < (expected * mass_relative_tolerance).max(1e-5));
    }
    #[cfg(not(feature = "step"))]
    let _ = result;
    std::fs::write(directory.join(format!("{name}.json")),serde_json::to_vec_pretty(&json!({"case":name,"expected_volume_mm3":expected,"actual_volume_mm3":result.scene.meshes[0].volume,"mass_relative_tolerance":mass_relative_tolerance,"mass_measurement":if mass_relative_tolerance>1e-7 {"numerical torus volume estimate; exact geometry checked independently"} else {"strict analytic comparison"},"faces":result.scene.meshes[0].faces.len(),"all_sketches_zero_dof":true,"topology_and_closed_mesh_verified":true,"document_roundtrip_verified":true,"step_roundtrip_verified":cfg!(feature="step")})).unwrap()).unwrap();
}

fn diagnostic(doc: &Document, result: &Evaluation, name: &str) {
    let directory = Path::new(DIRECTORY);
    std::fs::create_dir_all(directory).unwrap();
    doc.save(&directory.join(format!("{name}.cadpart")))
        .unwrap();
    if name == "diagnostic-torus" {
        use brepkit_topology::face::FaceSurface;
        let topo = &result.geometry.topo;
        let solid = topo.solid(result.geometry.parts[0].solid).unwrap();
        let mut types = BTreeMap::new();
        for fid in
            brepkit_topology::explorer::solid_faces(topo, result.geometry.parts[0].solid).unwrap()
        {
            let face = topo.face(fid).unwrap();
            let kind = match face.surface() {
                FaceSurface::Plane { .. } => "plane",
                FaceSurface::Cylinder(_) => "cylinder",
                FaceSurface::Cone(_) => "cone",
                FaceSurface::Sphere(_) => "sphere",
                FaceSurface::Torus(_) => "torus",
                FaceSurface::Nurbs(_) => "nurbs",
            };
            *types.entry((kind, face.is_reversed())).or_insert(0) += 1;
        }
        println!(
            "Torus face types/reversed {types:?}; inner shells {}",
            solid.inner_shells().len()
        );
    }
    #[cfg(feature = "step")]
    result
        .geometry
        .export("step", &directory.join(format!("{name}.step")))
        .unwrap();
    #[cfg(not(feature = "step"))]
    let _ = result;
}

#[test]
fn straight_annular_sweep_preserves_exact_cylinders_after_radius_and_length_edits() {
    let mut doc = document(
        "Straight swept tube",
        &[("outer", "5"), ("inner", "3"), ("rise", "40")],
    );
    let profile = sketch(
        &mut doc,
        "Tube profile",
        "Top",
        "0",
        circles("0", "0", &["#outer", "#inner"]),
    );
    let spine = sketch(&mut doc, "Spine", "Front", "0", path(false));
    sweep(&mut doc, &profile, &spine, "New", "");
    let mut runtime = Runtime::new(Registry::builtin().unwrap());
    let result = evaluate(&mut runtime, &doc);
    let expected = PI * (25. - 9.) * 40.;
    verify(&result, expected);
    assert_eq!(result.scene.meshes[0].faces.len(), 4);
    artifact(&doc, &result, "straight-annular", expected);
    for (key, value) in [("outer", "6"), ("inner", "2"), ("rise", "65")] {
        doc.variables.insert(key.into(), value.into());
    }
    let result = evaluate(&mut runtime, &doc);
    let expected = PI * (36. - 4.) * 65.;
    verify(&result, expected);
    artifact(&doc, &result, "straight-annular-edited", expected);
}

#[test]
fn circular_path_sweep_is_an_exact_hollow_torus_and_retains_position() {
    let mut doc = document(
        "Circular swept tube",
        &[("major", "20"), ("outer", "3"), ("inner", "1")],
    );
    let profile = sketch(
        &mut doc,
        "Tube profile",
        "Front",
        "0",
        circles("#major", "0", &["#outer", "#inner"]),
    );
    let spine = sketch(
        &mut doc,
        "Circular spine",
        "Top",
        "0",
        circles("0", "0", &["#major"]),
    );
    sweep(&mut doc, &profile, &spine, "New", "");
    let mut runtime = Runtime::new(Registry::builtin().unwrap());
    let result = evaluate(&mut runtime, &doc);
    let expected = 2. * PI * PI * 20. * (9. - 1.);
    diagnostic(&doc, &result, "diagnostic-torus");
    verify_with_mass_tolerance(&result, expected, 1e-4);
    artifact_with_mass_tolerance(&doc, &result, "circular-annular", expected, 1e-4);
    for (key, value) in [("major", "28"), ("outer", "4"), ("inner", "2")] {
        doc.variables.insert(key.into(), value.into());
    }
    let result = evaluate(&mut runtime, &doc);
    let expected = 2. * PI * PI * 28. * (16. - 4.);
    verify_with_mass_tolerance(&result, expected, 1e-4);
    artifact_with_mass_tolerance(&doc, &result, "circular-annular-edited", expected, 1e-4);
}

#[test]
fn polygon_sweep_forms_an_exact_mitred_elbow_with_parametric_legs() {
    let mut doc = document(
        "Mitred rectangular elbow",
        &[
            ("rise", "20"),
            ("run", "30"),
            ("width", "4"),
            ("height", "2"),
        ],
    );
    let profile = sketch(
        &mut doc,
        "Elbow profile",
        "Top",
        "0",
        rectangle("-#width/2", "-#height/2", "#width", "#height"),
    );
    let spine = sketch(&mut doc, "Elbow spine", "Front", "0", path(true));
    sweep(&mut doc, &profile, &spine, "New", "");
    let mut runtime = Runtime::new(Registry::builtin().unwrap());
    let result = evaluate(&mut runtime, &doc);
    verify(&result, 4. * 2. * 50.);
    artifact(&doc, &result, "mitred-elbow", 400.);
    for (key, value) in [
        ("rise", "35"),
        ("run", "18"),
        ("width", "6"),
        ("height", "3"),
    ] {
        doc.variables.insert(key.into(), value.into());
    }
    let result = evaluate(&mut runtime, &doc);
    let expected = 6. * 3. * 53.;
    verify(&result, expected);
    artifact(&doc, &result, "mitred-elbow-edited", expected);
}

fn circle_frustum(r0: f64, r1: f64, h: f64) -> f64 {
    PI * h * (r0 * r0 + r0 * r1 + r1 * r1) / 3.
}
fn rectangular_frustum(w0: f64, h0: f64, w1: f64, h1: f64, length: f64) -> f64 {
    length * (2. * w0 * h0 + w0 * h1 + w1 * h0 + 2. * w1 * h1) / 6.
}

#[test]
fn two_and_three_circle_lofts_have_analytic_frustum_volumes_and_editable_sections() {
    for three in [false, true] {
        let mut doc = document(
            "Circular section loft",
            &[
                ("r0", "5"),
                ("r1", "9"),
                ("r2", "3"),
                ("z1", "20"),
                ("z2", "50"),
            ],
        );
        let mut profiles = vec![
            sketch(&mut doc, "Start", "Top", "0", circles("0", "0", &["#r0"])),
            sketch(
                &mut doc,
                "Middle",
                "Top",
                "#z1",
                circles("0", "0", &["#r1"]),
            ),
        ];
        if three {
            profiles.push(sketch(
                &mut doc,
                "End",
                "Top",
                "#z2",
                circles("0", "0", &["#r2"]),
            ));
        }
        loft(&mut doc, &profiles, "New", "");
        let mut runtime = Runtime::new(Registry::builtin().unwrap());
        let expected = circle_frustum(5., 9., 20.)
            + if three {
                circle_frustum(9., 3., 30.)
            } else {
                0.
            };
        let result = evaluate(&mut runtime, &doc);
        verify(&result, expected);
        artifact(
            &doc,
            &result,
            if three {
                "loft-three-circles"
            } else {
                "loft-two-circles"
            },
            expected,
        );
        for (key, value) in [("r1", "12"), ("z1", "30"), ("z2", "80")] {
            doc.variables.insert(key.into(), value.into());
        }
        let expected = circle_frustum(5., 12., 30.)
            + if three {
                circle_frustum(12., 3., 50.)
            } else {
                0.
            };
        let result = evaluate(&mut runtime, &doc);
        verify(&result, expected);
        assert!((result.scene.sketches[&profiles[1]].plane.origin[2] - 30.).abs() < 1e-8);
        artifact(
            &doc,
            &result,
            if three {
                "loft-three-circles-edited"
            } else {
                "loft-two-circles-edited"
            },
            expected,
        );
    }
}

#[test]
fn two_and_three_rectangular_lofts_preserve_ruled_shape_under_size_and_offset_edits() {
    for three in [false, true] {
        let mut doc = document(
            "Offset rectangular section loft",
            &[
                ("w1", "10"),
                ("h1", "16"),
                ("x1", "5"),
                ("z1", "25"),
                ("z2", "60"),
            ],
        );
        let mut profiles = vec![
            sketch(
                &mut doc,
                "Start",
                "Top",
                "0",
                rectangle("-10", "-5", "20", "10"),
            ),
            sketch(
                &mut doc,
                "Middle",
                "Top",
                "#z1",
                rectangle("#x1-#w1/2", "-#h1/2", "#w1", "#h1"),
            ),
        ];
        if three {
            profiles.push(sketch(
                &mut doc,
                "End",
                "Top",
                "#z2",
                rectangle("-18", "-4", "30", "8"),
            ));
        }
        loft(&mut doc, &profiles, "New", "");
        let expected = rectangular_frustum(20., 10., 10., 16., 25.)
            + if three {
                rectangular_frustum(10., 16., 30., 8., 35.)
            } else {
                0.
            };
        let mut runtime = Runtime::new(Registry::builtin().unwrap());
        let result = evaluate(&mut runtime, &doc);
        verify(&result, expected);
        artifact(
            &doc,
            &result,
            if three {
                "loft-three-rectangles"
            } else {
                "loft-two-rectangles"
            },
            expected,
        );
        for (key, value) in [
            ("w1", "14"),
            ("h1", "12"),
            ("x1", "-2"),
            ("z1", "35"),
            ("z2", "80"),
        ] {
            doc.variables.insert(key.into(), value.into());
        }
        let expected = rectangular_frustum(20., 10., 14., 12., 35.)
            + if three {
                rectangular_frustum(14., 12., 30., 8., 45.)
            } else {
                0.
            };
        let result = evaluate(&mut runtime, &doc);
        verify(&result, expected);
        assert!((result.scene.sketches[&profiles[1]].entities[0].values[0] + 9.).abs() < 1e-7);
        artifact(
            &doc,
            &result,
            if three {
                "loft-three-rectangles-edited"
            } else {
                "loft-two-rectangles-edited"
            },
            expected,
        );
    }
}

fn stock() -> Document {
    let mut doc = document(
        "Sweep and loft failure recovery",
        &[("rise", "20"), ("run", "30")],
    );
    let profile = sketch(
        &mut doc,
        "Stock profile",
        "Top",
        "0",
        rectangle("0", "0", "10", "8"),
    );
    let mut feature = Registry::builtin()
        .unwrap()
        .create("extrude", &mut doc)
        .unwrap();
    feature.params["sketch"] = json!(profile);
    feature.params["depth"] = json!("3");
    doc.features.push(feature);
    doc
}

fn polygon(points: &[[f64; 2]], coordinates: &[(&str, &str)]) -> Sketch {
    let mut sketch = Sketch::default();
    for i in 0..points.len() {
        sketch.add_line(points[i], points[(i + 1) % points.len()]);
        sketch
            .constraints
            .push(constraint("X", sketch.next_id, coordinates[i].0));
        sketch
            .constraints
            .push(constraint("Y", sketch.next_id, coordinates[i].1));
    }
    sketch
}
fn area(points: &[[f64; 2]]) -> f64 {
    (0..points.len())
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.
}
fn morphed_polygon_volume(start: &[[f64; 2]], end: &[[f64; 2]], height: f64) -> f64 {
    let middle: Vec<_> = start
        .iter()
        .zip(end)
        .map(|(a, b)| [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.])
        .collect();
    height * (area(start) + 4. * area(&middle) + area(end)) / 6.
}

#[test]
fn rotated_polygon_loft_has_true_bilinear_sides_and_exact_integrated_volume() {
    let start = [[-10., -10.], [10., -10.], [10., 10.], [-10., 10.]];
    let end = [[0., -8.], [8., 0.], [0., 8.], [-8., 0.]];
    let mut doc = document(
        "Rotated section bilinear loft",
        &[("end_half", "8"), ("height", "30")],
    );
    let p = sketch(
        &mut doc,
        "Square start",
        "Top",
        "0",
        polygon(
            &start,
            &[("-10", "-10"), ("10", "-10"), ("10", "10"), ("-10", "10")],
        ),
    );
    let q = sketch(
        &mut doc,
        "Rotated end",
        "Top",
        "#height",
        polygon(
            &end,
            &[
                ("0", "-#end_half"),
                ("#end_half", "0"),
                ("0", "#end_half"),
                ("-#end_half", "0"),
            ],
        ),
    );
    loft(&mut doc, &[p, q], "New", "");
    let expected = morphed_polygon_volume(&start, &end, 30.);
    assert!((expected - 6880.).abs() < 1e-8);
    let mut runtime = Runtime::new(Registry::builtin().unwrap());
    let result = evaluate(&mut runtime, &doc);
    verify(&result, expected);
    // A planar replacement of a twisted quadrilateral changes this geometry.
    assert!(
        result.scene.meshes[0]
            .faces
            .iter()
            .any(|f| f.plane.is_none())
    );
    artifact(&doc, &result, "loft-bilinear-rotated", expected);
    doc.variables.insert("end_half".into(), "12".into());
    doc.variables.insert("height".into(), "45".into());
    let end = [[0., -12.], [12., 0.], [0., 12.], [-12., 0.]];
    let expected = morphed_polygon_volume(&start, &end, 45.);
    let result = evaluate(&mut runtime, &doc);
    diagnostic(&doc, &result, "diagnostic-bilinear-edited");
    verify(&result, expected);
    artifact(&doc, &result, "loft-bilinear-rotated-edited", expected);
}

#[test]
fn loft_handles_antiparallel_section_normals_and_adds_to_the_support_solid() {
    let mut doc = stock();
    doc.features[0].params["offset"] = json!("20");
    let lower = sketch(
        &mut doc,
        "Lower section",
        "Top",
        "0",
        rectangle("0", "0", "10", "8"),
    );
    let upper = sketch(
        &mut doc,
        "Upper reverse-normal section",
        "Top",
        "0",
        rectangle("0", "-8", "10", "8"),
    );
    doc.features.last_mut().unwrap().params["support"] = json!("f2:start");
    loft(&mut doc, &[lower.clone(), upper.clone()], "Add", "f2:0");
    let result = evaluate(&mut Runtime::new(Registry::builtin().unwrap()), &doc);
    assert_eq!(result.scene.sketches[&lower].plane.normal, [0., 0., 1.]);
    assert_eq!(result.scene.sketches[&upper].plane.normal, [-0., -0., -1.]);
    diagnostic(&doc, &result, "diagnostic-antiparallel-add");
    verify(&result, 10. * 8. * 23.);
    artifact(&doc, &result, "loft-antiparallel-add", 1840.);
}

#[test]
fn invalid_sweep_and_loft_inputs_preserve_the_previous_solid_and_runtime_recovers() {
    let mut runtime = Runtime::new(Registry::builtin().unwrap());
    let base = stock();
    verify(&evaluate(&mut runtime, &base), 240.);
    let mut cases = vec![];
    let mut doc = base.clone();
    let p = sketch(&mut doc, "Profile", "Top", "0", circles("0", "0", &["2"]));
    let spine = sketch(&mut doc, "Path", "Front", "0", path(false));
    sweep(&mut doc, &p, &spine, "New", "");
    let valid = doc.clone();
    doc.features.last_mut().unwrap().params["path"] = json!("missing");
    cases.push(("missing-sweep-path", doc));
    let mut doc = base.clone();
    let p = sketch(
        &mut doc,
        "Profile",
        "Top",
        "0",
        circles("0", "0", &["2", "1"]),
    );
    let spine = sketch(&mut doc, "Path", "Front", "0", path(true));
    sweep(&mut doc, &p, &spine, "New", "");
    cases.push(("unsupported-sharp-annular-bend", doc));
    let mut doc = base.clone();
    let p = sketch(&mut doc, "Profile", "Front", "0", circles("0", "0", &["2"]));
    let spine = sketch(&mut doc, "Path", "Front", "0", path(false));
    sweep(&mut doc, &p, &spine, "New", "");
    cases.push(("profile-not-normal-to-path", doc));
    let mut doc = base.clone();
    let p = sketch(&mut doc, "Section", "Top", "5", circles("0", "0", &["2"]));
    loft(&mut doc, std::slice::from_ref(&p), "New", "");
    cases.push(("one-loft-profile", doc));
    let mut doc = base.clone();
    let p = sketch(
        &mut doc,
        "Holed section",
        "Top",
        "5",
        circles("0", "0", &["3", "1"]),
    );
    let q = sketch(&mut doc, "Section", "Top", "15", circles("0", "0", &["2"]));
    loft(&mut doc, &[p, q], "New", "");
    cases.push(("loft-inner-wire", doc));
    let mut doc = base.clone();
    let p = sketch(
        &mut doc,
        "Circular section",
        "Top",
        "5",
        circles("0", "0", &["3"]),
    );
    let q = sketch(
        &mut doc,
        "Rectangular section",
        "Top",
        "15",
        rectangle("-2", "-2", "4", "4"),
    );
    loft(&mut doc, &[p, q], "New", "");
    cases.push(("mixed-loft-profile-types", doc));
    let mut doc = base.clone();
    let start = [[-10., -10.], [10., -10.], [10., 10.], [-10., 10.]];
    let end = [[10., 10.], [-10., 10.], [-10., -10.], [10., -10.]];
    let p = sketch(
        &mut doc,
        "Start section",
        "Top",
        "5",
        polygon(
            &start,
            &[("-10", "-10"), ("10", "-10"), ("10", "10"), ("-10", "10")],
        ),
    );
    let q = sketch(
        &mut doc,
        "Collapsed correspondence",
        "Top",
        "15",
        polygon(
            &end,
            &[("10", "10"), ("-10", "10"), ("-10", "-10"), ("10", "-10")],
        ),
    );
    loft(&mut doc, &[p, q], "New", "");
    cases.push(("loft-180-degree-correspondence-collapse", doc));
    let directory = Path::new(DIRECTORY);
    std::fs::create_dir_all(directory).unwrap();
    let mut failures = vec![];
    for (name, doc) in cases {
        let failed = runtime.evaluate(&doc).unwrap();
        let error = failed
            .scene
            .error
            .clone()
            .expect("Invalid input unexpectedly succeeded");
        verify(&failed, 240.);
        doc.save(&directory.join(format!("{name}-rejected.cadpart")))
            .unwrap();
        failures.push(json!({"case":name,"error":error,"previous_volume_preserved_mm3":240.}));
        let recovered = evaluate(&mut runtime, &valid);
        assert_eq!(recovered.scene.meshes.len(), 2);
        assert!((recovered.scene.meshes[0].volume - 240.).abs() < 1e-6);
    }
    std::fs::write(directory.join("rejections.json"),serde_json::to_vec_pretty(&json!({"source_workflow_references":["https://cad.onshape.com/help/Content/PartStudio/sweep.htm","https://cad.onshape.com/help/Content/PartStudio/loft.htm"],"verification":"Rust document/solver/kernel APIs, no native UI entry","cases":failures})).unwrap()).unwrap();
}
