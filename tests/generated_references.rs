//! A generated solid must remain usable by later parametric features.
use cad::{engine::{Evaluation, Runtime}, model::{Document, Sketch}, plugins::Registry};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

fn feature(r:&Registry, d:&mut Document, plugin:&str, params:Value) -> String {
    let mut f=r.create(plugin,d).unwrap();
    for (k,v) in params.as_object().unwrap() { f.params[k]=v.clone(); }
    let id=f.id.clone(); d.features.push(f); id
}
fn sketch(r:&Registry,d:&mut Document,s:Sketch,offset:&str)->String {
    let mut f=r.create("sketch",d).unwrap(); f.set_sketch(&s); f.params["offset"]=json!(offset);
    let id=f.id.clone(); d.features.push(f);id
}
fn valid(rt:&mut Runtime,d:&Document)->Evaluation {
    let e=rt.evaluate(d).unwrap();
    assert!(e.scene.error.is_none(),"{:?}",e.scene.statuses);
    for p in &e.geometry.parts {
        let check=brepkit_operations::validate::validate_solid(&e.geometry.topo,p.solid).unwrap();
        assert!(check.is_valid(),"{:?}",check.issues);
    }
    e
}
fn keys(e:&Evaluation)->BTreeSet<String> {
    e.scene.meshes.iter().flat_map(|m|m.faces.iter().map(|f|format!("{}:{}",m.part,f.key))
        .chain(m.edges.iter().map(|e|format!("{}:{}",m.part,e.key)))).collect()
}
fn assert_tracked(e:&Evaluation) {
    let keys=keys(e);
    assert!(!keys.is_empty());
    assert!(keys.iter().all(|key|!key.contains("untracked:")),"{keys:#?}");
}
#[cfg(feature="step")]
fn artifact(d:&Document,e:&Evaluation,name:&str,volume:f64,bounds:[[f64;3];2]) {
    let directory=Path::new("target/test-output/generated-references");
    std::fs::create_dir_all(directory).unwrap();
    d.save(&directory.join(format!("{name}.cadpart"))).unwrap();
    e.geometry.export("step",&directory.join(format!("{name}.step"))).unwrap();
    std::fs::write(directory.join(format!("{name}.json")),serde_json::to_vec_pretty(&json!({
        "case":name,"step_exported":true,
        "expected_solids":[{"volume_mm3":volume,"bounds_mm":bounds}],
        "verification":"Independent analytic dimensions, not application-measured mass or bounds"
    })).unwrap()).unwrap();
}
fn rectangle_loft(r:&Registry)->Document {
    let mut d=Document::default();
    d.variables.insert("rise".into(),"20".into());
    let mut s=Sketch::default();s.add_rect([-5.,-4.],[5.,4.]);
    let a=sketch(r,&mut d,s.clone(),"0");
    let b=sketch(r,&mut d,s,"#rise");
    feature(r,&mut d,"loft",json!({"profiles":[a,b]}));d
}

#[test]
fn supported_loft_and_sweep_side_and_ring_keys_survive_dimension_edits() {
    let r=Registry::builtin().unwrap();
    for (name,variable,value) in [("loft-reducer","r1","11"),("sweep-elbow","rise","25")] {
        let mut d=Document::load(&Path::new("examples").join(format!("{name}.cadpart"))).unwrap();
        let mut rt=Runtime::new(Registry::builtin().unwrap());
        let original=valid(&mut rt,&d);assert_tracked(&original);
        let before=keys(&original);
        d.variables.insert(variable.into(),value.into());
        let edited=valid(&mut rt,&d);assert_tracked(&edited);
        assert_eq!(keys(&edited),before);
        assert_ne!(edited.scene.meshes[0].volume,original.scene.meshes[0].volume);
    }
    let mut d=rectangle_loft(&r);
    let mut rt=Runtime::new(r);
    let first=valid(&mut rt,&d);assert_tracked(&first);
    d.variables.insert("rise".into(),"30".into());
    assert_eq!(keys(&valid(&mut rt,&d)),keys(&first));
}

#[test]
fn loft_side_attachment_cut_and_parent_edit_form_one_continuing_workflow() {
    let r=Registry::builtin().unwrap();let mut d=rectangle_loft(&r);
    let mut rt=Runtime::new(Registry::builtin().unwrap());
    let e=valid(&mut rt,&d);
    let mesh=&e.scene.meshes[0];
    let side=mesh.faces.iter().find(|f|f.key.contains(":generated:") && f.plane.is_some()).unwrap();
    let part=mesh.part.clone();let face=side.key.clone();
    let plane=feature(&r,&mut d,"plane",json!({"reference":{"kind":"face","part":part,"face":face},"offset":"0"}));
    let mut s=Sketch::default();s.add_circle([2.,2.],0.5);
    let mut f=r.create("sketch",&mut d).unwrap();f.set_sketch(&s);
    f.params["support_ref"]=json!({"kind":"datum","feature":plane});
    let sk=f.id.clone();d.features.push(f);
    feature(&r,&mut d,"extrude",json!({"sketch":sk,"depth":"2","mode":"Remove","targets":[part],"reverse":true}));
    let cut=valid(&mut rt,&d);
    assert!((cut.scene.meshes[0].volume-(1600.-std::f64::consts::PI*0.5)).abs()<1e-5);
    #[cfg(feature="step")]
    artifact(&d,&cut,"loft-side-cut",1600.-std::f64::consts::PI*0.5,[[-5.,-4.,0.],[5.,4.,20.]]);
    d.variables.insert("rise".into(),"30".into());
    let edited=valid(&mut rt,&d);
    assert!((edited.scene.meshes[0].volume-(2400.-std::f64::consts::PI*0.5)).abs()<1e-5);
    #[cfg(feature="step")]
    artifact(&d,&edited,"loft-side-cut-edited",2400.-std::f64::consts::PI*0.5,[[-5.,-4.,0.],[5.,4.,30.]]);
    let reloaded:Document=serde_json::from_slice(&serde_json::to_vec(&d).unwrap()).unwrap();
    assert_eq!(keys(&valid(&mut Runtime::new(r),&reloaded)),keys(&edited));
}

#[test]
fn mitred_sweep_side_sketch_cut_and_parent_edit_keep_the_selected_patch() {
    let r=Registry::builtin().unwrap();
    let mut d=Document::load(Path::new("examples/sweep-elbow.cadpart")).unwrap();
    let mut rt=Runtime::new(Registry::builtin().unwrap());let e=valid(&mut rt,&d);
    let side=e.scene.meshes[0].faces.iter().find(|face|face.plane.as_ref().is_some_and(|plane|
        plane.normal[1]< -0.99 && plane.origin[2].abs()<1e-7)).unwrap();
    let key=side.key.clone();assert!(key.contains(":generated:"));
    let mut s=Sketch::default();s.add_circle([2.,5.],0.25);
    let mut f=r.create("sketch",&mut d).unwrap();f.set_sketch(&s);
    f.params["support_ref"]=json!({"kind":"face","part":"f3:0","face":key});
    let sketch=f.id.clone();d.features.push(f);
    feature(&r,&mut d,"extrude",json!({"sketch":sketch,"depth":"1","mode":"Remove","targets":["f3:0"],"reverse":true}));
    let before=valid(&mut rt,&d);let removed=std::f64::consts::PI/16.;
    assert!((before.scene.meshes[0].volume-(400.-removed)).abs()<1e-5);
    #[cfg(feature="step")]
    artifact(&d,&before,"mitred-side-cut",400.-removed,[[-2.,-1.,0.],[30.,1.,22.]]);
    d.variables.insert("rise".into(),"25".into());
    let edited=valid(&mut rt,&d);
    assert!((edited.scene.meshes[0].volume-(440.-removed)).abs()<1e-5);
    // The cut can refine the final face. Its sketch still binds to the same
    // source-prefix patch and keeps the intended physical hole location.
    let plane=&edited.scene.sketches[&sketch].plane;
    let center=plane.at([2.,5.]);
    assert!(center.iter().zip([0.,-1.,5.]).all(|(a,b)|(a-b).abs()<1e-7));
    assert!(plane.normal[1]< -0.99);
    #[cfg(feature="step")]
    artifact(&d,&edited,"mitred-side-cut-edited",440.-removed,[[-2.,-1.,0.],[30.,1.,27.]]);
}

#[test]
fn generated_roles_follow_pattern_and_mirrored_occurrences_without_borrowing_owners() {
    let r=Registry::builtin().unwrap();let mut d=rectangle_loft(&r);
    let mut rt=Runtime::new(Registry::builtin().unwrap());
    let e=valid(&mut rt,&d);let original=e.scene.meshes[0].part.clone();
    feature(&r,&mut d,"pattern",json!({"target":original,"count":"3","spacing":"30","axis":"X"}));
    let p=valid(&mut rt,&d);assert_tracked(&p);
    let copied=p.scene.meshes[1].part.clone();
    let side=p.scene.meshes[1].faces.iter().find(|f|f.key.contains(":generated:") && f.plane.is_some()).unwrap();
    let selected=side.key.clone();let frame=side.plane.clone().unwrap();
    assert!(selected.contains(":instance:1::"));
    feature(&r,&mut d,"plane",json!({"reference":{"kind":"face","part":copied,"face":selected},"offset":"0"}));
    let plane=valid(&mut rt,&d);let actual=plane.scene.planes.values().next().unwrap();
    assert_eq!(actual.origin,frame.origin);
    feature(&r,&mut d,"mirror",json!({"target":original,"keep":true,"plane":"Right","offset":"-20"}));
    let mirrored=valid(&mut rt,&d);assert_tracked(&mirrored);
    let last=mirrored.scene.meshes.last().unwrap();
    assert!(last.faces.iter().filter(|f|f.key.contains(":generated:")).all(|f|f.key.contains(":instance:0::")));
    d.variables.insert("rise".into(),"24".into());
    valid(&mut rt,&d);
}

#[test]
fn generated_blend_inputs_are_usable_and_rejection_restores_the_prior_solid() {
    let r=Registry::builtin().unwrap();let mut d=rectangle_loft(&r);
    let mut rt=Runtime::new(Registry::builtin().unwrap());let e=valid(&mut rt,&d);
    let mesh=&e.scene.meshes[0];
    let edge=mesh.edges.iter().find(|edge|!edge.seam && edge.key.contains(":start")).unwrap().key.clone();
    let selected=e.geometry.edge_keys(e.geometry.parts[0].solid).unwrap().into_iter().find(|(_,key)|key==&edge).unwrap().0;
    let mut diagnostic_topology=e.geometry.topo.clone();
    let diagnostic=match brepkit_operations::blend_ops::fillet_v2(&mut diagnostic_topology,e.geometry.parts[0].solid,&[selected],100.) {
        Ok(result)=>format!("succeeded={:?} failed={:?} partial={} faces={} volume={:?} bounds={:?}",result.succeeded,result.failed,result.is_partial,
            brepkit_topology::explorer::solid_faces(&diagnostic_topology,result.solid).unwrap().len(),
            cad::engine::solid_volume(&diagnostic_topology,result.solid),
            brepkit_operations::measure::solid_bounding_box(&diagnostic_topology,result.solid)),
        Err(error)=>format!("rejected={error}"),
    };
    feature(&r,&mut d,"fillet",json!({"target":mesh.part,"edges":[edge],"radius":"0.3"}));
    let rounded=valid(&mut rt,&d);
    assert!(rounded.scene.meshes[0].volume<1600.);
    d.features.last_mut().unwrap().params["radius"]=json!("100");
    let failed=rt.evaluate(&d).unwrap();assert!(failed.scene.error.is_some(),"oversized result: volume={} bounds={:?}; {diagnostic}",failed.scene.meshes[0].volume,failed.scene.meshes[0].bounds);
    assert!((failed.scene.meshes[0].volume-1600.).abs()<1e-6);
    assert_eq!(keys(&failed),keys(&e));
    d.features.last_mut().unwrap().params["radius"]=json!("0.3");
    let repaired=valid(&mut rt,&d);
    assert!((repaired.scene.meshes[0].volume-rounded.scene.meshes[0].volume).abs()<1e-7);
}

#[test]
fn multiple_operations_have_owned_roles_and_failure_does_not_publish_the_first_operation() {
    let mut r=Registry::builtin().unwrap();
    r.add(r#"{"id":"paired-lofts","name":"Paired lofts","version":"1","category":"feature","script":"pair.rhai","fields":[{"key":"profiles","label":"Profiles","kind":"sketches","default":[]},{"key":"fail","label":"Fail","kind":"bool","default":false}]}"#,
        "fn build(p) { let last=if p.fail { [p.profiles[0],p.profiles[0]] } else { p.profiles }; [#{op:\"loft\",profiles:p.profiles},#{op:\"loft\",profiles:last}] }").unwrap();
    let mut d=rectangle_loft(&r);d.features.pop();
    feature(&r,&mut d,"paired-lofts",json!({"profiles":["f1","f2"]}));
    let mut rt=Runtime::new(r);
    let original=valid(&mut rt,&d);assert_tracked(&original);
    assert_eq!(original.scene.meshes.len(),2);
    for (ordinal,mesh) in original.scene.meshes.iter().enumerate() {
        let expected=format!("f4:generated:{ordinal}:");
        assert!(mesh.faces.iter().filter(|f|f.key.contains(":generated:")).all(|f|f.key.starts_with(&expected)),"{:?}",mesh.faces);
    }
    d.features.last_mut().unwrap().params["fail"]=json!(true);
    let failed=rt.evaluate(&d).unwrap();assert!(failed.scene.error.is_some());
    assert!(failed.scene.meshes.is_empty());
    assert!(!failed.geometry.outputs.contains_key("f4"));
    d.features.last_mut().unwrap().params["fail"]=json!(false);
    let repaired=valid(&mut rt,&d);assert_eq!(keys(&repaired),keys(&original));
}

#[test]
fn compound_loft_and_straight_sweep_caps_keep_each_owners_frame_after_parent_edits() {
    for sweep in [false,true] {
        let mut r=Registry::builtin().unwrap();
        r.add(r#"{"id":"paired-caps","name":"Paired caps","version":"1","category":"feature","script":"pair.rhai","fields":[{"key":"first","label":"First","kind":"sketches","default":[]},{"key":"second","label":"Second","kind":"sketches","default":[]},{"key":"sweep","label":"Sweep","kind":"bool","default":false}]}"#,
            "fn build(p) { if p.sweep { [#{op:\"sweep\",profile:p.first[0],path:p.first[1]},#{op:\"sweep\",profile:p.second[0],path:p.second[1]}] } else { [#{op:\"loft\",profiles:p.first},#{op:\"loft\",profiles:p.second}] } }").unwrap();
        let mut d=Document::default();d.variables.insert("rise".into(),"20".into());
        let mut inputs=vec![];
        for origin in [[0.,0.,0.],[30.,7.,0.]] {
            let mut s=Sketch::default();s.add_rect([-5.,-4.],[5.,4.]);
            let first=sketch(&r,&mut d,s.clone(),"0");
            d.features.last_mut().unwrap().params["context_plane"]=json!({"origin":origin,"normal":[0,0,1],"u":[1,0,0],"v":[0,1,0]});
            let second=if sweep {
                let mut path=Sketch::default();path.add_line([0.,0.],[0.,20.]);
                path.entities[0].values[3]="#rise".into();
                let id=sketch(&r,&mut d,path,"0");
                d.features.last_mut().unwrap().params["context_plane"]=json!({"origin":origin,"normal":[0,-1,0],"u":[1,0,0],"v":[0,0,1]});
                id
            } else {
                let id=sketch(&r,&mut d,s,"#rise");
                d.features.last_mut().unwrap().params["context_plane"]=json!({"origin":origin,"normal":[0,0,1],"u":[1,0,0],"v":[0,1,0]});
                id
            };
            inputs.push(vec![first,second]);
        }
        feature(&r,&mut d,"paired-caps",json!({"first":inputs[0],"second":inputs[1],"sweep":sweep}));
        let plane=feature(&r,&mut d,"plane",json!({"reference":{"kind":"face","part":"f5:100","face":"f5:end"},"offset":"2"}));
        let mut s=Sketch::default();s.add_circle([1.,2.],0.5);
        let mut attached=r.create("sketch",&mut d).unwrap();attached.set_sketch(&s);
        attached.params["support_ref"]=json!({"kind":"datum","feature":plane});
        let sketch_id=attached.id.clone();d.features.push(attached);
        let mut rt=Runtime::new(r);
        for rise in [20.,25.] {
            d.variables.insert("rise".into(),rise.to_string());
            let e=valid(&mut rt,&d);
            assert_eq!(e.scene.sketches["f3"].plane.origin,[30.,7.,0.]);
            assert_eq!(e.geometry.parts[1].reference_sources,
                BTreeSet::from(["f5".to_owned(),"f5:generated:1".to_owned()]));
            let end=e.scene.meshes[1].faces.iter().find(|f|f.key=="f5:end").unwrap().plane.as_ref().unwrap();
            assert_eq!(end.origin,[30.,7.,rise],"sweep={sweep}, anchors={:?}",e.geometry.anchors);
            let attached=&e.scene.sketches[&sketch_id].plane;
            assert_eq!(attached.origin,[30.,7.,rise+2.],"sweep={sweep}");
            assert_eq!(attached.at([1.,2.]),[31.,9.,rise+2.],"sweep={sweep}");
        }
    }
}

#[test]
fn warped_loft_sides_are_named_but_not_misrepresented_as_sketch_planes() {
    let r=Registry::builtin().unwrap();let mut d=rectangle_loft(&r);
    let angle=20_f64.to_radians();
    d.features[1].params["context_plane"]=json!({"origin":[0,0,0],"normal":[0,0,1],
        "u":[angle.cos(),angle.sin(),0.],"v":[-angle.sin(),angle.cos(),0.]});
    let mut rt=Runtime::new(Registry::builtin().unwrap());
    let original=valid(&mut rt,&d);assert_tracked(&original);
    let side=original.scene.meshes[0].faces.iter().find(|f|f.key.contains(":generated:")).unwrap();
    assert!(side.plane.is_none());
    feature(&r,&mut d,"plane",json!({"reference":{"kind":"face","part":"f3:0","face":side.key},"offset":"0"}));
    let rejected=rt.evaluate(&d).unwrap();
    assert!(rejected.scene.error.as_deref().unwrap().contains("planar"));
    assert_eq!(keys(&rejected),keys(&original));
    d.features.pop();
    feature(&r,&mut d,"transform",json!({"target":"f3:0","copy":true,"x":"30","y":"10","z":"5"}));
    let copied=valid(&mut rt,&d);assert_tracked(&copied);
    let before=keys(&copied);
    d.variables.insert("rise".into(),"25".into());
    assert_eq!(keys(&valid(&mut rt,&d)),before);
}

#[test]
fn straight_circular_sweep_keeps_old_caps_and_gains_usable_side_and_rim_references() {
    let r=Registry::builtin().unwrap();let mut d=Document::default();
    let mut circle=Sketch::default();circle.add_circle([0.,0.],3.);
    let profile=sketch(&r,&mut d,circle,"0");
    let mut line=Sketch::default();line.add_line([0.,0.],[0.,20.]);
    let path=sketch(&r,&mut d,line,"0");d.features.last_mut().unwrap().params["plane"]=json!("Front");
    feature(&r,&mut d,"sweep",json!({"profile":profile,"path":path}));
    let mut rt=Runtime::new(r);let e=valid(&mut rt,&d);assert_tracked(&e);
    let names=keys(&e);
    assert!(names.iter().any(|k|k.ends_with("f3:start")));
    assert!(names.iter().any(|k|k.ends_with("f3:end")));
    assert!(e.scene.meshes[0].faces.iter().filter(|f|f.plane.is_none()).all(|f|f.key.contains(":generated:0:")));
}
