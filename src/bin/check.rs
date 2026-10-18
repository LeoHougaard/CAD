use brepkit_math::mat::Mat4;
use brepkit_operations::{
    boolean::{BooleanOp, boolean, mesh_fallback_count},
    measure::solid_volume,
    primitives::{make_box, make_cylinder},
    transform::transform_solid,
};
use brepkit_topology::{Topology, explorer::solid_faces};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--inspect") {
        let path = args
            .get(2)
            .ok_or("Usage: cad-check --inspect document.cadpart [output.step]")?;
        let document = cad::model::Document::load(std::path::Path::new(path))?;
        let evaluation =
            cad::engine::Runtime::new(cad::plugins::Registry::builtin()?).evaluate(&document)?;
        if let Some(error) = &evaluation.scene.error {
            return Err(error.clone().into());
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "features": document.features.len(),
                "parts": evaluation.scene.meshes.len(),
                "volume": evaluation.scene.meshes.iter().map(|m| m.volume).sum::<f64>(),
                "sketches": evaluation.scene.sketches.iter().map(|(id, s)| serde_json::json!({"id": id,"dof": s.dof,"entities": s.entities.iter().map(|e| serde_json::json!({"kind":e.kind,"values":e.values})).collect::<Vec<_>>()})).collect::<Vec<_>>()
            }))?
        );
        if let Some(output) = args.get(3) {
            evaluation
                .geometry
                .export("step", std::path::Path::new(output))?;
        }
        return Ok(());
    }
    if std::env::args().any(|a| a == "--bench") {
        return benchmark().map_err(Into::into);
    }
    let t = std::time::Instant::now();
    let mut topo = Topology::new();
    let block = make_box(&mut topo, 60.0, 40.0, 8.0)?;
    let cutter = make_cylinder(&mut topo, 5.0, 12.0)?;
    transform_solid(&mut topo, cutter, &Mat4::translation(30.0, 20.0, -2.0))?;
    let before = mesh_fallback_count();
    let result = boolean(&mut topo, BooleanOp::Cut, block, cutter)?;
    let fallback = mesh_fallback_count() - before;
    let volume = solid_volume(&topo, result, 0.02)?;
    let expected = 60.0 * 40.0 * 8.0 - std::f64::consts::PI * 25.0 * 8.0;
    println!(
        "volume={volume}, expected={expected}, relative_error={}, faces={}, mesh_fallbacks={fallback}, elapsed_ms={}",
        (volume - expected).abs() / expected,
        solid_faces(&topo, result)?.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    assert!(fallback == 0, "precise boolean fell back to a mesh");
    assert!((volume - expected).abs() / expected < 1e-5);
    #[cfg(feature = "step")]
    {
        let step = brepkit_io::step::write_step(&topo, &[result])?;
        std::fs::create_dir_all("output")?;
        std::fs::write("output/kernel-probe.step", step)?;
    }
    Ok(())
}

fn benchmark() -> cad::model::Result<()> {
    use cad::{
        engine::Runtime,
        model::{Document, Sketch},
        plugins::Registry,
    };
    use serde_json::json;
    let registry = Registry::builtin()?;
    let mut d = Document {
        name: "Perforated panel".into(),
        ..Default::default()
    };
    let mut s = Sketch::default();
    s.add_rect([-100., -60.], [100., 60.]);
    let mut f = registry.create("sketch", &mut d)?;
    f.set_sketch(&s);
    d.features.push(f);
    let mut f = registry.create("extrude", &mut d)?;
    f.params["sketch"] = json!("f1");
    f.params["depth"] = json!("12");
    d.features.push(f);
    for n in 0..20 {
        let mut s = Sketch::default();
        s.add_circle(
            [-80. + (n % 5) as f64 * 40., -42. + (n / 5) as f64 * 28.],
            3.,
        );
        let mut f = registry.create("sketch", &mut d)?;
        f.params["support"] = json!("f2:end");
        f.set_sketch(&s);
        let id = f.id.clone();
        d.features.push(f);
        let mut f = registry.create("extrude", &mut d)?;
        f.params["sketch"] = json!(id);
        f.params["mode"] = json!("Remove");
        f.params["depth"] = json!("12");
        f.params["reverse"] = json!(true);
        d.features.push(f);
    }
    std::fs::create_dir_all("examples").map_err(|e| e.to_string())?;
    cad::model::demo().save(std::path::Path::new("examples/mounting-plate.cadpart"))?;
    d.save(std::path::Path::new("examples/perforated-panel.cadpart"))?;
    let mut rt = Runtime::new(registry);
    let mut full = vec![];
    let mut incremental = vec![];
    let mut last = None;
    for n in 0..12 {
        d.features[1].params["depth"] = json!((12. + (n % 2) as f64).to_string());
        let x = rt.evaluate(&d)?;
        if let Some(e) = &x.scene.error {
            return Err(e.clone());
        }
        full.push(x.scene.millis);
        d.features.last_mut().unwrap().params["depth"] = json!((12. + (n % 2) as f64).to_string());
        let x = rt.evaluate(&d)?;
        if let Some(e) = &x.scene.error {
            return Err(e.clone());
        }
        incremental.push(x.scene.millis);
        last = Some(x);
    }
    full.sort_by(f64::total_cmp);
    incremental.sort_by(f64::total_cmp);
    let x = last.unwrap();
    let report = json!({"features":d.features.len(),"parts":x.scene.meshes.len(),"faces":x.scene.meshes[0].faces.len(),"triangles":x.scene.meshes[0].indices.len()/3,"samples":12,"full_rebuild_median_ms":full[6],"full_rebuild_max_ms":full[11],"last_feature_edit_median_ms":incremental[6],"last_feature_edit_max_ms":incremental[11],"mesh_fallbacks":mesh_fallback_count(),"volume":x.scene.meshes[0].volume});
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    std::fs::create_dir_all("output").map_err(|e| e.to_string())?;
    std::fs::write(
        "output/benchmark.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    #[cfg(feature = "step")]
    x.geometry
        .export("step", std::path::Path::new("output/perforated-panel.step"))?;
    Ok(())
}
