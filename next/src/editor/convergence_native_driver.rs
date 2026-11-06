use super::super::*;
use std::{fs, thread::JoinHandle, io::Write};
#[path = "convergence_source_prefix.rs"]
mod source_prefix;

struct Native {
    editor: Editor,
    raw: Receiver<Reply>,
    forward: Sender<Reply>,
    actor: Option<JoinHandle<()>>,
    startup_ready: Option<Receiver<()>>,
    events: Vec<Value>,
    recorded: std::cell::Cell<usize>,
    folder: PathBuf,
    ctx: egui::Context,
    time: f64,
}
impl Native {
    fn new(case: &str) -> Self {
        let runtime: Value = serde_json::from_slice(
            &fs::read(std::env::var_os("CAD_CONVERGENCE_RUNTIME").expect("CAD_CONVERGENCE_RUNTIME"))
                .unwrap(),
        )
        .unwrap();
        let mut config = WorkerConfig::new(
            runtime["executable"].as_str().unwrap(),
            runtime["test_worker"].as_str().expect("pinned actual worker"),
        );
        config.args = runtime["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect();
        config.response_timeout = Duration::from_secs(45);
        let (jobs, input) = mpsc::sync_channel(1);
        let (output, raw) = mpsc::channel();
        let (forward, replies) = mpsc::channel();
        let (ready, startup_ready) = mpsc::channel();
        let model_ctx = egui::Context::default();
        model_ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("convergence-model-startup-ready"), ready);
        });
        let actor = std::thread::spawn(move || {
            model_thread(config, input, output, model_ctx)
        });
        let editor = Editor {
            jobs,
            replies,
            busy: false,
            registry: Registry::builtin().unwrap(),
            state: Value::Null,
            viewport: Viewport::default(),
            sketch: None,
            selected_object: None,
            sketch_ui: SketchUi::default(),
            sketch_creation: None,
            body_ui: BodyUi::default(),
            property_ui: properties::PropertyUi::default(),
            transform_ui: transforms::TransformUi::default(),
            exchange_ui: exchange::ExchangeUi::default(),
            draft: None,
            measurement_ui: super::super::measurements::MeasurementUi::default(),
            units_ui: Default::default(),
            sequence: 0,
            error: None,
            status: String::new(),
            path: String::new(),
            file_input: String::new(),
            file_dialog: None,
            file_browser: file_browser::FileBrowser::default(),
            overwrite: false,
            dirty: false,
            pending_file: None,
            allow_close: false,
            fit_next: false,
        };
        let folder = PathBuf::from(std::env::var_os("CAD_CONVERGENCE_OUTPUT").unwrap()).join(case);
        fs::create_dir(&folder).unwrap();
        let ctx = egui::Context::default();
        ctx.style_mut(|s| s.animation_time = 0.);
        Self {
            editor,
            raw,
            forward,
            actor: Some(actor),
            startup_ready: Some(startup_ready),
            events: vec![],
            recorded: std::cell::Cell::new(0),
            folder,
            ctx,
            time: 0.,
        }
    }
    fn record(&self) {
        let cursor=self.recorded.get(); assert!(cursor<=self.events.len());
        let mut log=fs::OpenOptions::new().create(true).append(true).open(self.folder.join("APP-events.jsonl")).unwrap();
        for (index,value) in self.events.iter().enumerate().skip(cursor.saturating_sub(1)) {
            serde_json::to_writer(&mut log,&json!({"index":index,"value":value})).unwrap();log.write_all(b"\n").unwrap();
        }
        log.flush().unwrap();self.recorded.set(self.events.len());
    }
    fn ui_state(&self)->Value {json!({"state":self.editor.state,"busy":self.editor.busy,"error":self.editor.error,"status":self.editor.status,"draft":self.editor.draft.as_ref().map(|d|json!({"parameters":d.parameters,"token":d.token,"valid":d.valid,"committing":d.committing,"changed":d.changed.is_some()}))})}
    fn protocol_cursor(&self)->Value {
        // Editor.sequence generates names. It is NOT a protocol counter.
        let path=self.folder.parent().unwrap().join("observer/requests.raw.jsonl");
        let bytes=match fs::read(path) {
            Ok(bytes)=>bytes,
            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>vec![],
            Err(e)=>panic!("protocol cursor read failed: {e}"),
        };
        // Only complete, flushed records can precede native dispatch. A writer's
        // incomplete trailing bytes cannot count as a dispatched request.
        let end=bytes.iter().rposition(|b|*b==b'\n').map_or(0,|i|i+1);
        let rows:Vec<Value>=bytes[..end].split(|b|*b==b'\n').filter(|b|!b.is_empty())
            .map(|line|serde_json::from_slice(line).expect("complete protocol JSON record")).collect();
        json!({"sequence":rows.len(),"bytes":end,"last_id":rows.last().map(|v|v["id"].clone())})
    }
    fn note(&mut self,phase:&str){let cursor=self.protocol_cursor();self.events.push(json!({"phase":phase,"request_sequence":cursor["sequence"],"request_cursor":cursor,"editor":self.ui_state()}));self.record();}
    fn frame(&mut self,events:Vec<egui::Event>)->egui::FullOutput {
        self.frame_checked(events, |_, _| {})
    }
    fn frame_checked(&mut self,events:Vec<egui::Event>,mut checked:impl FnMut(&Editor,&egui::Context))->egui::FullOutput {
        let modifiers=events.iter().find_map(|e|match e {egui::Event::Key{modifiers,..}|egui::Event::PointerButton{modifiers,..}=>Some(*modifiers),_=>None}).unwrap_or_default();
        let frame_cursor=events.iter().any(|e|matches!(e,egui::Event::PointerButton{
            button:egui::PointerButton::Primary,pressed:false,..})).then(||self.protocol_cursor());
        self.ctx.data_mut(|d| {
            d.remove::<Value>(egui::Id::new("convergence_input_projection"));
            d.remove::<Value>(egui::Id::new("convergence_feedback_projection"));
            d.remove::<(egui::Response, egui::Rect, egui::LayerId, Value)>(egui::Id::new("convergence_profile_response"));
            d.remove::<(egui::Response, egui::Rect, egui::LayerId)>(egui::Id::new("convergence_apply_response"));
        });
        self.ctx.data_mut(|d|d.remove::<(egui::Response,egui::Rect,u64)>(egui::Id::new("convergence_dimension_response")));
        self.time+=0.1;
        self.events.push(json!({"phase":"RawInput","time":self.time,"input":events.iter().map(|e|match e {egui::Event::Text(t)=>json!({"text":t}),egui::Event::PointerButton{pos,pressed,..}=>json!({"pointer":[pos.x,pos.y],"pressed":pressed}),_=>json!({"other":format!("{e:?}")})}).collect::<Vec<_>>(),"events_debug":format!("{events:?}"),"modifiers_debug":format!("{modifiers:?}")}));self.record();
        let result=self.ctx.run(egui::RawInput{time:Some(self.time),modifiers,screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1280.,850.))),events,..Default::default()},|ctx|{eframe::App::update(&mut self.editor,ctx,&mut eframe::Frame::_new_kittest());checked(&self.editor,ctx);});
        self.events.last_mut().unwrap()["editor"]=self.ui_state();
        let control = |r:egui::Response,clip:egui::Rect,frame:u64| json!({"id":format!("{:?}",r.id),"frame":frame,"rect":[r.rect.min.x,r.rect.min.y,r.rect.max.x,r.rect.max.y],"clip":[clip.min.x,clip.min.y,clip.max.x,clip.max.y],"layer":format!("{:?}",r.layer_id),"top_layer":format!("{:?}",self.ctx.layer_id_at(r.rect.center())),"focused":r.has_focus(),"enabled":r.enabled(),"paint":result.shapes.iter().any(|s|s.clip_rect.contains(r.rect.center())&&s.shape.visual_bounding_rect().intersects(r.rect))});
        let frame=self.ctx.cumulative_frame_nr()-1;
        self.events.last_mut().unwrap()["frame"]=json!(frame);
        let cursor=self.protocol_cursor();
        self.events.last_mut().unwrap()["request_sequence"]=cursor["sequence"].clone();
        self.events.last_mut().unwrap()["request_cursor"]=cursor;
        self.events.last_mut().unwrap()["dimension_response"]=self.ctx.data(|d|d.get_temp::<(egui::Response,egui::Rect,u64)>(egui::Id::new("convergence_dimension_response"))).map(|(r,c,f)|control(r,c,f)).unwrap_or(Value::Null);
        self.events.last_mut().unwrap()["path_response"]=self.editor.file_browser.test_path_response().map(|(r,c,f)|control(r,c,f)).unwrap_or(Value::Null);
        self.events.last_mut().unwrap()["apply_response"]=self.ctx.data(|d|d.get_temp::<(egui::Response,egui::Rect,egui::LayerId)>(egui::Id::new("convergence_apply_response"))).map(|(r,c,_)|control(r,c,frame)).unwrap_or(Value::Null);
        self.events.last_mut().unwrap()["profile_response"]=self.ctx.data(|d|d.get_temp::<(egui::Response,egui::Rect,egui::LayerId,Value)>(egui::Id::new("convergence_profile_response"))).map(|(r,c,_,value)|json!({"control":control(r,c,frame),"value":value,"schema":self.editor.property_ui.schema})).unwrap_or(Value::Null);
        self.record();
        let graphics=json!({"time":self.time,"field_events":self.events.last(),"paint":result.shapes.iter().map(|s|json!({"clip":format!("{:?}",s.clip_rect),"shape":format!("{:?}",s.shape)})).collect::<Vec<_>>()});
        let mut log=fs::OpenOptions::new().create(true).append(true).open(self.folder.join("graphicsFieldEvents.jsonl")).unwrap();serde_json::to_writer(&mut log,&graphics).unwrap();log.write_all(b"\n").unwrap();log.flush().unwrap();
        if let Some(mut receipt)=self.ctx.data(|d|d.get_temp::<Value>(egui::Id::new("convergence_input_projection"))) {
            receipt["phase"]=json!("R55 passive sketch projection");receipt["frame"]=json!(frame);
            receipt["request_sequence"]=frame_cursor.as_ref().expect("accepted projection is pointer release")["sequence"].clone();
            self.events.push(receipt);self.record();
        }
        result
    }
    fn await_startup(&mut self) {
        self.note("actual model_thread startup wait begins");
        let started = Instant::now();
        self.startup_ready.take().expect("actual model startup receiver")
            .recv_timeout(Duration::from_secs(60))
            .expect("actual WorkerSession startup did not complete");
        self.events.push(json!({"phase":"actual WorkerSession capability exchange completed",
            "startup_elapsed_ms":started.elapsed().as_millis(),"modeling_reply_synthesized":false}));
        self.record();
    }
    fn settle(&mut self) {
        let end=Instant::now()+Duration::from_secs(60);let mut idle_since=None;
        loop {
            while let Ok(reply)=self.raw.try_recv(){
                self.events.push(json!({"phase":"actual model_thread Reply","op":reply.op,"accepted":reply.accepted,"state":reply.state,"scenes":reply.scenes,"data":reply.data,"error":reply.error}));self.record();
                let rejected=!reply.accepted;let op=reply.op.clone();self.forward.send(reply).unwrap();
                // Adoption occurs inside the real App::update, not a fixture assignment.
                self.frame(vec![]);self.note("Reply adopted by App update");
                assert!(!rejected,"unexpected actual native rejection: {op}");
            }
            self.frame(vec![]);
            let pending=self.editor.busy || self.editor.draft.as_ref().is_some_and(|d|d.changed.is_some());
            if pending {idle_since=None;}else {let since=idle_since.get_or_insert_with(Instant::now);if since.elapsed()>Duration::from_millis(180){break;}}
            assert!(Instant::now()<end,"native/debounce settlement timed out");std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn bounds(&mut self,scope:&str)->egui::Rect {
        match scope {
            "toolbar"=>egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1280.,80.)),
            "dialog"=>self.editor.file_browser.test_selection(&self.ctx).expect("actual file dialog").2,
            "popup"=>self.ctx.content_rect(),
            _=>self.ctx.memory(|m|m.area_rect(egui::Id::new(if scope=="Features" {"features"} else {scope}))).expect("actual named area"),
        }
    }
    fn locate(&mut self,scope:&str,label:&str,row:Option<&str>)->egui::Pos2 {
        self.frame(vec![]);let out=self.frame(vec![]);let area=self.bounds(scope);
        let texts:Vec<_>=out.shapes.iter().filter_map(|s|match &s.shape {egui::Shape::Text(t)=>{let r=t.galley.rect.translate(t.pos.to_vec2());let p=r.center();(area.contains(p)&&s.clip_rect.contains(p)).then_some((t.galley.text().to_owned(),r))},_=>None}).collect();
        let anchor=row.map(|name|{let a:Vec<_>=texts.iter().filter(|(t,_)|t==name).collect();assert_eq!(a.len(),1,"unique actual row anchor {name}");a[0].1});
        let targets:Vec<_>=texts.iter().filter(|(text,r)|text==label && anchor.is_none_or(|a|r.center().x>a.max.x && (r.center().y-a.center().y).abs()<6.) && (scope!="popup" || self.ctx.layer_id_at(r.center()).is_some_and(|l|l.order==egui::Order::Foreground))).map(|(_,r)|r.center()).collect();
        self.events.push(json!({"phase":"rendered target locator","scope":scope,"row":row,"label":label,"area":format!("{area:?}"),"candidates":targets.iter().map(|p|[p.x,p.y]).collect::<Vec<_>>(),"rendered_text":texts.iter().map(|(t,r)|json!({"text":t,"rect":format!("{r:?}")})).collect::<Vec<_>>() }));self.record();
        assert_eq!(targets.len(),1,"target must be unique, visible and row scoped: {scope}/{label}");targets[0]
    }
    fn profile_control(&mut self) {
        // Observe the actual ComboBox response, never a guessed same-row label.
        for attempt in 0..=12 {
            let out=self.frame(vec![]);let area=self.bounds("Feature");
            let target=self.ctx.data(|d| d.get_temp::<(egui::Response,egui::Rect,egui::LayerId,Value)>(egui::Id::new("convergence_profile_response")));
            if let Some((response,clip,layer,value))=target {
                let p=response.rect.center();
                if area.contains(p) && clip.contains(p) {
                    assert!(response.enabled());assert!(value.is_null());
                    assert_eq!(self.ctx.layer_id_at(p),Some(layer));
                    assert!(out.shapes.iter().any(|s| s.clip_rect.contains(p) && s.shape.visual_bounding_rect().intersects(response.rect)));
                    self.events.last_mut().unwrap()["profile_receipt"]=json!(true);self.record();
                    self.events.push(json!({"phase":"actual Profile response","attempt":attempt,"id":format!("{:?}",response.id),"rect":format!("{:?}",response.rect),"clip":format!("{clip:?}"),"enabled":response.enabled(),"value":value}));self.record();
                    self.at(p);return;
                }
            }
            assert!(attempt<12,"Profile control unavailable after bounded actual wheel input");
            let p=area.center();assert!(self.ctx.layer_id_at(p).is_some());
            self.frame(vec![egui::Event::PointerMoved(p),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.,-100.),modifiers:egui::Modifiers::NONE}]);
        }
        unreachable!()
    }
    fn bound_profile_control(&mut self, source:&Value) {
        assert!(self.editor.rectangular_active(),"current validated rectangle task");
        assert!(self.editor.rectangular_owner().is_ok(),"current native source/body/preview ownership");
        let name=source["name"].as_str().expect("current native source name");
        let expected=json!({"name":name,"native_id":source["native_id"]});
        let draft=self.editor.draft.as_ref().unwrap();
        assert_eq!(draft.parameters["source"],expected);
        assert_eq!(draft.parameters["profile_input"],name);
        assert!(draft.valid && self.editor.state["preview"]["token"]==draft.token,"accepted native default preview");
        let parameters=draft.parameters.clone();let cursor=self.protocol_cursor();
        for attempt in 0..=12 {
            let out=self.frame(vec![]);let area=self.bounds("Feature");
            let current=self.ctx.data(|d|d.get_temp::<(egui::Response,egui::Rect,egui::LayerId,Value)>(egui::Id::new("convergence_profile_response")));
            if let Some((response,clip,layer,value))=current {
                let point=response.rect.center();
                if area.contains(point) && clip.contains(point) {
                    assert!(response.enabled());assert_eq!(value,json!({"link":name}));
                    assert_eq!(self.ctx.layer_id_at(point),Some(layer));
                    assert!(out.shapes.iter().any(|s|s.clip_rect.contains(point)&&matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text==name && response.rect.contains_rect(t.galley.rect.translate(t.pos.to_vec2())))));
                    self.events.last_mut().unwrap()["profile_receipt"]=json!(true);self.record();
                    self.events.push(json!({"phase":"actual bound Profile response","attempt":attempt,"id":format!("{:?}",response.id),"source":expected,"value":value,"collection":"source retained by normal Finish and Extrude; actual Profile field click"}));self.record();
                    self.at(point);self.frame(vec![]);
                    let (focused,_,_,_)=self.ctx.data(|d|d.get_temp::<(egui::Response,egui::Rect,egui::LayerId,Value)>(egui::Id::new("convergence_profile_response"))).unwrap();
                    assert_eq!(focused.id,response.id);assert!(focused.has_focus());
                    assert_eq!(self.editor.draft.as_ref().unwrap().parameters,parameters,"collection click must not change native inputs");
                    assert_eq!(self.protocol_cursor(),cursor,"collection click must not submit another preview");
                    return;
                }
            }
            assert!(attempt<12,"current bound Profile control must become visible");
            let point=area.center();self.frame(vec![egui::Event::PointerMoved(point),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.,-100.),modifiers:egui::Modifiers::NONE}]);
        }
        unreachable!()
    }
    fn disabled_apply(&mut self) {
        let before=self.ui_state();
        let out=self.frame(vec![]);
        let (response,clip,layer)=self.ctx.data(|d| d.get_temp::<(egui::Response,egui::Rect,egui::LayerId)>(egui::Id::new("convergence_apply_response"))).expect("actual current Apply response");
        let p=response.rect.center();assert!(!response.enabled());assert!(clip.contains(p));assert!(self.bounds("Feature").contains(p));assert_eq!(self.ctx.layer_id_at(p),Some(layer));
        assert!(out.shapes.iter().any(|s| s.clip_rect.contains(p) && s.shape.visual_bounding_rect().intersects(response.rect)));
        self.events.push(json!({"phase":"disabled Apply before pointer","enabled":response.enabled(),"id":format!("{:?}",response.id),"rect":format!("{:?}",response.rect)}));self.record();
        self.at(p);self.settle();
        let (after,after_clip,after_layer)=self.ctx.data(|d| d.get_temp::<(egui::Response,egui::Rect,egui::LayerId)>(egui::Id::new("convergence_apply_response"))).expect("actual post-pointer Apply response");
        assert!(!after.enabled());assert_eq!(after.id,response.id);assert!(after_clip.contains(after.rect.center()));assert_eq!(self.ctx.layer_id_at(after.rect.center()),Some(after_layer));
        assert_eq!(self.ui_state(),before,"disabled pointer must preserve complete Editor state/draft");
        self.events.push(json!({"phase":"disabled Apply after pointer","enabled":after.enabled(),"id":format!("{:?}",after.id)}));self.record();
    }
    fn at(&mut self,p:egui::Pos2){for pressed in [true,false]{self.frame(vec![egui::Event::PointerMoved(p),egui::Event::PointerButton{pos:p,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}]);}}
    fn click(&mut self,scope:&str,label:&str,row:Option<&str>){let p=self.locate(scope,label,row);self.at(p);}
    fn key(&mut self,key:Key,modifiers:egui::Modifiers){self.frame(vec![egui::Event::Key{key,physical_key:None,pressed:true,repeat:false,modifiers}]);self.frame(vec![egui::Event::Key{key,physical_key:None,pressed:false,repeat:false,modifiers:egui::Modifiers::NONE}]);}
    fn typed(&mut self,text:&str){
        self.key(Key::A,egui::Modifiers::CTRL|egui::Modifiers::COMMAND);
        self.events.push(json!({"phase":"literal queued text","text":text}));self.record();
        self.frame(vec![egui::Event::Text(text.into())]);self.key(Key::Enter,egui::Modifiers::NONE);
    }
    fn field(&mut self,row:&str,key:&str,text:&str){
        let old=self.editor.draft.as_ref().unwrap().parameters[key].as_str().unwrap().to_owned();
        self.click("Feature",&old,Some(row));self.typed(text);
        assert_eq!(self.editor.draft.as_ref().unwrap().parameters[key],text,"actual text edit must update visible draft before worker");self.note("literal text accepted by rendered field");self.settle();
    }
    fn open(&mut self,path:&str){
        self.click("toolbar","Open",None);assert!(self.editor.file_browser.active());
        self.dialog_path(path);
        self.settle();assert_eq!(self.editor.path,path);assert!(self.editor.state["document_uid"].is_string());self.note("actual File Open completed");
    }
    fn dialog_path(&mut self,path:&str){
        self.frame(vec![]);
        // SaveFile focuses its filename field. The dependency intentionally
        // suppresses Slash while ANY widget has keyboard focus. Activate its
        // visible path-edit button instead of bypassing focus or changing it.
        if self.ctx.memory(|m| m.focused()).is_some() {
            self.note("actual path activation through visible Edit path button; focused filename owns Slash");
            self.click("dialog","\u{1f58a}",None);
        } else {
            self.note("actual path activation through Slash key");
            self.key(Key::Slash,egui::Modifiers::NONE);
        }
        let out=self.frame(vec![]);
        let (response,clip,frame)=self.editor.file_browser.test_path_response().expect("actual dependency path TextEdit");assert_eq!(frame,self.ctx.cumulative_frame_nr()-1);assert!(response.has_focus());assert_eq!(self.ctx.memory(|m|m.focused()),Some(response.id));assert!(clip.contains(response.rect.center()));let area=self.bounds("dialog");let point=response.rect.center();
        assert!(response.enabled()&&area.contains(point)&&self.ctx.layer_id_at(point)==self.editor.file_browser.test_layer());
        assert!(out.shapes.iter().any(|s|s.clip_rect.contains(point)&&s.shape.visual_bounding_rect().intersects(response.rect)));self.events.last_mut().unwrap()["path_receipt"]=json!(true);self.record();self.note("actual dialog path-edit focused response bound to modal layer and clip");self.typed(path);
        self.frame(vec![]);
        if self.editor.file_browser.active(){let (selected,enabled,_,_,button)=self.editor.file_browser.test_selection(&self.ctx).unwrap();assert_eq!(selected,Some(PathBuf::from(path)));assert!(enabled);self.click("dialog",&button,None);}
    }
    fn save_as(&mut self,path:&str){
        assert!(!PathBuf::from(path).exists(),"private Save destination must be fresh");
        self.click("toolbar","Save as",None);assert!(self.editor.file_browser.active());
        self.dialog_path(path);self.settle();assert_eq!(self.editor.path,path);
        assert!(!self.editor.dirty);assert!(PathBuf::from(path).is_file());
        self.note("TAIL real Save as completed");
    }
    fn current_feature(&self)->Value {
        let found:Vec<_>=objects(&self.editor.state).iter().filter(|o|o["rectangular_draft"]["version"]==1).cloned().collect();
        assert_eq!(found.len(),1,"one current committed semantic rectangle");
        let f=found[0].clone();let m=&f["rectangular_draft"];
        assert_eq!(m["document_uid"],self.editor.state["document_uid"]);
        assert_eq!(m["revision"],self.editor.state["revision"]);
        assert_eq!(m["target"],json!({"name":f["name"],"native_id":f["native_id"]}));f
    }
    fn edit_current_input(&mut self){
        assert!(self.editor.draft.is_none());assert!(self.editor.state["preview"].is_null());
        let f=self.current_feature();let name=f["name"].as_str().unwrap();
        let tip=objects(&self.editor.state).iter().any(|o|o["type"]=="PartDesign::Body"&&o["body_state"]["tip"]==name);
        let text=if tip {format!("{} | tip",f["label"].as_str().or_else(||f["name"].as_str()).unwrap())}else{f["label"].as_str().or_else(||f["name"].as_str()).unwrap().to_owned()};
        let p=self.locate("Features",&text,None);
        self.events.push(json!({"phase":"TAIL actual tree double-click","feature":f,"text":text,"point":[p.x,p.y]}));self.record();
        self.at(p);self.at(p);
        assert!(self.editor.rectangular_active());
        assert_eq!(self.editor.draft.as_ref().unwrap().parameters["target"],f["rectangular_draft"]["target"]);
    }
    fn edit_current(&mut self){self.edit_current_input();self.settle();}
    fn history_input(&mut self,label:&str){
        assert!(matches!(label,"Undo"|"Redo"));
        assert!(self.editor.draft.is_none() && self.editor.state["preview"].is_null());
        let count=if label=="Undo"{"undo_count"}else{"redo_count"};
        assert!(self.editor.state[count].as_u64().unwrap_or(0)>0,"native history must enable {label}");
        self.note(&format!("HISTORY {label} input"));
        self.click("toolbar",label,None);
    }
    fn verification_tail(&mut self){
        self.observe("TAIL baseline");
        self.edit_current();self.observe("TAIL cancel opened");
        self.field("Length","length","1 in");self.observe("TAIL cancel changed");
        self.click("Feature","Cancel",None);self.settle();self.observe("TAIL cancel restored");
        self.edit_current();self.observe("TAIL apply opened");
        self.field("Length","length","1 in");self.observe("TAIL apply changed");
        self.click("Feature","Apply",None);self.settle();self.observe("TAIL applied");
        self.history_input("Undo");self.settle();self.observe("HISTORY undo");
        self.history_input("Redo");self.settle();self.observe("HISTORY redo");
        self.edit_current();self.observe("TAIL restore opened");
        self.field("Length","length","2 in");
        assert_eq!(self.editor.draft.as_ref().unwrap().parameters["inward"],true);
        assert_eq!(self.editor.draft.as_ref().unwrap().parameters["magnitude"],"30 deg");
        self.click("Feature","Apply",None);self.settle();self.observe("TAIL tutorial restored");
        let saved=self.folder.join("whole-tail.FCStd").to_str().unwrap().to_owned();
        self.save_as(&saved);self.observe("TAIL saved");
        let uid=self.editor.state["document_uid"].clone();
        // Verification-only protocol action: Editor has no independent Close
        // document button. This is never represented as a source gesture.
        self.note("TAIL verification-only normal protocol Close");
        self.editor.send("close",json!({}));self.settle();
        self.open(&saved);assert_ne!(self.editor.state["document_uid"],uid);
        self.current_feature();self.observe("TAIL reopened");
        self.edit_current();self.observe("TAIL reopened edit");
        self.field("Length","length","1 in");self.observe("TAIL reopened changed");
        self.click("Feature","Cancel",None);self.settle();self.observe("TAIL reopened cancel");
        self.observe("TAIL final verification");
    }
    fn observe(&mut self,tag:&str){
        self.note(tag);self.editor.send("test_read",json!({"tag":tag}));self.settle();
    }
    fn navigation(&mut self,step:u32){
        // Explicit bounded adaptation. No invented source deltas or screenshot match claim.
        self.events.push(json!({"source_step":step,"camera_match":"UNVERIFIED headless; declared input adaptation only"}));self.record();
        let p=egui::pos2(900.,520.);assert!(self.ctx.available_rect().contains(p));
        if matches!(step,37|39|45|46|50){
            self.frame(vec![egui::Event::PointerMoved(p),egui::Event::PointerButton{pos:p,button:egui::PointerButton::Secondary,pressed:true,modifiers:egui::Modifiers::NONE}]);
            let mut q=p;for i in 1..=8{q=p+egui::vec2(i as f32*10.,if step==46{i as f32*10.}else{0.});self.frame(vec![egui::Event::PointerMoved(q)]);}
            self.frame(vec![egui::Event::PointerButton{pos:q,button:egui::PointerButton::Secondary,pressed:false,modifiers:egui::Modifiers::NONE}]);
        }
        if matches!(step,39|43|44|47|48){let delta=egui::vec2(0.,if matches!(step,43|47){-40.}else{40.});self.frame(vec![egui::Event::PointerMoved(p),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta,modifiers:egui::Modifiers::NONE}]);}
        self.observe(&format!("S{step} navigation"));
    }
    fn hide_datum(&mut self,role:&str,body:&str){
        let rows:Vec<_>=objects(&self.editor.state).iter().filter(|o|o["type"]=="App::Plane" && o["properties"]["Role"]==role && o["body"]==body).cloned().collect();
        assert_eq!(rows.len(),1,"unique current source-body datum role {role}");let o=&rows[0];let name=o["name"].as_str().unwrap();assert_eq!(o["properties"]["Visibility"],true);
        // ID is read from the actual current native record, never historical.
        self.frame(vec![]);let out=self.frame(vec![]);let area=self.bounds("Features");
        let response=self.ctx.read_response(egui::Id::new(("feature_visibility",name))).expect("actual rendered native datum eye");let p=response.rect.center();
        assert!(response.enabled()&&area.contains(p));assert!(out.shapes.iter().any(|s|s.clip_rect.contains(p)&&s.shape.visual_bounding_rect().intersects(response.rect)),"eye must have actual paint in its clip");
        self.events.push(json!({"phase":"actual datum eye","role":role,"native_id":o["native_id"],"name":name,"rect":format!("{:?}",response.rect)}));self.record();self.at(p);self.settle();
        let current=objects(&self.editor.state).iter().find(|x|x["name"]==o["name"]&&x["native_id"]==o["native_id"]).unwrap();assert_eq!(current["properties"]["Visibility"],false);
    }
    fn cleanup(&mut self){
        let pending=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||self.settle()));
        if pending.is_err(){self.note("cleanup initial settle FAILED; retained, normal current-owner cleanup still attempted");}
        if self.editor.state["document"].is_string(){
            self.editor.send("inspect",json!({}));self.settle();
            if let Some(token)=self.editor.state["preview"]["token"].as_str().map(str::to_owned){self.editor.send("preview_cancel",json!({"token":token}));self.settle();}
            if self.editor.state["sketch_edit"]["token"].is_string(){
                self.editor.diagnostic_cancel_sketch_for_close();self.settle();
                assert!(self.editor.state["sketch_edit"].is_null(),"owned sketch canceled before Close");
            }
            self.editor.send("close",json!({}));self.settle();
        }
        self.editor.send("test_terminal_audit",json!({"audit":"final-no-owners-v1"}));self.settle();self.note("terminal audit before EOF");
    }
}
impl Drop for Native {fn drop(&mut self){let(sender,_)=mpsc::sync_channel(1);self.editor.jobs=sender;if let Some(actor)=self.actor.take(){let _=actor.join();}self.record();fs::write(self.folder.join("APP-events.json"),serde_json::to_vec_pretty(&self.events).unwrap()).unwrap();}}
#[test]
#[ignore="requires fresh independent review and separately serial build/native grants"]
fn actual_editor_source_s29_s51(){
    let mut n=Native::new("source-s29-s51");
    let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
        n.await_startup();
        let _current_source = source_prefix::run(&mut n).expect("current-build S08-S28 assertions before same-actor tail");
        let profiles:Vec<_>=objects(&n.editor.state).iter().filter(|o|o["rectangular_profile"]["eligible"]==true).cloned().collect();assert_eq!(profiles.len(),1);let source=profiles[0].clone();
        let source_name=source["name"].as_str().unwrap().to_owned();
        n.click("toolbar","Extrude",None);n.settle();
        // S30 collector adaptation: Finish retains the selected profile, and the
        // production Extrude command selects its native validated rectangle route.
        // Click its actual visible Profile control without manufacturing a popup.
        n.bound_profile_control(&source);
        assert!(n.editor.rectangular_active());let p=&n.editor.draft.as_ref().unwrap().parameters;assert_eq!(p["source"]["name"],source_name);assert_eq!(p["operation"],"New");assert_eq!(p["termination"],"Blind");assert_eq!(p["side"],"one-side");assert_eq!(p["solid"],true);
        // Source frame 0182 shows 1 in. The native default preview is
        // retained as preparation, not credited as the source S30 state.
        n.note("S30 implementation adaptation: explicitly enter shown 1 in; prior native default preview is preparation only");
        n.field("Length","length","1 in");n.observe("S30 default0");
        n.field("Length","length","2 in");n.observe("S31 length2in");n.note("S32 fixed task panel adaptation; no dialog drag supported");
        n.field("Draft magnitude","magnitude","3 deg");n.observe("S33 outward3");
        n.field("Draft magnitude","magnitude","-3 deg");assert!(!n.editor.draft.as_ref().unwrap().valid);assert!(n.editor.error.is_some());n.note("S34 before disabled Apply pointer proof");n.disabled_apply();assert!(n.editor.draft.is_some());assert!(!n.editor.draft.as_ref().unwrap().valid);n.observe("S34 rejected magnitude negative3");
        // Inspect is deliberately not sent here: observation must not advance published owner state.
        n.field("Draft magnitude","magnitude","30 deg");n.observe("S35 actual outward30 separate from source error");
        n.click("Feature","Inward",None);n.settle();assert_eq!(n.editor.draft.as_ref().unwrap().parameters["magnitude"],"30 deg");assert!(n.editor.draft.as_ref().unwrap().valid);n.observe("S36 inward30");
        n.navigation(37);n.click("Feature","Apply",None);n.settle();assert!(n.editor.draft.is_none());assert!(n.editor.state["preview"].is_null());n.observe("S38 commit");
        for step in 39..=51 {
            if matches!(step,40..=42){let role=match step{40=>"XY_Plane",41=>"XZ_Plane",_=>"YZ_Plane"};let body=source["rectangular_profile"]["body"]["name"].as_str().unwrap();n.hide_datum(role,body);n.observe(&format!("S{step} datum eye"));continue;}
            n.navigation(step);
        }
        n.observe("S51 endpoint");n.verification_tail();
    }));
    let cleanup=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||n.cleanup()));
    if outcome.is_err(){n.note("source sequence FAILED; original panic retained");}
    if cleanup.is_err(){n.note("normal cleanup FAILED independently; EOF remains fallback only");}
    drop(n);
    if let Err(e)=outcome{std::panic::resume_unwind(e);}if let Err(e)=cleanup{std::panic::resume_unwind(e);}
}
#[test]
fn journal_tail_is_lossless(){
    // Explicit offline fixture only. No worker is spawned by this selector.
    let (editor,input,_output)=super::fixture();let (_sender,raw)=mpsc::channel();let (forward,_receiver)=mpsc::channel();
    let folder=PathBuf::from(std::env::var_os("CAD_CONVERGENCE_OUTPUT").unwrap()).join("offline-journal-tail");fs::create_dir(&folder).unwrap();
    let mut n=Native{editor,raw,forward,actor:None,startup_ready:None,events:vec![],recorded:std::cell::Cell::new(0),folder:folder.clone(),ctx:egui::Context::default(),time:0.};
    n.events.push(json!({"raw":[1,2,3],"data":{"base_value":1.0000000000000002}}));n.record();
    let initial=fs::read_to_string(folder.join("APP-events.jsonl")).unwrap();
    n.events.last_mut().unwrap()["editor"]=n.ui_state();n.record();n.events.push(json!({"text":"2 in","native":false}));n.record();let expected=n.events.clone();drop(n);drop(input);
    let mut decoded:Vec<Value>=vec![];for line in fs::read_to_string(folder.join("APP-events.jsonl")).unwrap().lines(){let row:Value=serde_json::from_str(line).unwrap();assert_eq!(row.as_object().unwrap().len(),2);let index=row["index"].as_u64().unwrap() as usize;if index==decoded.len(){decoded.push(row["value"].clone());}else{assert_eq!(index,decoded.len()-1);decoded[index]=row["value"].clone();}}
    assert_eq!(decoded,expected);assert_eq!(serde_json::from_slice::<Vec<Value>>(&fs::read(folder.join("APP-events.json")).unwrap()).unwrap(),expected);
    let first:Value=serde_json::from_str(initial.lines().next().unwrap()).unwrap();assert!(first["value"].get("editor").is_none());assert!(decoded[0].get("editor").is_some());
}

#[test]
fn offline_full_app_postcommit_undo_redo_uses_current_native_history(){
    let(mut n,input,output)=offline_app("offline-postcommit-history");
    // Declared MOCK replies, not application/native qualification. Actual
    // rendered buttons dispatch each job; no job or widget state is fabricated.
    n.editor.state["document_uid"]=json!("mock-history-owner");
    n.editor.state["undo_count"]=json!(2);n.editor.state["redo_count"]=json!(0);
    n.history_input("Undo");
    let job=input.try_recv().expect("rendered Undo sends one job");
    assert_eq!(job.op,"undo");assert_eq!(job.parameters["expected_revision"],17);
    assert!(input.try_recv().is_err());
    let mut state=n.editor.state.clone();state["revision"]=json!(18);
    state["undo_count"]=json!(1);state["redo_count"]=json!(1);
    super::reply(&output,&mut n.editor,&n.ctx,job,true,Value::Null,state);
    n.history_input("Redo");
    let job=input.try_recv().expect("rendered Redo sends one job");
    assert_eq!(job.op,"redo");assert_eq!(job.parameters["expected_revision"],18);
    assert!(input.try_recv().is_err());
    let mut state=n.editor.state.clone();state["revision"]=json!(19);
    state["undo_count"]=json!(2);state["redo_count"]=json!(0);
    super::reply(&output,&mut n.editor,&n.ctx,job,true,Value::Null,state);
    assert_eq!(n.editor.state["document_uid"],"mock-history-owner");
    assert_eq!(n.editor.state["revision"],19);
    assert!(n.editor.draft.is_none() && n.editor.state["preview"].is_null());
    n.note("MOCK postcommit toolbar history dispatch; native outcome unqualified");
}

// Mocked full-App UI qualification only. These tests never construct WorkerSession.
// Schema bytes are the complete historical actual-pad-schema, not a minimal fake.
fn offline_app(case: &str) -> (Native, Receiver<Job>, Sender<Reply>) {
    let (mut editor, input, output) = super::fixture();
    // Explicit MOCK setup before any input. Neither fixture is native evidence.
    editor.dirty = false;
    if case == "offline-profile" {
        editor.state["objects"].as_array_mut().unwrap().push(json!({
            "name":"OfflineBody", "label":"Offline body", "type":"PartDesign::Body",
            "native_id":990, "body":"OfflineBody", "properties":{"Visibility":true},
            "body_state":{"tip":null,"consumed_by":[]}
        }));
        editor.body_ui.active = Some("OfflineBody".into());
    }
    let (_unused, raw) = mpsc::channel();
    let (forward, _replies) = mpsc::channel();
    let folder = PathBuf::from(std::env::var_os("CAD_CONVERGENCE_OUTPUT").unwrap()).join(case);
    fs::create_dir(&folder).unwrap();
    (Native {editor, raw, forward, actor:None, startup_ready:None, events:vec![], recorded:std::cell::Cell::new(0),
             folder, ctx:egui::Context::default(), time:0.}, input, output)
}

#[test]
fn offline_full_app_slash_selects_exact_path_without_model_setter() {
    let (mut n, input, _output) = offline_app("offline-slash");
    let path = std::env::var("CAD_CONVERGENCE_FIXTURE").unwrap();
    n.click("toolbar", "Open", None);
    n.key(Key::Slash, egui::Modifiers::NONE);
    let out = n.frame(vec![]);
    let (response, clip, frame) = n.editor.file_browser.test_path_response().expect("exact dependency path response");
    assert_eq!(frame, n.ctx.cumulative_frame_nr()-1);
    assert_eq!(n.ctx.memory(|m|m.focused()), Some(response.id));
    assert!(response.has_focus() && clip.contains(response.rect.center()));
    assert!(response.enabled());
    assert_eq!(Some(response.layer_id), n.editor.file_browser.test_layer());
    assert!(out.shapes.iter().any(|s|s.clip_rect.contains(response.rect.center()) && s.shape.visual_bounding_rect().intersects(response.rect)));
    // App Escape cancels the entire browser. Reopen through the real toolbar
    // before requesting a fresh path TextEdit. The standalone dependency loop
    // below separately tests hiding its path editor without closing its dialog.
    n.key(Key::Escape,egui::Modifiers::NONE);n.frame(vec![]);
    assert!(!n.editor.file_browser.active());
    assert!(n.editor.file_browser.test_path_response().is_none());
    n.click("toolbar","Open",None);
    n.key(Key::Slash,egui::Modifiers::NONE);n.frame(vec![]);
    assert!(n.editor.file_browser.test_path_response().is_some());
    n.typed(&path); n.frame(vec![]);
    if n.editor.file_browser.active() {
        let (selected, enabled, _, _, button) = n.editor.file_browser.test_selection(&n.ctx).unwrap();
        assert_eq!(selected, Some(PathBuf::from(&path))); assert!(enabled);
        n.click("dialog", &button, None);
    }
    let job = input.try_recv().expect("real App routes path as Open job");
    assert_eq!(job.op, "open"); assert_eq!(job.parameters["path"],path);
    assert!(input.try_recv().is_err());
    n.note("MOCKED offline Slash exact selected-path identity, zero native credit");
    // Exercise both public dependency entry points, including closed returns.
    // No private field or fabricated response is written by the test.
    for alternate in [false,true] {
        let context=egui::Context::default();
        let mut dialog=egui_file_dialog::FileDialog::new();dialog.pick_file();
        let mut update=|events:Vec<egui::Event>| {
            let _=context.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1280.,850.))),events,..Default::default()},|ctx|{
                if alternate {dialog.update_with_right_panel_ui(ctx,&mut |_ui,_dialog|{});} else {dialog.update(ctx);}
            });
            (dialog.cad_test_path_response().is_some(),dialog.state().clone())
        };
        let event=|key,pressed|egui::Event::Key{key,physical_key:None,pressed,repeat:false,modifiers:egui::Modifiers::NONE};
        assert!(!update(vec![]).0);
        assert!(update(vec![event(Key::Slash,true)]).0);
        assert!(update(vec![event(Key::Slash,false)]).0);
        assert!(!update(vec![event(Key::Escape,true)]).0);
        assert!(!update(vec![event(Key::Escape,false)]).0);
        let mut closed=false;
        for _ in 0..3 {
            let (_,state)=update(vec![event(Key::Escape,true)]);
            update(vec![event(Key::Escape,false)]);
            if state!=egui_file_dialog::DialogState::Open {closed=true;break;}
        }
        assert!(closed,"normal Escape must close standalone dependency dialog");
        assert!(!update(vec![]).0,"closed update must not retain a path response");
    }
}

#[test]
fn offline_full_app_profile_complete_schema_and_disabled_apply() {
    let (mut n, input, output) = offline_app("offline-profile");
    n.click("toolbar", "Extrude", None);
    let job = input.try_recv().expect("actual toolbar requests native schema");
    assert_eq!(job.op,"feature_schema");
    // Pending schema is an actual busy task. Verify real disabled widget and
    // no queued action across pointer input, without assigning draft validity.
    n.frame(vec![]);
    let (apply,clip,layer) = n.ctx.data(|d|d.get_temp::<(egui::Response,egui::Rect,egui::LayerId)>(egui::Id::new("convergence_apply_response"))).unwrap();
    assert!(!apply.enabled()); assert!(clip.contains(apply.rect.center()));
    assert_eq!(n.ctx.layer_id_at(apply.rect.center()),Some(layer));
    let before=n.ui_state();n.at(apply.rect.center());assert_eq!(n.ui_state(),before);
    assert!(input.try_recv().is_err());
    let schema:Value=serde_json::from_str(include_str!("../../tests/fixtures/offline-pad-schema.json")).unwrap();
    assert!(schema["property_types"].as_object().unwrap().len()>20);
    let state=n.editor.state.clone();
    super::reply(&output,&mut n.editor,&n.ctx,job,true,schema,state);
    n.frame(vec![]);n.frame(vec![]);
    let (control,clip,layer,value)=n.ctx.data(|d|d.get_temp::<(egui::Response,egui::Rect,egui::LayerId,Value)>(egui::Id::new("convergence_profile_response"))).expect("actual Profile ComboBox");
    assert!(value.is_null());assert!(control.enabled());
    if clip.contains(control.rect.center()) {
        assert_eq!(n.ctx.layer_id_at(control.rect.center()),Some(layer));
    }
    // Move the actual full native property scroll using input, retain a current
    // response, and require an observed clipped case. No fabricated clip rect.
    let mut clipped=false;
    for _ in 0..12 {
        let p=n.bounds("Feature").center();
        n.frame(vec![egui::Event::PointerMoved(p),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.,-180.),modifiers:egui::Modifiers::NONE}]);
        n.frame(vec![]);
        if let Some((current,current_clip,_,_))=n.ctx.data(|d|d.get_temp::<(egui::Response,egui::Rect,egui::LayerId,Value)>(egui::Id::new("convergence_profile_response"))) {
            if !current_clip.contains(current.rect.center()) {clipped=true;break;}
        }
    }
    assert!(clipped,"complete-schema actual scroll must produce a clipped Profile fixture");
    // Return with bounded wheel input, then re-resolve the actual current widget.
    for _ in 0..12 {
        let p=n.bounds("Feature").center();
        n.frame(vec![egui::Event::PointerMoved(p),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.,180.),modifiers:egui::Modifiers::NONE}]);
    }
    n.profile_control();
    n.note("MOCKED full App complete Pad schema/Profile and busy disabled Apply; zero native credit");
}


#[test]
fn offline_full_app_current_tree_edit_field_dispatch() {
    let (mut n,input,output)=offline_app("offline-tail-edit");
    // Explicit mock setup, zero native/whole credit. Identity deliberately
    // differs from every retained actual native ID.
    let binding=json!({"version":1,"document":"ActualDocument","document_uid":"mock-tail",
        "revision":17,"source":{"name":"Profile","native_id":8101},
        "body":{"name":"Body","native_id":8102},"boundary":{"geometry_ids":[1,2,3,4],"mapped_edges":["a","b","c","d"]},
        "target":{"name":"Editable","native_id":8103},"length":50.8,"angle":-30.,"reversed":false});
    n.editor.state["document_uid"]=json!("mock-tail");
    n.editor.state["objects"]=json!([
        {"name":"Body","label":"Body","type":"PartDesign::Body","native_id":8102,
         "body_state":{"tip":"Editable"},"properties":{"Group":[{"link":"Profile"},{"link":"Editable"}]}},
        {"name":"Profile","label":"Profile","body":"Body","type":"Sketcher::SketchObject","native_id":8101},
        {"name":"Editable","label":"Editable","body":"Body","type":"PartDesign::FeaturePython","native_id":8103,"rectangular_draft":binding}]);
    n.edit_current_input();
    assert!(n.editor.rectangular_active());assert_eq!(n.editor.draft.as_ref().unwrap().parameters["target"]["native_id"],8103);
    super::settle(&mut n.editor,&n.ctx);
    let job=input.try_recv().unwrap();assert_eq!(job.op,"quantities");
    let state=n.editor.state.clone();let result=super::quantity(&state,"30 deg",30.);
    super::reply(&output,&mut n.editor,&n.ctx,job,true,result,state);
    let job=input.try_recv().unwrap();assert_eq!(job.op,"preview");
    assert_eq!(job.parameters["edits"][0]["op"],"update_rectangular_draft");
    assert_eq!(job.parameters["edits"][0]["target"],json!({"name":"Editable","native_id":8103}));
    // Accept only a mocked worker reply. Real field input then dispatches a
    // second preview with the original literal and current native target.
    let mut state=n.editor.state.clone();state["preview"]=json!({"token":job.parameters["token"]});
    super::reply(&output,&mut n.editor,&n.ctx,job,true,Value::Null,state);
    let old=n.editor.draft.as_ref().unwrap().parameters["length"].as_str().unwrap().to_owned();
    n.click("Feature",&old,Some("Length"));n.typed("1 in");super::settle(&mut n.editor,&n.ctx);
    let job=input.try_recv().unwrap();assert_eq!(job.op,"preview");
    assert_eq!(job.parameters["edits"][0]["length"],"1 in");assert_eq!(job.parameters["edits"][0]["target"]["native_id"],8103);
    assert!(input.try_recv().is_err());
}

#[test]
fn offline_full_app_save_as_path_dispatches_new_path() {
    let (mut n,input,_output)=offline_app("offline-tail-save");
    let path=n.folder.join("mock-save.FCStd").to_str().unwrap().to_owned();assert!(!PathBuf::from(&path).exists());
    n.click("toolbar","Save as",None);assert!(n.editor.file_browser.active());
    n.dialog_path(&path);
    let job=input.try_recv().unwrap();assert_eq!(job.op,"save");assert_eq!(job.parameters["path"],path);
    assert!(!n.editor.file_browser.active());assert!(input.try_recv().is_err());
    assert!(!PathBuf::from(path).exists(),"MOCK never writes a native document");
}

// Declared MOCK replay of the completed R164 schema/preview boundaries. No
// WorkerSession/native actor is constructed; real full-App input is exercised.
fn offline_current_profile(case:&str)->(Native,Receiver<Job>,Sender<Reply>,Value) {
    let f:Value=serde_json::from_str(include_str!("../../tests/fixtures/offline-current-profile.json")).unwrap();
    let (mut n,input,output)=offline_app(case);
    n.editor.state=f["ready"].clone();
    let source=objects(&n.editor.state).iter().find(|o|o["rectangular_profile"]["eligible"]==true).unwrap().clone();
    n.editor.selected_object=Some(source["name"].as_str().unwrap().into());
    n.editor.body_ui.active=Some(source["body"].as_str().unwrap().into());
    n.click("toolbar","Extrude",None);
    let job=input.try_recv().unwrap();assert_eq!(job.op,"feature_schema");
    assert_eq!(job.parameters["values"],json!({"Profile":{"link":source["name"]}}));
    let state=n.editor.state.clone();
    super::reply(&output,&mut n.editor,&n.ctx,job,true,f["schema"].clone(),state);
    super::settle(&mut n.editor,&n.ctx);
    assert!(n.editor.rectangular_active(),"actual automatic schema-to-rectangle route");
    let query=input.try_recv().unwrap();assert_eq!(query.op,"quantities");
    let state=n.editor.state.clone();let data=super::quantity(&state,"0 deg",0.);
    super::reply(&output,&mut n.editor,&n.ctx,query,true,data,state);
    super::settle(&mut n.editor,&n.ctx);
    let preview=input.try_recv().unwrap();assert_eq!(preview.op,"preview");
    let mut state=f["preview"].get("state").unwrap_or(&f["preview"]).clone();
    let target=n.editor.draft.as_ref().unwrap().parameters["name"].as_str().unwrap().to_owned();
    fn rename(v:&mut Value,name:&str) {
        match v {
            Value::String(s) if s=="Pad5"=>*s=name.into(),
            Value::Array(a)=>for item in a {rename(item,name);},
            Value::Object(o)=>for item in o.values_mut() {rename(item,name);},
            _=>{}
        }
    }
    rename(&mut state,&target);
    state["preview"]["token"]=json!(n.editor.draft.as_ref().unwrap().token);
    super::reply(&output,&mut n.editor,&n.ctx,preview,true,Value::Null,state);
    assert!(n.editor.draft.as_ref().unwrap().valid);
    assert!(input.try_recv().is_err());
    (n,input,output,source)
}
#[test]
fn offline_current_profile_toolbar_route_and_owned_click() {
    let(mut n,input,_output,source)=offline_current_profile("offline-current-profile-positive");
    let before=n.ui_state();n.bound_profile_control(&source);
    assert_eq!(n.ui_state(),before,"visible collection preserves model/draft state");
    assert!(input.try_recv().is_err());
    assert!(n.events.iter().any(|e|e["phase"]=="actual bound Profile response"));
}
#[test]
fn offline_current_profile_rejects_foreign_source_and_owner() {
    for change in ["source","owner"] {
        let(mut n,input,_output,mut source)=offline_current_profile(&format!("offline-current-profile-{change}"));
        if change=="source" {source["native_id"]=json!(999999);}else{n.editor.state["document_uid"]=json!("foreign-owner");}
        let before=n.ui_state();
        let denied=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||n.bound_profile_control(&source)));
        assert!(denied.is_err());assert_eq!(n.ui_state(),before);assert!(input.try_recv().is_err());
    }
}
