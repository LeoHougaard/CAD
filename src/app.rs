use crate::{
    engine::{Runtime, Scene},
    model::{Document, Feature, Plane, Sketch},
    plugins::{Field, Manifest, Numbers, Registry},
    render::{self, Camera, Renderer},
    sketch_ui::{SketchUi, Tool},
};
use eframe::egui::{self, Color32, Rect, RichText, Sense, Stroke, Vec2};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

const ACCENT: Color32 = Color32::from_rgb(76, 191, 206);
const MUTED: Color32 = Color32::from_rgb(143, 155, 173);
const RED: Color32 = Color32::from_rgb(243, 125, 133);
const PANEL: Color32 = Color32::from_rgb(24, 30, 39);

fn save_checked(
    document: &Document,
    base: Option<&Document>,
    path: &Path,
) -> crate::model::Result<()> {
    let _lock = crate::file_lock::FileLock::try_acquire(path)?;
    if let Some(base) = base
        && Document::load(path)? != *base
    {
        return Err(
            "File changed on disk. Reload with Ctrl+R, or use Save as to keep your edits.".into(),
        );
    }
    document.save(path)
}

enum Request {
    Build {
        serial: u64,
        document: Document,
        registry: Arc<Registry>,
        source_end: Option<usize>,
    },
    Export {
        document: Document,
        registry: Arc<Registry>,
        format: String,
        path: PathBuf,
    },
}
enum Response {
    Built {
        serial: u64,
        result: crate::model::Result<Arc<Scene>>,
        source: Option<Arc<Scene>>,
    },
    Exported(crate::model::Result<PathBuf>),
}
struct Worker {
    send: mpsc::Sender<Request>,
    receive: mpsc::Receiver<Response>,
}
impl Worker {
    fn new(ctx: egui::Context, registry: Arc<Registry>) -> Self {
        let (tx, rx) = mpsc::channel::<Request>();
        let (out_tx, out_rx) = mpsc::channel();
        std::thread::Builder::new().name("cad-geometry".into()).spawn(move||{
            let mut runtime=Runtime::shared(registry);let mut pending=VecDeque::new();
            loop{let mut job=match pending.pop_front(){Some(x)=>x,None=>match rx.recv(){Ok(x)=>x,Err(_)=>break}};
                while let Ok(next)=rx.try_recv(){if matches!((&job,&next),(Request::Build{..},Request::Build{..}))&&pending.is_empty(){job=next;}else{pending.push_back(next);}}
                let response=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||match job{
                    Request::Build{serial,document,registry,source_end}=>{
                        if !Arc::ptr_eq(&runtime.registry,&registry){runtime=Runtime::shared(registry);}
                        let source=source_end.and_then(|end|{let mut d=document.clone();d.rollback=Some(end);runtime.evaluate(&d).ok().map(|e|e.scene)});
                        Response::Built{serial,result:runtime.evaluate(&document).map(|x|x.scene),source}
                    }
                    Request::Export{document,registry,format,path}=>{
                        if !Arc::ptr_eq(&runtime.registry,&registry){runtime=Runtime::shared(registry);}
                        let result=(||{let evaluation=runtime.evaluate(&document)?;if let Some(e)=&evaluation.scene.error{return Err(format!("Export stopped because the model has a feature error: {e}"));}evaluation.geometry.export(&format,&path)?;Ok(path)})();Response::Exported(result)
                    }
                }));
                match response{Ok(response)=>{let _=out_tx.send(response);},Err(_)=>{runtime=Runtime::shared(runtime.registry.clone());let _=out_tx.send(Response::Built{serial:u64::MAX,result:Err("Geometry operation panicked. The worker was reset; your editable document is preserved.".into()),source:None});}}
                ctx.request_repaint();
            }
        }).expect("geometry worker starts");
        Self {
            send: tx,
            receive: out_rx,
        }
    }
}

struct Editor {
    index: usize,
    is_new: bool,
    feature: Feature,
    sketch: Option<Sketch>,
    changed: bool,
    undo: Vec<Value>,
    redo: Vec<Value>,
}
impl Editor {
    fn sync(&mut self) {
        if let Some(sk) = &self.sketch {
            self.feature.set_sketch(sk);
        }
    }
}
enum FileAction {
    Open,
    Save,
    Export(String),
}
struct FileDialog {
    action: FileAction,
    directory: PathBuf,
    directory_text: String,
    filename: String,
    error: Option<String>,
}

pub struct CadApp {
    document: Document,
    saved: Document,
    path: Option<PathBuf>,
    past: VecDeque<Document>,
    future: Vec<Document>,
    registry: Arc<Registry>,
    plugin_dir: PathBuf,
    worker: Worker,
    scene: Arc<Scene>,
    source_scene: Option<Arc<Scene>>,
    serial: u64,
    render_revision: u64,
    busy: bool,
    last_error: Option<String>,
    notice: String,
    editor: Option<Editor>,
    selected_feature: Option<usize>,
    selected_part: Option<String>,
    selected_face: Option<(String, String, usize, Option<Plane>)>,
    selected_edges: Vec<(String, String)>,
    sketch_ui: SketchUi,
    camera: Camera,
    mouse_orbiting: bool,
    tree_open: bool,
    space_mouse: crate::spacemouse::SpaceMouse,
    shaded_edges: bool,
    renderer: Arc<Mutex<Renderer>>,
    hidden: BTreeSet<String>,
    pending_rebuild: Option<Instant>,
    fit_when_ready: bool,
    align_when_ready: bool,
    files: Option<FileDialog>,
    variables_open: bool,
    variables_draft: Vec<(String, String)>,
    variables_name: String,
    plugins_open: bool,
    plugin_catalog: Vec<Manifest>,
    help_open: bool,
    rename_feature: Option<usize>,
    rename_text: String,
    drag_feature: Option<usize>,
    pending_load: Option<(Document, Option<PathBuf>)>,
    viewport_rect: Rect,
    frame_count: u64,
    close_pending: bool,
    allow_close: bool,
}

impl CadApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        document: Document,
        path: Option<PathBuf>,
        plugin_dir: PathBuf,
        space_mouse: crate::spacemouse::SpaceMouse,
    ) -> Self {
        let ctx = &cc.egui_ctx;
        space_mouse.initialize(cc);
        ctx.set_theme(egui::Theme::Dark);
        let mut style = (*ctx.style()).clone();
        style.visuals.panel_fill = PANEL;
        style.visuals.window_fill = Color32::from_rgb(29, 36, 47);
        style.visuals.extreme_bg_color = Color32::from_rgb(15, 21, 29);
        style.visuals.faint_bg_color = Color32::from_rgb(31, 39, 51);
        style.visuals.selection.bg_fill = Color32::from_rgb(37, 88, 101);
        style.visuals.selection.stroke = Stroke::new(1.0f32, ACCENT);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(43, 105, 116);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 57, 73);
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(34, 43, 56);
        style.visuals.override_text_color = Some(Color32::from_rgb(215, 224, 234));
        style.visuals.window_shadow = egui::epaint::Shadow::NONE;
        style.spacing.item_spacing = Vec2::new(6., 4.);
        style.spacing.button_padding = Vec2::new(7., 3.);
        style.spacing.interact_size.y = 24.;
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.));
        ctx.set_style(style);
        let registry = Arc::new(Registry::load(&plugin_dir));
        let worker = Worker::new(ctx.clone(), registry.clone());
        let renderer = Arc::new(Mutex::new(
            Renderer::new(cc.gl.as_ref().expect("OpenGL renderer"))
                .expect("CAD viewport shaders compile"),
        ));
        let mut app = Self {
            saved: document.clone(),
            document,
            path,
            past: VecDeque::new(),
            future: vec![],
            registry,
            plugin_dir,
            worker,
            scene: Arc::new(Scene::default()),
            source_scene: None,
            serial: 0,
            render_revision: 0,
            busy: false,
            last_error: None,
            notice: String::new(),
            editor: None,
            selected_feature: None,
            selected_part: None,
            selected_face: None,
            selected_edges: vec![],
            sketch_ui: SketchUi::default(),
            camera: Camera::default(),
            mouse_orbiting: false,
            tree_open: true,
            space_mouse,
            shaded_edges: true,
            renderer,
            hidden: BTreeSet::new(),
            pending_rebuild: None,
            fit_when_ready: true,
            align_when_ready: false,
            files: None,
            variables_open: false,
            variables_draft: vec![],
            variables_name: String::new(),
            plugins_open: false,
            plugin_catalog: vec![],
            help_open: false,
            rename_feature: None,
            rename_text: String::new(),
            drag_feature: None,
            pending_load: None,
            viewport_rect: Rect::NOTHING,
            frame_count: 0,
            close_pending: false,
            allow_close: false,
        };
        app.rebuild();
        app
    }
    fn dirty(&self) -> bool {
        self.document != self.saved
    }
    fn preview_document(&self) -> Document {
        let mut d = self.document.clone();
        if let Some(edit) = &self.editor {
            let mut feature = edit.feature.clone();
            if let Some(sk) = &edit.sketch {
                feature.set_sketch(sk);
            }
            if edit.is_new {
                d.next_id += 1;
                d.features.insert(edit.index, feature);
            } else {
                d.features[edit.index] = feature;
            }
            d.rollback = Some(edit.index + 1);
        }
        d
    }
    fn rebuild(&mut self) {
        self.serial += 1;
        self.busy = true;
        self.pending_rebuild = None;
        let _ = self.worker.send.send(Request::Build {
            serial: self.serial,
            document: self.preview_document(),
            registry: self.registry.clone(),
            source_end: if self.source_scene.is_none() {
                self.editor
                    .as_ref()
                    .filter(|e| {
                        self.registry
                            .plugins
                            .get(&e.feature.plugin)
                            .is_some_and(|p| {
                                p.manifest
                                    .fields
                                    .iter()
                                    .any(|f| f.kind == "edges" || f.kind == "faces")
                            })
                    })
                    .map(|e| e.index)
            } else {
                None
            },
        });
    }
    fn schedule(&mut self, ctx: &egui::Context) {
        self.pending_rebuild = Some(Instant::now() + Duration::from_millis(65));
        ctx.request_repaint_after(Duration::from_millis(70));
    }
    fn remember(&mut self, before: Document) {
        if before == self.document {
            return;
        }
        self.notice.clear();
        self.past.push_back(before);
        while self.past.len() > 40
            || self
                .past
                .iter()
                .map(|d| d.features.len() + d.variables.len())
                .sum::<usize>()
                > 6000
        {
            self.past.pop_front();
        }
        self.future.clear();
        self.last_error = None;
        self.rebuild();
    }
    fn begin(&mut self, id: &str) {
        self.notice.clear();
        self.sketch_ui = SketchUi::default();
        if self.editor.is_some() {
            return;
        }
        let index = self
            .document
            .rollback
            .unwrap_or(self.document.features.len());
        let mut scratch = self.document.clone();
        let mut f = match self.registry.create(id, &mut scratch) {
            Ok(f) => f,
            Err(e) => {
                self.last_error = Some(e);
                return;
            }
        };
        let manifest = &self.registry.plugins[id].manifest;
        let mut primary_sketch = true;
        for field in &manifest.fields {
            match field.kind.as_str() {
                "sketch" if primary_sketch => {
                    primary_sketch = false;
                    if let Some(s) = self
                        .selected_feature
                        .filter(|i| *i < index)
                        .and_then(|i| self.document.features.get(i))
                        .filter(|s| {
                            !s.suppressed
                                && self
                                    .registry
                                    .plugins
                                    .get(&s.plugin)
                                    .is_some_and(|p| p.manifest.editor == "sketch")
                        })
                    {
                        f.params[&field.key] = json!(s.id);
                    } else if let Some(s) = self.document.features[..index].iter().rev().find(|s| {
                        !s.suppressed
                            && self
                                .registry
                                .plugins
                                .get(&s.plugin)
                                .is_some_and(|p| p.manifest.editor == "sketch")
                    }) {
                        f.params[&field.key] = json!(s.id);
                    }
                }
                "body" => {
                    if let Some(part) = &self.selected_part {
                        f.params[&field.key] = json!(part);
                    }
                }
                "edges" => {
                    f.params[&field.key] = json!(
                        self.selected_edges
                            .iter()
                            .map(|(_, k)| k)
                            .collect::<Vec<_>>()
                    );
                }
                "faces" => {
                    if let Some((_, key, _, _)) = &self.selected_face {
                        f.params[&field.key] = json!([key]);
                    }
                }
                _ => (),
            }
        }
        if manifest.editor == "sketch" {
            if self
                .selected_face
                .as_ref()
                .is_some_and(|(_, _, _, plane)| plane.is_none())
            {
                self.last_error = Some("Choose an origin plane or supported planar face.".into());
                return;
            }
            if let Some((_, key, _, Some(_))) = &self.selected_face {
                f.params["support"] = json!(key);
            }
            self.align_when_ready = true;
            self.sketch_ui.set_tool(Tool::Rectangle);
        } else {
            self.sketch_ui.cancel_gesture();
        }
        let sketch = f.sketch();
        self.source_scene = Some(self.scene.clone());
        if id == "extrude" || id == "revolve" {
            self.camera.rotation = Camera::default().rotation;
            self.fit_when_ready = true;
        }
        self.editor = Some(Editor {
            index,
            is_new: true,
            feature: f,
            sketch,
            changed: false,
            undo: vec![],
            redo: vec![],
        });
        self.rebuild();
    }
    fn edit_feature(&mut self, index: usize) {
        self.notice.clear();
        self.sketch_ui = SketchUi::default();
        if self.editor.is_some() {
            return;
        }
        let Some(feature) = self.document.features.get(index).cloned() else {
            return;
        };
        let sketch = feature.sketch();
        self.source_scene = None;
        let align = sketch.is_some();
        self.editor = Some(Editor {
            index,
            is_new: false,
            feature,
            sketch,
            changed: false,
            undo: vec![],
            redo: vec![],
        });
        self.sketch_ui.cancel_gesture();
        self.align_when_ready = align;
        self.rebuild();
    }
    fn finish_editor(&mut self, apply: bool) {
        self.source_scene = None;
        if let Some(mut edit) = self.editor.take() {
            if apply {
                edit.sync();
                let before = self.document.clone();
                if edit.is_new {
                    self.document.allocate_id();
                    self.document.features.insert(edit.index, edit.feature);
                } else {
                    self.document.features[edit.index] = edit.feature;
                }
                self.selected_feature = Some(edit.index);
                self.document.rollback = None;
                self.remember(before);
            } else {
                self.rebuild();
            }
        }
        self.sketch_ui.cancel_gesture();
    }
    fn undo(&mut self, redo: bool) {
        if let Some(editor) = &mut self.editor {
            editor.sync();
            let stack = if redo {
                &mut editor.redo
            } else {
                &mut editor.undo
            };
            if let Some(v) = stack.pop() {
                let old = std::mem::replace(&mut editor.feature.params, v);
                editor.sketch = editor.feature.sketch();
                if redo {
                    editor.undo.push(old);
                } else {
                    editor.redo.push(old);
                }
                self.sketch_ui.cancel_gesture();
                self.rebuild();
            }
            return;
        }
        let other = if redo {
            self.future.pop()
        } else {
            self.past.pop_back()
        };
        if let Some(d) = other {
            let old = std::mem::replace(&mut self.document, d);
            if redo {
                self.past.push_back(old);
            } else {
                self.future.push(old);
            }
            self.selected_feature = None;
            self.rebuild();
        }
    }
    fn queue_load(&mut self, d: Document, path: Option<PathBuf>) {
        if self.dirty() || self.editor.is_some() {
            self.pending_load = Some((d, path));
        } else {
            self.load(d, path);
        }
    }
    fn load(&mut self, d: Document, path: Option<PathBuf>) {
        self.notice.clear();
        self.saved = d.clone();
        self.document = d;
        self.path = path;
        self.past.clear();
        self.future.clear();
        self.editor = None;
        self.source_scene = None;
        self.selected_feature = None;
        self.selected_face = None;
        self.selected_part = None;
        self.selected_edges.clear();
        self.hidden.clear();
        self.fit_when_ready = true;
        self.rebuild();
    }
    fn open_dialog(&mut self, action: FileAction) {
        let dir = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|p| PathBuf::from(p).join("Documents"))
                    .filter(|p| p.is_dir())
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            });
        let ext = match &action {
            FileAction::Export(f) => f.as_str(),
            _ => "cadpart",
        };
        let filename = if matches!(action, FileAction::Open) {
            String::new()
        } else {
            format!(
                "{}.{}",
                self.document.name.replace(['/', '\\', ':'], "_"),
                ext
            )
        };
        self.files = Some(FileDialog {
            directory_text: dir.display().to_string(),
            directory: dir,
            filename,
            action,
            error: None,
        });
    }
    fn save(&mut self) {
        if self.editor.is_some() {
            self.notice = "Finish or cancel the active feature before saving.".into();
            return;
        }
        if let Some(p) = self.path.clone() {
            match self.save_to(&p) {
                Ok(()) => {}
                Err(e) => self.last_error = Some(e),
            }
        } else {
            self.open_dialog(FileAction::Save);
        }
    }
    fn save_to(&mut self, path: &Path) -> crate::model::Result<()> {
        let path = if path.exists() {
            path.canonicalize().map_err(|e| e.to_string())?
        } else {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            parent
                .canonicalize()
                .map_err(|e| e.to_string())?
                .join(path.file_name().ok_or("Choose a file name")?)
        };
        let same = self.path.as_ref().is_some_and(|old| {
            *old == path
                || old
                    .canonicalize()
                    .ok()
                    .zip(path.canonicalize().ok())
                    .is_some_and(|(a, b)| a == b)
        });
        save_checked(&self.document, same.then_some(&self.saved), &path)?;
        self.saved = self.document.clone();
        self.path = Some(path);
        self.last_error = None;
        self.notice = "Saved".into();
        Ok(())
    }
    fn reload(&mut self) {
        if let Some(path) = self.path.clone() {
            match Document::load(&path) {
                Ok(document) => self.queue_load(document, Some(path)),
                Err(error) => self.last_error = Some(error),
            }
        }
    }
    fn exporter(&mut self, id: &str) {
        if self.editor.is_some() {
            return;
        }
        let mut scratch = self.document.clone();
        let result = (|| {
            let f = self.registry.create(id, &mut scratch)?;
            let plan = self
                .registry
                .evaluate(&f, &Numbers::new(&self.document.variables)?)?;
            plan["format"]
                .as_str()
                .map(str::to_owned)
                .ok_or("Exporter did not declare a format".to_owned())
        })();
        match result {
            Ok(fmt) => self.open_dialog(FileAction::Export(fmt)),
            Err(e) => self.last_error = Some(e),
        }
    }
    fn poll(&mut self) {
        while let Ok(response) = self.worker.receive.try_recv() {
            match response {
                Response::Built {
                    serial,
                    result,
                    source,
                } => {
                    if serial != self.serial && serial != u64::MAX {
                        continue;
                    }
                    self.busy = false;
                    if source.is_some() {
                        self.source_scene = source;
                    }
                    match result {
                        Ok(scene) => {
                            self.last_error = scene.error.clone();
                            self.scene = scene;
                            self.selected_face =
                                self.selected_face.take().and_then(|(part, key, _, _)| {
                                    self.scene
                                        .meshes
                                        .iter()
                                        .filter(|m| m.part == part)
                                        .flat_map(|m| &m.faces)
                                        .find(|f| f.key == key)
                                        .map(|f| (part, key, f.global_id, f.plane.clone()))
                                });
                            self.render_revision += 1;
                        }
                        Err(e) => {
                            self.last_error = Some(e);
                        }
                    }
                }
                Response::Exported(result) => {
                    self.busy = false;
                    match result {
                        Ok(_) => self.notice = "Exported".into(),
                        Err(e) => self.last_error = Some(e),
                    }
                }
            }
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
            self.tree_open = !self.tree_open;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.open_dialog(FileAction::Open);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::R)) {
            self.reload();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
            self.undo(false);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)) {
            self.undo(true);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.fit();
        }
        for (key, direction) in [
            (egui::Key::Num1, glam::Vec3::NEG_Y),
            (egui::Key::Num2, glam::Vec3::Y),
            (egui::Key::Num3, glam::Vec3::NEG_X),
            (egui::Key::Num4, glam::Vec3::X),
            (egui::Key::Num5, glam::Vec3::Z),
            (egui::Key::Num6, glam::Vec3::NEG_Z),
            (egui::Key::Num7, glam::Vec3::new(1., -1., 1.)),
        ] {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, key)) {
                self.camera.pivot =
                    render::model_center(&self.scene, &self.hidden).unwrap_or(self.camera.center);
                self.camera.set_view(direction);
            }
        }
        if self.is_sketching() {
            self.sketch_ui.shortcuts(ctx);
        } else if self.editor.is_none() && ctx.input(|i| i.key_pressed(egui::Key::E)) {
            self.begin("extrude");
        }
    }
    fn is_sketching(&self) -> bool {
        self.editor.as_ref().is_some_and(|e| {
            self.registry
                .plugins
                .get(&e.feature.plugin)
                .is_some_and(|p| p.manifest.editor == "sketch")
        })
    }
    fn fit(&mut self) {
        if let Some(sk) = self
            .editor
            .as_ref()
            .and_then(|e| self.scene.sketches.get(&e.feature.id))
        {
            self.camera.fit_sketch_with_rect(sk, self.viewport_rect);
        } else {
            self.camera.fit_with_rect(&self.scene, self.viewport_rect);
        }
    }

    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(17, 23, 31))
                    .inner_margin(egui::Margin::symmetric(8, 3)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("New Part Studio").clicked() {
                            self.queue_load(Document::default(), None);
                            ui.close();
                        }
                        if ui.button("Open…    Ctrl+O").clicked() {
                            self.open_dialog(FileAction::Open);
                            ui.close();
                        }
                        if ui.button("Save      Ctrl+S").clicked() {
                            self.save();
                            ui.close();
                        }
                        if ui.button("Save as…").clicked() {
                            self.open_dialog(FileAction::Save);
                            ui.close();
                        }
                        if ui
                            .add_enabled(self.path.is_some(), egui::Button::new("Reload    Ctrl+R"))
                            .clicked()
                        {
                            self.reload();
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(!self.past.is_empty(), egui::Button::new("Undo    Ctrl+Z"))
                            .clicked()
                        {
                            self.undo(false);
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                !self.future.is_empty(),
                                egui::Button::new("Redo    Ctrl+Y"),
                            )
                            .clicked()
                        {
                            self.undo(true);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Open example plate").clicked() {
                            self.queue_load(crate::model::demo(), None);
                            ui.close();
                        }
                        ui.separator();
                        let exporters: Vec<_> = self
                            .registry
                            .plugins
                            .values()
                            .filter(|p| p.manifest.category == "exporter")
                            .map(|p| (p.manifest.id.clone(), p.manifest.name.clone()))
                            .collect();
                        for (id, name) in exporters {
                            if ui
                                .add_enabled(
                                    self.editor.is_none() && !self.scene.meshes.is_empty(),
                                    egui::Button::new(name),
                                )
                                .clicked()
                            {
                                self.exporter(&id);
                                ui.close();
                            }
                        }
                    });
                    ui.add_space(8.);
                    ui.scope(|ui| {
                        // Keep the document name from pushing the controls off screen.
                        ui.set_max_width((ui.available_width() - 180.).max(40.));
                        ui.add(
                            egui::Label::new(RichText::new(&self.document.name).strong())
                                .truncate(),
                        );
                    });
                    if self.dirty() {
                        ui.label(RichText::new("●").color(ACCENT));
                    }
                    if self.busy {
                        ui.add(egui::Spinner::new().size(14.));
                    }
                    if let Some(error) = &self.last_error {
                        ui.colored_label(RED, "!").on_hover_text(error);
                    }
                    if !self.notice.is_empty() {
                        let width = ui.available_width() - 120.;
                        if width > 40. {
                            ui.scope(|ui| {
                                ui.set_max_width(width);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&self.notice).small().color(MUTED),
                                    )
                                    .truncate(),
                                );
                            });
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.menu_button("…", |ui| {
                            if ui.button("Plugins").clicked() {
                                self.plugins_open = true;
                                ui.close();
                            }
                            if ui.button("Help").clicked() {
                                self.help_open = true;
                                ui.close();
                            }
                            ui.separator();
                            ui.label("Units: mm");
                        })
                        .response
                        .on_hover_text("Options");
                        if ui.button("Variables").clicked() {
                            self.variables_name = self.document.name.clone();
                            self.variables_draft = self
                                .document
                                .variables
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            self.variables_open = true;
                        }
                    });
                });
            });
        egui::TopBottomPanel::top("tools")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(8, 3)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .small_button(if self.tree_open { "<" } else { ">" })
                        .on_hover_text("Toggle feature tree (Tab)")
                        .clicked()
                    {
                        self.tree_open = !self.tree_open;
                    }
                    if self.is_sketching() {
                        self.sketch_ui.toolbar(ui);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Cancel").clicked() {
                                self.finish_editor(false);
                            }
                            if ui
                                .add_enabled(
                                    !self.busy
                                        && self.pending_rebuild.is_none()
                                        && self.last_error.is_none(),
                                    egui::Button::new(
                                        RichText::new("Finish").color(Color32::BLACK),
                                    )
                                    .fill(ACCENT),
                                )
                                .clicked()
                            {
                                self.finish_editor(true);
                            }
                        });
                    } else {
                        let items: Vec<_> = self
                            .registry
                            .plugins
                            .values()
                            .filter(|p| p.manifest.category == "feature")
                            .map(|p| {
                                (
                                    p.manifest.id.clone(),
                                    p.manifest.name.clone(),
                                    p.manifest.description.clone(),
                                )
                            })
                            .collect();
                        let order = ["sketch", "extrude", "revolve", "fillet", "chamfer"];
                        for id in order {
                            if let Some((_, name, description)) =
                                items.iter().find(|(k, _, _)| k == id)
                                && ui
                                    .add_enabled(self.editor.is_none(), egui::Button::new(name))
                                    .on_hover_text(description)
                                    .clicked()
                            {
                                self.begin(id);
                            }
                        }
                        ui.menu_button("More", |ui| {
                            for (id, name, description) in &items {
                                if !order.contains(&id.as_str())
                                    && ui
                                        .add_enabled(self.editor.is_none(), egui::Button::new(name))
                                        .on_hover_text(description)
                                        .clicked()
                                {
                                    self.begin(id);
                                    ui.close();
                                }
                            }
                        });
                    }
                });
            });
    }

    fn tree(&mut self, ctx: &egui::Context) {
        if !self.tree_open {
            return;
        }
        egui::SidePanel::left("feature_tree")
            .default_width(188.)
            .width_range(150. ..=340.)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Features").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.document.features.is_empty() {
                            return;
                        }
                        ui.label(
                            RichText::new(format!("{} features", self.document.features.len()))
                                .small()
                                .color(MUTED),
                        );
                    });
                });
                ui.separator();
                egui::CollapsingHeader::new("Origin")
                    .default_open(self.document.features.is_empty())
                    .show(ui, |ui| {
                        for name in ["Top", "Front", "Right"] {
                            if ui.selectable_label(false, name).clicked() {
                                self.selected_face = None;
                                self.camera.align(&Plane::named(name, 0.));
                                if let Some(e) = &mut self.editor {
                                    e.feature.params["plane"] = json!(name);
                                    e.feature.params["support"] = json!("");
                                    self.rebuild();
                                }
                            }
                        }
                    });
                let mut command = None;
                let mut open_edit = None;
                let mut moved = None;
                egui::ScrollArea::vertical()
                    .id_salt("history_scroll")
                    .max_height(
                        (ui.available_height() - 65. - self.scene.meshes.len().min(4) as f32 * 26.)
                            .max(100.),
                    )
                    .show(ui, |ui| {
                        for index in 0..self.document.features.len() {
                            let f = &self.document.features[index];
                            let status = self.scene.statuses.iter().find(|s| s.id == f.id);
                            let error = status.and_then(|s| s.error.clone());
                            let rollback = self.document.rollback.is_some_and(|r| index >= r);
                            let color = if error.is_some() {
                                RED
                            } else if f.suppressed || rollback {
                                MUTED
                            } else {
                                Color32::from_rgb(214, 223, 233)
                            };
                            let prefix = if error.is_some() {
                                "! "
                            } else if f.suppressed {
                                "- "
                            } else {
                                ""
                            };
                            let response = ui.add(
                                egui::Button::new(
                                    RichText::new(format!("{prefix}{}", f.name)).color(color),
                                )
                                .selected(self.selected_feature == Some(index))
                                .frame(false)
                                .min_size(Vec2::new(ui.available_width(), 24.))
                                .truncate()
                                .sense(Sense::click_and_drag()),
                            );
                            if let Some(e) = error {
                                response.clone().on_hover_text(e);
                            } else if let Some(s) = status {
                                response
                                    .clone()
                                    .on_hover_text(format!("{} · {:.2} ms", f.id, s.millis));
                            }
                            if response.clicked() {
                                self.selected_feature = Some(index);
                                self.selected_face = None;
                            }
                            if response.double_clicked() {
                                open_edit = Some(index);
                            }
                            if response.drag_started() {
                                self.drag_feature = Some(index);
                            }
                            if response.hovered()
                                && ui.input(|i| i.pointer.any_released())
                                && let Some(from) = self.drag_feature.take()
                                && from != index
                            {
                                moved = Some((from, index));
                            }
                            response.context_menu(|ui| {
                                if ui.button("Edit").clicked() {
                                    open_edit = Some(index);
                                    ui.close();
                                }
                                if ui.button("Rename").clicked() {
                                    self.rename_feature = Some(index);
                                    self.rename_text = f.name.clone();
                                    ui.close();
                                }
                                if ui
                                    .button(if f.suppressed {
                                        "Unsuppress"
                                    } else {
                                        "Suppress"
                                    })
                                    .clicked()
                                {
                                    command = Some((index, "suppress"));
                                    ui.close();
                                }
                                if ui.button("Roll back before this").clicked() {
                                    command = Some((index, "rollback"));
                                    ui.close();
                                }
                                if ui
                                    .add_enabled(index > 0, egui::Button::new("Move earlier"))
                                    .clicked()
                                {
                                    moved = Some((index, index - 1));
                                    ui.close();
                                }
                                if ui
                                    .add_enabled(
                                        index + 1 < self.document.features.len(),
                                        egui::Button::new("Move later"),
                                    )
                                    .clicked()
                                {
                                    moved = Some((index, index + 1));
                                    ui.close();
                                }
                                ui.separator();
                                if ui
                                    .button(RichText::new("Delete feature").color(RED))
                                    .clicked()
                                {
                                    command = Some((index, "delete"));
                                    ui.close();
                                }
                            });
                        }
                    });
                if self.editor.is_none() {
                    if let Some(i) = open_edit {
                        self.edit_feature(i);
                    }
                    if let Some((i, c)) = command {
                        let before = self.document.clone();
                        match c {
                            "suppress" => {
                                self.document.features[i].suppressed =
                                    !self.document.features[i].suppressed
                            }
                            "rollback" => self.document.rollback = Some(i),
                            "delete" => {
                                self.document.features.remove(i);
                                self.document.rollback = None;
                                self.selected_feature = None;
                            }
                            _ => (),
                        }
                        self.remember(before);
                    }
                    if let Some((a, b)) = moved {
                        let before = self.document.clone();
                        let f = self.document.features.remove(a);
                        self.document.features.insert(b, f);
                        self.selected_feature = Some(b);
                        self.remember(before);
                    }
                }
                if self.document.features.is_empty() {
                    return;
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("End").clicked() {
                            self.document.rollback = None;
                            self.rebuild();
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(RichText::new("Rollback").small().color(MUTED));
                            let mut value = self
                                .document
                                .rollback
                                .unwrap_or(self.document.features.len());
                            ui.spacing_mut().slider_width =
                                (ui.available_width() - 1.).floor().max(28.);
                            if ui
                                .add_enabled(
                                    self.editor.is_none(),
                                    egui::Slider::new(&mut value, 0..=self.document.features.len())
                                        .show_value(false),
                                )
                                .changed()
                            {
                                self.document.rollback = if value == self.document.features.len() {
                                    None
                                } else {
                                    Some(value)
                                };
                                self.rebuild();
                            }
                        });
                    });
                });
                if self.scene.meshes.is_empty() {
                    return;
                }
                ui.add_space(4.);
                ui.label(RichText::new(format!("Parts ({})", self.scene.meshes.len())).strong());
                egui::ScrollArea::vertical()
                    .id_salt("parts_scroll")
                    .show(ui, |ui| {
                        for mesh in &self.scene.meshes {
                            ui.horizontal(|ui| {
                                let mut visible = !self.hidden.contains(&mesh.part);
                                if ui.checkbox(&mut visible, "").changed() {
                                    if visible {
                                        self.hidden.remove(&mesh.part);
                                    } else {
                                        self.hidden.insert(mesh.part.clone());
                                    }
                                    self.render_revision += 1;
                                }
                                if ui
                                    .add(
                                        egui::Button::new(&mesh.name)
                                            .selected(
                                                self.selected_part.as_ref() == Some(&mesh.part),
                                            )
                                            .frame(false)
                                            .truncate(),
                                    )
                                    .on_hover_text(format!(
                                        "{}\nVolume {:.3} mm\u{00b3}",
                                        mesh.name, mesh.volume
                                    ))
                                    .clicked()
                                {
                                    self.selected_part = Some(mesh.part.clone());
                                    self.selected_face = None;
                                }
                            });
                        }
                    });
            });
    }

    fn inspector(&mut self, ctx: &egui::Context) {
        if self.editor.is_none() {
            return;
        }
        egui::SidePanel::right("properties")
            .default_width(244.)
            .width_range(220. ..=380.)
            .show(ctx, |ui| {
                if let Some(mut edit) = self.editor.take() {
                    edit.sync();
                    let before_params = edit.feature.params.clone();
                    let manifest = self
                        .registry
                        .plugins
                        .get(&edit.feature.plugin)
                        .map(|p| p.manifest.clone());
                    let mut changed = false;
                    let mut apply = false;
                    let mut cancel = false;
                    ui.horizontal(|ui| {
                        ui.scope(|ui| {
                            let reserved = if edit.sketch.is_none() { 65. } else { 0. };
                            ui.set_max_width((ui.available_width() - reserved).max(40.));
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&edit.feature.name).size(18.).strong(),
                                )
                                .truncate(),
                            );
                        });
                        if edit.sketch.is_none() {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    cancel = ui.small_button("Cancel").clicked();
                                },
                            );
                        }
                    });
                    ui.separator();
                    if let Some(m) = &manifest {
                        egui::ScrollArea::vertical()
                            .id_salt("feature_properties")
                            .max_height((ui.available_height() - 70.).max(150.))
                            .show(ui, |ui| {
                                for field in &m.fields {
                                    if field.kind == "support"
                                        && edit
                                            .feature
                                            .params
                                            .get(&field.key)
                                            .and_then(Value::as_str)
                                            .is_none_or(str::is_empty)
                                    {
                                        continue;
                                    }
                                    if field.kind == "body"
                                        && edit.feature.params.get("mode").and_then(Value::as_str)
                                            == Some("New")
                                    {
                                        continue;
                                    }
                                    if field.kind != "bool" {
                                        ui.label(RichText::new(&field.label).small().color(MUTED));
                                    }
                                    let old_plane = edit.feature.params.get("plane").cloned();
                                    changed |= parameter_ui(
                                        ui,
                                        field,
                                        &mut edit.feature.params,
                                        &self.document,
                                        edit.index,
                                        &self.registry,
                                        &self.scene,
                                    );
                                    if field.key == "plane"
                                        && old_plane != edit.feature.params.get("plane").cloned()
                                    {
                                        edit.feature.params["support"] = json!("");
                                        self.align_when_ready = true;
                                    }
                                    ui.add_space(4.);
                                }
                                if m.editor == "sketch"
                                    && let Some(sketch) = &mut edit.sketch
                                {
                                    changed |= self.sketch_ui.inspector(
                                        ui,
                                        sketch,
                                        self.scene.sketches.get(&edit.feature.id),
                                        self.last_error.as_deref(),
                                    );
                                }
                                if edit.feature.plugin_hash
                                    != self
                                        .registry
                                        .plugins
                                        .get(&edit.feature.plugin)
                                        .map(|p| p.hash.as_str())
                                        .unwrap_or("")
                                {
                                    ui.colored_label(RED, "Plugin version changed.");
                                    if ui.button("Update plugin").clicked()
                                        && let Some(p) =
                                            self.registry.plugins.get(&edit.feature.plugin)
                                    {
                                        edit.feature.plugin_hash = p.hash.clone();
                                        edit.feature.plugin_version = p.manifest.version.clone();
                                        changed = true;
                                    }
                                }
                            });
                    } else {
                        ui.colored_label(RED, "Plugin missing.");
                    }
                    if edit.sketch.is_none() {
                        ui.separator();
                        if let Some(error) = &self.last_error {
                            ui.label(RichText::new(error).small().color(RED));
                        }
                        apply = ui
                            .add_enabled(
                                !self.busy
                                    && self.pending_rebuild.is_none()
                                    && self.last_error.is_none(),
                                egui::Button::new(RichText::new("Apply").color(Color32::BLACK))
                                    .fill(ACCENT)
                                    .min_size(Vec2::new(ui.available_width(), 34.)),
                            )
                            .clicked();
                    }
                    if changed {
                        edit.undo.push(before_params);
                        if edit.undo.len() > 60 {
                            edit.undo.remove(0);
                        }
                        edit.redo.clear();
                        edit.changed = true;
                        edit.sync();
                    }
                    self.editor = Some(edit);
                    if changed {
                        self.schedule(ctx);
                    }
                    if apply {
                        self.finish_editor(true);
                    }
                    if cancel {
                        self.finish_editor(false);
                    }
                }
            });
    }

    fn viewport(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(17, 23, 32)))
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                self.viewport_rect = rect;
                let sketch_id = self
                    .editor
                    .as_ref()
                    .filter(|e| e.sketch.is_some())
                    .map(|e| e.feature.id.clone())
                    .or_else(|| {
                        self.selected_feature
                            .and_then(|i| self.document.features.get(i))
                            .filter(|f| f.sketch().is_some())
                            .map(|f| f.id.clone())
                    });
                let sketch_plane = sketch_id
                    .as_ref()
                    .and_then(|id| self.scene.sketches.get(id))
                    .map(|s| s.plane.clone());

                if !self.busy && self.pending_rebuild.is_none() {
                    if self.fit_when_ready {
                        self.camera.fit_with_rect(&self.scene, rect);
                        self.fit_when_ready = false;
                    }
                    if self.align_when_ready
                        && let Some(edit) = &self.editor
                        && let Some(sk) = self.scene.sketches.get(&edit.feature.id)
                    {
                        self.camera.align(&sk.plane);
                        self.camera.fit_sketch_with_rect(sk, rect);
                        self.align_when_ready = false;
                    }
                }

                let pointer = response.hover_pos();
                let controls = crate::navigation::bounds(rect);
                let canvas_hover = pointer.is_some_and(|p| !controls.contains(p));
                let orbiting = canvas_hover
                    && response.hovered()
                    && !self.is_sketching()
                    && ui.input(|i| {
                        i.pointer.button_down(egui::PointerButton::Secondary)
                            && !i.modifiers.shift
                            && !i.modifiers.ctrl
                    });
                if orbiting && !self.mouse_orbiting {
                    let origin = ui.input(|i| i.pointer.press_origin()).or(pointer);
                    self.camera.pivot = origin
                        .and_then(|p| {
                            render::pick(&self.scene, &self.camera, p, rect, &self.hidden)
                        })
                        .map(|p| p.point)
                        .or_else(|| render::model_center(&self.scene, &self.hidden))
                        .unwrap_or(self.camera.center);
                }
                self.mouse_orbiting = orbiting;
                if response.hovered() && canvas_hover {
                    let delta = ui.input(|i| i.pointer.delta());
                    let right = ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary));
                    let middle = ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
                    if middle
                        || (right
                            && (self.is_sketching()
                                || ui.input(|i| i.modifiers.shift || i.modifiers.ctrl)))
                    {
                        self.camera.pan(delta, rect);
                    } else if right {
                        if ui.input(|i| i.modifiers.alt) {
                            self.camera.orbit_upright(delta);
                        } else {
                            self.camera.orbit(delta);
                        }
                    }
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll.abs() > 0.01 {
                        let old = self.camera.span;
                        self.camera.span = (old * (-scroll * 0.0025).exp()).clamp(0.02, 1e6);
                        if let Some(p) = pointer {
                            let scale = (old - self.camera.span) / rect.height();
                            self.camera.center += self.camera.rotation
                                * glam::Vec3::new(
                                    (p.x - rect.center().x) * scale,
                                    (rect.center().y - p.y) * scale,
                                    0.,
                                );
                        }
                    }
                }
                let projector = self.camera.projector(rect);
                let selected = if self.is_sketching() {
                    0
                } else {
                    self.selected_face.as_ref().map(|x| x.2).unwrap_or(0)
                };
                ui.painter().add(render::paint_callback(
                    rect,
                    self.renderer.clone(),
                    self.scene.clone(),
                    self.hidden.clone(),
                    self.render_revision,
                    render::View {
                        matrix: self.camera.matrix(rect),
                        selected,
                        shaded_edges: self.shaded_edges,
                    },
                ));
                if self.mouse_orbiting {
                    let pivot =
                        projector.project(self.camera.pivot.to_array().map(f64::from), rect);
                    ui.painter()
                        .circle_stroke(pivot, 4., Stroke::new(1.2_f32, ACCENT));
                }
                if self.editor.is_none()
                    && let Some(sk) = sketch_id
                        .as_ref()
                        .and_then(|id| self.scene.sketches.get(id))
                {
                    crate::sketch_ui::paint_readonly(ui, sk, &projector, rect);
                }
                if self.is_sketching() {
                    let mut changed = false;
                    let mut live = false;
                    if let Some(edit) = &mut self.editor {
                        let before = edit.feature.params.clone();
                        if let Some(sketch) = &mut edit.sketch {
                            let plane = sketch_plane.clone().unwrap_or_else(|| {
                                Plane::named(
                                    edit.feature.params["plane"].as_str().unwrap_or("Top"),
                                    0.,
                                )
                            });
                            let action = self.sketch_ui.canvas(
                                ui,
                                rect,
                                &response,
                                &self.camera,
                                sketch,
                                self.scene.sketches.get(&edit.feature.id),
                                plane,
                                &self.document.variables,
                                self.last_error.is_some(),
                            );
                            if action.changed {
                                live = action.live;
                                if action.checkpoint {
                                    edit.undo.push(before);
                                    if edit.undo.len() > 60 {
                                        edit.undo.remove(0);
                                    }
                                    edit.redo.clear();
                                }
                                edit.sync();
                                changed = true;
                            }
                        }
                    }
                    if changed {
                        if live {
                            self.rebuild();
                        } else {
                            self.schedule(ctx);
                        }
                    }
                } else if response.clicked() && canvas_hover {
                    if self.editor.is_none() {
                        self.selected_feature = None;
                    }
                    let pos = pointer.unwrap();
                    let pick_scene = self.source_scene.as_ref().unwrap_or(&self.scene);
                    let hit = render::pick(pick_scene, &self.camera, pos, rect, &self.hidden);
                    let edge_mode = self.editor.as_ref().is_some_and(|e| {
                        self.registry
                            .plugins
                            .get(&e.feature.plugin)
                            .is_some_and(|p| p.manifest.fields.iter().any(|f| f.kind == "edges"))
                    }) || ui.input(|i| i.modifiers.shift);
                    if edge_mode {
                        let mut nearest = None;
                        let mut distance = 7.;
                        for mesh in &pick_scene.meshes {
                            if self.hidden.contains(&mesh.part) {
                                continue;
                            }
                            for edge in &mesh.edges {
                                if edge.seam || edge.key.contains("untracked:") {
                                    continue;
                                }
                                for p in edge.points.windows(2) {
                                    let a = p[0].map(|x| x as f64);
                                    let b = p[1].map(|x| x as f64);
                                    let d = render::point_segment_distance(
                                        pos,
                                        projector.project(a, rect),
                                        projector.project(b, rect),
                                    );
                                    let (ray_o, ray_d) = self.camera.ray(pos, rect);
                                    let pa = glam::Vec3::from_array(p[0]);
                                    let pb = glam::Vec3::from_array(p[1]);
                                    let sa = projector.project(a, rect);
                                    let sb = projector.project(b, rect);
                                    let v = sb - sa;
                                    let t =
                                        ((pos - sa).dot(v) / v.length_sq().max(1e-8)).clamp(0., 1.);
                                    let edge_depth = (pa.lerp(pb, t) - ray_o).dot(ray_d);
                                    let visible = hit.as_ref().is_none_or(|h| {
                                        edge_depth
                                            <= h.distance + self.camera.span / rect.height() * 5.
                                    });
                                    if d < distance && visible {
                                        distance = d;
                                        nearest = Some((edge.part.clone(), edge.key.clone()));
                                    }
                                }
                            }
                        }
                        if let Some(key) = nearest {
                            if !ui.input(|i| i.modifiers.ctrl) {
                                self.selected_edges.clear();
                            }
                            if self.selected_edges.contains(&key) {
                                self.selected_edges.retain(|k| k != &key);
                            } else {
                                self.selected_edges.push(key.clone());
                            }
                            self.selected_part = Some(key.0);
                            if let Some(edit) = &mut self.editor {
                                edit.feature.params["edges"] = json!(
                                    self.selected_edges
                                        .iter()
                                        .map(|(_, k)| k)
                                        .collect::<Vec<_>>()
                                );
                                self.schedule(ctx);
                            }
                        }
                    } else if let Some(hit) = hit {
                        self.selected_part = Some(hit.part.clone());
                        self.selected_face =
                            Some((hit.part, hit.face.clone(), hit.global_face, hit.plane));
                        if let Some(edit) = &mut self.editor
                            && self
                                .registry
                                .plugins
                                .get(&edit.feature.plugin)
                                .is_some_and(|p| {
                                    p.manifest.fields.iter().any(|f| f.kind == "faces")
                                })
                        {
                            let mut keys = edit.feature.params["faces"]
                                .as_array()
                                .cloned()
                                .unwrap_or_default();
                            if !ui.input(|i| i.modifiers.ctrl) {
                                keys.clear();
                            }
                            if !keys.contains(&json!(hit.face)) {
                                keys.push(json!(hit.face));
                            }
                            edit.feature.params["faces"] = json!(keys);
                            self.schedule(ctx);
                        }
                    } else {
                        self.selected_face = None;
                        self.selected_part = None;
                        self.selected_edges.clear();
                    }
                }
                for mesh in &self.source_scene.as_ref().unwrap_or(&self.scene).meshes {
                    for edge in &mesh.edges {
                        if self
                            .selected_edges
                            .contains(&(edge.part.clone(), edge.key.clone()))
                        {
                            ui.painter().add(egui::Shape::line(
                                edge.points
                                    .iter()
                                    .map(|p| projector.project(p.map(|x| x as f64), rect))
                                    .collect(),
                                Stroke::new(2.5f32, Color32::from_rgb(255, 211, 112)),
                            ));
                        }
                    }
                }
                crate::navigation::axes(ui, rect, &self.camera);
                let model_pivot =
                    render::model_center(&self.scene, &self.hidden).unwrap_or(self.camera.center);
                if crate::navigation::show(
                    ui,
                    rect,
                    &mut self.camera,
                    &mut self.shaded_edges,
                    &mut self.space_mouse,
                    model_pivot,
                ) {
                    self.fit();
                }
            });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut dialog) = self.files.take() {
            let mut open = true;
            let mut accept = false;
            let mut cancel = false;
            let title = match &dialog.action {
                FileAction::Open => "Open Part Studio",
                FileAction::Save => "Save Part Studio",
                FileAction::Export(_) => "Export parts",
            };
            egui::Window::new(title)
                .id(egui::Id::new("file_dialog"))
                .open(&mut open)
                .collapsible(false)
                .default_width(700.)
                .min_width(700.)
                .max_width(700.)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Up").clicked()
                            && let Some(p) = dialog.directory.parent()
                        {
                            dialog.directory = p.to_path_buf();
                            dialog.directory_text = dialog.directory.display().to_string();
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut dialog.directory_text)
                                .desired_width(520.),
                        );
                        if ui.button("Go").clicked() {
                            let p = PathBuf::from(&dialog.directory_text);
                            if p.is_dir() {
                                dialog.directory = p;
                                dialog.error = None;
                            } else {
                                dialog.error = Some("Directory does not exist".into());
                            }
                        }
                    });
                    ui.separator();
                    let mut next = None;
                    egui::ScrollArea::vertical()
                        .max_height(300.)
                        .min_scrolled_height(180.)
                        .show(ui, |ui| match std::fs::read_dir(&dialog.directory) {
                            Ok(entries) => {
                                let mut entries: Vec<_> =
                                    entries.flatten().map(|e| e.path()).collect();
                                entries.sort_by_key(|p| {
                                    (
                                        !p.is_dir(),
                                        p.file_name().map(|n| n.to_string_lossy().to_lowercase()),
                                    )
                                });
                                for path in entries {
                                    let dir = path.is_dir();
                                    if !dir
                                        && matches!(dialog.action, FileAction::Open)
                                        && path.extension().is_none_or(|e| e != "cadpart")
                                    {
                                        continue;
                                    }
                                    let name =
                                        path.file_name().unwrap_or_default().to_string_lossy();
                                    let response = ui.selectable_label(
                                        !dir && dialog.filename == name,
                                        format!("{}{}", name, if dir { "/" } else { "" }),
                                    );
                                    if response.clicked() {
                                        if dir {
                                            next = Some(path.clone());
                                        } else {
                                            dialog.filename = name.into_owned();
                                        }
                                    }
                                    if response.double_clicked()
                                        && !dir
                                        && matches!(dialog.action, FileAction::Open)
                                    {
                                        accept = true;
                                    }
                                }
                            }
                            Err(e) => {
                                ui.colored_label(RED, e.to_string());
                            }
                        });
                    if let Some(p) = next {
                        dialog.directory = p;
                        dialog.directory_text = dialog.directory.display().to_string();
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("File name");
                        ui.add(
                            egui::TextEdit::singleline(&mut dialog.filename).desired_width(585.),
                        );
                    });
                    if let Some(e) = &dialog.error {
                        ui.colored_label(RED, e);
                    }
                    let exists = dialog.directory.join(&dialog.filename).is_file();
                    let label = match &dialog.action {
                        FileAction::Open => "Open",
                        FileAction::Save => {
                            if exists {
                                "Replace existing file"
                            } else {
                                "Save"
                            }
                        }
                        FileAction::Export(_) => {
                            if exists {
                                "Replace existing file"
                            } else {
                                "Export"
                            }
                        }
                    };
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !dialog.filename.trim().is_empty(),
                                egui::Button::new(label).fill(Color32::from_rgb(34, 94, 108)),
                            )
                            .clicked()
                        {
                            accept = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            if accept {
                let mut path = dialog.directory.join(dialog.filename.trim());
                if path.extension().is_none() {
                    path.set_extension(match &dialog.action {
                        FileAction::Export(f) => f.as_str(),
                        _ => "cadpart",
                    });
                }
                let result = match &dialog.action {
                    FileAction::Open => {
                        Document::load(&path).map(|d| self.queue_load(d, Some(path.clone())))
                    }
                    FileAction::Save => {
                        if self.editor.is_some() {
                            Err("Finish the active feature before saving".into())
                        } else {
                            self.save_to(&path)
                        }
                    }
                    FileAction::Export(format) => {
                        self.busy = true;
                        self.worker
                            .send
                            .send(Request::Export {
                                document: self.document.clone(),
                                registry: self.registry.clone(),
                                format: format.clone(),
                                path,
                            })
                            .map_err(|e| e.to_string())
                    }
                };
                if let Err(e) = result {
                    dialog.error = Some(e);
                } else {
                    cancel = true;
                }
            }
            if open && !cancel {
                self.files = Some(dialog);
            }
        }
        if self.variables_open {
            let mut open = true;
            let mut apply = false;
            egui::Window::new("Variables")
                .open(&mut open)
                .collapsible(false)
                .default_width(600.)
                .show(ctx, |ui| {
                    ui.label(
                        "Expressions use millimeters and degrees. Examples: width / 2, 3 * inch.",
                    );
                    ui.horizontal(|ui| {
                        ui.label("Part Studio name");
                        ui.text_edit_singleline(&mut self.variables_name);
                    });
                    let mut remove = None;
                    egui::Grid::new("variables")
                        .num_columns(3)
                        .min_col_width(150.)
                        .show(ui, |ui| {
                            ui.label("Name");
                            ui.label("Expression");
                            ui.end_row();
                            for (i, (key, value)) in self.variables_draft.iter_mut().enumerate() {
                                ui.add(egui::TextEdit::singleline(key).desired_width(130.));
                                ui.add(egui::TextEdit::singleline(value).desired_width(220.));
                                if ui.small_button("Remove").clicked() {
                                    remove = Some(i);
                                }
                                ui.end_row();
                            }
                        });
                    if let Some(i) = remove {
                        self.variables_draft.remove(i);
                    }
                    if ui.button("Add variable").clicked() {
                        self.variables_draft.push((String::new(), "10".into()));
                    }
                    let vars: BTreeMap<_, _> = self.variables_draft.iter().cloned().collect();
                    let validation = if vars.len() != self.variables_draft.len() {
                        Err("Variable names must be unique".into())
                    } else {
                        Numbers::new(&vars).map(|_| ())
                    };
                    if let Err(e) = &validation {
                        ui.colored_label(RED, e);
                    }
                    if ui
                        .add_enabled(
                            validation.is_ok() && self.editor.is_none(),
                            egui::Button::new("Apply variables"),
                        )
                        .clicked()
                    {
                        apply = true;
                    }
                    if apply {
                        let before = self.document.clone();
                        self.document.name = self.variables_name.clone();
                        self.document.variables = vars;
                        self.remember(before);
                    }
                });
            self.variables_open = open && !apply;
        }
        if self.plugins_open {
            let mut open = true;
            let mut reload = false;
            if self.plugin_catalog.is_empty() {
                self.plugin_catalog = Registry::available(&self.plugin_dir)
                    .plugins
                    .into_values()
                    .map(|p| p.manifest)
                    .collect();
            }
            let mut disabled: BTreeSet<String> = self
                .plugin_catalog
                .iter()
                .map(|m| &m.id)
                .filter(|id| !self.registry.plugins.contains_key(*id))
                .cloned()
                .collect();
            egui::Window::new("Plugins")
                .open(&mut open)
                .default_width(570.)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(self.plugin_dir.display().to_string())
                            .small()
                            .color(MUTED),
                    );
                    ui.separator();
                    for m in &self.plugin_catalog {
                        let id = &m.id;
                        let mut enabled = !disabled.contains(id);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut enabled, &m.name).changed() {
                                if enabled {
                                    disabled.remove(id);
                                } else {
                                    disabled.insert(id.clone());
                                }
                                reload = true;
                            }
                            ui.label(RichText::new(&m.version).small().color(MUTED));
                        });
                    }

                    if ui.button("Reload").clicked() {
                        reload = true;
                    }
                    for e in &self.registry.errors {
                        ui.colored_label(RED, e);
                    }
                });
            self.plugins_open = open;
            if reload {
                let result = std::fs::create_dir_all(&self.plugin_dir).and_then(|()| {
                    std::fs::write(
                        self.plugin_dir.join("disabled.txt"),
                        disabled.into_iter().collect::<Vec<_>>().join("\n"),
                    )
                });
                match result {
                    Ok(()) => {
                        self.registry = Arc::new(Registry::load(&self.plugin_dir));
                        self.plugin_catalog.clear();
                        self.rebuild();
                    }
                    Err(e) => self.last_error = Some(e.to_string()),
                }
            }
        }
        if self.help_open {
            egui::Window::new("Help").open(&mut self.help_open).default_width(520.).show(ctx, |ui| {
                ui.label("Sketch: choose a plane, draw lines, rectangles or circles. Press D, select geometry, place the dimension, enter a value. Double-click a dimension to edit. Ctrl-click selects pairs for dimensions and constraints.");
                ui.separator();
                ui.label("Finish the sketch, then Extrude. Select a supported planar face for the next sketch. Use Remove for cuts.");
                ui.label("Double-click history to edit. Right-click to rename, reorder, suppress or delete. Rollback previews earlier history.");
                ui.separator();
                ui.label("Right drag: orbit. Middle drag or Shift + right drag: pan. Scroll: zoom. F: fit. E: extrude. Ctrl+Z / Ctrl+Y: undo / redo. Ctrl+S: save.");
                ui.label("Shift-click solid edges for fillet or chamfer. Ctrl-click adds selections. Shell removes selected faces. Pattern and mirror create separate parts.");
                ui.separator();
                ui.label("Save as .cadpart. STEP exports boundary geometry; STL exports a mesh in millimeters.");
                ui.label(RichText::new("Arcs, splines, trim, assemblies and FeatureScript are not implemented.").small().color(MUTED));
            });
        }
        if let Some(index) = self.rename_feature {
            let mut done = false;
            egui::Window::new("Rename feature")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.text_edit_singleline(&mut self.rename_text);
                    ui.horizontal(|ui| {
                        if ui.button("Rename").clicked() {
                            if index < self.document.features.len() {
                                let before = self.document.clone();
                                self.document.features[index].name = self.rename_text.clone();
                                self.remember(before);
                            }
                            done = true;
                        }
                        if ui.button("Cancel").clicked() {
                            done = true;
                        }
                    });
                });
            if done {
                self.rename_feature = None;
            }
        }
        if self.pending_load.is_some() {
            egui::Window::new("Unsaved changes")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(
                        "Opening another Part Studio will discard the current unsaved changes.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Discard and open").clicked()
                            && let Some((d, p)) = self.pending_load.take()
                        {
                            self.load(d, p);
                        }
                        if ui.button("Keep editing").clicked() {
                            self.pending_load = None;
                        }
                    });
                });
        }
        if self.close_pending {
            egui::Window::new("Close CAD?").collapsible(false).show(ctx,|ui|{ui.label("There are unsaved changes. Save your Part Studio before closing to keep them.");ui.horizontal(|ui|{if ui.button("Keep editing").clicked(){self.close_pending=false;}
if ui.button("Discard and close").clicked(){self.allow_close=true;ctx.send_viewport_cmd(egui::ViewportCommand::Close);}});});
        }
    }
}

impl eframe::App for CadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_count += 1;
        // Present again after the native window becomes visible. Some Windows
        // OpenGL drivers keep the pre-show buffer until the next swap.
        if self.frame_count <= 3 {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        self.poll();
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && (self.dirty() || self.editor.is_some())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_pending = true;
        }
        if let Some(deadline) = self.pending_rebuild {
            let now = Instant::now();
            if now >= deadline {
                self.rebuild();
            } else {
                ctx.request_repaint_after(deadline - now);
            }
        }
        let sketching = self.is_sketching();
        let blocked = self.files.is_some()
            || self.variables_open
            || self.plugins_open
            || self.help_open
            || ctx.wants_keyboard_input();
        if self
            .space_mouse
            .update(ctx, &mut self.camera, sketching, blocked, |camera| {
                render::automatic_pivot(&self.scene, camera, self.viewport_rect, &self.hidden)
            })
        {
            self.fit();
        }
        self.shortcuts(ctx);
        self.header(ctx);
        self.tree(ctx);
        self.inspector(ctx);
        self.viewport(ctx);
        self.dialogs(ctx);
    }
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Some(gl) = gl
            && let Ok(renderer) = self.renderer.lock()
        {
            renderer.destroy(gl);
        }
    }
}

fn parameter_ui(
    ui: &mut egui::Ui,
    field: &Field,
    params: &mut Value,
    document: &Document,
    index: usize,
    registry: &Registry,
    scene: &Scene,
) -> bool {
    let value = params
        .as_object_mut()
        .unwrap()
        .entry(field.key.clone())
        .or_insert_with(|| field.default.clone());
    match field.kind.as_str() {
        "bool" => {
            let mut b = value.as_bool().unwrap_or(false);
            let changed = ui.checkbox(&mut b, &field.label).changed();
            if changed {
                *value = json!(b);
            }
            changed
        }
        "choice" | "sketch" | "body" => {
            let mut selected = value.as_str().unwrap_or_default().to_owned();
            let old = selected.clone();
            let options: Vec<(String, String)> = match field.kind.as_str() {
                "choice" => field
                    .options
                    .iter()
                    .map(|x| (x.clone(), x.clone()))
                    .collect(),
                "sketch" => document.features[..index.min(document.features.len())]
                    .iter()
                    .filter(|f| {
                        !f.suppressed
                            && registry
                                .plugins
                                .get(&f.plugin)
                                .is_some_and(|p| p.manifest.editor == "sketch")
                    })
                    .map(|f| (f.id.clone(), f.name.clone()))
                    .collect(),
                _ => std::iter::once((String::new(), "Last part".into()))
                    .chain(
                        scene
                            .meshes
                            .iter()
                            .map(|p| (p.part.clone(), p.name.clone())),
                    )
                    .collect(),
            };
            let label = options
                .iter()
                .find(|(key, _)| key == &selected)
                .map(|(_, name)| name.as_str())
                .unwrap_or(if selected.is_empty() {
                    "Choose…"
                } else {
                    &selected
                });
            egui::ComboBox::from_id_salt(&field.key)
                .selected_text(label)
                .width(ui.available_width() - 10.)
                .truncate()
                .show_ui(ui, |ui| {
                    for (key, name) in &options {
                        ui.selectable_value(&mut selected, key.clone(), name);
                    }
                });
            if old != selected {
                *value = json!(selected);
                true
            } else {
                false
            }
        }
        "sketches" => {
            let options: Vec<_> = document.features[..index.min(document.features.len())]
                .iter()
                .filter(|f| {
                    !f.suppressed
                        && registry
                            .plugins
                            .get(&f.plugin)
                            .is_some_and(|p| p.manifest.editor == "sketch")
                })
                .map(|f| (f.id.clone(), f.name.clone()))
                .collect();
            let mut selected: Vec<String> = value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            let old = selected.clone();
            let mut remove = None;
            let mut moved = None;
            for row in 0..selected.len() {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("×")
                            .on_hover_text("Remove profile")
                            .clicked()
                        {
                            remove = Some(row);
                        }
                        if ui
                            .add_enabled(row + 1 < selected.len(), egui::Button::new("v").small())
                            .on_hover_text("Move profile down")
                            .clicked()
                        {
                            moved = Some((row, row + 1));
                        }
                        if ui
                            .add_enabled(row > 0, egui::Button::new("^").small())
                            .on_hover_text("Move profile up")
                            .clicked()
                        {
                            moved = Some((row, row - 1));
                        }
                        let label = options
                            .iter()
                            .find(|(key, _)| key == &selected[row])
                            .map(|(_, name)| name.clone())
                            .unwrap_or_else(|| selected[row].clone());
                        egui::ComboBox::from_id_salt((&field.key, row))
                            .selected_text(label)
                            .width((ui.available_width() - 10.).max(30.))
                            .truncate()
                            .show_ui(ui, |ui| {
                                for (key, name) in &options {
                                    if selected[row] == *key || !selected.contains(key) {
                                        ui.selectable_value(&mut selected[row], key.clone(), name);
                                    }
                                }
                            });
                    });
                });
            }
            if let Some(row) = remove {
                selected.remove(row);
            } else if let Some((from, to)) = moved {
                selected.swap(from, to);
            }
            let can_add =
                selected.len() < 32 && options.iter().any(|(key, _)| !selected.contains(key));
            ui.add_enabled_ui(can_add, |ui| {
                ui.menu_button("Add profile", |ui| {
                    for (key, name) in &options {
                        if !selected.contains(key) && ui.button(name).clicked() {
                            selected.push(key.clone());
                            ui.close();
                        }
                    }
                });
            });
            if selected != old {
                *value = json!(selected);
                true
            } else {
                false
            }
        }
        "edges" | "faces" => {
            let count = value.as_array().map_or(0, Vec::len);
            ui.label(format!("{count} selected"));
            if ui.small_button("Clear").clicked() {
                *value = json!([]);
                true
            } else if field.kind == "edges" && ui.small_button("All edges").clicked() {
                *value = json!(["@all"]);
                true
            } else {
                false
            }
        }
        "support" => {
            let s = value.as_str().unwrap_or_default();
            if s.is_empty() {
                ui.label(RichText::new("Origin plane").color(MUTED));
                false
            } else {
                let feature_name = s
                    .split(':')
                    .next()
                    .and_then(|id| document.features.iter().find(|f| f.id == id))
                    .map(|f| f.name.as_str())
                    .unwrap_or("Face");
                ui.label(RichText::new(feature_name).small().color(ACCENT));
                if ui.small_button("Detach").clicked() {
                    *value = json!("");
                    true
                } else {
                    false
                }
            }
        }
        _ => {
            let mut text = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            let changed = ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(ui.available_width()))
                .changed();
            if changed {
                *value = json!(text);
            }
            changed
        }
    }
}

#[cfg(test)]
mod save_tests {
    use super::*;
    #[test]
    fn desktop_save_preserves_an_external_edit_and_honors_the_shared_lock() {
        let dir = std::path::PathBuf::from("output")
            .join(format!("gui-save-guard-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shared.cadpart");
        let base = Document::default();
        base.save(&path).unwrap();
        let mut human = base.clone();
        human.name = "Human edit".into();
        let mut agent = base.clone();
        agent.name = "Agent edit".into();
        agent.save(&path).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(
            save_checked(&human, Some(&base), &path)
                .unwrap_err()
                .contains("changed on disk")
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        {
            let _lock = crate::file_lock::FileLock::try_acquire(&path).unwrap();
            assert!(
                save_checked(&human, Some(&agent), &path)
                    .unwrap_err()
                    .contains("busy")
            );
        }
        save_checked(&human, Some(&agent), &path).unwrap();
        assert_eq!(Document::load(&path).unwrap(), human);
    }

    #[test]
    fn ordered_profile_controls_keep_the_inspector_compact() {
        let registry = Registry::builtin().unwrap();
        let mut document = Document::default();
        for _ in 0..3 {
            let mut feature = registry.create("sketch", &mut document).unwrap();
            feature.name = format!("A long profile name that should truncate {}", feature.id);
            document.features.push(feature);
        }
        let field = Field {
            key: "profiles".into(),
            label: "Profiles".into(),
            kind: "sketches".into(),
            default: json!([]),
            options: vec![],
        };
        for width in [220., 244.] {
            let ctx = egui::Context::default();
            let mut params = json!({"profiles":["f1", "f2"]});
            for _ in 0..5 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1024., 900.),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        let panel = egui::SidePanel::right("test_profiles")
                            .default_width(width)
                            .show(ctx, |ui| {
                                assert!(!parameter_ui(
                                    ui,
                                    &field,
                                    &mut params,
                                    &document,
                                    2,
                                    &registry,
                                    &Scene::default()
                                ));
                            });
                        assert!(
                            (panel.response.rect.width() - width).abs() <= 1.,
                            "Profile controls widened the inspector"
                        );
                    },
                );
            }
            assert_eq!(params["profiles"], json!(["f1", "f2"]));
        }
    }
}
