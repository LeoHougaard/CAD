//! R50 deferred S08-S28 source-only integration against exact R47.
//! No native, physical, or screen-match credit until separately admitted execution.
use super::*;
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Unsupported { pub step: &'static str, pub reason: &'static str }
// R50 source-only callbacks. All native/screen outcomes remain UNVERIFIED.
pub(super) fn run(n: &mut Native) -> Result<Value, Unsupported> {
    prepare_blank(n);
    prepare_inches(n)?;
    prepare_front_sketch(n);
    let uid=n.editor.state["document_uid"].clone();
    let map=source_map(n);
    framing(n,&map,11,[443.,245.],[295.,97.,591.,393.],true);
    n.key(Key::R,egui::Modifiers::NONE);
    canvas(n,&map,[443.,245.],Some((-1,1)));
    feedback(n,&map,[443.,245.],[505.,295.],Some(2.527),Some(2.047),"S13",true);
    n.settle();
    let original=boundary(n);
    assert_eq!(original.as_array().unwrap().len(),4);
    let original_constraints=current(n)["sketch"]["constraints"].clone();
    framing(n,&map,14,[440.,228.],[366.,168.,515.,288.],false);
    // These values remain rendered assertions, never typed freehand targets.
    feedback(n,&map,[440.,228.],[515.,288.],Some(2.527),Some(2.047),"S14 retained",false);
    canvas(n,&map,[351.,228.],Some((-1,0)));
    feedback(n,&map,[351.,228.],[358.4,250.],Some(0.248),None,"S15 earlier",false);
    feedback(n,&map,[351.,228.],[362.,250.],Some(0.349),Some(0.744),"S15",true);
    n.settle(); assert_eq!(current(n)["sketch"]["geometry_count"],8);
    canvas(n,&map,[327.,228.],Some((-1,0)));
    feedback(n,&map,[327.,228.],[336.,264.],Some(0.288),Some(1.215),"S16",true);
    n.settle(); assert_eq!(current(n)["sketch"]["geometry_count"],12);
    for (step,count) in [(17,8),(18,4)] {
        n.key(Key::Z,egui::Modifiers::CTRL|egui::Modifiers::COMMAND); n.settle();
        assert_eq!(current(n)["sketch"]["geometry_count"],count);
        assert_eq!(boundary(n).as_array().unwrap()[..4],original.as_array().unwrap()[..]);
        checkpoint(n,&format!("S{step} real native Undo"));
    }
    assert_eq!(current(n)["sketch"]["constraints"],original_constraints);
    n.key(Key::Escape,egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool,crate::sketch_commands::Tool::Select);
    source_dimension_tool(n);
    canvas(n,&map,[366.,210.],Some((3,0)));
    canvas(n,&map,[342.,247.],None); n.settle();
    dimension(n,"height",&[(3,0)],"2.047",9);
    let accepted=current(n)["sketch"]["constraints"].clone();
    // S22 is an actual failed selection attempt, not a skipped row.
    source_dimension_tool(n);
    for (p,index) in [([430.,169.],2),([367.,204.],3),([513.,211.],1)] {
        canvas(n,&map,p,Some((index,0))); checkpoint(n,"S22 selection");
    }
    n.frame(vec![egui::Event::PointerMoved(map.point([442.,155.]))]);
    n.key(Key::Escape,egui::Modifiers::NONE); n.settle();
    assert_eq!(current(n)["sketch"]["constraints"],accepted,"failed attempt must accept no constraint");
    assert!(n.editor.sketch_ui.dimension.is_none());
    source_dimension_tool(n);
    canvas(n,&map,[367.,211.],Some((3,0)));
    canvas(n,&map,[513.,211.],Some((1,0)));
    canvas(n,&map,[454.,151.],None); n.settle();
    dimension(n,"width",&[(3,0),(1,0)],"2.469",10);
    let final_sketch=current(n);
    assert_eq!(boundary(n),original);
    assert_eq!(final_sketch["sketch"]["geometry_count"],4);
    assert_eq!(final_sketch["sketch"]["constraint_count"],11);
    assert_eq!(final_sketch["sketch"]["degrees_of_freedom"],0);
    let types:Vec<_>=final_sketch["sketch"]["constraints"].as_array().unwrap().iter().map(|c|c["type"].as_str().unwrap()).collect();
    assert_eq!(types,["Coincident","Coincident","Coincident","Coincident","Horizontal","Vertical","Horizontal","Vertical","Symmetric","Distance","Distance"]);
    let constraints=final_sketch["sketch"]["constraints"].as_array().unwrap();
    let mut selectors:Vec<[i64;6]>=(0..4).map(|i|[i,2,(i+1)%4,1,-2000,0]).collect();
    selectors.extend((0..4).map(|i|[i,0,-2000,0,-2000,0]));
    selectors.extend([[0,1,2,1,-1,1],[3,0,-2000,0,-2000,0],[3,1,1,0,-2000,0]]);
    for (constraint,expected) in constraints.iter().zip(selectors) {
        let actual=["first","first_pos","second","second_pos","third","third_pos"].map(|k|constraint[k].as_i64().unwrap());
        assert_eq!(actual,expected); assert_eq!(constraint["driving"],true);assert_eq!(constraint["active"],true);assert_eq!(constraint["virtual"],false);
    }
    n.frame(vec![]);
    for index in [9,10] {
        let (_,label)=n.editor.viewport.test_dimension_label(index).expect("actual dimension label paint");
        assert_eq!(label.split_whitespace().next().unwrap().parse::<f64>().unwrap(),2.);
    }
    checkpoint(n,"S26 square confirmation");
    let p=n.locate("toolbar","Finish",None); owned_at(n,p,None); n.settle();
    assert!(n.editor.sketch_ui.session.is_none());
    assert!(n.editor.viewport.sketch_plane_corners().is_none());
    // Choose an actually painted polygon corner, inset into the silhouette.
    let before=n.editor.state.clone();
    let out=n.frame(vec![]); let cube=n.editor.viewport.view_cube_rect().expect("actual visible cube");
    let mut points:Vec<_>=out.shapes.iter().filter_map(|s| match &s.shape {
        egui::Shape::Path(path) if path.closed && path.points.len()==4 && path.points.iter().all(|p|cube.contains(*p)) => Some(path.points.clone()),_=>None,
    }).flatten().collect();
    points.sort_by(|a,b|a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let corner=*points.first().expect("painted cube polygon corner");
    let p=corner+(cube.center()-corner).normalized()*1.;
    owned_at(n,p,Some(egui::Id::new("native_view_cube")));
    assert_eq!(n.editor.state,before,"cube must be model-passive");
    let out=n.frame(vec![]);
    let labels:Vec<_>=out.shapes.iter().filter_map(|s|match &s.shape {egui::Shape::Text(t) if cube.contains(t.pos) => Some(t.galley.text()),_=>None}).collect();
    assert!(labels.len()>=3,"corner must expose actual isometric cube faces");
    assert_eq!(n.editor.state["document_uid"],uid);
    assert!(objects(&n.editor.state).iter().all(|o|o["shape"]["solids"].as_u64().unwrap_or(0)==0));
    let source=objects(&n.editor.state).iter().find(|o|o["name"]==final_sketch["name"]).unwrap().clone();
    assert_eq!(source["rectangular_profile"]["eligible"],true);
    let body=objects(&n.editor.state).iter().find(|o|o["name"]==source["body"]).unwrap();
    assert!(body["body_state"]["tip"].is_null());
    checkpoint(n,"S28 same actor prefix endpoint; headless only");
    Ok(json!({"source":source,"original_boundary":original,"S01_S07":"unqualified","physical":0,
        "native_run":"UNVERIFIED until executed","screen_match":"UNVERIFIED","datum_internal_join":"requires Root observer contract"}))
}

fn source_dimension_tool(n:&mut Native) {
    let before=n.editor.state.clone();
    assert!(!n.editor.busy);
    if n.editor.sketch_ui.dimension_tool.is_some() {
        n.note("implementation adaptation: Escape ends persistent Dimension before the next source D");
        n.key(Key::Escape,egui::Modifiers::NONE);n.settle();
        assert!(n.editor.sketch_ui.dimension_tool.is_none());
    }
    n.key(Key::D,egui::Modifiers::NONE);
    assert!(n.editor.sketch_ui.dimension_tool.is_some());
    assert!(n.editor.sketch_ui.dimension.is_none() && n.editor.viewport.sketch_selection.is_empty() && !n.editor.busy);
    assert_eq!(n.editor.state,before,"source Dimension activation must be model-passive");
    n.note("source D adopted with fresh current Dimension tool and no selected geometry");
}

fn current(n:&Native)->Value {
    let name=n.editor.sketch.as_deref().expect("active native sketch");
    objects(&n.editor.state).iter().find(|o|o["name"]==name).unwrap().clone()
}
fn boundary(n:&Native)->Value {
    let s=current(n);
    let maps=s["sketch"]["mapped_edges"].as_array().expect("R50 passive snapshot maps contract");
    assert!(maps.len()>=4 && maps.iter().all(|m|m.as_str().is_some_and(|t|!t.is_empty())));
    json!(s["sketch"]["geometry"].as_array().unwrap().iter().enumerate().map(|(i,g)| {
        assert!(g["persistent_id"].is_i64());
        json!({"index":g["index"],"id":g["persistent_id"],"map":maps[i]})
    }).collect::<Vec<_>>())
}
fn checkpoint(n:&mut Native,step:&str) {
    n.events.push(json!({"phase":"R50 prefix observation","step":step,"editor":n.ui_state(),
        "selection":format!("{:?}",n.editor.viewport.sketch_selection),"cursor":n.protocol_cursor(),"physical_credit":0})); n.record();
}
fn rect_numbers(r:egui::Rect)->[f32;4] { [r.min.x,r.min.y,r.max.x,r.max.y] }
fn source_map(n:&mut Native)->SourceMap {
    let mut clip=None;
    n.frame_checked(vec![],|editor,ctx| {
        let r=editor.viewport.response.as_ref().unwrap();
        let widget=ctx.viewport(|v|v.this_pass.widgets.get(r.id).copied()).unwrap();
        clip=Some(r.rect.intersect(widget.interact_rect));
    });
    let clip=clip.unwrap(); let map=SourceMap::new(clip).unwrap();
    n.events.push(json!({"phase":"R50 frozen mapping","source_drawing":[191,81,854,450],"uncertainty":3,
        "actual_clip_numeric":rect_numbers(clip),"letterbox_numeric":rect_numbers(map.rect),
        "actual_clip":format!("{clip:?}"),"letterbox":format!("{:?}",map.rect),"world_authority":false})); n.record(); map
}
fn canvas(n:&mut Native,map:&SourceMap,source:[f32;2],hit:Option<(i32,i32)>) {
    let p=map.point(source); n.frame(vec![egui::Event::PointerMoved(p)]);
    let actual=n.editor.viewport.sketch_hit(p);
    if let Some((index,point))=hit { assert_eq!(actual,Some(crate::sketch_commands::SketchElement{index,point})); }
    else {
        assert!(actual.is_none(),"clear canvas expected; actual {actual:?}");
        assert!(n.editor.viewport.dimension_hit(p).is_none(),"clear label placement must not hit an existing dimension");
    }
    n.events.push(json!({"phase":"source canvas mapping","source":source,"uv":[(source[0]-191.)/663.,(source[1]-81.)/369.],"live":[p.x,p.y],"hit":format!("{actual:?}")})); n.record();
    let id=n.editor.viewport.response.as_ref().unwrap().id; owned_at(n,p,Some(id));
}
fn observed_bounds(n:&Native,guide:bool)->egui::Rect {
    if guide { return egui::Rect::from_points(&n.editor.viewport.sketch_plane_corners().expect("actual R47 painted guide")); }
    let s=current(n); let mut points=vec![];
    for g in s["sketch"]["geometry"].as_array().unwrap().iter().take(4) {
        for key in ["start","end"] { let a=&g[key]; points.push(n.editor.viewport.sketch_screen_point([a[0].as_f64().unwrap(),a[1].as_f64().unwrap(),a[2].as_f64().unwrap()])); }
    }
    egui::Rect::from_points(&points)
}
fn framing(n:&mut Native,map:&SourceMap,step:u32,origin:[f32;2],bounds:[f32;4],guide:bool) {
    let before=n.editor.state.clone();
    let target=egui::Rect::from_min_max(map.point([bounds[0],bounds[1]]),map.point([bounds[2],bounds[3]]));
    let tolerance=3.*map.rect.width()/663.;
    // 12 bounded real wheel increments; observe each settled camera.
    for _ in 0..12 {
        n.frame(vec![]); let b=observed_bounds(n,guide);
        if (b.width()-target.width()).abs()<=tolerance && (b.height()-target.height()).abs()<=tolerance {break;}
        let delta=((target.width()/b.width()).ln()/0.002).clamp(-180.,180.);
        let p=map.point(origin);
        n.frame(vec![egui::Event::PointerMoved(p)]);
        n.frame_checked(vec![],|editor,ctx| {
            let r=editor.viewport.response.as_ref().unwrap();
            assert!(r.hovered() && r.rect.contains(p));
            assert_eq!(ctx.layer_id_at(p),Some(r.layer_id));
        });
        n.frame(vec![egui::Event::PointerMoved(p),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.,delta),modifiers:egui::Modifiers::NONE}]);
        for _ in 0..12 { n.frame(vec![]); }
        checkpoint(n,&format!("S{step} wheel adaptation"));
    }
    for _ in 0..3 {
        let o=n.editor.viewport.sketch_screen_point([0.;3]); let d=map.point(origin)-o;
        if d.length()<=tolerance {break;}
        let p=map.point([600.,300.]);
        n.frame(vec![egui::Event::PointerMoved(p)]);
        n.frame_checked(vec![],|editor,ctx|assert_eq!(ctx.viewport(|v|v.hits.drag).unwrap().id,editor.viewport.response.as_ref().unwrap().id));
        n.frame(vec![egui::Event::PointerButton{pos:p,button:egui::PointerButton::Middle,pressed:true,modifiers:egui::Modifiers::NONE}]);
        for i in 1..=8 { n.frame(vec![egui::Event::PointerMoved(p+d*(i as f32/8.))]); }
        n.frame(vec![egui::Event::PointerButton{pos:p+d,button:egui::PointerButton::Middle,pressed:false,modifiers:egui::Modifiers::NONE}]);
        checkpoint(n,&format!("S{step} MMB adaptation"));
    }
    let output=n.frame(vec![]);
    let b=observed_bounds(n,guide); let o=n.editor.viewport.sketch_screen_point([0.;3]);
    if guide {
        let corners=n.editor.viewport.sketch_plane_corners().unwrap();
        assert!(output.shapes.iter().any(|s|match &s.shape {
            egui::Shape::Path(path) => path.points.len()==5 && corners.iter().all(|p|path.points.contains(p)),_=>false,
        }),"actual R47 guide outline must occur in current App paint");
    }
    n.events.push(json!({"phase":"actual framing result","step":step,"bounds":format!("{b:?}"),"target":format!("{target:?}"),"origin":[o.x,o.y],"tolerance":tolerance,"guide_is":"declared production presentation, not recovered native plane size"}));n.record();
    assert_eq!(n.editor.state,before,"framing must be state-passive");
    assert!(b.min.distance(target.min)<=tolerance && b.max.distance(target.max)<=tolerance && o.distance(map.point(origin))<=tolerance,"bounded framing mismatch");
}

fn painted_numbers(out:&egui::FullOutput,clip:egui::Rect)->Vec<String> {
    out.shapes.iter().filter_map(|s|match &s.shape {
        egui::Shape::Text(t) if clip.contains(t.pos)&&s.clip_rect.contains(t.pos) => Some(t.galley.text().to_owned()),_=>None,
    }).collect()
}
fn has_number(texts:&[String],v:f64)->bool {
    let expected=format!("{v:.3}"); texts.iter().any(|t|t.split_whitespace().next()==Some(expected.as_str()))
}
fn feedback(n:&mut Native,map:&SourceMap,center:[f32;2],source:[f32;2],width:Option<f64>,height:Option<f64>,step:&str,commit:bool) {
    let initial=map.point(source); let mut p=initial; let c=map.point(center);
    let tolerance=3.*map.rect.width()/663.; let before=n.editor.state.clone();
    for attempt in 0..=3 {
        let out=n.frame(vec![egui::Event::PointerMoved(p)]);
        let texts=painted_numbers(&out,map.rect);
        let ok=width.is_none_or(|v|has_number(&texts,v))&&height.is_none_or(|v|has_number(&texts,v));
        n.events.push(json!({"phase":"prospective rendered freehand assertion","step":step,"attempt":attempt,"source":source,"live":[p.x,p.y],"texts":texts,"expected":[width,height],"matched":ok,"uncertainty":tolerance})); n.record();
        if ok {
            if step=="S15 earlier" {
                let mut receipt=n.ctx.data(|d|d.get_temp::<Value>(egui::Id::new("convergence_feedback_projection")))
                    .expect("actual already-computed preview projection");
                receipt["phase"]=json!("R55 passive feedback projection");receipt["step"]=json!(step);
                receipt["frame"]=json!(n.ctx.cumulative_frame_nr()-1);
                assert_eq!(receipt["live"],json!([p.x,p.y]));
                n.events.push(receipt);n.record();
            }
            assert_eq!(n.editor.state,before);
            if commit {let id=n.editor.viewport.response.as_ref().unwrap().id;owned_at(n,p,Some(id));}
            return;
        }
        // Adjust from the actual current projection/native preview scale only.
        let a=n.editor.viewport.sketch_point(c).unwrap(); let b=n.editor.viewport.sketch_point(p).unwrap();
        let scale=n.editor.state["units"]["preview_length"]["scale"].as_f64().unwrap();
        if let Some(w)=width {p.x=c.x+(p.x-c.x)*(w/(2.*(b[0]-a[0]).abs()*scale)) as f32;}
        if let Some(h)=height {p.y=c.y+(p.y-c.y)*(h/(2.*(b[1]-a[1]).abs()*scale)) as f32;}
        assert!((p.x-initial.x).abs()<=tolerance && (p.y-initial.y).abs()<=tolerance,"calibration exceeds frozen source uncertainty");
    }
    panic!("{step}: native rendered feedback mismatch; no geometry committed");
}

fn dimension(n:&mut Native,role:&str,selection:&[(i32,i32)],preview:&str,index:usize) {
    n.frame(vec![]);
    let edit=n.editor.sketch_ui.dimension.as_ref().expect("actual dimension editor");
    assert_eq!(edit.kind,"Distance");
    assert_eq!(edit.selection.iter().map(|e|(e.index,e.point)).collect::<Vec<_>>(),selection);
    let seed=edit.value.clone();
    n.events.push(json!({"phase":"actual dimension preview before assertion","role":role,"seed":seed,"expected":preview,"kind":edit.kind,"selection":format!("{:?}",edit.selection)})); n.record();
    assert_eq!(seed,format!("{preview} in"),"exact source dimension text; no numeric rounding credit");
    let mut owner = None;
    let out=n.frame_checked(vec![], |editor,ctx| {
        let focused=ctx.memory(|m|m.focused()).expect("focused dimension TextEdit");
        let response=ctx.read_response(focused).expect("current dimension response");
        let (field,_,frame)=ctx.data(|d|d.get_temp::<(egui::Response,egui::Rect,u64)>(egui::Id::new("convergence_dimension_response"))).expect("current dimension field");
        assert_eq!(field.id,focused);assert_eq!(frame,ctx.cumulative_frame_nr());
        let widget=ctx.viewport(|v|v.this_pass.widgets.get(focused).copied()).expect("current-pass dimension owner");
        assert!(response.enabled() && response.has_focus() && widget.enabled);
        assert_eq!(Some(widget.layer_id),ctx.layer_id_at(response.rect.center()));
        assert_eq!(response.layer_id,widget.layer_id);
        assert_eq!(editor.sketch_ui.dimension.as_ref().unwrap().value,seed);
        owner=Some((response.clone(),widget.interact_rect));
    });
    let (response,clip)=owner.unwrap();
    let painted:Vec<_>=out.shapes.iter().filter_map(|s|match &s.shape {
        egui::Shape::Text(t) if t.galley.job.text == seed
            && response.rect.contains_rect(t.galley.rect.translate(t.pos.to_vec2()))
            && clip.contains_rect(t.galley.rect.translate(t.pos.to_vec2()))
            && s.clip_rect.contains_rect(t.galley.rect.translate(t.pos.to_vec2())) => Some(json!({
                "text":t.galley.job.text,"bounds":rect_numbers(t.galley.rect.translate(t.pos.to_vec2())),
                "clip":rect_numbers(s.clip_rect)})), _=>None,
    }).collect();
    assert_eq!(painted.len(),1,"exact current TextShape galley.job.text inside focused response and clip");
    n.events.push(json!({"phase":"R141 passive dimension paint","role":role,"seed":seed,
        "frame":n.ctx.cumulative_frame_nr()-1,"document_uid":n.editor.state["document_uid"],
        "revision":n.editor.state["revision"],"sketch":n.editor.sketch,"cursor":n.protocol_cursor(),
        "owner":{"id":format!("{:?}",response.id),"rect":rect_numbers(response.rect),
            "clip":rect_numbers(clip),"focused":response.has_focus(),"enabled":response.enabled()},
        "painted":painted}));n.record();
    n.events.push(json!({"phase":"R50 dimension role binding","role":role,"index_prospective":index,"seed":seed,"units":n.editor.state["units"],"selection":selection,"input_response":format!("{:?}",response.rect),"internal_parse_describe_reparse":"UNVERIFIED; Root observer contract required"}));n.record();
    n.typed("2 in");n.settle();
    assert!(n.editor.sketch_ui.dimension.is_none());
    let s=current(n); let constraints=s["sketch"]["constraints"].as_array().unwrap();
    let constraint=&constraints[index];
    assert_eq!(constraint["type"],"Distance");
    assert_eq!(constraint["quantity"]["default_unit"],"in","actual native describe schema default");
    let value=constraint["value"].as_f64().unwrap();
    assert_eq!(value.to_bits(),0x4049666666666666,"exact 2in native stored bits; never widen");
    n.events.push(json!({"phase":"R50 actual datum stored binding","role":role,"constraint":constraint,"bits":format!("{:016x}",value.to_bits()),"source":s["name"],"native_id":s["native_id"],"uid":n.editor.state["document_uid"],"cursor":n.protocol_cursor()}));n.record();
}

// Preserve aspect ratio with a centered letterbox, as required by the source
// mapping contract. This maps screen pixels only, never native model points.
struct SourceMap { rect: egui::Rect }
impl SourceMap {
    fn new(viewport: egui::Rect) -> Option<Self> {
        if !viewport.is_finite() || viewport.width() <= 0. || viewport.height() <= 0. { return None; }
        let scale = (viewport.width()/663.).min(viewport.height()/369.);
        Some(Self { rect: egui::Rect::from_center_size(viewport.center(),egui::vec2(663.,369.)*scale) })
    }
    fn point(&self, p: [f32;2]) -> egui::Pos2 {
        self.rect.min + egui::vec2((p[0]-191.)/663.*self.rect.width(), (p[1]-81.)/369.*self.rect.height())
    }
}

// These checks are also used by the real unit-menu input path. Reject stale
// frame receipts and ambiguous text before dispatch, rather than taking first.
fn unique_point(points: &[egui::Pos2], painted_frame: u64, current_frame: u64) -> Result<egui::Pos2, Unsupported> {
    if painted_frame != current_frame { return Err(Unsupported {step:"preparation",reason:"stale painted target"}); }
    if points.len() != 1 { return Err(Unsupported {step:"preparation",reason:"painted target missing or ambiguous"}); }
    Ok(points[0])
}

fn window_click(n: &mut Native, label: &str) -> Result<(), Unsupported> {
    n.frame(vec![]);
    let out=n.frame(vec![]);
    let frame=n.ctx.cumulative_frame_nr()-1;
    let area=n.ctx.memory(|m|m.area_rect(egui::Id::new("document_units")))
        .expect("actual Workspace units window");
    let points:Vec<_>=out.shapes.iter().filter_map(|s| match &s.shape {
        egui::Shape::Text(t) if t.galley.text()==label => {
            let p=t.galley.rect.translate(t.pos.to_vec2()).center();
            (area.contains(p)&&s.clip_rect.contains(p)).then_some(p)
        },_=>None,
    }).collect();
    let p=unique_point(&points,frame,n.ctx.cumulative_frame_nr()-1)?;
    owned_at(n,p,None);
    Ok(())
}

fn prepare_inches(n: &mut Native) -> Result<(), Unsupported> {
    let choices=n.editor.state["units"]["choices"].as_array().ok_or(Unsupported {
        step:"preparation",reason:"native document unit choices unavailable"})?;
    let current=choices.iter().find(|c|c["index"]==n.editor.state["units"]["index"])
        .ok_or(Unsupported {step:"preparation",reason:"current native unit choice unavailable"})?;
    let unit=current["workspace_defaults"]["length"]["default_unit"].as_str()
        .ok_or(Unsupported {step:"preparation",reason:"native workspace length metadata unavailable"})?;
    if unit=="in" {
        assert!(current["label"].as_str().unwrap().starts_with("Imperial decimal ("),"actual inch document schema required");
        assert_eq!(n.editor.state["units"]["defaults"]["mm"]["default_unit"],"in");
        return Ok(());
    }
    if unit!="mm" { return Err(Unsupported {step:"preparation",reason:"Workspace Length UI current unit is not Millimeter or Inch"}); }
    let inches:Vec<_>=choices.iter().filter(|c|
        c["workspace_defaults"]["length"]["default_unit"]=="in" &&
        c["label"].as_str().is_some_and(|s|s.starts_with("Imperial decimal ("))).collect();
    if inches.len()!=1 { return Err(Unsupported {step:"preparation",reason:"native inch choice missing or ambiguous"}); }
    let index=inches[0]["index"].clone();
    let uid=n.editor.state["document_uid"].clone();
    let revision=n.editor.state["revision"].clone();
    toolbar(n,"Document");
    let p=n.locate("popup","Workspace units...",None); owned_at(n,p,None);
    window_click(n,"Millimeter")?;
    let p=n.locate("popup","Inch",None); owned_at(n,p,None);
    window_click(n,"\u{2713}")?;
    n.note("preparation actual Workspace units dispatch; requires document_settings obligation");
    n.settle();
    assert_eq!(n.editor.state["document_uid"],uid);
    assert!(n.editor.state["revision"].as_u64().unwrap()>=revision.as_u64().unwrap());
    assert_eq!(n.editor.state["units"]["index"],index);
    assert!(crate::document_units::preview_length(&n.editor.state,1.).is_some(),"actual native preview scale metadata");
    Ok(())
}

fn owned_at(n: &mut Native, p: egui::Pos2, expected: Option<egui::Id>) {
    n.frame(vec![egui::Event::PointerMoved(p)]);
    let mut receipt = None;
    let output = n.frame_checked(vec![egui::Event::PointerMoved(p)], |editor, ctx| {
        assert!(!editor.busy);
        let hit = ctx.viewport(|v| v.hits.click).expect("actual input owner");
        let widget = ctx.viewport(|v| v.this_pass.widgets.get(hit.id).copied())
            .expect("current-pass input owner");
        let response = ctx.read_response(hit.id).expect("actual current response");
        if let Some(id) = expected { assert_eq!(id, hit.id); }
        assert!(widget.enabled && response.enabled() && widget.sense.senses_click());
        // interact_rect is egui's clipped interaction rectangle.
        assert!(widget.interact_rect.contains(p) && response.rect.contains(p));
        assert_eq!(Some(widget.layer_id), ctx.layer_id_at(p));
        assert_eq!(response.layer_id, widget.layer_id);
        receipt = Some(json!({"phase":"prefix painted target input owner",
            "point":[p.x,p.y],"id":format!("{:?}",widget.id),
            "rect":format!("{:?}",response.rect),
            "clip_interaction":format!("{:?}",widget.interact_rect),
            "layer":format!("{:?}",widget.layer_id),"enabled":response.enabled(),
            "revision":editor.state["revision"],"document_uid":editor.state["document_uid"]}));
    });
    assert!(output.shapes.iter().any(|s| s.clip_rect.contains(p)
        && s.shape.visual_bounding_rect().contains(p)), "target must be painted");
    n.events.push(receipt.expect("current input receipt")); n.record();
    n.at(p);
}

fn toolbar(n: &mut Native, label: &str) {
    let p = n.locate("toolbar", label, None);
    owned_at(n, p, None);
}

fn prepare_blank(n: &mut Native) {
    assert!(n.editor.state["document"].is_null(), "fresh Native only");
    toolbar(n, "New"); n.settle();
    assert!(n.editor.state["document_uid"].is_string());
    assert!(!objects(&n.editor.state).iter().any(|o|
        o["type"] == "Sketcher::SketchObject" || o["shape"]["solids"].as_u64().unwrap_or(0) > 0));
    n.note("native toolbar New preparation only; S01-S07 unqualified");
}

fn prepare_front_sketch(n: &mut Native) -> Value {
    toolbar(n, "Sketch");
    assert!(n.editor.awaiting_standard_plane());
    n.frame(vec![]);
    let p = n.editor.viewport.standard_guide_probe(crate::viewport::StandardPlane::Front)
        .expect("actual production projected Front guide hit");
    let canvas = n.editor.viewport.response.as_ref().unwrap().id;
    n.events.push(json!({"phase":"prefix preparation Front mapping",
        "source_xy":[550,166],"live_xy":[p.x,p.y],
        "mapping":"production standard_guide_hit Front/XZ; no authored world coordinates",
        "source_credit":false})); n.record();
    owned_at(n, p, Some(canvas)); n.settle();
    let name = n.editor.sketch.as_deref().expect("current owned sketch");
    assert!(n.editor.sketch_ui.session.is_some());
    let current = objects(&n.editor.state).iter().find(|o| o["name"] == name).unwrap().clone();
    assert_eq!(current["type"], "Sketcher::SketchObject");
    // Actual face label is located inside the actual cube response, not toolbar.
    let rect = n.editor.viewport.view_cube_rect().expect("visible view cube");
    let out = n.frame(vec![]);
    let labels: Vec<_> = out.shapes.iter().filter_map(|s| match &s.shape {
        egui::Shape::Text(t) if t.galley.text() == "Front" => {
            let p = t.galley.rect.translate(t.pos.to_vec2()).center();
            (rect.contains(p) && s.clip_rect.contains(p)).then_some(p)
        }, _ => None,
    }).collect();
    assert_eq!(labels.len(), 1, "one actual cube Front face");
    owned_at(n, labels[0], Some(egui::Id::new("native_view_cube")));
    n.note("S08-S10 actual callbacks complete; physical/source execution remains unqualified");
    current
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_prefix_map_letterboxes_without_stretch_or_model_coordinates() {
        let map=SourceMap::new(egui::Rect::from_min_size(egui::pos2(20.,80.),egui::vec2(1000.,700.))).unwrap();
        assert!((map.rect.width()/map.rect.height()-663./369.).abs()<1e-5);
        assert_eq!(map.point([191.,81.]),map.rect.min);
        assert_eq!(map.point([854.,450.]),map.rect.max);
        assert!(SourceMap::new(egui::Rect::NOTHING).is_none());
    }
    #[test]
    fn offline_prefix_targets_reject_ambiguous_and_stale_paint() {
        let p=egui::pos2(10.,20.);
        assert_eq!(unique_point(&[p],3,3),Ok(p));
        assert!(unique_point(&[p,p],3,3).is_err());
        assert!(unique_point(&[],3,3).is_err());
        assert_eq!(unique_point(&[p],2,3).unwrap_err().reason,"stale painted target");
    }
    #[test]
    fn offline_prefix_actual_toolbar_new_dispatches_one_job() {
        let (mut n,input,_output)=offline_app("offline-prefix-r50-new");
        toolbar(&mut n,"New");
        let job=input.try_recv().expect("actual App New callback");
        assert_eq!(job.op,"new");
        assert!(input.try_recv().is_err());
    }
    #[test]
    fn offline_prefix_actual_sketch_collector_dispatches_no_job() {
        let (mut n,input,_output)=offline_app("offline-prefix-r50-sketch");
        toolbar(&mut n,"Sketch");
        assert!(n.editor.sketch_creation.is_some());
        assert!(input.try_recv().is_err());
    }
    #[test]
    fn offline_prefix_feedback_requires_rendered_numeric_token() {
        assert!(has_number(&["2.527 in".into()],2.527));
        assert!(!has_number(&["12.527 in".into()],2.527));
        assert!(!has_number(&["2.526 in".into()],2.527));
    }
    #[test]
    fn offline_prefix_actual_full_app_escape_routes_without_jobs() {
        let (mut n,input,_output)=offline_app("offline-prefix-r50-escape");
        n.key(Key::Escape,egui::Modifiers::NONE);
        assert!(n.editor.sketch_ui.dimension.is_none());
        assert!(input.try_recv().is_err());
    }
    #[test]
    fn offline_prefix_actual_full_app_ctrl_z_dispatches_one_undo() {
        let (mut n,input,_output)=offline_app("offline-prefix-r50-undo");
        // Explicit MOCK precondition, never current-native identity evidence.
        n.editor.state["undo_count"]=json!(1);
        n.frame(vec![]);
        n.key(Key::Z,egui::Modifiers::CTRL|egui::Modifiers::COMMAND);
        let job=input.try_recv().expect("actual App shortcut undo");
        assert_eq!(job.op,"undo");
        assert!(input.try_recv().is_err());
    }
    #[test]
    fn offline_prefix_dimension_replay_after_height_apply_avoids_stale_measurement() {
        let (editor,input,output)=super::super::super::dimension_escape_fixture();
        let (mut n,_unused_input,_unused_output)=offline_app("offline-dimension-replay");
        n.editor=editor;
        let state:Value=serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/days002-post-height.json"))).unwrap();
        let profile=objects(&state).iter().find(|o|o["type"]=="Sketcher::SketchObject").unwrap();
        let profile_name=profile["name"].as_str().unwrap().to_owned();
        let token=state["sketch_edit"]["token"].as_str().unwrap().to_owned();
        n.editor.state=state.clone();n.editor.sketch=Some(profile_name);n.editor.sketch_ui.session=Some(token);
        n.editor.sync_sketch();n.editor.viewport.set_references(&n.editor.state);
        n.editor.viewport.align_sketch();n.editor.viewport.fit_sketch();
        n.editor.start_dimension_tool();
        // Declared native success; real App reply adoption retains continuous Dimension.
        output.send(Reply{op:"transaction".into(),accepted:true,state:state.clone(),scenes:vec![],
            error:None,elapsed:Duration::ZERO,data:Value::Null}).unwrap();
        n.frame(vec![]);assert!(n.editor.sketch_ui.dimension_tool.is_some());
        source_dimension_tool(&mut n);
        let pick=|n:&mut Native,index:usize| {
            let geometry=current(n)["sketch"]["geometry"][index].clone();
            let a=&geometry["start"];let b=&geometry["end"];
            // Avoid the axis intersection at each edge midpoint in this unit fixture.
            n.frame(vec![]);
            let point=[a[0].as_f64().unwrap()*0.75+b[0].as_f64().unwrap()*0.25,
                a[1].as_f64().unwrap()*0.75+b[1].as_f64().unwrap()*0.25,0.];
            let p=n.editor.viewport.sketch_screen_point(point);
            assert_eq!(n.editor.viewport.sketch_hit(p),Some(crate::sketch_commands::SketchElement{index:index as i32,point:0}),"fixture must identify intended edge before input");
            n.at(p);
        };
        for index in [2,3,1] {pick(&mut n,index);}
        assert_eq!(n.editor.viewport.sketch_selection.iter().map(|p|(p.index,p.point)).collect::<Vec<_>>(),vec![(2,0),(3,0),(1,0)]);
        n.key(Key::Escape,egui::Modifiers::NONE);
        assert!(n.editor.viewport.sketch_selection.is_empty());assert!(n.editor.sketch_ui.dimension_tool.is_none());
        source_dimension_tool(&mut n);
        for index in [3,1] {pick(&mut n,index);}
        assert_eq!(n.editor.viewport.sketch_selection.iter().map(|p|(p.index,p.point)).collect::<Vec<_>>(),vec![(3,0),(1,0)]);
        assert!(!n.editor.busy);assert!(input.try_recv().is_err(),"source D must not read a stale single height");
        let p=n.editor.viewport.sketch_screen_point([12.,10.,0.]);
        assert!(n.editor.viewport.response.as_ref().unwrap().rect.contains(p),"fixture placement is inside the current canvas");
        assert!(n.editor.viewport.sketch_hit(p).is_none(),"fixture placement is off axes and curves");
        n.at(p);
        let job=input.try_recv().expect("actual two-line dimension preparation");
        assert_eq!(job.op,"quantities");assert_eq!(job.parameters["items"],json!([{"unit":"mm","value":64.18582153320312}]));
        assert_eq!(n.editor.state,state,"failed selection and preparation do not mutate native model snapshot");
        assert!(input.try_recv().is_err());
    }

}
