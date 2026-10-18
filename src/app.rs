use crate::{
    engine::{Runtime, Scene},
    model::{Document, Feature, Plane, Sketch, reference_matches},
    plugins::{Field, Manifest, Numbers, Registry},
    render::{self, Camera, Renderer},
    selection::{Filter, Selection, Target},
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
mod assembly_workspace;
mod feature_inputs;
mod planes;

fn rgb(value: [u8; 3]) -> Color32 { Color32::from_rgb(value[0], value[1], value[2]) }

const ACCENT: Color32 = Color32::from_rgb(112, 184, 190);
const MUTED: Color32 = Color32::from_rgb(143, 155, 173);
const RED: Color32 = Color32::from_rgb(243, 125, 133);
const PANEL: Color32 = Color32::from_rgb(12, 12, 15);

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

enum SearchAction {
    Feature(String), Export(String), SketchTool(Tool), History(usize), Part(String),
    Settings, Variables, Plugins, Help, Save, Open, Fit, Planes,
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
#[derive(Default)]
struct WorkState {
    build: Option<u64>,
    exports: usize,
}
impl WorkState {
    fn busy(&self) -> bool { self.build.is_some() || self.exports > 0 }
    fn preview_is_current(&self, scheduled: bool) -> bool { self.build.is_none() && !scheduled }
    fn finish_build(&mut self, serial: u64) -> bool {
        if self.build != Some(serial) { return false; }
        self.build = None;
        true
    }
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
                let build_serial=match &job {Request::Build{serial,..}=>Some(*serial),Request::Export{..}=>None};
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
                match response{Ok(response)=>{let _=out_tx.send(response);},Err(_)=>{runtime=Runtime::shared(runtime.registry.clone());let error="Geometry operation panicked. The worker was reset; your editable document is preserved.".to_owned();let response=match build_serial{Some(serial)=>Response::Built{serial,result:Err(error),source:None},None=>Response::Exported(Err(error))};let _=out_tx.send(response);}}
                ctx.request_repaint();
            }
        }).expect("geometry worker starts");
        Self {
            send: tx,
            receive: out_rx,
        }
    }
}

// The editor borrows a workspace view. Preview rebuilds never own this state.
struct EditWorkspace {
    camera: Camera,
    automatic_camera: Option<Camera>,
    scene: Arc<Scene>,
    hidden: BTreeSet<String>,
    hidden_sketches: BTreeSet<String>,
    selected_plane: Option<String>,
    selected_feature: Option<usize>,
    selection: Vec<Target>,
    selection_filter: Filter,
    show_planes: bool,
    show_sketches: bool,
}
impl EditWorkspace {
    fn capture(app: &CadApp) -> Self {
        Self {
            camera: app.camera.clone(), automatic_camera: None, scene: app.scene.clone(),
            hidden: app.hidden.clone(), hidden_sketches: app.hidden_sketches.clone(),
            selected_plane: app.selected_plane.clone(), selected_feature: app.selected_feature,
            selection: app.selection.items().to_vec(), selection_filter: app.selection_filter,
            show_planes: app.show_planes, show_sketches: app.show_sketches,
        }
    }
    fn align_camera(&mut self, camera: &mut Camera, sketch: &crate::sketch::ResolvedSketch, rect: Rect) {
        if self.automatic_camera.as_ref().is_none_or(|automatic| automatic != camera) {
            self.camera = camera.clone();
        }
        camera.align(&sketch.plane);
        camera.fit_sketch_with_rect(sketch, rect);
        self.automatic_camera = Some(camera.clone());
    }
    fn restore_camera(&self, camera: &mut Camera) {
        if self.automatic_camera.as_ref() == Some(camera) {
            *camera = self.camera.clone();
        }
    }
    fn restore(self, app: &mut CadApp) {
        self.restore_camera(&mut app.camera);
        app.scene = self.scene;
        app.last_error = app.scene.error.clone();
        app.hidden = self.hidden;
        app.hidden_sketches = self.hidden_sketches;
        app.selected_plane = self.selected_plane;
        app.selected_feature = self.selected_feature;
        app.selection.clear();
        for item in self.selection { app.selection.add(item); }
        app.selection_filter = self.selection_filter;
        app.show_planes = self.show_planes;
        app.show_sketches = self.show_sketches;
        app.fit_when_ready = false;
        app.align_when_ready = false;
        app.choosing_plane = false;
        app.mouse_orbiting = false;
        app.selection_box = None;
        app.select_other = None;
        app.render_revision += 1;
    }
}
enum EditRequest {
    New(String),
    Existing(String),
}
struct Editor {
    workspace: EditWorkspace,
    index: usize,
    is_new: bool,
    feature: Feature,
    sketch: Option<Sketch>,
    changed: bool,
    input_field: Option<String>,
    undo: Vec<Value>,
    redo: Vec<Value>,
}
impl Editor {
    fn sync(&mut self) {
        if let Some(sk) = &self.sketch {
            self.feature.set_sketch(sk);
        }
    }
    fn apply_to(&self, document: &mut Document) {
        let mut feature = self.feature.clone();
        if let Some(sketch) = &self.sketch { feature.set_sketch(sketch); }
        if self.is_new {
            document.allocate_id();
            document.features.insert(self.index, feature);
            if let Some(end) = &mut document.rollback { *end += 1; }
        } else { document.features[self.index] = feature; }
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
impl FileDialog {
    fn target_path(&self) -> PathBuf {
        let mut path = self.directory.join(self.filename.trim());
        if path.extension().is_none() {
            path.set_extension(match &self.action {
                FileAction::Export(format) => format.as_str(),
                _ => "cadpart",
            });
        }
        path
    }
}

pub struct CadApp {
    assembly_workspace: Option<crate::assembly_ui::AssemblyUi>,
    assembly_mode: bool,
    assembly_camera: Camera,
    assembly_context: Option<assembly_workspace::ContextEdit>,
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
    edit_highlight: Option<(u64, String, Vec<usize>)>,
    inspector_rect: Rect,
    serial: u64,
    render_revision: u64,
    work: WorkState,
    last_error: Option<String>,
    notice: String,
    editor: Option<Editor>,
    pending_edit: Option<EditRequest>,
    selected_feature: Option<usize>,
    selection: Selection,
    selection_filter: Filter,
    selection_box: Option<egui::Pos2>,
    context_position: Option<egui::Pos2>,
    select_other: Option<(egui::Pos2, Vec<Target>, usize)>,
    shortcut_menu: Option<egui::Pos2>,
    shortcut_search: String,
    shortcut_focus: bool,
    search_index: usize,
    settings_open: bool,
    settings_error: Option<String>,
    settings: crate::settings::Settings,
    settings_saved: crate::settings::Settings,
    pending_accept: Option<bool>,
    selected_plane: Option<String>,
    choosing_plane: bool,
    show_sketches: bool,
    show_planes: bool,
    sketch_ui: SketchUi,
    camera: Camera,
    mouse_orbiting: bool,
    tree_open: bool,
    space_mouse: crate::spacemouse::SpaceMouse,
    shaded_edges: bool,
    renderer: Arc<Mutex<Renderer>>,
    hidden: BTreeSet<String>,
    hidden_sketches: BTreeSet<String>,
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
        let settings_result = crate::settings::Settings::load();
        let settings_error = settings_result.as_ref().err().cloned();
        let settings = settings_result.unwrap_or_default();
        let ctx = &cc.egui_ctx;
        space_mouse.initialize(cc);
        ctx.set_theme(egui::Theme::Dark);
        let mut style = (*ctx.style()).clone();
        style.visuals.panel_fill = PANEL;
        style.visuals.window_fill = Color32::from_rgb(19, 18, 23);
        style.visuals.extreme_bg_color = Color32::from_rgb(5, 5, 7);
        style.visuals.faint_bg_color = Color32::from_rgb(23, 22, 28);
        style.visuals.selection.bg_fill = Color32::from_rgb(34, 55, 57);
        style.visuals.selection.stroke = Stroke::new(1.0f32, rgb(settings.appearance.turquoise));
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(39, 61, 63);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(47, 35, 61);
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(28, 27, 33);
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
            assembly_workspace: None,
            assembly_mode: false,
            assembly_camera: Camera::default(),
            assembly_context: None,
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
            edit_highlight: None,
            inspector_rect: Rect::NOTHING,
            serial: 0,
            render_revision: 0,
            work: WorkState::default(),
            last_error: None,
            notice: String::new(),
            editor: None,
            pending_edit: None,
            selected_feature: None,
            selection: Selection::default(),
            selection_filter: Filter::All,
            selection_box: None,
            context_position: None,
            select_other: None,
            shortcut_menu: None,
            shortcut_search: String::new(),
            shortcut_focus: false,
            search_index: 0,
            settings_open: settings_error.is_some(),
            settings_error,
            settings: settings.clone(),
            settings_saved: settings,
            pending_accept: None,
            selected_plane: None,
            choosing_plane: false,
            show_sketches: false,
            show_planes: true,
            sketch_ui: SketchUi::default(),
            camera: Camera::default(),
            mouse_orbiting: false,
            tree_open: true,
            space_mouse,
            shaded_edges: true,
            renderer,
            hidden: BTreeSet::new(),
            hidden_sketches: BTreeSet::new(),
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
        app.apply_settings(ctx);
        app.rebuild();
        app
    }
    fn dirty(&self) -> bool {
        self.document != self.saved
    }
    fn missing_input(&self) -> Option<String> {
        if self.choosing_plane { return Some("Select a plane or planar face".into()); }
        self.editor.as_ref().and_then(|e| feature_inputs::missing(e, &self.registry))
    }
    fn preview_document(&self) -> Document {
        let mut d = self.document.clone();
        if let Some(edit) = &self.editor {
            edit.apply_to(&mut d);
            if !edit.is_new && let Some(feature) = d.features.get_mut(edit.index)
                && let Some(plugin) = self.registry.plugins.get(&feature.plugin) { plugin.adopt_inputs(feature); }
            d.rollback = Some(edit.index + usize::from(self.missing_input().is_none()));
        }
        d
    }
    fn rebuild(&mut self) {
        self.serial += 1;
        self.work.build = Some(self.serial);
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
                                    .any(|f| feature_inputs::selectable(&f.kind))
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
    fn edited_faces(&mut self) -> Vec<usize> {
        let Some(edit) = &self.editor else { self.edit_highlight = None; return vec![]; };
        if self.edit_highlight.as_ref().is_none_or(|(revision, id, _)| {
            *revision != self.render_revision || id != &edit.feature.id
        }) {
            let faces = render::edited_face_ids(&self.scene, self.source_scene.as_deref(), &edit.feature.id);
            self.edit_highlight = Some((self.render_revision, edit.feature.id.clone(), faces));
        }
        self.edit_highlight.as_ref().unwrap().2.clone()
    }
    fn paint_edit_inputs(&self, ui: &egui::Ui, rect: Rect, hover: Option<&Target>) {
        if self.is_sketching() { return; }
        let scene = self.source_scene.as_ref().unwrap_or(&self.scene);
        let projector = self.camera.projector(rect);
        for (id, sketch) in self.region_sketches(scene) {
            let whole_selected = self.selection.items().iter().any(|t|matches!(t,Target::Feature(key) if key == id));
            let whole_hovered = matches!(hover,Some(Target::Feature(key)) if key == id);
            for entity in &sketch.entities {
                for edge in 0..if entity.kind == "rectangle" {4} else {1} {
                let is_entity = |t: &Target| matches!(t,Target::SketchEntity {sketch,entity:entity_id,edge:side} if sketch == id && *entity_id == entity.id && *side == edge);
                let selected = whole_selected || self.selection.items().iter().any(is_entity);
                let hovered = whole_hovered || hover.is_some_and(is_entity);
                if !selected && !hovered { continue; }
                let color = if selected { rgb(self.settings.appearance.turquoise) } else { rgb(self.settings.appearance.purple) };
                let width = if selected {3.2_f32} else {2.5_f32};
                let entity_points = entity.points();
                let points = if entity.kind == "rectangle" {&entity_points[edge..edge+2]} else {entity_points.as_slice()};
                let points: Vec<_> = points.iter().map(|p|projector.project(sketch.plane.at(*p),rect)).collect();
                for stroke in [Stroke::new(width+2.,Color32::BLACK), Stroke::new(width,color)] {
                    if entity.construction {
                        for segment in points.windows(2) {
                            if let Some((lo,hi)) = render::clip_segment(segment[0],segment[1],rect) {
                                let a = segment[0].lerp(segment[1],lo as f32);
                                let b = segment[0].lerp(segment[1],hi as f32);
                                ui.painter().extend(egui::Shape::dashed_line(&[a,b],stroke,6.,4.));
                            }
                        }
                    } else { ui.painter().add(egui::Shape::line(points.clone(),stroke)); }
                }
                }
            }
        }
    }

    fn active_selection_filter(&self) -> Filter {
        if let Some(edit) = &self.editor {
            if feature_inputs::filters(edit,&self.registry).contains(&self.selection_filter) && self.selection_filter != Filter::All {
                return self.selection_filter;
            }
            return feature_inputs::filter(edit,&self.registry);
        }
        self.selection_filter
    }

    fn face_ids(&self, scene: &Scene, targets: &[Target]) -> Vec<usize> {
        scene
            .meshes
            .iter()
            .flat_map(|m| {
                m.faces
                    .iter()
                    .filter(|f| {
                        targets.iter().any(|t| match t {
                            Target::Part(part) => part == &m.part,
                            Target::Face { part, key } => part == &m.part && reference_matches(key, &f.key),
                            _ => false,
                        })
                    })
                    .map(|f| f.global_id)
            })
            .collect()
    }
    fn region_sketches<'a>(
        &self,
        scene: &'a Scene,
    ) -> Vec<(&'a str, &'a crate::sketch::ResolvedSketch)> {
        let input = self
            .editor
            .as_ref()
            .and_then(|e| e.feature.params.get("sketch"))
            .and_then(Value::as_str);
        let references = self.editor.as_ref().map(|e| feature_inputs::exposed_sketches(e, &self.document, &self.registry)).unwrap_or_default();
        let axis = self.editor.as_ref().is_some_and(|e| feature_inputs::axis_input(e, &self.registry));
        if axis {
            return scene.sketches.iter().filter(|(id, _)| !self.hidden_sketches.contains(*id)).map(|(id, sk)| (id.as_str(), sk)).collect();
        }
        scene
            .sketches
            .iter()
            .filter(|(id, _)| {
                (!self.hidden_sketches.contains(*id) || input == Some(id.as_str()) || references.contains(&id.as_str()))
                    && (self.show_sketches
                        || input == Some(id.as_str())
                        || references.contains(&id.as_str())
                        || self
                            .selected_feature
                            .and_then(|i| self.document.features.get(i))
                            .is_some_and(|f| &f.id == *id)
                        || self
                            .selection
                            .items()
                            .iter()
                            .any(|t| matches!(t,Target::Feature(key) if key==*id)))
            })
            .map(|(id, sk)| (id.as_str(), sk))
            .collect()
    }
    fn hovered_sketch(&self, pos: egui::Pos2) -> Option<String> {
        let scene = self.source_scene.as_ref().unwrap_or(&self.scene);
        let overlay = self.editor.as_ref().is_some_and(|e| feature_inputs::sketch_input(e, &self.registry));
        feature_inputs::sketch_candidate(scene, &self.camera, pos, self.viewport_rect,
            &self.hidden, &self.region_sketches(scene), overlay)
    }
    fn selection_candidates(
        &self,
        scene: &Scene,
        pos: egui::Pos2,
        rect: Rect,
        filter: Filter,
        through: bool,
    ) -> Vec<Target> {
        if self.editor.as_ref().is_some_and(|e| feature_inputs::axis_input(e, &self.registry)) {
            return feature_inputs::axis_candidates(scene, &self.camera, pos, rect, &self.hidden, &self.region_sketches(scene), through);
        }
        if self.editor.as_ref().is_some_and(|e| feature_inputs::sketch_input(e, &self.registry)) {
            let mut result = vec![];
            if let Some(id) = self.hovered_sketch(pos) { result.push(Target::Feature(id)); }
            for target in feature_inputs::profile_candidates(scene, &self.camera, (pos,rect,Filter::Regions,through), &self.hidden, &self.region_sketches(scene)) {
                if let Target::Region {sketch,..} = target {
                    let target = Target::Feature(sketch);
                    if !result.contains(&target) { result.push(target); }
                }
            }
            return result;
        }
        if self.editor.as_ref().is_some_and(|e| feature_inputs::profile_input(e, &self.registry)) {
            return feature_inputs::profile_candidates(scene, &self.camera, (pos, rect, filter, through), &self.hidden, &self.region_sketches(scene));
        }
        let faces = render::picks(scene, &self.camera, pos, rect, &self.hidden);
        if self.plane_input_active() {
            let mut targets: Vec<_> = self.plane_hits(pos, rect).into_iter().map(Target::Plane).collect();
            targets.extend(faces.into_iter().filter(|f| f.plane.is_some()).map(|f| Target::Face {part:f.part,key:f.face}));
            return targets;
        }
        let depth = faces.first().map(|f| f.distance);
        let mut result = vec![];
        if matches!(filter, Filter::All | Filter::Edges) {
            for e in render::pick_edges(scene, &self.camera, pos, rect, &self.hidden) {
                if through
                    || depth.is_none_or(|d| e.distance <= d + self.camera.span / rect.height() * 5.)
                {
                    result.push(Target::Edge {
                        part: e.part,
                        key: e.key,
                    });
                }
            }
        }
        if matches!(filter, Filter::All | Filter::Regions) {
            let (origin, direction) = self.camera.ray(pos, rect);
            let mut regions = vec![];
            for (id, sketch) in self.region_sketches(scene) {
                let Some(point) = self.camera.plane_point(pos, rect, &sketch.plane) else {
                    continue;
                };
                let world = glam::Vec3::from_array(sketch.plane.at(point).map(|v| v as f32));
                let distance = (world - origin).dot(direction);
                let editing = self.editor.as_ref().is_some_and(|e| {
                    e.feature.params.get("sketch").and_then(Value::as_str) == Some(id)
                        || feature_inputs::referenced_sketches(&e.feature, &self.registry).contains(&id)
                });
                if distance < 0.
                    || (!through
                        && !editing
                        && depth
                            .is_some_and(|d| distance > d + self.camera.span / rect.height() * 2.))
                {
                    continue;
                }
                if let Some(items) = scene.regions.get(id) {
                    for region in items.iter().filter(|r| r.contains(point)) {
                        regions.push((
                            distance,
                            Target::Region {
                                sketch: id.to_owned(),
                                key: region.key.clone(),
                            },
                        ));
                    }
                }
            }
            regions.sort_by(|a, b| a.0.total_cmp(&b.0));
            result.extend(regions.into_iter().map(|(_, t)| t));
        }
        if matches!(filter, Filter::All | Filter::Faces | Filter::Parts) {
            for face in faces.into_iter().take(if through { usize::MAX } else { 1 }) {
                let t = if filter == Filter::Parts {
                    Target::Part(face.part)
                } else {
                    Target::Face {
                        part: face.part,
                        key: face.face,
                    }
                };
                if !result.contains(&t) {
                    result.push(t);
                }
            }
        }
        if matches!(filter, Filter::All) && (through || depth.is_none()) {
            result.extend(
                self.plane_hits(pos, rect)
                    .into_iter()
                    .map(Target::Plane),
            );
        }
        result
    }
    fn toggle_target(&mut self, target: Target, ctx: &egui::Context) {
        ctx.request_repaint();
        self.selection.toggle(target);
        self.selected_plane = self.selection.items().iter().rev().find_map(|t| {
            if let Target::Plane(name) = t {
                Some(name.clone())
            } else {
                None
            }
        });
        self.sync_selection_to_editor(ctx);
    }
    fn clear_selection(&mut self, ctx: &egui::Context) {
        ctx.request_repaint();
        self.selection.clear();
        self.selected_feature = None;
        self.sketch_ui.clear_selection();
        self.selected_plane = None;
        self.select_other = None;
        self.sync_selection_to_editor(ctx);
    }
    fn sync_selection_to_editor(&mut self, ctx: &egui::Context) {
        self.route_feature_selection(ctx);
    }
    fn sync_selection_from_editor(&mut self) {
        self.restore_feature_selection();
    }
    fn box_regions(
        &self,
        scene: &Scene,
        viewport: Rect,
        area: Rect,
        crossing: bool,
    ) -> Vec<Target> {
        let mut out = vec![];
        for (id, sketch) in self.region_sketches(scene) {
            if let Some(regions) = scene.regions.get(id) {
                for region in regions {
                    let points: Vec<_> = region
                        .outer
                        .iter()
                        .map(|p| self.camera.project(sketch.plane.at(*p), viewport))
                        .collect();
                    let hit = if crossing {
                        region.triangles.iter().any(|tri| {
                            render::triangle_crosses_rect(
                                tri.map(|p| self.camera.project(sketch.plane.at(p), viewport)),
                                area,
                            )
                        })
                    } else {
                        !points.is_empty() && points.iter().all(|p| area.contains(*p))
                    };
                    if hit {
                        out.push(Target::Region {
                            sketch: id.into(),
                            key: region.key.clone(),
                        });
                    }
                }
            }
        }
        out
    }
    fn paint_regions(&self, ui: &egui::Ui, scene: &Scene, rect: Rect, hover: Option<&Target>) {
        for (id, sketch) in self.region_sketches(scene) {
            if let Some(regions) = scene.regions.get(id) {
                for region in regions {
                    let target = Target::Region {
                        sketch: id.into(),
                        key: region.key.clone(),
                    };
                    let selected = self.selection.contains(&target)
                        || self.selection.contains(&Target::Feature(id.into()));
                    let hovered = hover == Some(&target);
                    if !selected && !hovered {
                        continue;
                    }
                    let color = if selected { rgb(self.settings.appearance.turquoise) } else { rgb(self.settings.appearance.purple) };
                    let mut fill = egui::epaint::Mesh::default();
                    for tri in &region.triangles {
                        let offset = fill.vertices.len() as u32;
                        for p in tri {
                            fill.colored_vertex(
                                self.camera.project(sketch.plane.at(*p), rect),
                                color.gamma_multiply(if selected { 0.24 } else { 0.15 }),
                            );
                        }
                        fill.add_triangle(offset, offset + 1, offset + 2);
                    }
                    ui.painter().add(egui::Shape::mesh(fill));
                    // Boundary strokes retain holes, concavity and exact region identity.
                    for contour in std::iter::once(&region.outer).chain(region.holes.iter()) {
                        let mut points: Vec<_> = contour
                            .iter()
                            .map(|p| self.camera.project(sketch.plane.at(*p), rect))
                            .collect();
                        if let Some(first) = points.first().copied() {
                            points.push(first);
                        }
                        ui.painter().add(egui::Shape::line(
                            points,
                            Stroke::new(if selected { 3.0f32 } else { 2.0f32 }, color),
                        ));
                    }
                }
            }
        }
    }
    fn selection_controls(&mut self, ctx: &egui::Context, rect: Rect) {
        let filters = self.editor.as_ref().map_or(Filter::ALL.as_slice(),|e|feature_inputs::filters(e,&self.registry));
        if filters.len() == 1 { return; }
        egui::Area::new(egui::Id::new("selection_filter")).fixed_pos(rect.min+egui::vec2(8.,8.)).show(ctx,|ui|{
            ui.set_min_width(180.);
            ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Extend);
            ui.menu_button(format!("Select: {}",self.active_selection_filter().label()),|ui|{
                for &filter in filters {if ui.selectable_value(&mut self.selection_filter,filter,filter.label()).clicked(){ui.close();}}
            }).response.on_hover_text("Click adds or removes. Space clears. Drag right to enclose, left to cross. Ctrl-drag removes.");
        });
    }
    fn select_other_popup(&mut self, ctx: &egui::Context) {
        let Some((pos, items, index)) = self.select_other.clone() else {
            return;
        };
        let mut chosen = None;
        let mut close = false;
        egui::Area::new(egui::Id::new("select_other"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("Select other");
                        if ui.small_button("x").clicked() {
                            close = true;
                        }
                    });
                    egui::ScrollArea::vertical()
                        .max_height(280.)
                        .show(ui, |ui| {
                            for (i, target) in items.iter().enumerate() {
                                let label = self.target_label(target);
                                ui.set_max_width(320.);
                                let response = ui
                                    .add(
                                        egui::Button::new(&label)
                                            .selected(i == index)
                                            .frame(false)
                                            .truncate(),
                                    )
                                    .on_hover_text(&label);
                                if response.hovered()
                                    && let Some((_, _, current)) = &mut self.select_other
                                {
                                    *current = i;
                                }
                                if response.clicked() {
                                    chosen = Some(target.clone());
                                }
                            }
                        });
                });
            });
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            chosen = items.get(index).cloned();
        }
        if let Some(target) = chosen {
            self.toggle_target(target, ctx);
            close = true;
        }
        if close {
            self.select_other = None;
        }
    }
    fn target_label(&self, target: &Target) -> String {
        let scene = self.source_scene.as_ref().unwrap_or(&self.scene);
        match target {
            Target::Face { part, key } => scene
                .meshes
                .iter()
                .find(|m| &m.part == part)
                .map(|m| {
                    format!(
                        "Face {} · {}",
                        m.faces.iter().position(|f| reference_matches(&f.key, key)).unwrap_or(0) + 1,
                        m.name
                    )
                })
                .unwrap_or_else(|| "Face".into()),
            Target::Edge { part, key } => scene
                .meshes
                .iter()
                .find(|m| &m.part == part)
                .map(|m| {
                    format!(
                        "Edge {} · {}",
                        m.edges
                            .iter()
                            .filter(|e| !e.seam)
                            .position(|e| reference_matches(&e.key, key))
                            .unwrap_or(0)
                            + 1,
                        m.name
                    )
                })
                .unwrap_or_else(|| "Edge".into()),
            Target::Region { sketch, key } => {
                let index = scene
                    .regions
                    .get(sketch)
                    .and_then(|r| r.iter().position(|r| reference_matches(&r.key, key)))
                    .unwrap_or(0)
                    + 1;
                let name = self
                    .document
                    .features
                    .iter()
                    .find(|f| &f.id == sketch)
                    .map(|f| f.name.as_str())
                    .unwrap_or("Sketch");
                format!("Region {index} · {name}")
            }
            Target::Part(part) => scene
                .meshes
                .iter()
                .find(|m| &m.part == part)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| "Part".into()),
            Target::Feature(id) => self
                .document
                .features
                .iter()
                .find(|f| &f.id == id)
                .map(|f| f.name.clone())
                .unwrap_or_else(|| "Feature".into()),
            Target::Plane(name) => self.plane_name(name),
            Target::SketchEntity {sketch,entity,edge} => {
                let name = self.document.features.iter().find(|f|&f.id == sketch).map_or(sketch.as_str(),|f|f.name.as_str());
                format!("{name} / Entity {entity} / Edge {}",edge+1)
            }
        }
    }
    fn begin(&mut self, id: &str) {
        self.notice.clear();
        self.sketch_ui = SketchUi::default();
        if self.editor.is_some() || self.rename_feature.is_some() {
            return;
        }
        if self.work.busy() || self.pending_rebuild.is_some() {
            self.pending_edit = Some(EditRequest::New(id.into()));
            return;
        }
        let workspace = EditWorkspace::capture(self);
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
        if manifest.editor == "sketch" {
            let support = self.selected_sketch_support();
            match &support {
                Some(Target::Face { part, key }) => {
                    if !self.scene.meshes.iter().any(|m| &m.part == part && m.faces.iter().any(|f| reference_matches(&f.key, key) && f.plane.is_some())) {
                        self.last_error = Some("Choose a plane or planar face.".into());
                        return;
                    }
                    if let Err(error) = planes::attach_face(&mut f.params, part, key) {
                        self.last_error = Some(error);
                        return;
                    }
                }
                Some(Target::Plane(id)) => {
                    if let Err(error) = planes::attach(&mut f.params, id) {
                        self.last_error = Some(error);
                        return;
                    }
                }
                _ => (),
            }
            self.choosing_plane = support.is_none();
            if self.choosing_plane {
                self.show_planes = true;
            }
            self.align_when_ready = !self.choosing_plane;
            self.sketch_ui.set_tool(Tool::Select);
        } else {
            self.sketch_ui.cancel_gesture();
            if manifest.fields.iter().any(|f| f.kind == "plane_ref") { self.show_planes = true; }
        }
        let input_field = feature_inputs::seed_inputs(&mut f, self.selection.items(), &self.scene, &self.registry);
        let sketch = f.sketch();
        self.source_scene = Some(self.scene.clone());
        self.editor = Some(Editor {
            workspace,
            index,
            is_new: true,
            feature: f,
            sketch,
            changed: false,
            input_field,
            undo: vec![],
            redo: vec![],
        });
        self.selection_filter = Filter::All;
        self.selection.clear();
        self.sync_selection_from_editor();
        self.rebuild();
    }
    fn edit_feature(&mut self, index: usize) {
        self.notice.clear();
        self.sketch_ui = SketchUi::default();
        if self.editor.is_some() || self.rename_feature.is_some() {
            return;
        }
        let Some(feature) = self.document.features.get(index).cloned() else {
            return;
        };
        if self.work.busy() || self.pending_rebuild.is_some() {
            self.pending_edit = Some(EditRequest::Existing(feature.id));
            return;
        }
        let workspace = EditWorkspace::capture(self);
        if self.scene.planes.contains_key(&feature.id) {
            self.show_planes = true;
            self.hidden_sketches.remove(&feature.id);
        }
        self.choosing_plane = false;
        let sketch = feature.sketch();
        self.source_scene = None;
        let align = sketch.is_some();
        self.editor = Some(Editor {
            workspace,
            index,
            is_new: false,
            feature,
            sketch,
            changed: false,
            input_field: None,
            undo: vec![],
            redo: vec![],
        });
        self.sketch_ui.cancel_gesture();
        self.align_when_ready = align;
        self.selection_filter = Filter::All;
        self.selection.clear();
        self.sync_selection_from_editor();
        self.rebuild();
    }
    fn finish_editor(&mut self, apply: bool) {
        if apply && let Some(error) = self.missing_input() {
            self.last_error = Some(error);
            return;
        }
        let Some(mut edit) = self.editor.take() else { return; };
        self.pending_accept = None;
        self.source_scene = None;
        edit.sync();
        let before = self.document.clone();
        if apply {
            if !edit.is_new && let Some(plugin) = self.registry.plugins.get(&edit.feature.plugin) { plugin.adopt_inputs(&mut edit.feature); }
            edit.apply_to(&mut self.document);
        }
        let new_plane = apply && edit.is_new && self.scene.planes.contains_key(&edit.feature.id);
        edit.workspace.restore(self);
        if new_plane { self.show_planes = true; self.hidden_sketches.remove(&edit.feature.id); }
        if apply {
            self.selected_feature = Some(edit.index);
            self.selection.clear();
            self.selection.add(Target::Feature(self.document.features[edit.index].id.clone()));
        }
        self.sketch_ui.cancel_gesture();
        if before == self.document { self.rebuild(); } else { self.remember(before); }
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
                self.sync_selection_from_editor();
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
        if self.assembly_context.is_some() { self.notice = "Commit or cancel the current assembly context first".into(); return; }
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
        self.pending_edit = None;
        self.source_scene = None;
        self.selected_feature = None;
        self.selection.clear();
        self.select_other = None;
        self.selection_box = None;
        self.selected_plane = None;
        self.pending_accept = None;
        self.hidden.clear();
        self.hidden_sketches.clear();
        self.choosing_plane = false;
        self.fit_when_ready = true;
        self.rebuild();
    }
    fn open_dialog(&mut self, action: FileAction) {
        if self.assembly_context.is_some() {
            match action { FileAction::Save => { self.commit_context(true); return; }, FileAction::Open => { self.notice = "Commit or cancel the current assembly context first".into(); return; }, FileAction::Export(_) => () }
        }
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
        if self.assembly_context.is_some() { self.commit_context(true); return; }
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
        if self.assembly_context.is_some() { self.notice = "Commit or cancel the current assembly context first".into(); return; }
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
                .evaluate(&f, &Numbers::for_document(&self.document)?)?;
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
                    if !self.work.finish_build(serial) { continue; }
                    if source.is_some() {
                        self.source_scene = source;
                    }
                    match result {
                        Ok(scene) => {
                            self.last_error = scene.error.clone().or_else(|| self.missing_input());
                            self.scene = scene;
                            let visible_scene = self.source_scene.as_ref().unwrap_or(&self.scene);
                            self.selection.retain(|item| match item {
                                Target::Part(part) => {
                                    visible_scene.meshes.iter().any(|m| &m.part == part)
                                }
                                Target::Face { part, key } => {
                                    visible_scene.meshes.iter().any(|m| {
                                        &m.part == part && m.faces.iter().any(|f| reference_matches(&f.key, key))
                                    })
                                }
                                Target::Edge { part, key } => {
                                    visible_scene.meshes.iter().any(|m| {
                                        &m.part == part && m.edges.iter().any(|e| reference_matches(&e.key, key))
                                    })
                                }
                                Target::Feature(id) => {
                                    self.document.features.iter().any(|f| &f.id == id)
                                }
                                Target::Plane(_) => true,
                                Target::Region { sketch, .. } => {
                                    visible_scene.sketches.contains_key(sketch)
                                }
                                Target::SketchEntity { sketch, entity, .. } => visible_scene.sketches.get(sketch).is_some_and(|s| s.entities.iter().any(|e| e.id == *entity)),
                            });
                            // The source prefix may contain inputs consumed by
                            // the edited feature and absent from its result.
                            if self.editor.is_some() { self.sync_selection_from_editor(); }
                            self.render_revision += 1;
                        }
                        Err(e) => {
                            self.last_error = Some(e);
                        }
                    }
                }
                Response::Exported(result) => {
                    self.work.exports = self.work.exports.saturating_sub(1);
                    match result {
                        Ok(_) => self.notice = "Exported".into(),
                        Err(e) => self.last_error = Some(e),
                    }
                }
            }
        }
        if !self.work.busy() && self.pending_rebuild.is_none()
            && let Some(request) = self.pending_edit.take() {
            match request {
                EditRequest::New(id) => self.begin(&id),
                EditRequest::Existing(id) => {
                    if let Some(index) = self.document.features.iter().position(|f| f.id == id) {
                        self.edit_feature(index);
                    }
                }
            }
        }
    }
    fn start_feature(&mut self, id: &str) {
        if self.is_sketching() {
            if self.work.busy() || self.pending_rebuild.is_some() || self.last_error.is_some() {
                return;
            }
            self.finish_editor(true);
        }
        if self.editor.is_none() {
            self.begin(id);
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        use crate::shortcuts::{Command, Context, View};
        if self.files.is_some()
            || self.variables_open
            || self.plugins_open
            || self.settings_open
            || self.close_pending
            || self.pending_load.is_some()
        {
            return;
        }
        if self.shortcut_menu.is_some() {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.shortcut_menu = None;
                ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("tool_search")));
            }
            return;
        }
        if ctx.wants_keyboard_input() {
            if self.editor.as_ref().is_some_and(|e| e.sketch.is_none())
                && ctx.input(|i| i.key_pressed(egui::Key::Enter))
            {
                self.pending_accept = Some(ctx.input(|i| i.modifiers.shift));
                ctx.request_repaint();
            }
            return;
        }
        if self.editor.as_ref().is_some_and(|e| e.sketch.is_none())
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE,egui::Key::Tab) || i.consume_key(egui::Modifiers::SHIFT,egui::Key::Tab)) {
            let reverse = ctx.input(|i| i.modifiers.shift);
            feature_inputs::cycle(self.editor.as_mut().unwrap(), &self.registry, reverse);
            self.selection_filter = Filter::All;
            self.sync_selection_from_editor();
        }
        // Local documents add save/open/reload without stealing an Onshape tool key.
        for (key, action) in [(egui::Key::S, 0), (egui::Key::O, 1), (egui::Key::R, 2)] {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, key)) {
                match action {
                    0 => self.save(),
                    1 => self.open_dialog(FileAction::Open),
                    _ => self.reload(),
                }
            }
        }
        let sketching = self.is_sketching();
        if sketching {
            self.sketch_ui.shortcuts(ctx);
        }
        let context = if sketching {
            Context::Sketch
        } else {
            Context::PartStudio
        };
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            let egui::Event::Key {
                key,
                physical_key,
                modifiers,
                pressed: true,
                repeat: false,
                ..
            } = event
            else {
                continue;
            };
            let Some(command) =
                crate::shortcuts::command_for_event(key, physical_key, modifiers, context)
            else {
                continue;
            };
            if matches!(
                command,
                Command::SketchTool(_) | Command::Relationship(_) | Command::Construction
            ) {
                continue;
            }
            match command {
                Command::ClearSelection if !sketching => self.clear_selection(ctx),
                Command::Cancel => {
                    self.pending_edit = None;
                    if self.select_other.take().is_none() && !sketching {
                        self.finish_editor(false);
                    }
                    self.shortcut_menu = None;
                }
                Command::Accept | Command::AcceptAndRepeat if self.select_other.is_none() => {
                    if self.editor.is_some()
                        && !self.work.busy()
                        && self.pending_rebuild.is_none()
                        && self.last_error.is_none()
                        && self.missing_input().is_none()
                    {
                        let id = self.editor.as_ref().unwrap().feature.plugin.clone();
                        self.finish_editor(true);
                        if command == Command::AcceptAndRepeat {
                            self.begin(&id);
                        }
                    }
                }
                Command::Undo => self.undo(false),
                Command::Redo => self.undo(true),
                Command::Fit => self.fit(),
                Command::View(view) => {
                    let direction = match view {
                        View::Front => glam::Vec3::NEG_Y,
                        View::Back => glam::Vec3::Y,
                        View::Left => glam::Vec3::NEG_X,
                        View::Right => glam::Vec3::X,
                        View::Top => glam::Vec3::Z,
                        View::Bottom => glam::Vec3::NEG_Z,
                        View::Isometric => glam::Vec3::new(1., -1., 1.),
                    };
                    self.camera.pivot = render::model_center(&self.scene, &self.hidden)
                        .unwrap_or(self.camera.center);
                    self.camera.set_view(direction);
                }
                Command::Normal => {
                    let hover = ctx
                        .input(|i| i.pointer.hover_pos())
                        .filter(|p| self.viewport_rect.contains(*p));
                    let plane = self
                        .editor
                        .as_ref()
                        .and_then(|e| self.scene.sketches.get(&e.feature.id))
                        .map(|s| s.plane.clone())
                        .or_else(|| {
                            hover
                                .and_then(|p| self.hovered_sketch(p))
                                .and_then(|id| {
                                    self.source_scene
                                        .as_ref()
                                        .unwrap_or(&self.scene)
                                        .sketches
                                        .get(&id)
                                })
                                .map(|sk| sk.plane.clone())
                        })
                        .or_else(|| {
                            hover
                                .and_then(|p| {
                                    render::pick(
                                        &self.scene,
                                        &self.camera,
                                        p,
                                        self.viewport_rect,
                                        &self.hidden,
                                    )
                                })
                                .and_then(|f| f.plane)
                        })
                        .or_else(|| self.selected_support_plane())
                        .or_else(|| {
                            self.selected_feature
                                .and_then(|i| self.document.features.get(i))
                                .and_then(|f| self.scene.sketches.get(&f.id).map(|s| s.plane.clone())
                                    .or_else(|| self.plane_frame(&f.id)))
                        });
                    let plane = plane.or_else(|| {
                        hover
                            .and_then(|pos| {
                                self.plane_hits(pos, self.viewport_rect)
                                    .into_iter()
                                    .next()
                            })
                            .and_then(|name| self.plane_frame(&name))
                    });
                    if let Some(plane) = plane {
                        let mut aligned = self.camera.clone();
                        aligned.align(&plane);
                        let same = (self.camera.rotation * glam::Vec3::Z)
                            .dot(aligned.rotation * glam::Vec3::Z)
                            > 0.9999;
                        self.camera.rotate_to(if same {
                            aligned.rotation * glam::Quat::from_rotation_y(std::f32::consts::PI)
                        } else {
                            aligned.rotation
                        });
                    }
                }
                Command::ZoomIn => self.camera.span = (self.camera.span / 1.25).max(0.02),
                Command::ZoomOut => self.camera.span = (self.camera.span * 1.25).min(1e6),
                Command::Rotate {
                    horizontal,
                    vertical,
                    degrees,
                } => self.camera.rotate_to(
                    self.camera.rotation
                        * glam::Quat::from_scaled_axis(
                            glam::Vec3::new(-(vertical as f32), -(horizontal as f32), 0.)
                                * (degrees as f32).to_radians(),
                        ),
                ),
                Command::Pan {
                    horizontal,
                    vertical,
                } => self.camera.pan(
                    egui::vec2(horizontal as f32, -(vertical as f32))
                        * self.viewport_rect.height()
                        * 0.05,
                    self.viewport_rect,
                ),
                Command::NewSketch => self.start_feature("sketch"),
                Command::Extrude => self.start_feature("extrude"),
                Command::Revolve => self.start_feature("revolve"),
                Command::Fillet => self.start_feature("fillet"),
                Command::Help => self.help_open = !self.help_open,
                Command::ShortcutToolbar | Command::SearchTools => {
                    self.shortcut_menu = ctx
                        .input(|i| i.pointer.hover_pos())
                        .or(Some(self.viewport_rect.center()));
                    self.shortcut_search.clear();
                    self.shortcut_focus = true;
                    self.search_index = 0;
                }
                Command::ToggleSketches => self.show_sketches = !self.show_sketches,
                Command::TogglePlanes => self.show_planes = !self.show_planes,
                Command::HideConstruction => {
                    self.show_sketches = false;
                    self.show_planes = false;
                    self.hidden_sketches
                        .extend(self.scene.sketches.keys().chain(self.scene.planes.keys()).cloned());
                }
                Command::HideSelectedOrHovered => {
                    let mut changed = false;
                    let mut handled = false;
                    for target in self.selection.items() {
                        match target {
                            Target::Part(part)
                            | Target::Face { part, .. }
                            | Target::Edge { part, .. } => {
                                handled = true;
                                changed |= self.hidden.insert(part.clone());
                            }
                            Target::Feature(id) | Target::Region { sketch: id, .. }
                                if self.scene.sketches.contains_key(id) || self.scene.planes.contains_key(id) =>
                            {
                                handled = true;
                                self.hidden_sketches.insert(id.clone());
                            }
                            Target::Plane(id) => { handled = true; self.hidden_sketches.insert(id.clone()); }
                            _ => (),
                        }
                    }
                    if !handled
                        && let Some(pos) = ctx
                            .input(|i| i.pointer.hover_pos())
                            .filter(|p| self.viewport_rect.contains(*p))
                    {
                        if let Some(plane) = self.plane_hits(pos, self.viewport_rect).into_iter().next() {
                            self.hidden_sketches.insert(plane);
                        } else if let Some(sketch) = self.hovered_sketch(pos) {
                            self.hidden_sketches.insert(sketch);
                        } else if let Some(hit) = render::pick(
                            &self.scene,
                            &self.camera,
                            pos,
                            self.viewport_rect,
                            &self.hidden,
                        ) {
                            changed |= self.hidden.insert(hit.part);
                        }
                    }
                    if changed {
                        self.render_revision += 1;
                    }
                }
                Command::ShowHidden => {
                    self.hidden.clear();
                    self.hidden_sketches.clear();
                    self.render_revision += 1;
                }
                Command::Rename if self.editor.is_none() => {
                    if let Some(index) = self.selected_feature.or_else(|| self.selection.items().iter().rev().find_map(|t| {
                        if let Target::Feature(id) = t { self.document.features.iter().position(|f| &f.id == id) } else { None }
                    })) && let Some(feature) = self.document.features.get(index) {
                        self.rename_feature = Some(index);
                        self.rename_text = feature.name.clone();
                        ctx.memory_mut(|m| m.request_focus(egui::Id::new("rename_input")));
                    }
                }
                Command::Delete if !sketching && self.editor.is_none() => {
                    let ids: Vec<_> = self
                        .selection
                        .items()
                        .iter()
                        .filter_map(|t| {
                            if let Target::Feature(id) = t {
                                Some(id.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !ids.is_empty() {
                        let before = self.document.clone();
                        self.document.features.retain(|f| !ids.contains(&f.id));
                        self.document.rollback = None;
                        self.selection.clear();
                        self.selected_feature = None;
                        self.remember(before);
                    }
                }
                _ => (),
            }
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
            let scene = self.context_display_scene(self.scene.clone(), false);
            let sketches: Vec<_> = self.region_sketches(&scene).into_iter().map(|(_, sk)| sk).collect();
            self.camera.fit_visible_with_rect(&scene, sketches, &self.hidden, self.viewport_rect);
        }
    }

    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(17, 17, 21))
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
                    let _ = ui.selectable_label(true, "Part Studio");
                    if ui.add_enabled(self.assembly_context.is_none() && self.files.is_none() && self.shortcut_menu.is_none() && !self.variables_open && !self.plugins_open, egui::Button::new("Assembly")).clicked() { self.switch_to_assembly(ctx); }
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
                        ui.label(RichText::new("●").color(rgb(self.settings.appearance.turquoise)));
                    }
                    if self.work.busy() {
                        ui.add(egui::Spinner::new().size(14.));
                    }
                    if let Some(error) = &self.last_error {
                        ui.colored_label(RED, "!").on_hover_text(error);
                    }
                    let search_width = (ui.available_width() - 170.).max(100.);
                    if ui.add_sized([search_width, 24.], egui::Button::new("Search commands, history, settings     Alt+C"))
                        .clicked() {
                        self.shortcut_menu = Some(self.viewport_rect.center() - egui::vec2(180., 150.));
                        self.shortcut_search.clear();
                        self.shortcut_focus = true;
                        self.search_index = 0;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.menu_button("…", |ui| {
                            if ui.button("Settings").clicked() {
                                self.open_settings();
                                ui.close();
                            }
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
                    .fill(rgb(self.settings.appearance.panel))
                    .inner_margin(egui::Margin::symmetric(8, 3)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .small_button(if self.tree_open { "<" } else { ">" })
                        .on_hover_text("Toggle feature tree")
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
                                    !self.work.busy()
                                        && self.pending_rebuild.is_none()
                                        && self.last_error.is_none()
                                        && self.missing_input().is_none(),
                                    egui::Button::new(
                                        RichText::new("Finish").color(Color32::BLACK),
                                    )
                                    .fill(rgb(self.settings.appearance.turquoise)),
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
                        let order = ["sketch", "extrude", "revolve", "sweep", "loft", "fillet", "chamfer", "shell", "mirror", "pattern", "transform"];
                        let mut items = items;
                        items.sort_by_key(|(id, _, _)| order.iter().position(|v| *v == id).unwrap_or(order.len()));
                        let mut overflow = Vec::new();
                        let total: f32 = items.iter().map(|(_, name, _)| {
                            ui.painter().layout_no_wrap(name.clone(), egui::FontId::proportional(14.), Color32::WHITE).size().x + 18.
                        }).sum();
                        let reserve = if total > ui.available_width() { 54. } else { 0. };
                        for (id, name, description) in items {
                            let width = ui.painter().layout_no_wrap(name.clone(), egui::FontId::proportional(14.), Color32::WHITE).size().x + 14.;
                            if width + reserve > ui.available_width() {
                                overflow.push((id, name, description));
                                continue;
                            }
                            if ui.add_enabled(self.editor.is_none(), egui::Button::new(name)
                                .corner_radius(1).stroke(Stroke::new(1_f32, Color32::from_rgb(65, 65, 73))))
                                .on_hover_text(description).clicked() {
                                self.begin(&id);
                            }
                        }
                        if !overflow.is_empty() {
                            ui.menu_button("More", |ui| {
                                for (id, name, description) in overflow {
                                    if ui.add_enabled(self.editor.is_none(), egui::Button::new(name))
                                        .on_hover_text(description).clicked() {
                                        self.begin(&id);
                                        ui.close();
                                    }
                                }
                            });
                        }
                    }
                });
            });
    }

    fn tree(&mut self, ctx: &egui::Context) {
        if !self.tree_open {
            return;
        }
        let mut selection_changed = false;
        let mut open_edit = None;
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
                            if ui
                                .selectable_label(
                                    self.selected_plane.as_deref() == Some(name),
                                    name,
                                )
                                .clicked()
                            {
                                self.show_planes = true;
                                self.hidden_sketches.remove(name);
                                if self.is_sketching() {
                                    self.choose_plane(name);
                                } else {
                                    self.toggle_target(Target::Plane(name.into()), ctx);
                                }
                            }
                        }
                    });
                let mut command = None;
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
                            let editing = self.editor.as_ref().is_some_and(|e| e.feature.id == f.id);
                            let button =
                                egui::Button::new(
                                    RichText::new(format!("{prefix}{}", f.name)).color(color),
                                )
                                .selected(self.selection.contains(&Target::Feature(f.id.clone())))
                                .frame(false)
                                .min_size(Vec2::new(ui.available_width(), 24.))
                                .truncate()
                                .sense(Sense::click_and_drag());
                            let button = if editing {
                                button.fill(rgb(self.settings.appearance.purple).gamma_multiply(0.18))
                                    .stroke(Stroke::new(1_f32, rgb(self.settings.appearance.purple).gamma_multiply(0.6)))
                                    .frame(true)
                            } else { button };
                            let response = ui.add(button);
                            if let Some(e) = error {
                                response.clone().on_hover_text(e);
                            } else if let Some(s) = status {
                                response
                                    .clone()
                                    .on_hover_text(format!("{} · {:.2} ms", f.id, s.millis));
                            }
                            if response.clicked() {
                                selection_changed = true;
                                if self.scene.planes.contains_key(&f.id) { self.show_planes = true; self.hidden_sketches.remove(&f.id); }
                                if ui.input(|i| i.modifiers.shift) {
                                    let anchor = self.selected_feature.unwrap_or(index);
                                    for f in &self.document.features
                                        [anchor.min(index)..=anchor.max(index)]
                                    {
                                        self.selection.add(Target::Feature(f.id.clone()));
                                    }
                                } else {
                                    self.selection.toggle(Target::Feature(f.id.clone()));
                                }
                                self.selected_feature = self
                                    .selection
                                    .contains(&Target::Feature(f.id.clone()))
                                    .then_some(index);
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
                                if self.scene.planes.contains_key(&f.id) || self.scene.sketches.contains_key(&f.id) {
                                    let hidden = self.hidden_sketches.contains(&f.id);
                                    if ui.button(if hidden { "Show" } else { "Hide" }).clicked() {
                                        if hidden { self.hidden_sketches.remove(&f.id); if self.scene.planes.contains_key(&f.id) { self.show_planes = true; } }
                                        else { self.hidden_sketches.insert(f.id.clone()); }
                                        ui.close();
                                    }
                                }
                                if ui.button("Edit").clicked() {
                                    open_edit = Some(index);
                                    ui.close();
                                }
                                if ui.add_enabled(self.editor.is_none(), egui::Button::new("Rename")).clicked() {
                                    self.rename_feature = Some(index);
                                    self.rename_text = f.name.clone();
                                    ctx.memory_mut(|m| m.request_focus(egui::Id::new("rename_input")));
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
                        if ui.add_enabled(self.editor.is_none(), egui::Button::new("End").small()).clicked() {
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
                                                self.selection
                                                    .contains(&Target::Part(mesh.part.clone())),
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
                                    self.selection.toggle(Target::Part(mesh.part.clone()));
                                    selection_changed = true;
                                }
                            });
                        }
                    });
            });
        if selection_changed { self.sync_selection_to_editor(ctx); }
        // A history click belongs to the current workspace, before the new
        // editor projects its authoritative inputs into selection.
        if self.editor.is_none() && let Some(index) = open_edit {
            self.edit_feature(index);
        }
    }

    fn inspector(&mut self, ctx: &egui::Context) {
        if self.editor.is_none() {
            self.inspector_rect = Rect::NOTHING;
            return;
        }
        let preview_current = self.work.preview_is_current(self.pending_rebuild.is_some());
        self.inspector_rect = editor_overlay(ctx, rgb(self.settings.appearance.purple), |ui| {
                if let Some(mut edit) = self.editor.take() {
                    edit.sync();
                    let before_params = edit.feature.params.clone();
                    let manifest = self
                        .registry
                        .plugins
                        .get(&edit.feature.plugin)
                        .map(|p| p.manifest.clone());
                    let mut changed = false;
                    let mut input_changed = false;
                    let mut apply = false;
                    let mut cancel = false;
                    ui.horizontal(|ui| {
                        ui.scope(|ui| {
                            let reserved = if edit.sketch.is_none() { 65. } else { 0. };
                            ui.set_max_width((ui.available_width() - reserved).max(40.));
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&edit.feature.name).size(18.).strong()
                                        .color(rgb(self.settings.appearance.purple)),
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
                    changed |= self.context_reference_controls(ui, &mut edit);
                    if let Some(m) = &manifest {
                        egui::ScrollArea::vertical()
                            .id_salt("feature_properties")
                            .max_height((ui.available_height() - 70.).max(150.))
                            .show(ui, |ui| {
                                for field in &m.fields {
                                    if !feature_inputs::visible(field, &edit.feature) { continue; }
                                    if self.choosing_plane && field.key == "plane" {
                                        ui.label(RichText::new("Sketch support").small().color(MUTED));
                                        ui.label("Select a plane or planar face");
                                        continue;
                                    }
                                    if (edit.feature.params.get("context_plane").is_some_and(|v|!v.is_null()) && matches!(field.key.as_str(),"plane"|"support")) || (edit.feature.params.get("context_up_to").is_some_and(|v|!v.is_null()) && matches!(field.key.as_str(),"depth"|"symmetric")) {continue;}
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
                                let selectable = feature_inputs::selectable(&field.kind);
                                let active = feature_inputs::active_key(&edit, &self.registry) == Some(field.key.as_str());
                                egui::Frame::new().inner_margin(if selectable {5.} else {0.})
                                    .stroke(Stroke::new(if selectable {1_f32} else {0_f32}, if active {rgb(self.settings.appearance.turquoise)} else {ui.visuals().widgets.noninteractive.bg_stroke.color}))
                                    .show(ui, |ui| {
                                if selectable {
                                        let selected = feature_inputs::active_key(&edit, &self.registry) == Some(field.key.as_str());
                                        if ui.selectable_label(selected, &field.label).on_hover_text("Select this input, then pick in the viewport").clicked() {
                                            edit.input_field = Some(field.key.clone());
                                            input_changed = true;
                                        }
                                    } else if field.kind != "bool" {
                                        ui.label(RichText::new(if field.kind == "support" { "Sketch support" } else { &field.label }).small().color(MUTED));
                                    }
                                    let old_plane = edit.feature.params.get("plane").cloned();
                                    let old_support = edit.feature.params.get("support").cloned();
                                    let old_sketch = edit.feature.params.get("sketch").cloned();
                                    let detach_ready = !self.work.busy() && self.pending_rebuild.is_none() && self.last_error.is_none();
                                    let parameter = ui.add_enabled_ui(field.kind != "support" || detach_ready, |ui| parameter_ui(
                                        ui, field, &mut edit.feature.params, &self.document,
                                        edit.index, &self.registry,
                                        self.source_scene.as_ref().unwrap_or(&self.scene),
                                    ));
                                    changed |= parameter.inner;
                                    if field.kind == "body" && parameter.inner {
                                        feature_inputs::clear_owned_inputs(&mut edit.feature, &self.registry);
                                    }
                                    if field.key == "target" && m.fields.iter().any(|f| f.key == "targets")
                                        && ui.small_button("Multiple parts").clicked() {
                                        let target = edit.feature.params["target"].as_str().filter(|s| !s.is_empty())
                                            ;
                                        edit.feature.params["targets"] = json!(target.into_iter().collect::<Vec<_>>());
                                        edit.feature.params["target"] = json!("");
                                        edit.input_field = Some("targets".into());
                                        changed = true;
                                        input_changed = true;
                                    }
                                    if feature_inputs::selectable(&field.kind)
                                        && (parameter.inner || (parameter.response.contains_pointer() && ui.input(|i| i.pointer.any_pressed()))) {
                                        edit.input_field = Some(field.key.clone());
                                        input_changed = true;
                                    }
                                    if field.kind == "support" && parameter.inner
                                        && old_support.as_ref().and_then(Value::as_str).is_some_and(|s| !s.is_empty())
                                        && edit.feature.params["support"] == ""
                                        && let Some(sketch) = self.scene.sketches.get(&edit.feature.id) {
                                        planes::detach(&mut edit.feature.params, &sketch.plane);
                                    }
                                    if field.key == "plane"
                                        && old_plane != edit.feature.params.get("plane").cloned()
                                    {
                                        let id = edit.feature.params["plane"].as_str().unwrap_or("Top").to_owned();
                                        match planes::attach(&mut edit.feature.params, &id) {
                                            Ok(()) => {
                                                self.align_when_ready = true;
                                                self.choosing_plane = false;
                                            }
                                            Err(error) => {
                                                self.last_error = Some(error);
                                                if let Some(value) = old_plane.clone() {
                                                    edit.feature.params["plane"] = value;
                                                } else if let Some(object) = edit.feature.params.as_object_mut() {
                                                    object.remove("plane");
                                                }
                                            }
                                        }
                                    }
                                    if field.kind == "sketch"
                                        && old_sketch != edit.feature.params.get("sketch").cloned()
                                        && m.fields.iter().any(|f| f.kind == "regions")
                                    {
                                        edit.feature.params["regions"] = Value::Null;
                                    }
                                    });
                                ui.add_space(4.);
                                }
                                if m.editor == "sketch"
                                    && let Some(sketch) = &mut edit.sketch
                                {
                                    changed |= self.sketch_ui.inspector(
                                        ui,
                                        sketch,
                                        if preview_current && !changed { self.scene.sketches.get(&edit.feature.id) } else { None },
                                        self.last_error.as_deref(),
                                        &self.document.variables,
                                        self.document.format,
                                    );
                                }
                                if !self
                                    .registry
                                    .plugins
                                    .get(&edit.feature.plugin)
                                    .is_some_and(|p| p.matches(&edit.feature))
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
                                !self.work.busy()
                                    && self.pending_rebuild.is_none()
                                    && self.last_error.is_none(),
                                egui::Button::new(RichText::new("Apply").color(Color32::BLACK))
                                    .fill(rgb(self.settings.appearance.turquoise))
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
                    if input_changed { self.selection_filter = Filter::All; }
                    if changed || input_changed { self.sync_selection_from_editor(); }
                    if changed { self.schedule(ctx); }
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
            .frame(egui::Frame::new().fill(rgb(self.settings.appearance.background)))
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                self.viewport_rect = rect;
                let display_scene = self.context_display_scene(self.scene.clone(), false);
                let sketch_id = self
                    .editor
                    .as_ref()
                    .and_then(|e| {
                        if e.sketch.is_some() {
                            Some(e.feature.id.clone())
                        } else {
                            e.feature
                                .params
                                .get("sketch")
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                        }
                    })
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

                if !self.work.busy() && self.pending_rebuild.is_none() {
                    if self.fit_when_ready {
                        self.fit();
                        self.fit_when_ready = false;
                    }
                    if self.align_when_ready
                        && let Some(edit) = &mut self.editor
                        && let Some(sk) = self.scene.sketches.get(&edit.feature.id)
                    {
                        edit.workspace
                            .align_camera(&mut self.camera, sk, rect);
                        self.align_when_ready = false;
                    }
                }

                let pointer = response.hover_pos();
                let controls = crate::navigation::bounds(rect);
                let select_control =
                    Rect::from_min_size(rect.min + egui::vec2(8., 8.), egui::vec2(170., 28.));
                let canvas_hover =
                    pointer.is_some_and(|p| !controls.contains(p) && !select_control.contains(p)
                        && !(self.editor.is_some() && self.inspector_rect.contains(p)));
                let (right_drag, middle_drag) = ui.input(|i| (
                    render::navigation_drag(i, egui::PointerButton::Secondary),
                    render::navigation_drag(i, egui::PointerButton::Middle),
                ));
                let orbiting = canvas_hover
                    && response.hovered()
                    && (!self.is_sketching() || self.choosing_plane || self.context_picking())
                    && ui.input(|i| {
                        right_drag.is_some()
                            && !i.modifiers.shift
                            && !i.modifiers.ctrl
                    });
                if orbiting && !self.mouse_orbiting {
                    let origin = right_drag.and_then(|m| m.start)
                        .or_else(|| ui.input(|i| i.pointer.press_origin())).or(pointer);
                    self.camera.pivot = origin
                        .and_then(|p| {
                            render::pick(&self.scene, &self.camera, p, rect, &self.hidden)
                        })
                        .map(|p| p.point)
                        .or_else(|| render::model_center(&self.scene, &self.hidden))
                        .unwrap_or(self.camera.center);
                }
                self.mouse_orbiting = orbiting && ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary));
                if response.hovered() && canvas_hover {
                    let delta = middle_drag.or(right_drag).map_or(Vec2::ZERO, |m| m.delta);
                    let right = right_drag.is_some();
                    let middle = middle_drag.is_some();
                    if middle
                        || (right
                            && ((self.is_sketching() && !self.choosing_plane && !self.context_picking())
                                || ui.input(|i| i.modifiers.shift || i.modifiers.ctrl)))
                    {
                        self.camera.pan(delta * self.settings.mouse_pan_speed, rect);
                    } else if right {
                        if ui.input(|i| i.modifiers.alt) {
                            self.camera.orbit_upright(delta * self.settings.mouse_rotation_speed);
                        } else {
                            self.camera.orbit(delta * self.settings.mouse_rotation_speed);
                        }
                    }
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll.abs() > 0.01 {
                        let old = self.camera.span;
                        self.camera.span = (old * (-scroll * 0.0025 * self.settings.mouse_zoom_speed).exp()).clamp(0.02, 1e6);
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
                let pick_base = self.source_scene.as_ref().unwrap_or(&self.scene).clone();
                let pick_scene = self.context_display_scene(pick_base, true);
                let filter = self.active_selection_filter();
                let candidates = pointer
                    .filter(|_| canvas_hover && (!self.is_sketching() || self.context_picking()))
                    .map(|p| if self.context_picking() {
                        self.assembly_context.as_ref().and_then(|s| render::pick(&s.ghosts, &self.camera, p, rect, &BTreeSet::new())).map(|hit| vec![Target::Face { part: hit.part, key: hit.face }]).unwrap_or_default()
                    } else { self.selection_candidates(&pick_scene, p, rect, filter, false) })
                    .unwrap_or_default();
                let mut hover = candidates.first().cloned();
                if let Some((_, targets, index)) = &self.select_other {
                    hover = targets.get(*index).cloned();
                }
                let selected = self.face_ids(&display_scene, self.selection.items());
                let edited = self.edited_faces();
                let hovered =
                    self.face_ids(&display_scene, &hover.clone().into_iter().collect::<Vec<_>>());
                ui.painter().add(render::paint_callback(
                    rect,
                    self.renderer.clone(),
                    display_scene,
                    self.hidden.clone(),
                    self.render_revision,
                    render::View {
                        matrix: self.camera.matrix(rect),
                        edited,
                        selected,
                        hovered,
                        selected_edges: self
                            .selection
                            .edges()
                            .map(|(p, k)| (p.to_owned(), k.to_owned()))
                            .collect(),
                        hovered_edges: hover
                            .as_ref()
                            .and_then(|t| {
                                if let Target::Edge { part, key } = t {
                                    Some(vec![(part.clone(), key.clone())])
                                } else {
                                    None
                                }
                            })
                            .unwrap_or_default(),
                        shaded_edges: self.shaded_edges,
                        selection_color: self.settings.appearance.turquoise.map(|v| f32::from(v)/255.),
                        hover_color: self.settings.appearance.purple.map(|v| f32::from(v)/255.),
                        ghost_parts: self.context_ghost_parts(),
                    },
                ));
                if self.mouse_orbiting {
                    let pivot =
                        projector.project(self.camera.pivot.to_array().map(f64::from), rect);
                    ui.painter()
                        .circle_stroke(pivot, 4., Stroke::new(1.2_f32, rgb(self.settings.appearance.turquoise)));
                }
                if !self.is_sketching() {
                    for (_, sk) in self.region_sketches(&pick_scene) {
                        crate::sketch_ui::paint_readonly(ui, sk, &projector, rect);
                    }
                }
                self.paint_edit_inputs(ui, rect, hover.as_ref());
                self.paint_planes(ui, rect, pointer);
                if self.context_picking() {
                    if response.clicked() && canvas_hover && let Some(pos) = pointer { self.pick_context_reference(pos, rect); }
                } else if self.choosing_plane {
                    if response.clicked()
                        && canvas_hover
                        && let Some(pos) = pointer
                        && let Some(target) = self.sketch_support_hit(&pick_scene, pos, rect)
                    {
                            match target {
                                Target::Plane(name) => self.choose_plane(&name),
                                Target::Face { part, key } => {
                                    if let Some(edit) = &mut self.editor {
                                        let before = edit.feature.params.clone();
                                        match planes::attach_face(&mut edit.feature.params, &part, &key) {
                                            Ok(()) => {
                                                if before != edit.feature.params { edit.undo.push(before); edit.redo.clear(); edit.changed = true; }
                                                self.choosing_plane = false;
                                                self.align_when_ready = true;
                                                self.rebuild();
                                            }
                                            Err(error) => self.last_error = Some(error),
                                        }
                                    }
                                }
                                _ => (),
                            }
                    }
                } else if self.is_sketching() {
                    let preview_current = self.work.preview_is_current(self.pending_rebuild.is_some());
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
                                if preview_current { self.scene.sketches.get(&edit.feature.id) } else { None },
                                plane,
                                &self.document.variables,
                                self.document.format,
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
                } else {
                    if response.clicked() && canvas_hover {
                        if let Some(target) = candidates.first().cloned() {
                            self.toggle_target(target, ctx);
                        } else {
                            self.clear_selection(ctx);
                        }
                    }
                    if response.drag_started_by(egui::PointerButton::Primary) && canvas_hover {
                        self.selection_box = ui.input(|i| i.pointer.press_origin());
                    }
                    if let Some(start) = self.selection_box {
                        if let Some(end) = ui.input(|i| i.pointer.interact_pos()) {
                            let area = Rect::from_two_pos(start, end);
                            let crossing = end.x < start.x;
                            ui.painter().rect_filled(
                                area,
                                0.,
                                if crossing {
                                    rgb(self.settings.appearance.purple).gamma_multiply(0.12)
                                } else {
                                    rgb(self.settings.appearance.turquoise).gamma_multiply(0.12)
                                },
                            );
                            ui.painter().rect_stroke(
                                area,
                                0.,
                                Stroke::new(1.0f32, if crossing { rgb(self.settings.appearance.purple) } else { rgb(self.settings.appearance.turquoise) }),
                                egui::StrokeKind::Inside,
                            );
                            if response.drag_stopped_by(egui::PointerButton::Primary) {
                                let mut targets = render::box_targets(
                                    &pick_scene,
                                    &self.camera,
                                    rect,
                                    area,
                                    crossing,
                                    filter,
                                    &self.hidden,
                                );
                                if matches!(filter, Filter::Regions) {
                                    targets = self.box_regions(&pick_scene, rect, area, crossing);
                                } else if self.editor.as_ref().is_some_and(|e| feature_inputs::profile_input(e, &self.registry)) {
                                    targets.retain(|t| match t {
                                        Target::Face { part, key } => pick_scene.meshes.iter().any(|m| &m.part == part && m.faces.iter().any(|f| reference_matches(&f.key, key) && f.plane.is_some() && !key.contains("untracked:"))),
                                        _ => false,
                                    });
                                    if filter == Filter::All { targets.extend(self.box_regions(&pick_scene, rect, area, crossing)); }
                                }
                                for target in targets {
                                    if ui.input(|i| i.modifiers.ctrl) {
                                        self.selection.remove(&target);
                                    } else {
                                        self.selection.add(target);
                                    }
                                }
                                self.sync_selection_to_editor(ctx);
                                ctx.request_repaint();
                                self.selection_box = None;
                            }
                        }
                        if !ui.input(|i| i.pointer.primary_down()) {
                            self.selection_box = None;
                        }
                    }
                    let reverse_other = ui
                        .input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Backtick));
                    if canvas_hover
                        && (reverse_other
                            || ui.input_mut(|i| {
                                i.consume_key(egui::Modifiers::NONE, egui::Key::Backtick)
                            }))
                    {
                        if let Some((_, items, index)) = &mut self.select_other {
                            if !items.is_empty() {
                                *index = (*index + if reverse_other { items.len() - 1 } else { 1 })
                                    % items.len();
                            }
                        } else if let Some(p) = pointer {
                            let all = self.selection_candidates(&pick_scene, p, rect, filter, true);
                            if !all.is_empty() {
                                self.select_other = Some((p, all, 0));
                            }
                        }
                    }
                    if response.secondary_clicked() {
                        self.context_position = pointer;
                    }
                    response.context_menu(|ui| {
                        if ui.button("Select other").clicked() {
                            if let Some(p) = self.context_position {
                                let all =
                                    self.selection_candidates(&pick_scene, p, rect, filter, true);
                                self.select_other = Some((p, all, 0));
                            }
                            ui.close();
                        }
                        if ui.button("Clear selection").clicked() {
                            self.clear_selection(ctx);
                            ui.close();
                        }
                        ui.separator();
                        for &option in self.editor.as_ref().map_or(Filter::ALL.as_slice(),|e|feature_inputs::filters(e,&self.registry)) {
                            if ui
                                .selectable_value(
                                    &mut self.selection_filter,
                                    option,
                                    option.label(),
                                )
                                .clicked()
                            {
                                ui.close();
                            }
                        }
                    });
                    self.paint_feature_axis(ui, &pick_scene, rect);
                self.paint_regions(ui, &pick_scene, rect, hover.as_ref());
                    if !self.selection.items().is_empty()
                        && let Some(pos) = pointer
                    {
                        let label = ui.painter().layout_no_wrap(
                            format!("{} selected", self.selection.items().len()),
                            egui::FontId::proportional(12.),
                            rgb(self.settings.appearance.turquoise),
                        );
                        let badge = Rect::from_min_size(
                            pos + egui::vec2(15., 18.),
                            label.size() + egui::vec2(8., 4.),
                        );
                        ui.painter()
                            .rect_filled(badge, 3., Color32::from_rgb(7, 7, 10));
                        ui.painter()
                            .galley(badge.min + egui::vec2(4., 2.), label, rgb(self.settings.appearance.turquoise));
                    }
                    self.selection_controls(ctx, rect);
                    self.select_other_popup(ctx);
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

    fn notice_ui(&mut self, ctx: &egui::Context) {
        if self.notice.is_empty() || !self.viewport_rect.is_positive() { return; }
        egui::Area::new(egui::Id::new("status_notice"))
            .order(egui::Order::Foreground)
            .fixed_pos(self.viewport_rect.left_bottom() + egui::vec2(8., -34.))
            .show(ctx, |ui| {
                ui.set_max_width((self.viewport_rect.width() - 24.).clamp(100., 620.));
                egui::Frame::new().fill(rgb(self.settings.appearance.panel))
                    .inner_margin(4).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(RichText::new(&self.notice).small()).truncate())
                                .on_hover_text(&self.notice);
                            if ui.small_button("X").on_hover_text("Dismiss").clicked() { self.notice.clear(); }
                        });
                    });
            });
    }

    fn open_settings(&mut self) {
        if self.settings_open {
            return;
        }
        self.settings.shaded_edges = self.shaded_edges;
        self.settings.tree_open = self.tree_open;
        self.settings.space_mouse.enabled = self.space_mouse.options.enabled;
        self.settings.space_mouse.speed = self.space_mouse.options.speed;
        self.settings.space_mouse.reverse_zoom = self.space_mouse.options.reverse_zoom;
        self.settings.space_mouse.reverse_rotation = self.space_mouse.options.reverse_rotation;
        self.settings_saved = self.settings.clone();
        self.settings_error = None;
        self.settings_open = true;
    }

    fn apply_settings(&mut self, ctx: &egui::Context) {
        self.settings.sanitize();
        let mut style = (*ctx.style()).clone();
        style.visuals.panel_fill = rgb(self.settings.appearance.panel);
        style.visuals.selection.stroke.color = rgb(self.settings.appearance.turquoise);
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1_f32, rgb(self.settings.appearance.purple));
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1_f32, Color32::from_rgb(64, 64, 72));
        style.visuals.widgets.active.bg_stroke = Stroke::new(1_f32, rgb(self.settings.appearance.turquoise));
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(1);
        style.spacing.button_padding = if self.settings.compact_toolbar { Vec2::new(7., 3.) } else { Vec2::new(10., 5.) };
        ctx.set_style(style);
        self.shaded_edges = self.settings.shaded_edges;
        self.tree_open = self.settings.tree_open;
        self.space_mouse.options.enabled = self.settings.space_mouse.enabled;
        self.space_mouse.options.speed = self.settings.space_mouse.speed;
        self.space_mouse.options.reverse_zoom = self.settings.space_mouse.reverse_zoom;
        self.space_mouse.options.reverse_rotation = self.settings.space_mouse.reverse_rotation;
    }

    fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_open { return; }
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let before = self.settings.clone();
        egui::Window::new("Settings").open(&mut open).collapsible(false).default_width(420.)
            .show(ctx, |ui| {
                ui.strong("Appearance");
                egui::Grid::new("appearance_preferences").show(ui, |ui| {
                    for (label, color) in [("Viewport", &mut self.settings.appearance.background),
                        ("Panels", &mut self.settings.appearance.panel), ("Selection", &mut self.settings.appearance.turquoise),
                        ("Hover", &mut self.settings.appearance.purple)] {
                        ui.label(label); ui.color_edit_button_srgb(color); ui.end_row();
                    }
                });
                ui.checkbox(&mut self.settings.shaded_edges, "Visible model edges");
                ui.checkbox(&mut self.settings.tree_open, "Feature tree");
                ui.checkbox(&mut self.settings.compact_toolbar, "Compact buttons");
                ui.separator();
                ui.strong("Mouse");
                ui.add(egui::Slider::new(&mut self.settings.mouse_rotation_speed, 0.1..=5.).text("Rotation"));
                ui.add(egui::Slider::new(&mut self.settings.mouse_pan_speed, 0.1..=5.).text("Pan"));
                ui.add(egui::Slider::new(&mut self.settings.mouse_zoom_speed, 0.1..=5.).text("Zoom"));
                ui.separator();
                ui.strong("SpaceMouse");
                ui.checkbox(&mut self.settings.space_mouse.enabled, "Enabled");
                ui.add(egui::Slider::new(&mut self.settings.space_mouse.speed, 0.1..=5.).text("Speed"));
                ui.checkbox(&mut self.settings.space_mouse.reverse_zoom, "Reverse zoom");
                ui.checkbox(&mut self.settings.space_mouse.reverse_rotation, "Reverse rotation");
                ui.separator();
                if let Some(error) = &self.settings_error { ui.colored_label(RED, error); }
                ui.horizontal(|ui| {
                    save = ui.button("Save").clicked();
                    cancel = ui.button("Cancel").clicked();
                    if ui.button("Restore defaults").clicked() { self.settings = Default::default(); }
                });
            });
        if before != self.settings { self.apply_settings(ctx); }
        if save {
            match self.settings.save() {
                Ok(()) => { self.settings_saved = self.settings.clone(); self.settings_open = false; }
                Err(error) => self.settings_error = Some(error),
            }
        } else if cancel || !open {
            self.settings = self.settings_saved.clone();
            self.apply_settings(ctx);
            self.settings_open = false;
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(pos) = self.shortcut_menu {
            let mut entries = Vec::new();
            if self.is_sketching() {
                for (name, tool) in [("Line  L", Tool::Line), ("Corner rectangle  G", Tool::Rectangle),
                    ("Center rectangle  R", Tool::CenterRectangle), ("Circle  C", Tool::Circle),
                    ("Three-point arc  A", Tool::ThreePointArc), ("Center-point arc", Tool::Arc), ("Tangent arc  Shift+A", Tool::TangentArc),
                    ("Point  Shift+S", Tool::Point),
                    ("Trim  M", Tool::Trim),
                    ("Dimension  D", Tool::Dimension), ("Select", Tool::Select)] {
                    entries.push((name.to_owned(), SearchAction::SketchTool(tool)));
                }
                for &name in crate::sketch_ui::RELATIONSHIPS {
                    let label = if name == "Midpoint" { "Midpoint  Shift+M" } else { name };
                    entries.push((label.to_owned(),SearchAction::SketchTool(Tool::Relationship(name))));
                }
            }
            for p in self.registry.plugins.values() {
                let action = if p.manifest.category == "feature" {
                    SearchAction::Feature(p.manifest.id.clone())
                } else if p.manifest.category == "exporter" {
                    SearchAction::Export(p.manifest.id.clone())
                } else { continue; };
                entries.push((p.manifest.name.clone(), action));
            }
            entries.extend([
                ("Settings: Appearance, mouse, SpaceMouse, display".into(), SearchAction::Settings),
                ("Variables".into(), SearchAction::Variables), ("Save  Ctrl+S".into(), SearchAction::Save),
                ("Open  Ctrl+O".into(), SearchAction::Open), ("Fit view  F".into(), SearchAction::Fit),
                ("Show or hide planes  P".into(), SearchAction::Planes),
                ("Plugins".into(), SearchAction::Plugins), ("Keyboard shortcuts and help".into(), SearchAction::Help),
            ]);
            for (index, feature) in self.document.features.iter().enumerate() {
                entries.push((format!("History: {}", feature.name), SearchAction::History(index)));
            }
            for mesh in &self.scene.meshes {
                entries.push((format!("Part: {}", mesh.name), SearchAction::Part(mesh.part.clone())));
            }
            let history_available = self.editor.is_none();
            let feature_available = history_available || (self.is_sketching() && !self.work.busy() && self.pending_rebuild.is_none() && self.last_error.is_none());
            let export_available = history_available && !self.scene.meshes.is_empty() && self.scene.error.is_none();
            let available = |action: &SearchAction| match action {
                SearchAction::Feature(_) => feature_available,
                SearchAction::History(_) | SearchAction::Save => history_available,
                SearchAction::Export(_) => export_available,
                _ => true,
            };
            let mut open = true;
            let mut chosen = None;
            egui::Window::new("Search")
                .default_pos(pos).default_width(420.).resizable(true).collapsible(false)
                .open(&mut open).show(ctx, |ui| {
                    let search = ui.add(egui::TextEdit::singleline(&mut self.shortcut_search)
                        .id(egui::Id::new("tool_search")).desired_width(f32::INFINITY)
                        .hint_text("Commands, history, parts, settings"));
                    if self.shortcut_focus { search.request_focus(); self.shortcut_focus = false; }
                    if search.changed() { self.search_index = 0; }
                    let matches = crate::search::ranked(&self.shortcut_search, entries.iter().map(|(name, _)| name.as_str()));
                    let down = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
                    let up = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
                    let enabled_rows: Vec<_> = matches.iter().enumerate().filter_map(|(row, &index)| available(&entries[index].1).then_some(row)).collect();
                    let mut cursor = enabled_rows.iter().position(|row| *row == self.search_index).unwrap_or(0);
                    if down { cursor = cursor.saturating_add(1).min(enabled_rows.len().saturating_sub(1)); }
                    if up { cursor = cursor.saturating_sub(1); }
                    self.search_index = enabled_rows.get(cursor).copied().unwrap_or(0);
                    let accept = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
                        for (row, &index) in matches.iter().enumerate() {
                            let enabled = available(&entries[index].1);
                            let response = ui.add_enabled_ui(enabled, |ui| ui.selectable_label(row == self.search_index, &entries[index].0)).inner;
                            if !enabled { response.clone().on_hover_text("Finish the active feature first"); }
                            if row == self.search_index && (up || down) { response.scroll_to_me(Some(egui::Align::Center)); }
                            if enabled && (response.clicked() || (accept && row == self.search_index)) { chosen = Some(index); }
                        }
                        if matches.is_empty() { ui.weak("No matches"); }
                        if !history_available { ui.weak("Finish the active feature to edit other features or export."); }
                    });
                });
            if let Some(index) = chosen {
                let (_, action) = entries.swap_remove(index);
                open = false;
                ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("tool_search")));
                match action {
                    SearchAction::Feature(id) => self.start_feature(&id),
                    SearchAction::Export(id) => self.exporter(&id),
                    SearchAction::SketchTool(tool) => self.sketch_ui.set_tool(tool),
                    SearchAction::History(index) => {
                        self.selected_feature = Some(index);
                        self.selection.clear();
                        self.selection.add(Target::Feature(self.document.features[index].id.clone()));
                        if self.editor.is_none() { self.edit_feature(index); }
                    }
                    SearchAction::Part(part) => { self.selection.clear(); self.selection.add(Target::Part(part)); }
                    SearchAction::Settings => self.open_settings(),
                    SearchAction::Variables => {
                        self.variables_name = self.document.name.clone();
                        self.variables_draft = self.document.variables.iter().map(|(k,v)| (k.clone(),v.clone())).collect();
                        self.variables_open = true;
                    }
                    SearchAction::Save => self.save(), SearchAction::Open => self.open_dialog(FileAction::Open),
                    SearchAction::Fit => self.fit(), SearchAction::Planes => self.show_planes = !self.show_planes,
                    SearchAction::Plugins => self.plugins_open = true, SearchAction::Help => self.help_open = true,
                }
            }
            if !open { self.shortcut_menu = None; }
        }
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
                                        && path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("cadpart"))
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
                    let exists = dialog.target_path().is_file();
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
                                egui::Button::new(label).fill(Color32::from_rgb(48, 73, 78)),
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
                let path = dialog.target_path();
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
                    FileAction::Export(_) if self.editor.is_some() => {
                        Err("Finish the active feature before exporting".into())
                    }
                    FileAction::Export(format) => {
                        self.work.exports += 1;
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
                        Numbers::for_format(&vars, self.document.format).map(|_| ())
                    };
                    if let Err(e) = &validation {
                        ui.colored_label(RED, e);
                    }
                    let button = ui.add_enabled(validation.is_ok() && self.editor.is_none(), egui::Button::new("Apply variables"));
                    let button = if self.editor.is_some() { button.on_disabled_hover_text("Finish or cancel the active feature first") } else { button };
                    if button.clicked() { apply = true; }
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
                ui.label("Sketch: choose a plane, draw lines, rectangles or circles. Press D, select geometry, place the dimension, enter a value. Double-click a dimension to edit. Click additional geometry to select pairs for dimensions and constraints. Space clears selection.");
                ui.separator();
                ui.label("Finish the sketch, then Extrude. Select a supported planar face for the next sketch. Use Remove for cuts.");
                ui.label("Double-click history to edit. Right-click to rename, reorder, suppress or delete. Rollback previews earlier history.");
                ui.separator();
                ui.label("Right drag: orbit. Middle drag or Shift + right drag: pan. Scroll: zoom. F: fit. Shift+E: extrude. Shift+W: revolve. Shift+S: sketch. Ctrl+Z / Ctrl+Y: undo / redo. Ctrl+S: save.");
                ui.label("Click to add or remove selections. Use the selection filter for faces, edges, parts or sketch regions. The ` key opens Select other. Shell removes selected faces. Pattern and mirror create separate parts.");
                ui.separator();
                ui.label("Save as .cadpart. STEP exports boundary geometry; STL exports a mesh in millimeters.");
                ui.label(RichText::new("Splines, offset, projected edges and FeatureScript are not implemented.").small().color(MUTED));
            });
        }
        if let Some(index) = self.rename_feature {
            let mut done = false;
            egui::Window::new("Rename feature")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.rename_text).id(egui::Id::new("rename_input")));
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("Rename").clicked() || enter {
                            if index < self.document.features.len() {
                                let before = self.document.clone();
                                self.document.features[index].name = self.rename_text.clone();
                                self.remember(before);
                            }
                            done = true;
                        }
                        if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            done = true;
                        }
                    });
                });
            if done {
                ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("rename_input")));
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
            egui::Window::new("Close CAD?").collapsible(false).show(ctx,|ui|{ui.label("There are unsaved changes. Save your changes before closing to keep them.");ui.horizontal(|ui|{if ui.button("Keep editing").clicked(){self.close_pending=false;}
if ui.button("Discard and close").clicked(){self.allow_close=true;ctx.send_viewport_cmd(egui::ViewportCommand::Close);}});});
        }
    }
}

fn editor_overlay(ctx: &egui::Context, accent: Color32, contents: impl FnOnce(&mut egui::Ui)) -> Rect {
    let viewport = ctx.available_rect();
    egui::Area::new(egui::Id::new("feature_editor_overlay"))
        .order(egui::Order::Middle)
        .movable(false)
        .fixed_pos(viewport.min + egui::vec2(10., 44.))
        .default_size(egui::vec2(244., 180.))
        .constrain_to(viewport)
        .show(ctx, |ui| {
            ui.set_width(244.);
            ui.set_max_height((viewport.height() - 56.).max(180.));
            egui::Frame::new().fill(PANEL).inner_margin(8.)
                .stroke(Stroke::new(1_f32, accent.gamma_multiply(0.6)))
                .show(ui, contents);
        }).response.rect
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
        self.poll_assembly(ctx);
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && (self.dirty() || self.editor.is_some() || self.assembly_context.is_some() || self.assembly_workspace.as_ref().is_some_and(|a| a.dirty() || a.busy()))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_pending = true;
        }
        if self.assembly_mode {
            self.assembly_workspace_ui(ctx);
            self.settings_dialog(ctx);
            if self.close_pending { self.dialogs(ctx); }
            return;
        }
        if let Some(deadline) = self.pending_rebuild {
            let now = Instant::now();
            if now >= deadline {
                self.rebuild();
            } else {
                ctx.request_repaint_after(deadline - now);
            }
        }
        let sketching = self.is_sketching() && !self.choosing_plane;
        let blocked = self.files.is_some()
            || self.variables_open
            || self.plugins_open
            || self.help_open
            || self.settings_open
            || self.shortcut_menu.is_some()
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
        if self.assembly_mode { ctx.request_repaint(); return; }
        self.context_toolbar(ctx);
        self.tree(ctx);
        self.inspector(ctx);
        self.viewport(ctx);
        self.notice_ui(ctx);
        self.dialogs(ctx);
        self.settings_dialog(ctx);
        if let Some(repeat) = self.pending_accept {
            if !self.work.busy() && self.pending_rebuild.is_none() {
                self.pending_accept = None;
                if self.last_error.is_none() && self.editor.is_some() && self.missing_input().is_none() {
                    let id = self.editor.as_ref().unwrap().feature.plugin.clone();
                    self.finish_editor(true);
                    if repeat {
                        self.begin(&id);
                    }
                }
            } else {
                ctx.request_repaint_after(Duration::from_millis(20));
            }
        }
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
    if field.kind == "profiles" { return feature_inputs::profiles_ui(ui, field, params, scene, document); }
    let sketch_reference = params
        .get("sketch")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let value = params
        .as_object_mut()
        .unwrap()
        .entry(field.key.clone())
        .or_insert_with(|| field.default.clone());
    match field.kind.as_str() {
        "bodies" | "profile_face" | "plane_ref" | "axis" => feature_inputs::input_ui(ui, field, value, scene, document),
        "regions" => {
            let before = value.clone();
            let regions = scene.regions.get(&sketch_reference);
            let mut explicit = value.as_array().map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            });
            if ui
                .selectable_label(explicit.is_none(), "Whole sketch")
                .clicked()
            {
                explicit = None;
            }
            if let Some(regions) = regions {
                for (index, region) in regions.iter().enumerate() {
                    let mut selected = explicit
                        .as_ref()
                        .map_or(region.legacy_selected, |keys| keys.iter().any(|key| reference_matches(key, &region.key)));
                    if ui
                        .checkbox(&mut selected, format!("Region {}", index + 1))
                        .on_hover_text(&region.key)
                        .changed()
                    {
                        let keys = explicit.get_or_insert_with(|| {
                            regions
                                .iter()
                                .filter(|r| r.legacy_selected)
                                .map(|r| r.key.clone())
                                .collect()
                        });
                        keys.retain(|k| !reference_matches(k, &region.key));
                        if selected {
                            keys.push(region.key.clone());
                        }
                    }
                }
                if let Some(keys) = &mut explicit {
                    let missing: Vec<_> = keys
                        .iter()
                        .filter(|k| !regions.iter().any(|r| reference_matches(&r.key, k)))
                        .cloned()
                        .collect();
                    for key in missing {
                        ui.horizontal(|ui| {
                            ui.colored_label(RED, "Missing region").on_hover_text(&key);
                            if ui.small_button("Remove").clicked() {
                                keys.retain(|k| k != &key);
                            }
                        });
                    }
                }
                if ui.small_button("Clear").clicked() {
                    explicit = Some(vec![]);
                }
            } else {
                ui.colored_label(MUTED, "No closed regions");
            }
            *value = explicit.map_or(Value::Null, |keys| json!(keys));
            *value != before
        }
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
                "sketch" => std::iter::once((String::new(),"Select sketch".into())).chain(document.features[..index.min(document.features.len())]
                    .iter()
                    .filter(|f| {
                        !f.suppressed
                            && registry
                                .plugins
                                .get(&f.plugin)
                                .is_some_and(|p| p.manifest.editor == "sketch")
                    })
                    .map(|f| (f.id.clone(), f.name.clone())))
                    .collect(),
                _ => std::iter::once((String::new(), "Select part".into()))
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
    fn edit(document: &Document, index: usize, is_new: bool) -> Editor {
        Editor {
            workspace: EditWorkspace {
                camera: Camera::default(), automatic_camera: None, scene: Arc::new(Scene::default()),
                hidden: BTreeSet::new(), hidden_sketches: BTreeSet::new(),
                selected_plane: None, selected_feature: None, selection: vec![],
                selection_filter: Filter::All, show_planes: true, show_sketches: false,
            },
            index, is_new, feature: document.features[index].clone(),
            sketch: document.features[index].sketch(), changed: false,
            input_field: None, undo: vec![], redo: vec![],
        }
    }
    #[test]
    fn edit_camera_restores_only_automatic_framing_and_keeps_manual_navigation() {
        let document = crate::model::demo();
        let mut workspace = edit(&document, 0, false).workspace;
        let mut camera = Camera::default();
        let initial = camera.clone();
        let sketch = crate::sketch::ResolvedSketch {
            entities: vec![crate::sketch::ResolvedEntity {
                id: 1, kind: "rectangle".into(), curve_identity: None,
                values: vec![0., 0., 6., 4.], construction: false, fixed: false,
            }],
            plane: Plane::named("Top", 0.), dof: 0,
        };
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        workspace.align_camera(&mut camera, &sketch, rect);
        assert!(camera != initial);
        workspace.restore_camera(&mut camera);
        assert!(camera == initial);
        for navigate in 0..4 {
            workspace.align_camera(&mut camera, &sketch, rect);
            match navigate {
                0 => camera.pan(egui::vec2(25., -10.), rect),
                1 => camera.span *= 1.4,
                2 => camera.orbit(egui::vec2(30., 20.)),
                _ => camera.fit_sketch_with_rect(&sketch, Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400., 600.))),
            }
            let deliberate = camera.clone();
            workspace.restore_camera(&mut camera);
            assert!(camera == deliberate);
            // A later automatic plane change returns to the last deliberate view.
            workspace.align_camera(&mut camera, &sketch, rect);
            workspace.restore_camera(&mut camera);
            assert!(camera == deliberate);
        }
        let workspace = edit(&document, 1, false).workspace;
        camera.pan(egui::vec2(12., 3.), rect);
        let deliberate = camera.clone();
        workspace.restore_camera(&mut camera);
        assert!(camera == deliberate, "Normal feature edits never own the camera");
    }
    #[test]
    fn stale_async_rectangle_preview_cannot_drop_the_remapped_selected_side() {
        fn frame(ctx: &egui::Context, editor: &mut SketchUi, sketch: &mut Sketch, camera: &Camera,
            cached: &crate::sketch::ResolvedSketch, current: bool, events: Vec<egui::Event>) {
            let _ = ctx.run(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1000.,700.))), events,
                ..Default::default()
            }, |ctx| {
                editor.shortcuts(ctx);
                let resolved=current.then_some(cached);
                egui::Area::new(egui::Id::new("properties")).fixed_pos(egui::Pos2::ZERO).show(ctx, |ui| {
                    ui.set_width(200.);
                    editor.inspector(ui,sketch,resolved,None,&BTreeMap::new(),2);
                });
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
                    let rect=ui.available_rect_before_wrap();
                    let response=ui.allocate_rect(rect,Sense::click_and_drag());
                    editor.canvas(ui,rect,&response,camera,sketch,resolved,Plane::named("Top",0.),&BTreeMap::new(),2,false);
                });
            });
        }
        let ctx=egui::Context::default(); let mut editor=SketchUi::default();
        let mut sketch=Sketch::default();sketch.add_rect([-20.,-10.],[20.,10.]);
        let cached=crate::sketch::resolve(&sketch,Plane::named("Top",0.),&Numbers::new(&BTreeMap::new()).unwrap()).unwrap();
        let mut camera=Camera::default();camera.align(&Plane::named("Top",0.));
        let mut work=WorkState::default();
        let point=camera.project([0.,10.,0.],Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1000.,700.)));
        frame(&ctx,&mut editor,&mut sketch,&camera,&cached,true,vec![]);
        frame(&ctx,&mut editor,&mut sketch,&camera,&cached,true,vec![egui::Event::PointerMoved(point)]);
        for pressed in [true,false] { frame(&ctx,&mut editor,&mut sketch,&camera,&cached,true,vec![
            egui::Event::PointerMoved(point),egui::Event::PointerButton {pos:point,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}
        ]); }
        let q=|pressed|egui::Event::Key {key:egui::Key::Q,physical_key:Some(egui::Key::Q),pressed,repeat:false,modifiers:egui::Modifiers::NONE};
        frame(&ctx,&mut editor,&mut sketch,&camera,&cached,true,vec![q(true)]);
        assert_eq!(sketch.entities.len(),4);
        assert!(sketch.entities[2].construction);
        // The worker's rectangle Scene can remain visible during debounce and
        // regeneration. Neither inspector nor canvas may ingest that stale data.
        frame(&ctx,&mut editor,&mut sketch,&camera,&cached,work.preview_is_current(true),vec![q(false)]);
        work.build=Some(1);
        frame(&ctx,&mut editor,&mut sketch,&camera,&cached,work.preview_is_current(false),vec![]);
        frame(&ctx,&mut editor,&mut sketch,&camera,&cached,work.preview_is_current(false),vec![q(true)]);
        assert!(sketch.entities.iter().all(|e| !e.construction),"Second Q must toggle the same remapped side");
        assert!(work.finish_build(1));
        assert!(!work.preview_is_current(true),"A scheduled newer draft still owns the preview");
        work.exports=1;
        assert!(work.preview_is_current(false),"An unrelated export does not make geometry stale");
        let solved=crate::sketch::resolve(&sketch,Plane::named("Top",0.),&Numbers::new(&BTreeMap::new()).unwrap()).unwrap();
        assert_eq!(crate::sketch::regions(&solved).unwrap().len(),1);
    }
    #[test]
    fn export_completion_cannot_accept_an_unvalidated_edit_preview() {
        let mut work = WorkState { build: Some(2), exports: 1 };
        work.exports -= 1;
        assert!(work.busy(), "Export must not finish a newer preview");
        assert!(!work.finish_build(1), "An obsolete build must not finish the active preview");
        assert!(work.busy());
        assert!(work.finish_build(2));
        assert!(!work.busy());
        work.build = Some(3);
        work.exports = 1;
        assert!(work.finish_build(3));
        assert!(work.busy(), "Build completion must not finish an export");
    }
    #[test]
    fn history_edit_preview_and_commit_preserve_the_whole_document() {
        let original = crate::model::demo();
        let mut session = edit(&original, 0, false);
        session.feature.name = "Edited outline".into();
        let mut preview = original.clone();
        session.apply_to(&mut preview);
        preview.rollback = Some(session.index + 1);
        assert_eq!(original.features.len(), preview.features.len());
        assert_eq!(&original.features[1..], &preview.features[1..]);
        assert_eq!(original.rollback, None, "Preview must not mutate the real history");
        let mut committed = original.clone();
        session.apply_to(&mut committed);
        assert_eq!(committed.rollback, None);
        assert_eq!(&committed.features[1..], &original.features[1..]);
        let mut runtime = crate::engine::Runtime::new(Registry::builtin().unwrap());
        let final_scene = runtime.evaluate(&committed).unwrap().scene;
        assert!(final_scene.error.is_none(), "{:?}", final_scene.error);
        assert_eq!(final_scene.meshes.len(), 1, "Downstream extrusion must return");
        let mut manual = original.clone();
        manual.rollback = Some(1);
        session.apply_to(&mut manual);
        assert_eq!(manual.rollback, Some(1), "Editing preserves a deliberate rollback");
        session.is_new = true;
        session.index = 1;
        session.feature.id = format!("f{}", manual.next_id);
        session.apply_to(&mut manual);
        assert_eq!(manual.rollback, Some(2), "Insertion advances the manual bar");
        assert_eq!(manual.features[2..], original.features[1..]);
    }
    #[test]
    fn document_dialogs_remain_above_the_model_editor_overlay() {
        let ctx=egui::Context::default();
        for _ in 0..3 {
            let mut point=egui::Pos2::ZERO;
            let mut expected=None;
            let _=ctx.run(egui::RawInput { screen_rect:Some(Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1000.,700.))), ..Default::default() }, |ctx| {
                egui::TopBottomPanel::top("tools").exact_height(60.).show(ctx, |_| {});
                egui::SidePanel::left("history").exact_width(188.).show(ctx, |_| {});
                let inspector=editor_overlay(ctx,ACCENT,|ui| {ui.label("Sketch");ui.allocate_space(egui::vec2(200.,200.));});
                let dialog=egui::Window::new("Variables").fixed_pos(inspector.min+egui::vec2(5.,5.))
                    .fixed_size(egui::vec2(240.,180.)).show(ctx, |ui| {ui.label("width");ui.text_edit_singleline(&mut "40".to_owned());}).unwrap();
                point=inspector.intersect(dialog.response.rect).center();
                expected=Some(dialog.response.layer_id);
                egui::CentralPanel::default().show(ctx, |_| {});
            });
            assert_eq!(ctx.layer_id_at(point),expected,"The normal document dialog must receive overlapping pointer input");
        }
    }
    #[test]
    fn editor_overlay_does_not_resize_or_shift_the_model_view() {
        for size in [egui::vec2(1024., 900.), egui::vec2(700., 1000.)] {
            for editing in [false, true] {
                let ctx = egui::Context::default();
                let camera = Camera::default();
                for _ in 0..3 {
                    let _ = ctx.run(egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    }, |ctx| {
                        egui::TopBottomPanel::top("tools").exact_height(60.).show(ctx, |_| {});
                        egui::SidePanel::left("history").exact_width(188.).show(ctx, |_| {});
                        let before = ctx.available_rect();
                        let marker = camera.project([10., 20., 30.], before);
                        if editing {
                            let overlay = editor_overlay(ctx, ACCENT, |ui| {
                                ui.label("Extrude");
                                ui.label("Depth");
                                ui.text_edit_singleline(&mut "10".to_owned());
                                ui.button("Apply").clicked();
                            });
                            assert!(before.contains_rect(overlay), "The editor must stay over the viewport: {before:?} {overlay:?}");
                        }
                        assert_eq!(before, ctx.available_rect());
                        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
                            let viewport = ui.available_rect_before_wrap();
                            assert_eq!(viewport, before);
                            assert!(camera.project([10., 20., 30.], viewport).distance(marker) < 0.001);
                        });
                    });
                }
            }
        }
    }
    #[test]
    fn extensionless_save_and_export_names_detect_the_existing_destination() {
        let directory = PathBuf::from("output").join(format!("dialog-target-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        for (action, filename, extension) in [
            (FileAction::Save, " existing-part ", "cadpart"),
            (FileAction::Export("step".into()), "existing-part", "step"),
        ] {
            let destination = directory.join(format!("existing-part.{extension}"));
            std::fs::write(&destination, b"preserve this file").unwrap();
            let dialog = FileDialog { action, directory: directory.clone(), directory_text: String::new(), filename: filename.into(), error: None };
            assert_eq!(dialog.target_path(), destination);
            assert!(dialog.target_path().is_file(), "The dialog must offer explicit replacement");
        }
    }
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
            hidden: false,
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
