//! Desktop assembly workspace. Geometry and linked-source validation stay on a
//! sleeping worker; pose edits reuse its evaluated source cache.
use crate::{
    assembly::{
        AssemblyDocument, AssemblyRuntime, Connector, Context, ContextBinding, Instance, Limit,
        Mate, MateKind, Source, Transform,
    },
    engine::Scene,
    model::{Document, Result},
    plugins::{Numbers, Registry},
    render::{self, Camera, Renderer},
    selection::{Filter, Selection, Target},
    settings::Settings,
    spacemouse::SpaceMouse,
};
use eframe::egui::{self, Color32, Rect, Sense};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
};

pub enum Action {
    PartStudio,
    Settings,
    VisibleEdges(bool),
    BeginContext {
        context: Context,
        document: Document,
        ghosts: Arc<Scene>,
    },
    BoundContext {
        context: Context,
        document: Document,
    },
    CommittedContext,
}
enum Work {
    Evaluate(AssemblyDocument),
    LoadSource(AssemblyDocument, PathBuf),
    ReloadSources(AssemblyDocument, PathBuf),
    Capture(AssemblyDocument, String, bool),
    Bind(Context, Document, ContextBinding),
    Commit(AssemblyDocument, Context, Document),
    UpdateContext(AssemblyDocument, String),
    Export(AssemblyDocument, String, PathBuf),
}
struct Job {
    serial: u64,
    registry: Arc<Registry>,
    work: Work,
}
struct Ready {
    document: AssemblyDocument,
    scene: Arc<Scene>,
    parts: BTreeMap<String, Vec<PartChoice>>,
    dof: usize,
    finish_context: bool,
    stale_contexts: BTreeSet<String>,
    mate_frames: BTreeMap<String, (Transform, Transform)>,
    poses: BTreeMap<String, Transform>,
}
struct PartChoice {
    key: String,
    name: String,
    min_x: f64,
    width: f64,
}
enum Payload {
    Ready(Ready),
    Context(Context, Document, Arc<Scene>),
    Bound(Context, Document),
    Exported(PathBuf),
}
struct Reply {
    serial: u64,
    result: Result<Payload>,
}
struct Worker {
    send: mpsc::Sender<Job>,
    receive: mpsc::Receiver<Reply>,
}
impl Worker {
    fn new(ctx: egui::Context, registry: Arc<Registry>) -> Self {
        let (send, receive) = mpsc::channel::<Job>();
        let (out, replies) = mpsc::channel();
        std::thread::Builder::new()
            .name("cad-assembly".into())
            .spawn(move || {
                let mut runtime = AssemblyRuntime::new(registry);
                let mut queued = VecDeque::new();
                loop {
                    let mut job = match queued.pop_front().or_else(|| receive.recv().ok()) {
                        Some(job) => job,
                        None => break,
                    };
                    while let Ok(next) = receive.try_recv() {
                        if matches!(
                            (&job.work, &next.work),
                            (Work::Evaluate(_), Work::Evaluate(_))
                        ) && queued.is_empty()
                        {
                            job = next;
                        } else {
                            queued.push_back(next);
                        }
                    }
                    if !Arc::ptr_eq(&runtime.registry, &job.registry) {
                        runtime = AssemblyRuntime::new(job.registry);
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        execute(&mut runtime, job.work)
                    }))
                    .unwrap_or_else(|_| {
                        runtime = AssemblyRuntime::new(runtime.registry.clone());
                        Err(
                            "Assembly operation panicked; the editable workspace is preserved"
                                .into(),
                        )
                    });
                    if out
                        .send(Reply {
                            serial: job.serial,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("assembly worker starts");
        Self {
            send,
            receive: replies,
        }
    }
}
fn ready(
    runtime: &mut AssemblyRuntime,
    mut document: AssemblyDocument,
    finish_context: bool,
) -> Result<Payload> {
    for mate in &mut document.mates {
        runtime.resolve_mate(mate)?;
    }
    let evaluated = runtime.evaluate(&document)?;
    let parts = evaluated
        .sources
        .iter()
        .map(|(id, source)| {
            (
                id.clone(),
                source
                    .scene
                    .meshes
                    .iter()
                    .map(|m| PartChoice {
                        key: m.part.clone(),
                        name: m.name.clone(),
                        min_x: f64::from(m.bounds[0][0]),
                        width: f64::from(m.bounds[1][0] - m.bounds[0][0]),
                    })
                    .collect(),
            )
        })
        .collect();
    let stale_contexts = document
        .contexts
        .iter()
        .filter(|c| c.is_stale(&document))
        .map(|c| c.id.clone())
        .collect();
    let connector_frame = |connector: &Connector| -> Option<Transform> {
        let instance = document
            .instances
            .iter()
            .find(|i| i.id == connector.instance)?;
        let source = evaluated.sources.get(&instance.source)?;
        let local = if let Some(key) = &connector.face {
            *source.face_frames.get(&(
                instance.part.clone(),
                key.split_once("::").map_or(key.as_str(), |(_, k)| k).into(),
            ))?
        } else if let Some(key) = &connector.edge {
            *source.edge_frames.get(&(
                instance.part.clone(),
                key.split_once("::").map_or(key.as_str(), |(_, k)| k).into(),
            ))?
        } else {
            Transform::default()
        };
        Some(
            evaluated
                .poses
                .get(&instance.id)?
                .compose(local.compose(connector.frame)),
        )
    };
    let mate_frames = document
        .mates
        .iter()
        .filter(|m| !m.suppressed)
        .filter_map(|m| {
            Some((
                m.id.clone(),
                (connector_frame(&m.a)?, connector_frame(&m.b)?),
            ))
        })
        .collect();
    Ok(Payload::Ready(Ready {
        document,
        scene: evaluated.scene,
        parts,
        dof: evaluated.degrees_of_freedom,
        finish_context,
        stale_contexts,
        mate_frames,
        poses: evaluated.poses,
    }))
}
fn execute(runtime: &mut AssemblyRuntime, work: Work) -> Result<Payload> {
    match work {
        Work::Evaluate(document) => ready(runtime, document, false),
        Work::Export(document, format, path) => {
            runtime
                .evaluate(&document)?
                .export(&document, &format, &path)?;
            Ok(Payload::Exported(path))
        }
        Work::LoadSource(mut document, path) => {
            let source = Document::load(&path)?;
            let id = document.allocate_id("source");
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            document.sources.push(Source::new(
                id,
                name,
                source,
                Some(path.canonicalize().unwrap_or(path)),
            )?);
            ready(runtime, document, false)
        }
        Work::ReloadSources(mut document, directory) => {
            // Work on one candidate so a later source failure cannot leave a
            // partially refreshed assembly or invalidate the user's saved copy.
            for source in &mut document.sources {
                if let Some(path) = &source.path {
                    let path = if path.is_absolute() {
                        path.clone()
                    } else {
                        directory.join(path)
                    };
                    source.document = Document::load(&path)?;
                    source.refresh_revision()?;
                }
            }
            ready(runtime, document, false)
        }
        Work::Capture(document, primary, new_context) => {
            let context = if !new_context {
                document
                    .contexts
                    .iter()
                    .rev()
                    .find(|c| c.primary_instance == primary)
                    .cloned()
            } else {
                None
            };
            let context = match context {
                Some(c) => c,
                None => runtime.capture_context(&document, &primary)?,
            };
            let source = document
                .sources
                .iter()
                .find(|s| s.id == context.primary_source)
                .ok_or("Missing primary source")?
                .document
                .clone();
            let ghosts = runtime.context_scene(&context)?;
            Ok(Payload::Context(context, source, ghosts))
        }
        Work::Bind(mut context, mut document, binding) => {
            runtime.bind_context(&mut context, &mut document, binding)?;
            Ok(Payload::Bound(context, document))
        }
        Work::Commit(mut assembly, context, document) => {
            runtime.update_source(&mut assembly, &context.primary_source, document)?;
            if let Some(existing) = assembly.contexts.iter_mut().find(|c| c.id == context.id) {
                *existing = context;
            } else {
                assembly.contexts.push(context);
            }
            ready(runtime, assembly, true)
        }
        Work::UpdateContext(mut document, id) => {
            runtime.update_context(&mut document, &id)?;
            ready(runtime, document, false)
        }
    }
}

enum History {
    Record,
    Undo,
    Redo,
    Load(PathBuf),
}
struct Pending {
    before: AssemblyDocument,
    history: History,
}
#[derive(Clone, Copy)]
enum FileKind {
    Open,
    Save,
    Insert,
    Export,
}
struct FileDialog {
    kind: FileKind,
    directory: PathBuf,
    directory_text: String,
    filename: String,
    error: Option<String>,
    format: String,
}
impl FileDialog {
    fn extension(&self) -> &str {
        match self.kind {
            FileKind::Insert => "cadpart",
            FileKind::Export => &self.format,
            _ => "cadasm",
        }
    }
    fn target_path(&self) -> PathBuf {
        let mut path = self.directory.join(self.filename.trim());
        if path.extension().is_none() {
            path.set_extension(self.extension());
        }
        path
    }
}
#[derive(Clone)]
struct MateEditor {
    mate: Mate,
    is_new: bool,
}

pub struct AssemblyUi {
    document: AssemblyDocument,
    saved: AssemblyDocument,
    path: Option<PathBuf>,
    past: VecDeque<AssemblyDocument>,
    future: Vec<AssemblyDocument>,
    scene: Arc<Scene>,
    parts: BTreeMap<String, Vec<PartChoice>>,
    registry: Arc<Registry>,
    worker: Worker,
    serial: u64,
    revision: u64,
    pending: Option<Pending>,
    busy: bool,
    error: Option<String>,
    selection: Selection,
    filter: Filter,
    hidden: BTreeSet<String>,
    viewport: Rect,
    orbiting: bool,
    fit: bool,
    dof: usize,
    insert_open: bool,
    files: Option<FileDialog>,
    pending_open: Option<PathBuf>,
    mate_editor: Option<MateEditor>,
    selected_mate: Option<String>,
    search_open: bool,
    search: String,
    search_focus: bool,
    box_start: Option<egui::Pos2>,
    stale_contexts: BTreeSet<String>,
    mate_frames: BTreeMap<String, (Transform, Transform)>,
    show_mates: bool,
    poses: BTreeMap<String, Transform>,
    applying_mate: Option<MateEditor>,
    exported: Option<PathBuf>,
}
impl AssemblyUi {
    pub fn new(ctx: &egui::Context, registry: Arc<Registry>) -> Self {
        Self {
            document: AssemblyDocument::default(),
            saved: AssemblyDocument::default(),
            path: None,
            past: VecDeque::new(),
            future: vec![],
            scene: Arc::new(Scene::default()),
            parts: BTreeMap::new(),
            worker: Worker::new(ctx.clone(), registry.clone()),
            registry,
            serial: 0,
            revision: 1_000_000,
            pending: None,
            busy: false,
            error: None,
            selection: Selection::default(),
            filter: Filter::All,
            hidden: BTreeSet::new(),
            viewport: Rect::NOTHING,
            orbiting: false,
            fit: true,
            dof: 0,
            insert_open: false,
            files: None,
            pending_open: None,
            mate_editor: None,
            selected_mate: None,
            search_open: false,
            search: String::new(),
            search_focus: false,
            box_start: None,
            stale_contexts: BTreeSet::new(),
            mate_frames: BTreeMap::new(),
            show_mates: false,
            poses: BTreeMap::new(),
            applying_mate: None,
            exported: None,
        }
    }
    pub fn dirty(&self) -> bool {
        self.document != self.saved
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    pub fn poll(&mut self) -> Vec<Action> {
        let mut actions = vec![];
        while let Ok(reply) = self.worker.receive.try_recv() {
            if reply.serial != self.serial {
                continue;
            }
            self.busy = false;
            match reply.result {
                Err(error) => {
                    self.pending = None;
                    if let Some(editor) = self.applying_mate.take() {
                        self.mate_editor = Some(editor);
                    }
                    self.error = Some(error);
                }
                Ok(Payload::Ready(ready)) => {
                    self.applying_mate = None;
                    if let Some(pending) = self.pending.take() {
                        match pending.history {
                            History::Record => {
                                if pending.before != ready.document {
                                    self.past.push_back(pending.before);
                                    self.future.clear();
                                }
                            }
                            History::Undo => {
                                self.past.pop_back();
                                self.future.push(pending.before);
                            }
                            History::Redo => {
                                self.future.pop();
                                self.past.push_back(pending.before);
                            }
                            History::Load(path) => {
                                self.saved = ready.document.clone();
                                self.path = Some(path);
                                self.past.clear();
                                self.future.clear();
                                self.hidden.clear();
                                self.selection.clear();
                                self.fit = true;
                            }
                        }
                    }
                    while self.past.len() > 60 {
                        self.past.pop_front();
                    }
                    self.document = ready.document;
                    self.scene = ready.scene;
                    self.parts = ready.parts;
                    self.dof = ready.dof;
                    self.stale_contexts = ready.stale_contexts;
                    self.mate_frames = ready.mate_frames;
                    self.poses = ready.poses;
                    self.revision += 1;
                    self.error = None;
                    let ids: BTreeSet<_> = self
                        .document
                        .instances
                        .iter()
                        .map(|i| i.id.as_str())
                        .collect();
                    self.selection.retain(|t| match t {
                        Target::Part(id)
                        | Target::Face { part: id, .. }
                        | Target::Edge { part: id, .. } => ids.contains(id.as_str()),
                        _ => false,
                    });
                    if ready.finish_context {
                        actions.push(Action::CommittedContext);
                    }
                }
                Ok(Payload::Context(context, document, ghosts)) => {
                    actions.push(Action::BeginContext {
                        context,
                        document,
                        ghosts,
                    })
                }
                Ok(Payload::Exported(path)) => {
                    self.exported = Some(path);
                    self.error = None;
                }
                Ok(Payload::Bound(context, document)) => {
                    actions.push(Action::BoundContext { context, document })
                }
            }
        }
        actions
    }
    fn send(&mut self, work: Work, history: Option<History>) {
        if self.busy {
            return;
        }
        self.serial += 1;
        if let Some(history) = history {
            self.pending = Some(Pending {
                before: self.document.clone(),
                history,
            });
        }
        self.busy = true;
        self.error = None;
        if self
            .worker
            .send
            .send(Job {
                serial: self.serial,
                registry: self.registry.clone(),
                work,
            })
            .is_err()
        {
            self.busy = false;
            self.error = Some("Assembly worker stopped".into());
        }
    }
    fn submit(&mut self, document: AssemblyDocument) {
        self.send(Work::Evaluate(document), Some(History::Record));
    }
    pub fn bind_context(&mut self, context: Context, document: Document, binding: ContextBinding) {
        self.send(Work::Bind(context, document, binding), None);
    }
    pub fn commit_context(&mut self, context: Context, document: Document) {
        self.send(
            Work::Commit(self.document.clone(), context, document),
            Some(History::Record),
        );
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn open(&mut self, path: PathBuf) {
        if self.busy {
            return;
        }
        if self.dirty() {
            self.pending_open = Some(path);
            return;
        }
        self.load(path);
    }
    fn load(&mut self, path: PathBuf) {
        match AssemblyDocument::load(&path) {
            Ok(document) => self.send(Work::Evaluate(document), Some(History::Load(path))),
            Err(error) => self.error = Some(error),
        }
    }
    fn directory(&self) -> PathBuf {
        self.path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    }
    fn file(&mut self, kind: FileKind) {
        let directory = self.directory();
        self.files = Some(FileDialog {
            directory_text: directory.display().to_string(),
            directory,
            filename: if matches!(kind, FileKind::Save) {
                self.path
                    .as_deref()
                    .and_then(Path::file_name)
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                String::new()
            },
            kind,
            format: String::new(),
            error: None,
        });
    }
    fn exporter(&mut self, id: &str) {
        let result = (|| {
            let feature = self.registry.create(id, &mut Document::default())?;
            let plan = self
                .registry
                .evaluate(&feature, &Numbers::new(&BTreeMap::new())?)?;
            if plan["op"].as_str() != Some("export") {
                return Err("Exporter must return an export operation".into());
            }
            plan["format"]
                .as_str()
                .filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric()) && !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| "Exporter did not declare a format".into())
        })();
        match result {
            Ok(format) => {
                self.file(FileKind::Export);
                let dialog = self.files.as_mut().unwrap();
                dialog.format = format;
                dialog.filename = format!("{}.{}", self.document.name, dialog.format);
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn save(&mut self) {
        if self.busy {
            return;
        }
        let Some(path) = self.path.clone() else {
            self.file(FileKind::Save);
            return;
        };
        let result = self
            .saved
            .revision()
            .and_then(|expected| self.document.save_checked(&path, Some(&expected)));
        match result {
            Ok(()) => {
                self.saved = self.document.clone();
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
    pub fn save_workspace(&mut self) {
        self.save();
    }
    fn save_as(&mut self, path: PathBuf) -> Result<()> {
        if self.busy {
            return Err("Wait for the current assembly operation".into());
        }
        let same = self.path.as_ref().is_some_and(|old| same_path(old, &path));
        let expected = if same {
            Some(self.saved.revision()?)
        } else if path.exists() {
            Some(AssemblyDocument::load(&path)?.revision()?)
        } else {
            None
        };
        let mut candidate = self.document.clone();
        let directory = self.directory();
        if !same {
            absolute_source_links(&mut candidate, &directory);
        }
        candidate.save_checked(&path, expected.as_deref())?;
        if !same {
            for document in self.past.iter_mut().chain(self.future.iter_mut()) {
                absolute_source_links(document, &directory);
            }
        }
        self.document = candidate;
        self.saved = self.document.clone();
        self.path = Some(path);
        Ok(())
    }
    fn undo(&mut self, redo: bool) {
        if self.busy {
            return;
        }
        let document = if redo {
            self.future.last()
        } else {
            self.past.back()
        }
        .cloned();
        if let Some(document) = document {
            self.send(
                Work::Evaluate(document),
                Some(if redo { History::Redo } else { History::Undo }),
            );
        }
    }
    fn selected_instances(&self) -> Vec<String> {
        let mut result = vec![];
        for item in self.selection.items() {
            let id = match item {
                Target::Part(id)
                | Target::Face { part: id, .. }
                | Target::Edge { part: id, .. } => id,
                _ => continue,
            };
            if !result.contains(id) {
                result.push(id.clone());
            }
        }
        result
    }
    fn begin_mate(&mut self) {
        let connectors: Vec<_> = self
            .selection
            .items()
            .iter()
            .filter_map(|t| match t {
                Target::Face { part, key } => Some(Connector::on_face(
                    part,
                    key.split_once("::").map_or(key.as_str(), |(_, key)| key),
                )),
                Target::Part(id) => Some(Connector::origin(id)),
                Target::Edge { part, key } => Some(Connector::on_edge(part, key)),
                _ => None,
            })
            .collect();
        let mut unique = vec![];
        for connector in connectors {
            if !unique
                .iter()
                .any(|c: &Connector| c.instance == connector.instance)
            {
                unique.push(connector);
            }
        }
        if unique.len() != 2 {
            self.error = Some(
                "Select a face, circular edge or instance on each of two different parts".into(),
            );
            return;
        }
        let mut mate = Mate::new(MateKind::Fastened, unique.remove(0), unique.remove(0));
        if mate.a.face.is_some() && mate.b.face.is_some() {
            mate.offset = Transform::from_euler_degrees([180., 0., 0.]);
        }
        let mut scratch = self.document.clone();
        mate.id = scratch.allocate_id("mate");
        mate.plugin = self
            .registry
            .create("mate-fastened", &mut Document::default())
            .ok();
        self.mate_editor = Some(MateEditor { mate, is_new: true });
    }
    fn fix_selected(&mut self) {
        let selected = self.selected_instances();
        if selected.is_empty() {
            return;
        }
        let mut doc = self.document.clone();
        let fix = doc
            .instances
            .iter()
            .filter(|i| selected.contains(&i.id))
            .any(|i| !i.fixed);
        for instance in &mut doc.instances {
            if let Some(pose) = self.poses.get(&instance.id) {
                instance.transform = *pose;
            }
            if selected.contains(&instance.id) {
                instance.fixed = fix;
            }
        }
        self.submit(doc);
    }
    fn capture(&mut self) {
        self.capture_new(false);
    }
    fn capture_new(&mut self, new_context: bool) {
        let ids = self.selected_instances();
        if ids.len() != 1 {
            self.error = Some("Select one instance to edit in context".into());
            return;
        }
        self.send(
            Work::Capture(self.document.clone(), ids[0].clone(), new_context),
            None,
        );
    }
    fn update_sources(&mut self) {
        self.send(
            Work::ReloadSources(self.document.clone(), self.directory()),
            Some(History::Record),
        );
    }
    fn update_context(&mut self) {
        let primary = self.selected_instances();
        let context = self
            .document
            .contexts
            .iter()
            .rev()
            .find(|c| primary.is_empty() || primary.contains(&c.primary_instance));
        if let Some(context) = context {
            self.send(
                Work::UpdateContext(self.document.clone(), context.id.clone()),
                Some(History::Record),
            );
        } else {
            self.error = Some("Select an instance with an assembly context".into());
        }
    }
}

pub struct Host<'a> {
    pub camera: &'a mut Camera,
    pub space: &'a mut SpaceMouse,
    pub settings: &'a Settings,
    pub renderer: Arc<Mutex<Renderer>>,
    pub registry: Arc<Registry>,
    pub document: &'a Document,
    pub path: Option<&'a Path>,
    pub scene: &'a Scene,
    pub part_ready: bool,
    pub shaded_edges: bool,
    pub blocked: bool,
}
fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}
fn face_ids(scene: &Scene, selection: &Selection) -> Vec<usize> {
    let parts: BTreeSet<_> = selection
        .items()
        .iter()
        .filter_map(|t| {
            if let Target::Part(id) = t {
                Some(id.as_str())
            } else {
                None
            }
        })
        .collect();
    let faces: BTreeSet<_> = selection.faces().collect();
    scene
        .meshes
        .iter()
        .flat_map(|mesh| {
            mesh.faces
                .iter()
                .filter(|face| {
                    parts.contains(mesh.part.as_str())
                        || faces.contains(&(mesh.part.as_str(), face.key.as_str()))
                })
                .map(|face| face.global_id)
        })
        .collect()
}

impl AssemblyUi {
    pub fn show(&mut self, ctx: &egui::Context, mut host: Host<'_>) -> Vec<Action> {
        self.registry = host.registry.clone();
        let mut actions = vec![];
        if !host.blocked {
            self.keys(ctx, host.camera, &mut actions);
        }
        egui::TopBottomPanel::top("assembly_header")
            .frame(
                egui::Frame::new()
                    .fill(rgb(host.settings.appearance.panel))
                    .inner_margin(egui::Margin::symmetric(8, 3)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Part Studio").clicked() {
                        actions.push(Action::PartStudio);
                    }
                    let _ = ui.selectable_label(true, "Assembly");
                    ui.label(format!(
                        "{}{}",
                        self.document.name,
                        if self.dirty() { " *" } else { "" }
                    ));
                    if ui
                        .button("Search")
                        .on_hover_text("Search commands, instances and mates \u{00b7} S")
                        .clicked()
                    {
                        self.search_open = true;
                        self.search_focus = true;
                    }
                    if ui.button("Settings").clicked() {
                        actions.push(Action::Settings);
                    }
                    if self.busy {
                        ui.spinner();
                    } else if !self.document.instances.is_empty() {
                        ui.weak(format!("{} DOF", self.dof));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add_enabled_ui(!self.busy, |ui| {
                        if ui.button("Insert (I)").clicked() {
                            self.insert_open = true;
                        }
                        if ui.button("Mate (M)").clicked() {
                            self.begin_mate();
                        }
                        if ui
                            .add_enabled(
                                !self.selected_instances().is_empty(),
                                egui::Button::new("Fix / unfix"),
                            )
                            .clicked()
                        {
                            self.fix_selected();
                        }
                        let edit_context = ui.add_enabled(
                            self.selected_instances().len() == 1,
                            egui::Button::new("Edit in context"),
                        );
                        edit_context.context_menu(|ui| {
                            if ui.button("New context").clicked() {
                                self.capture_new(true);
                                ui.close();
                            }
                        });
                        if edit_context.clicked() {
                            self.capture();
                        }
                        if ui.button("Update sources").clicked() {
                            self.update_sources();
                        }
                        if ui
                            .add_enabled(
                                !self.document.contexts.is_empty(),
                                egui::Button::new("Update context"),
                            )
                            .clicked()
                        {
                            self.update_context();
                        }
                        if ui.button("Open").clicked() {
                            self.file(FileKind::Open);
                        }
                        if ui.button("Save").clicked() {
                            self.save();
                        }
                        if ui.button("Save as").clicked() {
                            self.file(FileKind::Save);
                        }
                        ui.menu_button("Export", |ui| {
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
                                        !self.document.instances.is_empty(),
                                        egui::Button::new(name),
                                    )
                                    .clicked()
                                {
                                    self.exporter(&id);
                                    ui.close();
                                }
                            }
                        });
                        if ui
                            .add_enabled(!self.past.is_empty(), egui::Button::new("Undo"))
                            .clicked()
                        {
                            self.undo(false);
                        }
                        if ui
                            .add_enabled(!self.future.is_empty(), egui::Button::new("Redo"))
                            .clicked()
                        {
                            self.undo(true);
                        }
                    });
                });
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::from_rgb(220, 137, 150), error);
                }
                if let Some(path) = self.exported.clone() {
                    ui.horizontal(|ui| {
                        ui.weak(format!(
                            "Exported {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ));
                        if ui.small_button("Dismiss").clicked() {
                            self.exported = None;
                        }
                    });
                }
            });
        if host.settings.tree_open {
            self.sidebar(ctx);
        }
        if let Some(value) = self.canvas(ctx, &mut host) {
            actions.push(Action::VisibleEdges(value));
        }
        self.insert_dialog(ctx, host.document, host.path, host.scene, host.part_ready);
        self.mate_dialog(ctx);
        self.file_dialog(ctx);
        self.search_dialog(ctx, &mut actions);
        if let Some(path) = self.pending_open.clone() {
            egui::Window::new("Unsaved assembly")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Save the current assembly before replacing it.");
                    ui.horizontal(|ui| {
                        if ui.button("Keep editing").clicked() {
                            self.pending_open = None;
                        }
                        if ui.button("Discard and open").clicked() {
                            self.pending_open = None;
                            self.load(path);
                        }
                    });
                });
        }
        actions
    }
    fn keys(&mut self, ctx: &egui::Context, camera: &mut Camera, actions: &mut Vec<Action>) {
        if ctx.wants_keyboard_input()
            || self.busy
            || self.pending_open.is_some()
            || self.files.is_some()
            || self.insert_open
            || self.mate_editor.is_some()
            || self.search_open
        {
            return;
        }
        for event in ctx.input(|i| i.events.clone()) {
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
            if modifiers.command || modifiers.ctrl {
                match key {
                    egui::Key::S => self.save(),
                    egui::Key::O => self.file(FileKind::Open),
                    egui::Key::Z => self.undo(modifiers.shift),
                    egui::Key::Y => self.undo(true),
                    _ => (),
                };
                continue;
            }
            if !modifiers.shift && !modifiers.alt {
                match key {
                    egui::Key::I => {
                        self.insert_open = true;
                        continue;
                    }
                    egui::Key::M => {
                        self.begin_mate();
                        continue;
                    }
                    egui::Key::J => {
                        self.show_mates = !self.show_mates;
                        continue;
                    }
                    egui::Key::Delete | egui::Key::Backspace => {
                        self.remove_selected();
                        continue;
                    }
                    _ => (),
                }
            }
            if let Some(command) = crate::shortcuts::command_for_event(
                key,
                physical_key,
                modifiers,
                crate::shortcuts::Context::PartStudio,
            ) {
                use crate::shortcuts::{Command, View};
                match command {
                    Command::ClearSelection => self.selection.clear(),
                    Command::Cancel => {
                        self.selection.clear();
                        self.error = None;
                    }
                    Command::Fit => self.fit = true,
                    Command::ShortcutToolbar | Command::SearchTools => {
                        self.search_open = true;
                        self.search_focus = true;
                    }
                    Command::View(view) => camera.set_view(match view {
                        View::Front => glam::Vec3::NEG_Y,
                        View::Back => glam::Vec3::Y,
                        View::Left => glam::Vec3::NEG_X,
                        View::Right => glam::Vec3::X,
                        View::Top => glam::Vec3::Z,
                        View::Bottom => glam::Vec3::NEG_Z,
                        View::Isometric => glam::Vec3::new(1., -1., 1.),
                    }),
                    Command::Normal => {
                        let target = self
                            .selection
                            .faces()
                            .next()
                            .and_then(|(part, key)| {
                                self.scene
                                    .meshes
                                    .iter()
                                    .find(|m| m.part == part)
                                    .and_then(|m| m.faces.iter().find(|f| f.key == key))
                                    .and_then(|f| f.plane.clone())
                            })
                            .or_else(|| {
                                ctx.input(|i| i.pointer.hover_pos())
                                    .and_then(|p| {
                                        render::pick(
                                            &self.scene,
                                            camera,
                                            p,
                                            self.viewport,
                                            &self.hidden,
                                        )
                                    })
                                    .and_then(|p| p.plane)
                            });
                        if let Some(plane) = target {
                            let normal = glam::Vec3::from_array(plane.normal.map(|v| v as f32));
                            if (camera.rotation * glam::Vec3::Z).dot(normal) > 0.999 {
                                camera.set_view(-normal);
                            } else {
                                camera.align(&plane);
                            }
                        }
                    }
                    Command::HideSelectedOrHovered => {
                        let ids = self.selected_instances();
                        if ids.is_empty() {
                            if let Some(pick) = ctx.input(|i| i.pointer.hover_pos()).and_then(|p| {
                                render::pick(&self.scene, camera, p, self.viewport, &self.hidden)
                            }) {
                                self.hidden.insert(pick.part);
                            }
                        } else {
                            self.hidden.extend(ids);
                        }
                        self.revision += 1;
                    }
                    Command::ShowHidden => {
                        self.hidden.clear();
                        self.revision += 1;
                    }
                    Command::ZoomIn => camera.span = (camera.span / 1.2).max(0.02),
                    Command::ZoomOut => camera.span = (camera.span * 1.2).min(1e6),
                    Command::Rotate {
                        horizontal,
                        vertical,
                        degrees,
                    } => camera.rotate_to(
                        camera.rotation
                            * glam::Quat::from_scaled_axis(
                                glam::Vec3::new(-(vertical as f32), -(horizontal as f32), 0.)
                                    * (degrees as f32).to_radians(),
                            ),
                    ),
                    Command::Pan {
                        horizontal,
                        vertical,
                    } => camera.pan(
                        egui::vec2(horizontal as f32, -(vertical as f32))
                            * self.viewport.height()
                            * 0.05,
                        self.viewport,
                    ),
                    Command::Help => actions.push(Action::Settings),
                    _ => (),
                }
            }
        }
    }
    fn remove_selected(&mut self) {
        if self.busy {
            return;
        }
        let ids = self.selected_instances();
        let mut doc = self.document.clone();
        doc.instances.retain(|i| !ids.contains(&i.id));
        doc.mates.retain(|m| {
            !ids.contains(&m.a.instance)
                && !ids.contains(&m.b.instance)
                && self.selected_mate.as_deref() != Some(m.id.as_str())
        });
        doc.contexts.retain(|c| !ids.contains(&c.primary_instance));
        self.submit(doc);
    }
    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("assembly_tree")
            .default_width(188.)
            .width_range(150. ..=340.)
            .show(ctx, |ui| {
                let mut changed_visibility = false;
                let selected_ids = self.selected_instances();
                ui.strong("Instances");
                for instance in &self.document.instances {
                    ui.horizontal(|ui| {
                        let mut visible = !self.hidden.contains(&instance.id);
                        if ui.checkbox(&mut visible, "").changed() {
                            if visible {
                                self.hidden.remove(&instance.id);
                            } else {
                                self.hidden.insert(instance.id.clone());
                            }
                            changed_visibility = true;
                        }
                        let selected = selected_ids.contains(&instance.id);
                        if ui
                            .selectable_label(
                                selected,
                                format!(
                                    "{}{}",
                                    instance.name,
                                    if instance.fixed {
                                        " \u{00b7} Fixed"
                                    } else {
                                        ""
                                    }
                                ),
                            )
                            .clicked()
                        {
                            self.selection.toggle(Target::Part(instance.id.clone()));
                            self.selected_mate = None;
                        }
                    });
                }
                if changed_visibility {
                    self.revision += 1;
                }
                ui.separator();
                ui.strong("Mates");
                for mate in &self.document.mates {
                    let response = ui.selectable_label(
                        self.selected_mate.as_deref() == Some(&mate.id),
                        format!(
                            "{:?}{}",
                            mate.kind,
                            if mate.suppressed {
                                " \u{00b7} Suppressed"
                            } else {
                                ""
                            }
                        ),
                    );
                    if response.clicked() {
                        self.selected_mate = Some(mate.id.clone());
                        self.selection.clear();
                    }
                    if response.double_clicked() {
                        self.mate_editor = Some(MateEditor {
                            mate: mate.clone(),
                            is_new: false,
                        });
                    }
                }
                let chosen = self.selected_instances();
                if chosen.len() == 1 && !self.busy {
                    let mut doc = self.document.clone();
                    if let Some(instance) = doc.instances.iter_mut().find(|i| i.id == chosen[0]) {
                        ui.separator();
                        let mut changed = ui.text_edit_singleline(&mut instance.name).changed();
                        let controlled = !instance.fixed
                            && doc.mates.iter().any(|m| {
                                !m.suppressed
                                    && (m.a.instance == instance.id || m.b.instance == instance.id)
                            });
                        if controlled {
                            ui.weak("Position controlled by mates");
                        }
                        ui.add_enabled_ui(!controlled, |ui| {
                            changed |=
                                coordinates(ui, "Position", &mut instance.transform.translation);
                            let q = glam::DQuat::from_array(instance.transform.rotation);
                            let (x, y, z) = q.to_euler(glam::EulerRot::XYZEx);
                            let mut degrees = [x.to_degrees(), y.to_degrees(), z.to_degrees()];
                            if coordinates(ui, "Rotation deg", &mut degrees) {
                                let mut transform = Transform::from_euler_degrees(degrees);
                                transform.translation = instance.transform.translation;
                                instance.transform = transform;
                                changed = true;
                            }
                        });
                        if changed {
                            self.submit(doc);
                        }
                    }
                }
                if !self.document.contexts.is_empty() {
                    ui.separator();
                    ui.strong("Contexts");
                    for context in &self.document.contexts {
                        ui.label(format!(
                            "{}{}",
                            context.name,
                            if self.stale_contexts.contains(&context.id) {
                                " \u{00b7} Update available"
                            } else {
                                ""
                            }
                        ));
                    }
                }
            });
    }
    fn canvas(&mut self, ctx: &egui::Context, host: &mut Host<'_>) -> Option<bool> {
        let camera = &mut *host.camera;
        let space = &mut *host.space;
        let settings = host.settings;
        let renderer = host.renderer.clone();
        let shaded_edges = host.shaded_edges;
        let host_blocked = host.blocked;
        let mut changed_edges = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(rgb(settings.appearance.background)))
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                self.viewport = rect;
                if self.fit && !self.scene.meshes.is_empty() {
                    camera.fit_with_rect(&self.scene, rect);
                    self.fit = false;
                }
                let blocked = host_blocked
                    || self.pending_open.is_some()
                    || self.files.is_some()
                    || self.insert_open
                    || self.mate_editor.is_some()
                    || self.search_open
                    || ctx.wants_keyboard_input();
                if space.update(ctx, camera, false, blocked, |c| {
                    render::automatic_pivot(&self.scene, c, rect, &self.hidden)
                }) {
                    self.fit = true;
                }
                let pointer = response.hover_pos();
                let controls = crate::navigation::bounds(rect);
                let canvas_hover = pointer.is_some_and(|p| {
                    !controls.contains(p)
                        && !Rect::from_min_size(
                            rect.min + egui::vec2(8., 8.),
                            egui::vec2(170., 28.),
                        )
                        .contains(p)
                }) && !blocked;
                let orbiting = canvas_hover
                    && response.hovered()
                    && ui.input(|i| {
                        i.pointer.button_down(egui::PointerButton::Secondary)
                            && !i.modifiers.shift
                            && !i.modifiers.ctrl
                    });
                if orbiting && !self.orbiting {
                    camera.pivot = ui
                        .input(|i| i.pointer.press_origin())
                        .or(pointer)
                        .and_then(|p| render::pick(&self.scene, camera, p, rect, &self.hidden))
                        .map(|p| p.point)
                        .or_else(|| render::model_center(&self.scene, &self.hidden))
                        .unwrap_or(camera.center);
                }
                self.orbiting = orbiting;
                if response.hovered() && canvas_hover {
                    let delta = ui.input(|i| i.pointer.delta());
                    let right = ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary));
                    let middle = ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
                    if middle || (right && ui.input(|i| i.modifiers.shift || i.modifiers.ctrl)) {
                        camera.pan(delta * settings.mouse_pan_speed, rect);
                    } else if right {
                        if ui.input(|i| i.modifiers.alt) {
                            camera.orbit_upright(delta * settings.mouse_rotation_speed);
                        } else {
                            camera.orbit(delta * settings.mouse_rotation_speed);
                        }
                    }
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll.abs() > 0.01 {
                        let before = camera.span;
                        camera.span = (before
                            * (-scroll * 0.0025 * settings.mouse_zoom_speed).exp())
                        .clamp(0.02, 1e6);
                        if let Some(p) = pointer {
                            camera.center += camera.rotation
                                * glam::Vec3::new(p.x - rect.center().x, rect.center().y - p.y, 0.)
                                * ((before - camera.span) / rect.height());
                        }
                    }
                }
                let hit = pointer.filter(|_| canvas_hover).and_then(|pos| {
                    pick_target(&self.scene, camera, pos, rect, self.filter, &self.hidden)
                });
                if response.clicked() && canvas_hover {
                    if let Some(hit) = hit.clone() {
                        self.selection.toggle(hit);
                    } else {
                        self.selection.clear();
                    }
                    self.selected_mate = None;
                }
                if response.drag_started_by(egui::PointerButton::Primary) && canvas_hover {
                    self.box_start = ui.input(|i| i.pointer.press_origin());
                }
                if let Some(start) = self.box_start
                    && let Some(end) = ui.input(|i| i.pointer.interact_pos())
                {
                    let area = Rect::from_two_pos(start, end);
                    let crossing = end.x < start.x;
                    if area.width() > 3. || area.height() > 3. {
                        ui.painter().rect_stroke(
                            area,
                            0.,
                            egui::Stroke::new(
                                1_f32,
                                rgb(if crossing {
                                    settings.appearance.purple
                                } else {
                                    settings.appearance.turquoise
                                }),
                            ),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if response.drag_stopped_by(egui::PointerButton::Primary) {
                        self.box_start = None;
                        for target in render::box_targets(
                            &self.scene,
                            camera,
                            rect,
                            area,
                            crossing,
                            self.filter,
                            &self.hidden,
                        ) {
                            if ui.input(|i| i.modifiers.ctrl) {
                                self.selection.remove(&target);
                            } else {
                                self.selection.add(target);
                            }
                        }
                    }
                }
                let mut hovered = Selection::default();
                if let Some(hit) = hit {
                    hovered.add(hit);
                }
                ui.painter().add(render::paint_callback(
                    rect,
                    renderer,
                    self.scene.clone(),
                    self.hidden.clone(),
                    self.revision,
                    render::View {
                        matrix: camera.matrix(rect),
                        edited: Vec::new(),
                        selected: face_ids(&self.scene, &self.selection),
                        hovered: face_ids(&self.scene, &hovered),
                        selected_edges: self
                            .selection
                            .edges()
                            .map(|(p, k)| (p.into(), k.into()))
                            .collect(),
                        hovered_edges: hovered.edges().map(|(p, k)| (p.into(), k.into())).collect(),
                        shaded_edges,
                        selection_color: settings.appearance.turquoise.map(|v| f32::from(v) / 255.),
                        hover_color: settings.appearance.purple.map(|v| f32::from(v) / 255.),
                        ghost_parts: BTreeSet::new(),
                    },
                ));
                for (id, (a, b)) in &self.mate_frames {
                    if self.show_mates || self.selected_mate.as_ref() == Some(id) {
                        let color = rgb(settings.appearance.purple);
                        let pa = camera.project(a.translation, rect);
                        let pb = camera.project(b.translation, rect);
                        ui.painter()
                            .line_segment([pa, pb], egui::Stroke::new(1_f32, color));
                        for frame in [a, b] {
                            let origin = camera.project(frame.translation, rect);
                            ui.painter().circle_stroke(
                                origin,
                                4.,
                                egui::Stroke::new(1.5_f32, color),
                            );
                            for (axis, c) in [
                                ([1., 0., 0.], Color32::from_rgb(195, 130, 140)),
                                ([0., 1., 0.], Color32::from_rgb(125, 180, 160)),
                                ([0., 0., 1.], Color32::from_rgb(140, 155, 200)),
                            ] {
                                let end =
                                    frame.point(axis.map(|x| x * f64::from(camera.span) * 0.045));
                                ui.painter().line_segment(
                                    [origin, camera.project(end, rect)],
                                    egui::Stroke::new(1.5_f32, c),
                                );
                            }
                        }
                    }
                }
                let mut edges = shaded_edges;
                let pivot =
                    render::model_center(&self.scene, &self.hidden).unwrap_or(camera.center);
                if crate::navigation::show(ui, rect, camera, &mut edges, space, pivot) {
                    self.fit = true;
                }
                if edges != shaded_edges {
                    changed_edges = Some(edges);
                }
                if !self.selection.items().is_empty() {
                    let text = format!("{} selected", self.selection.items().len());
                    let pos = rect.left_bottom() + egui::vec2(12., -12.);
                    ui.painter().text(
                        pos,
                        egui::Align2::LEFT_BOTTOM,
                        text,
                        egui::FontId::proportional(12.),
                        rgb(settings.appearance.turquoise),
                    );
                }
            });
        egui::Area::new(egui::Id::new("assembly_filter"))
            .fixed_pos(self.viewport.min + egui::vec2(8., 8.))
            .show(ctx, |ui| {
                ui.set_min_width(170.);
                ui.menu_button(format!("Select: {}", self.filter.label()), |ui| {
                    for filter in [Filter::All, Filter::Faces, Filter::Edges, Filter::Parts] {
                        if ui
                            .selectable_value(&mut self.filter, filter, filter.label())
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                });
            });
        changed_edges
    }
    fn add_instance(
        &mut self,
        mut doc: AssemblyDocument,
        source: String,
        part: String,
        name: String,
        minimum_x: f64,
        width: f64,
    ) {
        let id = doc.allocate_id("instance");
        let mut instance = Instance::new(id, source, part);
        instance.name = name;
        instance.fixed = doc.instances.is_empty();
        if !self.scene.meshes.is_empty() {
            let max = self
                .scene
                .meshes
                .iter()
                .map(|m| f64::from(m.bounds[1][0]))
                .fold(f64::NEG_INFINITY, f64::max);
            instance.transform =
                Transform::translated([max - minimum_x + (width * 0.2).max(5.), 0., 0.]);
        }
        doc.instances.push(instance);
        self.fit = true;
        self.submit(doc);
    }
    fn insert_dialog(
        &mut self,
        ctx: &egui::Context,
        current: &Document,
        path: Option<&Path>,
        scene: &Scene,
        ready: bool,
    ) {
        if !self.insert_open {
            return;
        }
        let mut open = true;
        let mut chosen = None;
        let mut file = false;
        egui::Window::new("Insert parts")
            .open(&mut open)
            .default_width(350.)
            .collapsible(false)
            .show(ctx, |ui| {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Open Part Studio file"))
                    .clicked()
                {
                    file = true;
                }
                egui::ScrollArea::vertical()
                    .max_height(360.)
                    .show(ui, |ui| {
                        if !scene.meshes.is_empty() {
                            ui.strong("Current Part Studio");
                            for mesh in &scene.meshes {
                                if ui
                                    .add_enabled(ready && !self.busy, egui::Button::new(&mesh.name))
                                    .clicked()
                                {
                                    chosen = Some((
                                        None,
                                        mesh.part.clone(),
                                        mesh.name.clone(),
                                        f64::from(mesh.bounds[0][0]),
                                        f64::from(mesh.bounds[1][0] - mesh.bounds[0][0]),
                                    ));
                                }
                            }
                        }
                        for source in &self.document.sources {
                            ui.strong(&source.name);
                            if let Some(parts) = self.parts.get(&source.id) {
                                for part in parts {
                                    if ui
                                        .add_enabled(!self.busy, egui::Button::new(&part.name))
                                        .clicked()
                                    {
                                        chosen = Some((
                                            Some(source.id.clone()),
                                            part.key.clone(),
                                            part.name.clone(),
                                            part.min_x,
                                            part.width,
                                        ));
                                    }
                                }
                            }
                        }
                    });
            });
        self.insert_open = open;
        if file {
            self.file(FileKind::Insert);
        }
        if let Some((source, part, name, min, width)) = chosen {
            let mut doc = self.document.clone();
            let source = if let Some(source) = source {
                source
            } else if let Some(source) = self
                .document
                .sources
                .iter()
                .find(|s| s.document == *current)
            {
                source.id.clone()
            } else {
                let id = doc.allocate_id("source");
                match Source::new(
                    id.clone(),
                    current.name.clone(),
                    current.clone(),
                    path.map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf())),
                ) {
                    Ok(source) => {
                        doc.sources.push(source);
                        id
                    }
                    Err(error) => {
                        self.error = Some(error);
                        return;
                    }
                }
            };
            self.add_instance(doc, source, part, name, min, width);
        }
    }
    fn mate_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.mate_editor.take() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Mate")
            .open(&mut open)
            .collapsible(false)
            .default_width(300.)
            .show(ctx, |ui| {
                let installed: Vec<_> = self
                    .registry
                    .plugins
                    .values()
                    .filter(|p| p.manifest.category == "mate")
                    .map(|p| (p.manifest.id.clone(), p.manifest.name.clone()))
                    .collect();
                let current = editor
                    .mate
                    .plugin
                    .as_ref()
                    .map(|p| p.plugin.as_str())
                    .unwrap_or("");
                let label = installed
                    .iter()
                    .find(|(id, _)| id == current)
                    .map(|(_, name)| name.clone())
                    .unwrap_or_else(|| format!("{:?}", editor.mate.kind));
                let mut chosen = None;
                egui::ComboBox::from_id_salt("mate_kind")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for (id, name) in installed {
                            if ui.selectable_label(current == id, name).clicked() {
                                chosen = Some(id);
                            }
                        }
                    });
                if let Some(id) = chosen {
                    match self.registry.create(&id, &mut Document::default()) {
                        Ok(feature) => {
                            editor.mate.plugin = Some(feature);
                            editor.mate.position = [0.; 3];
                            editor.mate.rotation_deg = [0.; 3];
                            editor.mate.translation_limits = [None; 3];
                            editor.mate.rotation_limits = [None; 3];
                            if let Err(error) = AssemblyRuntime::new(self.registry.clone())
                                .resolve_mate(&mut editor.mate)
                            {
                                self.error = Some(error);
                            }
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                if let Some(feature) = &mut editor.mate.plugin
                    && let Some(package) = self.registry.plugins.get(&feature.plugin)
                {
                    for field in &package.manifest.fields {
                        let value = &mut feature.params[&field.key];
                        match field.kind.as_str() {
                            "bool" | "boolean" => {
                                let mut b = value.as_bool().unwrap_or(false);
                                if ui.checkbox(&mut b, &field.label).changed() {
                                    *value = b.into();
                                }
                            }
                            "choice" => {
                                let mut choice = value.as_str().unwrap_or_default().to_string();
                                egui::ComboBox::from_id_salt((&feature.id, &field.key))
                                    .selected_text(&choice)
                                    .show_ui(ui, |ui| {
                                        for option in &field.options {
                                            ui.selectable_value(
                                                &mut choice,
                                                option.clone(),
                                                option,
                                            );
                                        }
                                    });
                                *value = choice.into();
                            }
                            _ => {
                                ui.label(&field.label);
                                let mut text = value
                                    .as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| value.to_string());
                                if ui.text_edit_singleline(&mut text).changed() {
                                    *value = text.into();
                                }
                            }
                        }
                    }
                }
                ui.label(format!(
                    "{} / {}",
                    editor.mate.a.instance, editor.mate.b.instance
                ));
                coordinates(ui, "Connector offset", &mut editor.mate.offset.translation);
                let mut flip = editor.mate.offset.rotation[0].abs() > 0.5;
                if ui.checkbox(&mut flip, "Flip connector").changed() {
                    let translation = editor.mate.offset.translation;
                    editor.mate.offset =
                        Transform::from_euler_degrees(if flip { [180., 0., 0.] } else { [0.; 3] });
                    editor.mate.offset.translation = translation;
                }
                let (linear, angular) = allowed(editor.mate.kind);
                for axis in 0..3 {
                    if linear[axis] {
                        motion_axis(
                            ui,
                            format!("{} mm", ["X", "Y", "Z"][axis]),
                            &mut editor.mate.position[axis],
                            &mut editor.mate.translation_limits[axis],
                        );
                    }
                    if angular[axis] {
                        motion_axis(
                            ui,
                            format!("{} deg", ["X", "Y", "Z"][axis]),
                            &mut editor.mate.rotation_deg[axis],
                            &mut editor.mate.rotation_limits[axis],
                        );
                    }
                }
                if !editor.is_new {
                    ui.checkbox(&mut editor.mate.suppressed, "Suppressed");
                }
                apply = ui
                    .add_enabled(!self.busy, egui::Button::new("Apply"))
                    .clicked();
            });
        if apply {
            self.applying_mate = Some(editor.clone());
            let mut doc = self.document.clone();
            if editor.is_new {
                doc.next_id += 1;
                doc.mates.push(editor.mate);
            } else if let Some(existing) = doc.mates.iter_mut().find(|m| m.id == editor.mate.id) {
                *existing = editor.mate;
            }
            self.submit(doc);
        } else if open {
            self.mate_editor = Some(editor);
        }
    }
    fn file_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.files.take() else {
            return;
        };
        let mut open = true;
        let mut accept = false;
        let mut cancel = false;
        egui::Window::new(match dialog.kind {
            FileKind::Open => "Open assembly",
            FileKind::Save => "Save assembly",
            FileKind::Insert => "Open Part Studio",
            FileKind::Export => "Export assembly",
        })
        .id(egui::Id::new("assembly_files"))
        .open(&mut open)
        .collapsible(false)
        .default_width(620.)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Up").clicked()
                    && let Some(parent) = dialog.directory.parent()
                {
                    dialog.directory = parent.into();
                    dialog.directory_text = dialog.directory.display().to_string();
                }
                ui.text_edit_singleline(&mut dialog.directory_text);
                if ui.button("Go").clicked() {
                    let path = PathBuf::from(&dialog.directory_text);
                    if path.is_dir() {
                        dialog.directory = path;
                    } else {
                        dialog.error = Some("Directory does not exist".into());
                    }
                }
            });
            let mut next = None;
            egui::ScrollArea::vertical()
                .max_height(240.)
                .show(ui, |ui| {
                    if let Ok(entries) = std::fs::read_dir(&dialog.directory) {
                        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
                        paths.sort_by_key(|p| {
                            (
                                !p.is_dir(),
                                p.file_name().map(|s| s.to_string_lossy().to_lowercase()),
                            )
                        });
                        for path in paths {
                            let directory = path.is_dir();
                            if !directory
                                && path
                                    .extension()
                                    .is_none_or(|e| !e.eq_ignore_ascii_case(dialog.extension()))
                            {
                                continue;
                            }
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            let response = ui.selectable_label(
                                !directory && dialog.filename == name,
                                format!("{}{}", name, if directory { "/" } else { "" }),
                            );
                            if response.clicked() {
                                if directory {
                                    next = Some(path.clone());
                                } else {
                                    dialog.filename = name.into_owned();
                                }
                            }
                            if response.double_clicked()
                                && !directory
                                && !self.busy
                                && matches!(dialog.kind, FileKind::Open | FileKind::Insert)
                            {
                                accept = true;
                            }
                        }
                    }
                });
            if let Some(path) = next {
                dialog.directory = path;
                dialog.directory_text = dialog.directory.display().to_string();
            }
            ui.text_edit_singleline(&mut dialog.filename);
            if let Some(error) = &dialog.error {
                ui.colored_label(Color32::from_rgb(220, 137, 150), error);
            }
            let exists = dialog.target_path().exists();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.filename.trim().is_empty() && !self.busy,
                        egui::Button::new(
                            if matches!(dialog.kind, FileKind::Save | FileKind::Export) && exists {
                                "Replace existing file"
                            } else if matches!(dialog.kind, FileKind::Save) {
                                "Save"
                            } else if matches!(dialog.kind, FileKind::Export) {
                                "Export"
                            } else {
                                "Open"
                            },
                        ),
                    )
                    .clicked()
                {
                    accept = true;
                }
                cancel = ui.button("Cancel").clicked();
            });
        });
        if accept {
            let path = dialog.target_path();
            let result = match dialog.kind {
                FileKind::Open => {
                    self.open(path);
                    Ok(())
                }
                FileKind::Insert => {
                    self.send(
                        Work::LoadSource(self.document.clone(), path),
                        Some(History::Record),
                    );
                    Ok(())
                }
                FileKind::Save => self.save_as(path),
                FileKind::Export => {
                    self.send(
                        Work::Export(self.document.clone(), dialog.format.clone(), path),
                        None,
                    );
                    Ok(())
                }
            };
            match result {
                Ok(()) => open = false,
                Err(error) => dialog.error = Some(error),
            }
        }
        if open && !cancel {
            self.files = Some(dialog);
        }
    }
    fn search_dialog(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if !self.search_open {
            return;
        }
        let mut open = true;
        let mut command = None;
        let mut chosen_item = false;
        egui::Window::new("Search assembly")
            .open(&mut open)
            .collapsible(false)
            .default_width(340.)
            .show(ctx, |ui| {
                let response = ui.text_edit_singleline(&mut self.search);
                if self.search_focus {
                    response.request_focus();
                    self.search_focus = false;
                }
                let mut enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let query = self.search.to_lowercase();
                for (index, label) in [
                    "Insert",
                    "Mate",
                    "Fix / unfix",
                    "Edit in context",
                    "Update sources",
                    "Update context",
                    "Save",
                    "Open",
                    "Settings",
                    "Part Studio",
                ]
                .iter()
                .enumerate()
                {
                    if label.to_lowercase().contains(&query)
                        && (ui
                            .add_enabled(!self.busy, egui::Button::new(*label))
                            .clicked()
                            || (enter && !self.busy))
                    {
                        command = Some(index);
                        enter = false;
                    }
                }
                for instance in &self.document.instances {
                    if instance.name.to_lowercase().contains(&query)
                        && ui.button(&instance.name).clicked()
                    {
                        self.selection.clear();
                        self.selection.add(Target::Part(instance.id.clone()));
                        self.fit = true;
                        chosen_item = true;
                    }
                }
                for mate in &self.document.mates {
                    if format!("{:?}", mate.kind).to_lowercase().contains(&query)
                        && ui
                            .button(format!("{:?} \u{00b7} {}", mate.kind, mate.id))
                            .clicked()
                    {
                        self.mate_editor = Some(MateEditor {
                            mate: mate.clone(),
                            is_new: false,
                        });
                        chosen_item = true;
                    }
                }
            });
        if chosen_item {
            open = false;
        }
        if let Some(command) = command {
            open = false;
            match command {
                0 => self.insert_open = true,
                1 => self.begin_mate(),
                2 => self.fix_selected(),
                3 => self.capture(),
                4 => self.update_sources(),
                5 => self.update_context(),
                6 => self.save(),
                7 => self.file(FileKind::Open),
                8 => actions.push(Action::Settings),
                _ => actions.push(Action::PartStudio),
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
        self.search_open = open;
    }
}
fn pick_target(
    scene: &Scene,
    camera: &Camera,
    pos: egui::Pos2,
    rect: Rect,
    filter: Filter,
    hidden: &BTreeSet<String>,
) -> Option<Target> {
    let face = render::pick(scene, camera, pos, rect, hidden);
    if matches!(filter, Filter::All | Filter::Edges) {
        let edge = render::pick_edges(scene, camera, pos, rect, hidden)
            .into_iter()
            .find(|edge| {
                face.as_ref().is_none_or(|face| {
                    edge.distance <= face.distance + camera.span / rect.height() * 5.
                })
            });
        if let Some(edge) = edge {
            return Some(Target::Edge {
                part: edge.part,
                key: edge.key,
            });
        }
        if filter == Filter::Edges {
            return None;
        }
    }
    face.map(|face| {
        if filter == Filter::Parts {
            Target::Part(face.part)
        } else {
            Target::Face {
                part: face.part,
                key: face.face,
            }
        }
    })
}
fn same_path(a: &Path, b: &Path) -> bool {
    a == b
        || a.canonicalize()
            .ok()
            .zip(b.canonicalize().ok())
            .is_some_and(|(a, b)| a == b)
}
fn absolute_source_links(document: &mut AssemblyDocument, directory: &Path) {
    let directory = if directory.is_absolute() {
        directory.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(directory)
    };
    for source in document.sources.iter_mut().chain(
        document
            .contexts
            .iter_mut()
            .flat_map(|c| c.sources.iter_mut()),
    ) {
        if let Some(path) = &source.path
            && path.is_relative()
        {
            source.path = Some(directory.join(path));
        }
    }
}
fn coordinates(ui: &mut egui::Ui, label: &str, values: &mut [f64; 3]) -> bool {
    ui.label(label);
    let mut changed = false;
    ui.horizontal(|ui| {
        for (axis, value) in ["X", "Y", "Z"].into_iter().zip(values) {
            changed |= ui
                .add(egui::DragValue::new(value).speed(0.2).prefix(axis))
                .changed();
        }
    });
    changed
}
fn allowed(kind: MateKind) -> ([bool; 3], [bool; 3]) {
    match kind {
        MateKind::Fastened => ([false; 3], [false; 3]),
        MateKind::Revolute => ([false; 3], [false, false, true]),
        MateKind::Slider => ([false, false, true], [false; 3]),
        MateKind::Cylindrical => ([false, false, true], [false, false, true]),
        MateKind::Planar => ([true, true, false], [false, false, true]),
        MateKind::Ball => ([false; 3], [true; 3]),
    }
}
fn motion_axis(ui: &mut egui::Ui, label: String, value: &mut f64, limit: &mut Option<Limit>) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(0.2));
        let mut enabled = limit.is_some();
        if ui.checkbox(&mut enabled, "Limits").changed() {
            *limit = enabled.then_some(Limit {
                min: *value - 10.,
                max: *value + 10.,
            });
        }
    });
    if let Some(limit) = limit {
        ui.horizontal(|ui| {
            ui.label("Min");
            ui.add(egui::DragValue::new(&mut limit.min));
            ui.label("Max");
            ui.add(egui::DragValue::new(&mut limit.max));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Sketch;
    use serde_json::json;
    fn fixture(registry: &Registry) -> AssemblyDocument {
        let mut part = Document::default();
        let mut sketch = Sketch::default();
        sketch.add_rect([0., 0.], [20., 10.]);
        let mut f = registry.create("sketch", &mut part).unwrap();
        f.set_sketch(&sketch);
        part.features.push(f);
        let mut f = registry.create("extrude", &mut part).unwrap();
        f.params["sketch"] = json!("f1");
        f.params["depth"] = json!("5");
        part.features.push(f);
        let mut doc = AssemblyDocument::default();
        doc.sources.push(
            Source::new(
                "source".into(),
                "Block".into(),
                part,
                Some("parts/block.cadpart".into()),
            )
            .unwrap(),
        );
        for n in 0..2 {
            let mut i = Instance::new(format!("instance{n}"), "source".into(), "f2:0".into());
            i.fixed = n == 0;
            i.transform = Transform::translated([30. * n as f64, 0., 0.]);
            doc.instances.push(i);
        }
        doc
    }
    #[test]
    fn all_filter_prefers_visible_edges_but_faces_and_parts_filters_are_explicit() {
        let registry = Arc::new(Registry::builtin().unwrap());
        let mut runtime = AssemblyRuntime::new(registry.clone());
        let mut doc = fixture(&registry);
        doc.instances.truncate(1);
        let scene = runtime.evaluate(&doc).unwrap().scene;
        let mut camera = Camera::default();
        camera.set_view(glam::Vec3::Z);
        camera.span = 50.;
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        let hidden = BTreeSet::new();
        let rim = camera.project([10., 0., 5.], rect);
        assert!(matches!(
            pick_target(&scene, &camera, rim, rect, Filter::All, &hidden),
            Some(Target::Edge { .. })
        ));
        assert!(matches!(
            pick_target(&scene, &camera, rim, rect, Filter::Faces, &hidden),
            Some(Target::Face { .. })
        ));
        assert!(matches!(
            pick_target(&scene, &camera, rim, rect, Filter::Parts, &hidden),
            Some(Target::Part(_))
        ));
        let inside = camera.project([10., 5., 5.], rect);
        assert!(matches!(
            pick_target(&scene, &camera, inside, rect, Filter::All, &hidden),
            Some(Target::Face { .. })
        ));
    }
    #[test]
    fn oblique_circular_rims_use_the_same_five_pixel_depth_tolerance_as_part_studio() {
        let registry = Arc::new(Registry::builtin().unwrap());
        let mut doc = fixture(&registry);
        doc.instances.truncate(1);
        let mut sketch = Sketch::default();
        sketch.add_circle([0., 0.], 10.);
        sketch.add_circle([0., 0.], 3.);
        doc.sources[0].document.features[0].set_sketch(&sketch);
        doc.sources[0].refresh_revision().unwrap();
        let mut runtime = AssemblyRuntime::new(registry);
        let scene = runtime.evaluate(&doc).unwrap().scene;
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        let mut camera = Camera::default();
        camera.fit_with_rect(&scene, rect);
        let hidden = BTreeSet::new();
        let mut samples = 0;
        for edge in scene
            .meshes
            .iter()
            .flat_map(|m| &m.edges)
            .filter(|e| !e.seam)
        {
            for pair in edge.points.windows(2) {
                let point = std::array::from_fn(|n| f64::from((pair[0][n] + pair[1][n]) * 0.5));
                let projected = camera.project(point, rect);
                for delta in [
                    egui::vec2(2., 0.),
                    egui::vec2(-2., 0.),
                    egui::vec2(0., 2.),
                    egui::vec2(0., -2.),
                ] {
                    let pos = projected + delta;
                    let Some(face) = render::pick(&scene, &camera, pos, rect, &hidden) else {
                        continue;
                    };
                    let edges = render::pick_edges(&scene, &camera, pos, rect, &hidden);
                    if edges.iter().any(|e| {
                        e.distance > face.distance + camera.span * 1e-4
                            && e.distance <= face.distance + camera.span / rect.height() * 5.
                    }) {
                        assert!(matches!(
                            pick_target(&scene, &camera, pos, rect, Filter::All, &hidden),
                            Some(Target::Edge { .. })
                        ));
                        samples += 1;
                    }
                }
            }
        }
        assert!(
            samples > 0,
            "Oblique circular rim must exercise screen-space depth tolerance"
        );
    }
    #[test]
    fn file_dialog_normalizes_default_extension_before_replacement_check() {
        let mut dialog = FileDialog {
            kind: FileKind::Save,
            directory: std::env::temp_dir(),
            directory_text: String::new(),
            filename: "assembly".into(),
            error: None,
            format: String::new(),
        };
        assert_eq!(
            dialog.target_path(),
            std::env::temp_dir().join("assembly.cadasm")
        );
        dialog.kind = FileKind::Export;
        dialog.format = "step".into();
        assert_eq!(
            dialog.target_path(),
            std::env::temp_dir().join("assembly.step")
        );
        dialog.filename = "assembly.STEP".into();
        assert_eq!(
            dialog.target_path(),
            std::env::temp_dir().join("assembly.STEP")
        );
    }
    #[test]
    fn desktop_export_worker_writes_exact_instances_and_preserves_workspace_on_error() {
        let registry = Arc::new(Registry::builtin().unwrap());
        let mut runtime = AssemblyRuntime::new(registry.clone());
        let doc = fixture(&registry);
        let before = doc.clone();
        let directory = std::env::temp_dir().join(format!(
            "cad-ui-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("assembly.step");
        let Payload::Exported(exported) = execute(
            &mut runtime,
            Work::Export(doc.clone(), "step".into(), path.clone()),
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(exported, path);
        let step = std::fs::read_to_string(&path).unwrap();
        assert_eq!(step.matches("MANIFOLD_SOLID_BREP").count(), 2);
        assert!(
            execute(
                &mut runtime,
                Work::Export(
                    doc.clone(),
                    "invalid-format".into(),
                    directory.join("bad.file")
                )
            )
            .is_err()
        );
        assert_eq!(doc, before);
        assert!(!directory.join("bad.file").exists());
        for entry in std::fs::read_dir(&directory).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn context_reentry_preserves_frozen_external_pose_until_explicit_new_context() {
        let registry = Arc::new(Registry::builtin().unwrap());
        let mut runtime = AssemblyRuntime::new(registry.clone());
        let mut doc = fixture(&registry);
        let context = runtime.capture_context(&doc, "instance0").unwrap();
        let frozen = context.poses["instance1"];
        let id = context.id.clone();
        doc.contexts.push(context);
        doc.instances[1].transform.translation[0] = 80.;
        let Payload::Context(existing, _, _) = execute(
            &mut runtime,
            Work::Capture(doc.clone(), "instance0".into(), false),
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(existing.id, id);
        assert_eq!(existing.poses["instance1"], frozen);
        let Payload::Context(new, _, _) =
            execute(&mut runtime, Work::Capture(doc, "instance0".into(), true)).unwrap()
        else {
            panic!()
        };
        assert_ne!(new.id, id);
        assert_eq!(new.poses["instance1"].translation[0], 80.);
    }
    #[test]
    fn moving_assembly_preserves_link_targets_in_sources_and_frozen_contexts() {
        let registry = Arc::new(Registry::builtin().unwrap());
        let mut runtime = AssemblyRuntime::new(registry.clone());
        let mut doc = fixture(&registry);
        let context = runtime.capture_context(&doc, "instance0").unwrap();
        doc.contexts.push(context);
        let base = std::env::temp_dir().join("cad-original-assembly");
        absolute_source_links(&mut doc, &base);
        let expected = Some(base.join("parts/block.cadpart"));
        assert_eq!(doc.sources[0].path, expected);
        assert_eq!(doc.contexts[0].sources[0].path, expected);
        absolute_source_links(&mut doc, &std::env::temp_dir().join("cad-moved-assembly"));
        assert_eq!(doc.sources[0].path, expected);
    }
    #[test]
    fn busy_workspace_keeps_pending_transaction_and_serial() {
        let registry = Arc::new(Registry::builtin().unwrap());
        let mut ui = AssemblyUi::new(&egui::Context::default(), registry);
        ui.busy = true;
        ui.serial = 7;
        ui.pending = Some(Pending {
            before: ui.document.clone(),
            history: History::Record,
        });
        ui.submit(AssemblyDocument {
            name: "Must not overwrite pending document".into(),
            ..Default::default()
        });
        assert_eq!(ui.serial, 7);
        assert!(ui.pending.is_some());
        assert!(ui.busy);
    }
    #[test]
    fn mixed_axis_rotation_roundtrips_through_sidebar_angles() {
        for angles in [[37., -18., 110.], [-70., 50., 23.], [12., 80., -150.]] {
            let original = Transform::from_euler_degrees(angles);
            let (x, y, z) =
                glam::DQuat::from_array(original.rotation).to_euler(glam::EulerRot::XYZEx);
            let rebuilt =
                Transform::from_euler_degrees([x.to_degrees(), y.to_degrees(), z.to_degrees()]);
            for p in [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]] {
                let a = original.vector(p);
                let b = rebuilt.vector(p);
                for n in 0..3 {
                    assert!((a[n] - b[n]).abs() < 1e-10);
                }
            }
        }
    }
    #[test]
    fn fixed_instance_uses_visible_solved_pose() {
        let registry = Arc::new(Registry::builtin().unwrap());
        let mut ui = AssemblyUi::new(&egui::Context::default(), registry.clone());
        ui.document = fixture(&registry);
        let mut mate = Mate::new(
            MateKind::Fastened,
            Connector::origin("instance0"),
            Connector::origin("instance1"),
        );
        mate.id = "mate1".into();
        mate.offset = Transform::translated([20., 0., 0.]);
        ui.document.mates.push(mate);
        let mut runtime = AssemblyRuntime::new(registry);
        let evaluated = runtime.evaluate(&ui.document).unwrap();
        ui.poses = evaluated.poses;
        ui.selection.add(Target::Part("instance1".into()));
        ui.fix_selected();
        let pending = ui.pending.take().unwrap();
        assert_eq!(pending.before.instances[1].transform.translation[0], 30.);
        // The actual queued candidate must solve successfully; poll it rather than assert a copied implementation.
        let reply = ui
            .worker
            .receive
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let Payload::Ready(ready) = reply.result.unwrap() else {
            panic!()
        };
        assert!(ready.document.instances[1].fixed);
        assert_eq!(ready.document.instances[1].transform.translation[0], 20.);
    }
}
