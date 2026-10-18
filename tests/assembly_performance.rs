//! Cached moderate-part pose updates and fresh exact assembly STEP fixtures.
use cad::{
    assembly::*,
    engine::MeshView,
    model::{Document, Sketch},
    plugins::Registry,
};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Instant};
fn block(r: &Registry) -> Document {
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [10., 8.]);
    let mut f = r.create("sketch", &mut d).unwrap();
    f.set_sketch(&s);
    d.features.push(f);
    let mut f = r.create("extrude", &mut d).unwrap();
    f.params["sketch"] = json!("f1");
    f.params["depth"] = json!("5");
    d.features.push(f);
    d
}
fn ring(r: &Registry) -> Document {
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 7.);
    s.add_circle([0., 0.], 3.);
    let mut f = r.create("sketch", &mut d).unwrap();
    f.set_sketch(&s);
    d.features.push(f);
    let mut f = r.create("extrude", &mut d).unwrap();
    f.params["sketch"] = json!("f1");
    f.params["depth"] = json!("9");
    d.features.push(f);
    d
}
fn pose(n: usize, sample: usize) -> Transform {
    let mut t = Transform::from_euler_degrees([0., 0., ((n + sample) % 4) as f64 * 90.]);
    t.translation = [
        (n % 6) as f64 * 110.,
        (n / 6) as f64 * 90.,
        sample as f64 * 0.125,
    ];
    t
}
fn percentile(values: &[f64], q: f64) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * q).ceil() as usize]
}
fn close(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() < tol, "{a} != {b}");
}
fn transformed_bounds(bounds: [[f64; 3]; 2], t: Transform) -> [[f64; 3]; 2] {
    let mut result = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    for x in [bounds[0][0], bounds[1][0]] {
        for y in [bounds[0][1], bounds[1][1]] {
            for z in [bounds[0][2], bounds[1][2]] {
                let p = t.point([x, y, z]);
                for k in 0..3 {
                    result[0][k] = result[0][k].min(p[k]);
                    result[1][k] = result[1][k].max(p[k]);
                }
            }
        }
    }
    result
}
fn mesh_bytes(m: &MeshView) -> usize {
    m.positions.len() * 12
        + m.normals.len() * 12
        + m.indices.len() * 4
        + m.edges.iter().map(|e| e.points.len() * 12).sum::<usize>()
}
fn resources() -> Value {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output=std::process::Command::new("powershell.exe").args(["-NoProfile","-Command",&format!("Get-Process -Id {} | Select-Object WorkingSet64,PeakWorkingSet64,PrivateMemorySize64,@{{Name='CpuModel';Expression={{(Get-CimInstance Win32_Processor).Name}}}} | ConvertTo-Json -Compress",std::process::id())]).creation_flags(0x08000000).output();
        output
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| serde_json::from_slice(&o.stdout).ok())
            .unwrap_or(Value::Null)
    }
    #[cfg(not(windows))]
    {
        json!({"proc_status":std::fs::read_to_string("/proc/self/status").ok().map(|s|s.lines().filter(|l|l.starts_with("VmRSS:")||l.starts_with("VmHWM:")).collect::<Vec<_>>().join("\n")),"cpu_model":std::fs::read_to_string("/proc/cpuinfo").ok().and_then(|s|s.lines().find(|l|l.starts_with("model name")||l.starts_with("Hardware")).map(str::to_owned))})
    }
}
fn fixture(
    folder: &Path,
    name: &str,
    d: &AssemblyDocument,
    runtime: &mut AssemblyRuntime,
    expected: Vec<Value>,
) {
    let e = runtime.evaluate(d).unwrap();
    assert!(e.scene.error.is_none());
    let exact = e.geometry(d).unwrap();
    assert_eq!(exact.parts.len(), expected.len());
    for p in &exact.parts {
        brepkit_operations::validate::validate_solid(&exact.topo, p.solid).unwrap();
    }
    #[cfg(feature = "step")]
    e.export(d, "step", &folder.join(format!("{name}.step")))
        .unwrap();
    d.save(&folder.join(format!("{name}.cadasm"))).unwrap();
    std::fs::write(
        folder.join(format!("{name}.json")),
        serde_json::to_vec_pretty(
            &json!({"case":name,"step_exported":cfg!(feature="step"),"expected_solids":expected}),
        )
        .unwrap(),
    )
    .unwrap();
}
#[test]
fn twenty_four_bearing_blocks_reuse_exact_source_and_verify_assembly_exports() {
    let r = Arc::new(Registry::builtin().unwrap());
    let bearing = Document::load(Path::new("examples/project-bearing-block.cadpart")).unwrap();
    let mut d = AssemblyDocument {
        name: "Bearing block pose benchmark".into(),
        ..Default::default()
    };
    d.sources
        .push(Source::new("bearing".into(), "Bearing block".into(), bearing, None).unwrap());
    for n in 0..24 {
        let mut i = Instance::new(format!("bearing{n}"), "bearing".into(), "f2:0".into());
        i.fixed = n == 0;
        d.instances.push(i);
        if n > 0 {
            let mut m = Mate::new(
                MateKind::Fastened,
                Connector::origin("bearing0"),
                Connector::origin(format!("bearing{n}")),
            );
            m.id = format!("mount{n}");
            m.offset = pose(n, 0);
            d.mates.push(m);
        }
    }
    let mut runtime = AssemblyRuntime::new(r.clone());
    let first = runtime.evaluate(&d).unwrap();
    assert_eq!(first.evaluated_sources, 1);
    assert_eq!(first.sources["bearing"].scene.meshes.len(), 1);
    let original = &first.sources["bearing"].scene.meshes[0];
    assert_eq!(d.sources[0].document.features.len(), 10);
    let mut samples = vec![];
    for sample in 0..24 {
        for (n, m) in d.mates.iter_mut().enumerate() {
            m.offset = pose(n + 1, sample);
        }
        let start = Instant::now();
        let e = runtime.evaluate(&d).unwrap();
        samples.push(start.elapsed().as_secs_f64() * 1000.);
        assert_eq!(e.evaluated_sources, 0);
        assert_eq!(e.degrees_of_freedom, 0);
        assert_eq!(e.scene.meshes.len(), 24);
        assert!(Arc::ptr_eq(
            &first.sources["bearing"],
            &e.sources["bearing"]
        ));
        for n in 1..24 {
            let mesh = &e.scene.meshes[n];
            let p = original.positions[0];
            let (x, y) = match (n + sample) % 4 {
                0 => (p[0], p[1]),
                1 => (-p[1], p[0]),
                2 => (-p[0], -p[1]),
                _ => (p[1], -p[0]),
            };
            close(
                mesh.positions[0][0] as f64,
                x as f64 + (n % 6) as f64 * 110.,
                1e-4,
            );
            close(
                mesh.positions[0][1] as f64,
                y as f64 + (n / 6) as f64 * 90.,
                1e-4,
            );
            close(
                mesh.positions[0][2] as f64,
                p[2] as f64 + sample as f64 * 0.125,
                1e-4,
            );
            close(mesh.volume, original.volume, 1e-6);
        }
    }
    let folder = Path::new("target/test-output/assembly-performance");
    std::fs::create_dir_all(folder).unwrap();
    let report = json!({"result":"passed","profile":if cfg!(debug_assertions){"debug"}else{"release"},"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,"instances":24,"source_features":10,"source_triangles":original.indices.len()/3,"source_evaluations_after_warmup":0,"samples":samples.len(),"pose_update_ms":{"p50":percentile(&samples,0.5),"p95":percentile(&samples,0.95)},"display_buffer_bytes":mesh_bytes(original)*24,"source_buffer_bytes":mesh_bytes(original),"process_memory_after_updates":resources(),"scope":"Cached source topology and tessellation; includes CPU transformed display-buffer copies, excludes GPU uploads and interactive frames"});
    std::fs::write(
        folder.join("verification.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
    // Three independently placed moderate solids from one exact source arena.
    let mut separated = d.clone();
    separated.instances.truncate(3);
    separated.mates.truncate(2);
    for (n, m) in separated.mates.iter_mut().enumerate() {
        m.offset = pose(n + 1, 0);
    }
    let bounds = [[-40., -25., 0.], [40., 25., 36.]];
    let volume = 60059.02730788345;
    fixture(folder,"bearing-blocks",&separated,&mut runtime,(0..3).map(|n|json!({"volume_mm3":volume,"bounds_mm":transformed_bounds(bounds,if n==0{Transform::default()}else{pose(n,0)})})).collect());
    let mut curved = AssemblyDocument::default();
    curved
        .sources
        .push(Source::new("ring".into(), "Ring".into(), ring(&r), None).unwrap());
    let mut expected = vec![];
    for n in 0..3 {
        let mut i = Instance::new(format!("ring{n}"), "ring".into(), "f2:0".into());
        i.fixed = true;
        i.transform = Transform::from_euler_degrees(if n == 1 {
            [90., 0., 0.]
        } else if n == 2 {
            [0., 90., 0.]
        } else {
            [0.; 3]
        });
        i.transform.translation = [n as f64 * 40., 0., 0.];
        expected.push(json!({"volume_mm3":std::f64::consts::PI*40.*9.,"bounds_mm":transformed_bounds([[-7.,-7.,0.],[7.,7.,9.]],i.transform)}));
        curved.instances.push(i);
    }
    fixture(folder, "analytic-rings", &curved, &mut runtime, expected);
    let mut context = AssemblyDocument::default();
    context
        .sources
        .push(Source::new("primary".into(), "Primary".into(), block(&r), None).unwrap());
    context
        .sources
        .push(Source::new("reference".into(), "Reference".into(), block(&r), None).unwrap());
    let mut p = Instance::new("primary_instance".into(), "primary".into(), "f2:0".into());
    p.fixed = true;
    context.instances.push(p);
    let mut reference = Instance::new(
        "reference_instance".into(),
        "reference".into(),
        "f2:0".into(),
    );
    reference.fixed = true;
    reference.transform = Transform::translated([30., 0., 20.]);
    context.instances.push(reference);
    let mut snapshot = runtime
        .capture_context(&context, "primary_instance")
        .unwrap();
    let mut primary = context.sources[0].document.clone();
    runtime
        .bind_context(
            &mut snapshot,
            &mut primary,
            ContextBinding {
                feature: "f2".into(),
                instance: "reference_instance".into(),
                face: "f2:end".into(),
                kind: BindingKind::ExtrudeUpToFace,
                offset: 0.,
            },
        )
        .unwrap();
    runtime
        .update_source(&mut context, "primary", primary)
        .unwrap();
    context.contexts.push(snapshot);
    fixture(
        folder,
        "context-bound",
        &context,
        &mut runtime,
        vec![
            json!({"volume_mm3":2000.,"bounds_mm":[[0.,0.,0.],[10.,8.,25.]]}),
            json!({"volume_mm3":400.,"bounds_mm":[[30.,0.,20.],[40.,8.,25.]]}),
        ],
    );
}
