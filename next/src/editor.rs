//! The UI owns interaction state. The worker owns all document state and history.

#[cfg(test)]
mod tests;

use crate::{
    WorkerConfig, WorkerSession,
    packages::{ParameterType, Registry},
    viewport::Viewport,
};
use eframe::egui::{self, Color32, Key, RichText};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender, SyncSender},
    time::{Duration, Instant},
};

struct Job {
    op: String,
    parameters: Value,
}
struct Reply {
    op: String,
    accepted: bool,
    state: Value,
    mesh: Value,
    topology: Value,
    error: Option<String>,
    elapsed: Duration,
}

fn call(worker: &mut WorkerSession, op: &str, parameters: Value) -> Result<Value, String> {
    worker
        .call(op, parameters.as_object().cloned().unwrap_or_default())
        .map_err(|e| e.to_string())
}

/// Pipes and all native recomputation run away from input and drawing. The
/// bounded queue and disabled mutation tools permit only one outstanding job.
fn model_thread(
    config: WorkerConfig,
    input: Receiver<Job>,
    output: Sender<Reply>,
    ctx: egui::Context,
) {
    let mut worker = match WorkerSession::spawn(&config) {
        Ok(worker) => worker,
        Err(error) => {
            let _ = output.send(Reply {
                op: "startup".into(),
                accepted: false,
                state: Value::Null,
                mesh: Value::Null,
                topology: Value::Null,
                error: Some(error.to_string()),
                elapsed: Duration::ZERO,
            });
            ctx.request_repaint();
            return;
        }
    };
    let mut current = Value::Null;
    while let Ok(job) = input.recv() {
        let start = Instant::now();
        let previous = current.clone();
        // Preserve the old document until a replacement loads successfully.
        // Opening the same path twice returns the worker's already_open error.
        let result = call(&mut worker, &job.op, job.parameters);
        let accepted = result.is_ok();
        let mut error = None;
        match result {
            Ok(result) => {
                if result.get("state").is_some() {
                    current = result["state"].clone();
                } else if result.get("objects").is_some() {
                    current = result;
                } else if result.get("revision").is_some() {
                    current["revision"] = result["revision"].clone();
                }
            }
            Err(message) => error = Some(message),
        }
        if accepted
            && matches!(job.op.as_str(), "new" | "open")
            && let Some(old) = previous["document"].as_str()
            && previous["document"] != current["document"]
        {
            let _ = call(
                &mut worker,
                "close",
                json!({"document":old,"expected_revision":previous["revision"]}),
            );
        }
        // Successful mutations already return the solved native snapshot. A
        // rejected native edit may still invalidate a revision during rollback.
        if !accepted && let Some(document) = current["document"].as_str() {
            match call(&mut worker, "inspect", json!({"document":document})) {
                Ok(state) => current = state,
                Err(message) => error = Some(message),
            }
        }
        let mut mesh = Value::Null;
        let mut topology = Value::Null;
        if let Some(object) = display_object(&current) {
            let parameters =
                json!({"document":current["document"],"object":object,"deflection":0.03});
            match (
                call(&mut worker, "mesh", parameters.clone()),
                call(&mut worker, "topology", parameters),
            ) {
                (Ok(m), Ok(t))
                    if m["revision"] == current["revision"]
                        && t["revision"] == current["revision"] =>
                {
                    mesh = m;
                    topology = t;
                }
                (Err(message), _) | (_, Err(message)) => error = Some(message),
                _ => error = Some("Display revision changed".into()),
            }
        }
        let reply = Reply {
            op: job.op,
            accepted,
            state: current.clone(),
            mesh,
            topology,
            error,
            elapsed: start.elapsed(),
        };
        if output.send(reply).is_err() {
            break;
        }
        ctx.request_repaint();
    }
}

fn objects(state: &Value) -> &[Value] {
    state["objects"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}
fn display_object(state: &Value) -> Option<String> {
    let all = objects(state);
    // A Body's tip names the native feature to which face support must attach.
    // Linking a new sketch to its own Body would introduce a dependency cycle.
    all.iter()
        .find(|o| o["type"] == "PartDesign::Body")
        .and_then(|body| body["properties"]["Tip"]["link"].as_str())
        .and_then(|tip| {
            all.iter()
                .find(|o| o["name"] == tip && o["shape"]["solids"].as_u64().unwrap_or(0) > 0)
        })
        .or_else(|| {
            all.iter().rev().find(|o| {
                o["type"] != "PartDesign::Body" && o["shape"]["solids"].as_u64().unwrap_or(0) > 0
            })
        })
        .and_then(|o| o["name"].as_str())
        .map(str::to_owned)
}

#[derive(Clone, Copy, PartialEq)]
enum Draw {
    Select,
    Rectangle,
    Circle,
}
struct Draft {
    id: String,
    name: String,
    token: String,
    parameters: Value,
    changed: Option<Instant>,
    valid: bool,
    all_parameters: bool,
}
#[derive(Clone, Copy)]
enum FileAction {
    New,
    Open,
    Close,
}

pub struct Editor {
    jobs: SyncSender<Job>,
    replies: Receiver<Reply>,
    busy: bool,
    registry: Registry,
    state: Value,
    viewport: Viewport,
    sketch: Option<String>,
    selected_object: Option<String>,
    draw: Draw,
    first: Option<[f64; 3]>,
    draft: Option<Draft>,
    sequence: u64,
    error: Option<String>,
    status: String,
    path: String,
    file_input: String,
    file_dialog: Option<bool>,
    overwrite: bool,
    dirty: bool,
    pending_file: Option<FileAction>,
    allow_close: bool,
    dimensions: Vec<(usize, String, String)>,
    fit_next: bool,
}

impl Editor {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        config: WorkerConfig,
        registry: Registry,
        path: Option<PathBuf>,
    ) -> Result<Self, String> {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::from_rgb(13, 15, 18);
        style.visuals.window_fill = Color32::from_rgb(19, 22, 26);
        style.visuals.extreme_bg_color = Color32::from_rgb(5, 7, 9);
        style.visuals.selection.bg_fill = Color32::from_rgb(45, 91, 91);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(49, 94, 94);
        style.spacing.item_spacing = egui::vec2(6., 6.);
        cc.egui_ctx.set_style(style);
        let mut viewport = Viewport::default();
        viewport.set_gl(cc.gl.clone().ok_or("The desktop editor requires OpenGL")?)?;
        let (jobs, input) = mpsc::sync_channel(1);
        let (output, replies) = mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || model_thread(config, input, output, ctx));
        let mut app = Self {
            jobs,
            replies,
            busy: false,
            registry,
            state: Value::Null,
            viewport,
            sketch: None,
            selected_object: None,
            draw: Draw::Select,
            first: None,
            draft: None,
            sequence: 0,
            error: None,
            status: "Starting native model".into(),
            path: path
                .as_ref()
                .map(|p| p.to_string_lossy().into())
                .unwrap_or_default(),
            file_input: String::new(),
            file_dialog: None,
            overwrite: false,
            dirty: false,
            pending_file: None,
            allow_close: false,
            dimensions: Vec::new(),
            fit_next: false,
        };
        if let Some(path) = path {
            app.send("open", json!({"path":path}));
        } else {
            app.send("new", json!({"name":"PartStudio"}));
        }
        Ok(app)
    }

    fn send(&mut self, op: &str, mut parameters: Value) {
        if self.busy {
            return;
        }
        if !matches!(op, "new" | "open") {
            parameters["document"] = self.state["document"].clone();
            parameters["expected_revision"] = self.state["revision"].clone();
        }
        self.busy = true;
        self.error = None;
        if self
            .jobs
            .try_send(Job {
                op: op.into(),
                parameters,
            })
            .is_err()
        {
            self.busy = false;
            self.error = Some("Model thread stopped".into());
        }
    }

    fn transaction(&mut self, label: &str, edits: Vec<Value>) {
        self.send("transaction", json!({"label":label,"edits":edits}));
    }
    fn unique(&mut self, prefix: &str) -> String {
        loop {
            self.sequence += 1;
            let name = format!("{prefix}{}", self.sequence);
            if !objects(&self.state).iter().any(|o| o["name"] == name) {
                return name;
            }
        }
    }
    fn body(&self) -> Option<String> {
        objects(&self.state)
            .iter()
            .find(|o| o["type"] == "PartDesign::Body")
            .and_then(|o| o["name"].as_str())
            .map(str::to_owned)
    }
    fn sketch_snapshot(&self) -> Option<Value> {
        self.sketch
            .as_ref()
            .and_then(|name| objects(&self.state).iter().find(|o| o["name"] == *name))
            .cloned()
    }
    fn sync_sketch(&mut self) {
        let snapshot = self.sketch_snapshot();
        if snapshot.is_none() {
            self.sketch = None;
            self.draw = Draw::Select;
            self.first = None;
        }
        if self
            .selected_object
            .as_ref()
            .is_some_and(|name| !objects(&self.state).iter().any(|o| o["name"] == *name))
        {
            self.selected_object = None;
        }
        self.dimensions.clear();
        if let Some(o) = &snapshot
            && let Some(constraints) = o["sketch"]["constraints"].as_array()
        {
            for c in constraints {
                let kind = c["type"].as_str().unwrap_or_default();
                if c["driving"] == true
                    && [
                        "Distance",
                        "DistanceX",
                        "DistanceY",
                        "Radius",
                        "Diameter",
                        "Angle",
                    ]
                    .contains(&kind)
                {
                    self.dimensions.push((
                        c["index"].as_u64().unwrap_or(0) as usize,
                        kind.into(),
                        format!(
                            "{} {}",
                            c["value"],
                            if kind == "Angle" { "rad" } else { "mm" }
                        ),
                    ));
                }
            }
        }
        self.viewport.set_sketch(snapshot);
    }
    fn poll(&mut self) {
        while let Ok(reply) = self.replies.try_recv() {
            self.busy = false;
            let success = reply.accepted;
            self.error = reply.error;
            self.state = reply.state;
            self.viewport.set_scene(reply.mesh, reply.topology);
            if success && self.fit_next && reply.op == "preview" {
                self.viewport.fit();
                self.fit_next = false;
            }
            self.status = format!(
                "{} · {:.0} ms",
                if success { "Ready" } else { "Edit rejected" },
                reply.elapsed.as_secs_f64() * 1000.
            );
            if success {
                match reply.op.as_str() {
                    "transaction" | "preview_commit" | "undo" | "redo" => self.dirty = true,
                    "new" | "open" => {
                        self.dirty = false;
                        self.sketch = None;
                        self.selected_object = None;
                        self.viewport.fit();
                        if reply.op == "new" {
                            self.path.clear();
                        } else if !self.file_input.is_empty() {
                            self.path = self.file_input.clone();
                        }
                    }
                    "save" => {
                        self.dirty = false;
                        self.path = self.file_input.clone();
                    }
                    _ => (),
                }
                if reply.op == "preview"
                    && let Some(d) = &mut self.draft
                {
                    d.valid = self.error.is_none();
                }
                if matches!(reply.op.as_str(), "preview_commit" | "preview_cancel") {
                    self.draft = None;
                }
            }
            self.sync_sketch();
            if matches!(reply.op.as_str(), "undo" | "redo" | "new" | "open") {
                self.first = None;
            }
            if reply.op == "transaction"
                && self
                    .sketch_snapshot()
                    .is_some_and(|o| o["sketch"]["geometry_count"] == 0)
            {
                self.viewport.align_sketch();
                self.viewport.fit();
            }
        }
    }
    fn new_sketch(&mut self) {
        let name = self.unique("Sketch");
        let mut edits = Vec::new();
        let body = self.body().unwrap_or_else(|| {
            edits.push(json!({"op":"add_object","type":"PartDesign::Body","name":"Body"}));
            "Body".into()
        });
        edits.push(
            json!({"op":"add_object","type":"Sketcher::SketchObject","name":name,"body":body}),
        );
        if let Some(selection) = &self.viewport.selected
            && selection.subelement.starts_with("Face")
        {
            edits.push(json!({"op":"set","object":name,"property":"AttachmentSupport","value":{"link":selection.object,"subelements":[selection.subelement]}}));
            edits.push(json!({"op":"set","object":name,"property":"MapMode","value":"FlatFace"}));
        }
        self.selected_object = Some(name.clone());
        self.sketch = Some(name);
        self.draw = Draw::Select;
        self.first = None;
        self.transaction("Create sketch", edits);
    }
    fn start_feature(&mut self, id: &str) {
        let body = match self.body() {
            Some(b) => b,
            None => {
                self.error = Some("Create a sketch first".into());
                return;
            }
        };
        let name = self.unique(if id == "feature.pad" {
            "Extrude"
        } else if id == "feature.pocket" {
            "Pocket"
        } else {
            "Fillet"
        });
        let parameters = if id == "feature.fillet" {
            let Some(selection) = &self.viewport.selected else {
                self.error = Some("Select an edge".into());
                return;
            };
            if !selection.subelement.starts_with("Edge") {
                self.error = Some("Select an edge".into());
                return;
            }
            json!({"name":name,"body":body,"source":selection.object,"edge":selection.subelement,"radius":2.})
        } else {
            let profile = self.sketch.clone().or_else(|| {
                self.selected_object.clone().filter(|n| {
                    objects(&self.state)
                        .iter()
                        .any(|o| o["name"] == *n && o["type"] == "Sketcher::SketchObject")
                })
            });
            let Some(profile) = profile else {
                self.error = Some("Choose a sketch in the feature list".into());
                return;
            };
            json!({"name":name,"body":body,"profile":profile,"length":10.})
        };
        self.draft = Some(Draft {
            id: id.into(),
            name,
            token: format!("editor-{}-{}", std::process::id(), self.sequence),
            parameters,
            changed: Some(Instant::now() - Duration::from_secs(1)),
            valid: false,
            all_parameters: false,
        });
        self.sketch = None;
        self.viewport.set_sketch(None);
        self.draw = Draw::Select;
        self.first = None;
        self.viewport.iso();
        self.fit_next = true;
    }
    fn preview(&mut self) {
        let Some(d) = &mut self.draft else {
            return;
        };
        d.changed = None;
        let edits = if d.id == "native.property" {
            Ok(vec![
                json!({"op":"set","object":d.parameters["object"],"property":d.parameters["property"],"value":d.parameters["value"]}),
            ])
        } else {
            self.registry.build(&d.id, &d.parameters)
        };
        match edits {
            Ok(edits) => {
                let params = json!({"label":d.name,"token":d.token,"edits":edits});
                self.send("preview", params);
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn edit_feature(&mut self, name: &str) {
        let Some(object) = objects(&self.state).iter().find(|o| o["name"] == name) else {
            return;
        };
        let Some(property) = ["Length", "Radius"]
            .into_iter()
            .find(|p| object["properties"][*p].is_number())
        else {
            return;
        };
        let parameters =
            json!({"object":name,"property":property,"value":object["properties"][property]});
        self.sequence += 1;
        self.draft = Some(Draft {
            id: "native.property".into(),
            name: format!("Edit {name}"),
            token: format!("editor-{}-{}", std::process::id(), self.sequence),
            parameters,
            changed: None,
            valid: false,
            all_parameters: false,
        });
        self.sketch = None;
        self.viewport.set_sketch(None);
        self.draw = Draw::Select;
    }
    fn start_package(&mut self, id: &str) {
        let name = self.unique("Feature");
        let package = self.registry.get(id).unwrap();
        let mut parameters = serde_json::Map::new();
        for (key, p) in &package.parameters {
            let value = p.default.clone().unwrap_or_else(|| match key.as_str() {
                "name" => json!(name),
                "body" => json!(self.body().unwrap_or_default()),
                "profile" | "object" => json!(
                    self.sketch
                        .clone()
                        .or(self.selected_object.clone())
                        .unwrap_or_default()
                ),
                "source" => json!(
                    self.viewport
                        .selected
                        .as_ref()
                        .map(|s| s.object.clone())
                        .unwrap_or_default()
                ),
                "edge" => json!(
                    self.viewport
                        .selected
                        .as_ref()
                        .map(|s| s.subelement.clone())
                        .unwrap_or_default()
                ),
                _ => match p.kind {
                    ParameterType::Number => json!(10.),
                    ParameterType::Integer => json!(0),
                    ParameterType::Boolean => json!(false),
                    _ => json!(""),
                },
            });
            parameters.insert(key.clone(), value);
        }
        if package.parameters.contains_key("index") {
            let object = parameters
                .get("object")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Some(snapshot) = objects(&self.state).iter().find(|o| o["name"] == object) {
                parameters.insert("index".into(), snapshot["sketch"]["geometry_count"].clone());
            }
        }
        self.draft = Some(Draft {
            id: id.into(),
            name: package.label.clone(),
            token: format!("editor-{}-{}", std::process::id(), self.sequence),
            parameters: Value::Object(parameters),
            changed: Some(Instant::now()),
            valid: false,
            all_parameters: true,
        });
        self.draw = Draw::Select;
        self.first = None;
    }
    fn file_dialog(&mut self, save: bool) {
        self.file_input = self.path.clone();
        self.file_dialog = Some(save);
    }
    fn save(&mut self) {
        if self.path.is_empty() {
            self.file_dialog(true);
        } else {
            self.file_input = self.path.clone();
            self.send("save", json!({"path":self.path}));
        }
    }
    fn finish_preview(&mut self, commit: bool) {
        if let Some(d) = &self.draft {
            if self.state["preview"].is_null() {
                if !commit {
                    self.draft = None;
                }
                return;
            }
            let params = json!({"token":d.token});
            self.send(
                if commit {
                    "preview_commit"
                } else {
                    "preview_cancel"
                },
                params,
            );
        }
    }
    fn request_file(&mut self, action: FileAction) {
        if self.dirty {
            self.pending_file = Some(action);
        } else {
            self.file_action(action);
        }
    }
    fn file_action(&mut self, action: FileAction) {
        match action {
            FileAction::New => {
                let name = self.unique("PartStudio");
                self.send("new", json!({"name":name}));
            }
            FileAction::Open => self.file_dialog(false),
            FileAction::Close => {
                self.allow_close = true;
            }
        }
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("CAD")
                        .strong()
                        .color(Color32::from_rgb(110, 182, 177)),
                );
                ui.separator();
                let idle = !self.busy
                    && self.draft.is_none()
                    && self.pending_file.is_none()
                    && self.file_dialog.is_none()
                    && !self.overwrite;
                ui.add_enabled_ui(idle, |ui| {
                    if ui.button("New").clicked() {
                        self.request_file(FileAction::New);
                    }
                    if ui.button("Open").clicked() {
                        self.request_file(FileAction::Open);
                    }
                    if ui.button("Save").clicked() {
                        self.save();
                    }
                    if ui.button("Save as").clicked() {
                        self.file_dialog(true);
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            self.state["undo_count"].as_u64().unwrap_or(0) > 0,
                            egui::Button::new("Undo"),
                        )
                        .clicked()
                    {
                        self.send("undo", json!({}));
                    }
                    if ui
                        .add_enabled(
                            self.state["redo_count"].as_u64().unwrap_or(0) > 0,
                            egui::Button::new("Redo"),
                        )
                        .clicked()
                    {
                        self.send("redo", json!({}));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Fit").clicked() {
                        self.viewport.fit();
                    }
                    if ui.button("Iso").clicked() {
                        self.viewport.iso();
                    }
                    if ui.button("Top").clicked() {
                        self.viewport.top();
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.add_enabled_ui(
                    !self.busy
                        && self.draft.is_none()
                        && self.pending_file.is_none()
                        && self.file_dialog.is_none(),
                    |ui| {
                        if ui.button("Sketch").clicked() {
                            self.new_sketch();
                        }
                        if self.sketch.is_some() {
                            if ui
                                .selectable_label(self.draw == Draw::Rectangle, "Rectangle")
                                .clicked()
                            {
                                self.draw = Draw::Rectangle;
                                self.first = None;
                                self.viewport.align_sketch();
                            }
                            if ui
                                .selectable_label(self.draw == Draw::Circle, "Circle")
                                .clicked()
                            {
                                self.draw = Draw::Circle;
                                self.first = None;
                                self.viewport.align_sketch();
                            }
                            if ui.button("Finish sketch").clicked() {
                                self.sketch = None;
                                self.draw = Draw::Select;
                                self.first = None;
                                self.viewport.set_sketch(None);
                                self.viewport.iso();
                            }
                        }
                        ui.separator();
                        if ui.button("Extrude").clicked() {
                            self.start_feature("feature.pad");
                        }
                        if ui.button("Pocket").clicked() {
                            self.start_feature("feature.pocket");
                        }
                        if ui.button("Fillet").clicked() {
                            self.start_feature("feature.fillet");
                        }
                        ui.menu_button("Packages", |ui| {
                            let packages: Vec<_> = self
                                .registry
                                .entries()
                                .iter()
                                .map(|p| (p.id.clone(), p.label.clone()))
                                .collect();
                            for (id, label) in packages {
                                if ui.button(label).clicked() {
                                    self.start_package(&id);
                                    ui.close();
                                }
                            }
                        });
                    },
                );
                if self.busy {
                    ui.spinner();
                }
            });
        });
    }

    fn panels(&mut self, ctx: &egui::Context) {
        // The narrow feature list overlays the viewport and reads native objects.
        egui::Window::new("Features")
            .id(egui::Id::new("features"))
            .default_pos([10., 82.])
            .default_width(145.)
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                let rows: Vec<_> = objects(&self.state)
                    .iter()
                    .filter(|o| {
                        o["type"] == "Sketcher::SketchObject"
                            || o["type"] != "PartDesign::Body"
                                && o["shape"]["solids"].as_u64().unwrap_or(0) > 0
                    })
                    .map(|o| {
                        (
                            o["name"].as_str().unwrap_or_default().to_owned(),
                            o["label"].as_str().unwrap_or_default().to_owned(),
                            o["type"] == "Sketcher::SketchObject",
                        )
                    })
                    .collect();
                for (name, label, sketch) in rows {
                    let response =
                        ui.selectable_label(self.selected_object.as_ref() == Some(&name), label);
                    if response.clicked() && !self.busy && self.draft.is_none() {
                        self.selected_object = Some(name.clone());
                        if sketch {
                            self.sketch = Some(name.clone());
                            self.sync_sketch();
                            self.viewport.align_sketch();
                        }
                    }
                    if !sketch {
                        if response.double_clicked() && !self.busy && self.draft.is_none() {
                            self.edit_feature(&name);
                        }
                        response.on_hover_text("Double-click to edit its native dimension.");
                    }
                }
                if self.state["objects"].as_array().is_some_and(Vec::is_empty) {
                    ui.weak("No features");
                }
            });
        if self.sketch.is_some() && self.draft.is_none() {
            egui::Window::new("Sketch dimensions")
                .default_pos([175., 85.])
                .default_width(235.)
                .resizable(false)
                .show(ctx, |ui| {
                    if let Some(o) = self.sketch_snapshot() {
                        ui.label(format!(
                            "{} · {} DOF",
                            o["name"].as_str().unwrap_or("Sketch"),
                            o["sketch"]["degrees_of_freedom"]
                        ));
                        if let Some(support) = o["properties"]["AttachmentSupport"]
                            .as_array()
                            .and_then(|s| s.first())
                        {
                            let source = support["link"].as_str().unwrap_or_default();
                            let faces = support["subelements"]
                                .as_array()
                                .map(|a| {
                                    a.iter()
                                        .filter_map(Value::as_str)
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_default();
                            ui.weak(format!("On {source} · {faces}"));
                        }
                    }
                    let mut apply = None;
                    ui.add_enabled_ui(!self.busy, |ui| {
                        for (index, kind, value) in &mut self.dimensions {
                            ui.horizontal(|ui| {
                                ui.label(format!("{kind} {index}"));
                                let r =
                                    ui.add(egui::TextEdit::singleline(value).desired_width(90.));
                                if ui.small_button("Set").clicked()
                                    || r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
                                {
                                    apply = Some((*index, value.clone()));
                                }
                            });
                        }
                    });
                    if let Some((index, value)) = apply {
                        let object = self.sketch.clone().unwrap();
                        self.transaction(
                            "Edit dimension",
                            vec![json!({"op":"datum","object":object,"index":index,"value":value})],
                        );
                    }
                    ui.weak(match self.draw {
                        Draw::Rectangle => {
                            if self.first.is_some() {
                                "Click opposite corner"
                            } else {
                                "Click first corner"
                            }
                        }
                        Draw::Circle => {
                            if self.first.is_some() {
                                "Click radius"
                            } else {
                                "Click circle center"
                            }
                        }
                        Draw::Select => "Rectangle or circle · dimensions in mm",
                    });
                });
        }
        if self.draft.is_some() {
            let mut commit = false;
            let mut cancel = false;
            egui::Window::new("Feature")
                .default_pos([180., 85.])
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    let draft = self.draft.as_mut().unwrap();
                    ui.label(&draft.name);
                    let fields: Vec<_> = draft
                        .parameters
                        .as_object()
                        .unwrap()
                        .keys()
                        .cloned()
                        .collect();
                    ui.add_enabled_ui(!self.busy, |ui| {
                        for field in fields {
                            // Quick modeling tools keep link/name inputs out of the
                            // way. Packages expose every declared parameter.
                            if draft.id == "native.property" && field != "value" {
                                continue;
                            }
                            if !draft.all_parameters
                                && draft.id.starts_with("feature.")
                                && ["body", "name", "profile", "source", "edge"]
                                    .contains(&field.as_str())
                            {
                                continue;
                            }
                            let current = &mut draft.parameters[&field];
                            let mut changed = false;
                            ui.horizontal(|ui| {
                                ui.label(if draft.id == "native.property" {
                                    "Dimension"
                                } else {
                                    &field
                                });
                                if let Some(mut value) = current.as_bool() {
                                    changed = ui.checkbox(&mut value, "").changed();
                                    if changed {
                                        *current = json!(value);
                                    }
                                } else if let Some(mut value) = current.as_i64().filter(|_| {
                                    self.registry
                                        .get(&draft.id)
                                        .and_then(|p| p.parameters.get(&field))
                                        .is_some_and(|p| matches!(p.kind, ParameterType::Integer))
                                }) {
                                    changed = ui.add(egui::DragValue::new(&mut value)).changed();
                                    if changed {
                                        *current = json!(value);
                                    }
                                } else if let Some(mut value) = current.as_f64() {
                                    changed = ui
                                        .add(egui::DragValue::new(&mut value).speed(0.2))
                                        .changed();
                                    if changed {
                                        *current = json!(value);
                                    }
                                } else if let Some(value) = current.as_str() {
                                    let mut value = value.to_owned();
                                    changed = ui
                                        .add(
                                            egui::TextEdit::singleline(&mut value)
                                                .desired_width(150.),
                                        )
                                        .changed();
                                    if changed {
                                        *current = json!(value);
                                    }
                                }
                            });
                            if changed {
                                draft.changed = Some(Instant::now());
                                draft.valid = false;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        commit = ui
                            .add_enabled(
                                !self.busy && draft.valid && draft.changed.is_none(),
                                egui::Button::new("Apply"),
                            )
                            .clicked();
                        cancel = ui
                            .add_enabled(!self.busy, egui::Button::new("Cancel"))
                            .clicked();
                    });
                });
            if commit {
                self.finish_preview(true);
            } else if cancel {
                self.finish_preview(false);
            }
        }
        if let Some(save) = self.file_dialog {
            let mut done = false;
            egui::Modal::new(egui::Id::new("file_dialog")).show(ctx, |ui| {
                ui.heading(if save {
                    "Save native document"
                } else {
                    "Open native document"
                });
                ui.label("FCStd path");
                ui.add(egui::TextEdit::singleline(&mut self.file_input).desired_width(380.));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.busy && !self.file_input.is_empty(),
                            egui::Button::new(if save { "Save" } else { "Open" }),
                        )
                        .clicked()
                    {
                        let different = std::fs::canonicalize(&self.file_input).ok()
                            != std::fs::canonicalize(&self.path).ok();
                        if save && different && PathBuf::from(&self.file_input).is_file() {
                            self.overwrite = true;
                        } else {
                            self.send(
                                if save { "save" } else { "open" },
                                json!({"path":self.file_input}),
                            );
                        }
                        done = true;
                    }
                    if ui.button("Cancel").clicked() {
                        done = true;
                    }
                });
            });
            if done {
                self.file_dialog = None;
            }
        }
        if self.overwrite {
            egui::Modal::new(egui::Id::new("overwrite")).show(ctx, |ui| {
                ui.heading("Replace existing document?");
                ui.label(&self.file_input);
                ui.horizontal(|ui| {
                    if ui.button("Replace").clicked() {
                        self.overwrite = false;
                        self.send("save", json!({"path":self.file_input}));
                    }
                    if ui.button("Cancel").clicked() {
                        self.overwrite = false;
                    }
                });
            });
        }
        if let Some(action) = self.pending_file {
            egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
                ui.label("Discard unsaved changes to this document?");
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.busy && self.draft.is_none(),
                            egui::Button::new("Discard"),
                        )
                        .clicked()
                    {
                        self.pending_file = None;
                        self.file_action(action);
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_file = None;
                    }
                });
            });
        }
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        self.viewport.ui(ui);
        if self.busy || self.draft.is_some() || self.draw == Draw::Select {
            return;
        }
        let Some(response) = &self.viewport.response else {
            return;
        };
        if response.clicked()
            && let Some(position) = response.interact_pointer_pos()
            && let Some(point) = self.viewport.sketch_point(position)
        {
            if let Some(first) = self.first.take() {
                let Some(snapshot) = self.sketch_snapshot() else {
                    return;
                };
                let index = snapshot["sketch"]["geometry_count"].as_u64().unwrap_or(0);
                let object = self.sketch.as_ref().unwrap();
                let (id, parameters) = if self.draw == Draw::Rectangle {
                    (
                        "sketch.rectangle",
                        json!({"object":object,"index":index,"x":first[0].min(point[0]),"y":first[1].min(point[1]),"width":(first[0]-point[0]).abs(),"height":(first[1]-point[1]).abs()}),
                    )
                } else {
                    (
                        "sketch.circle",
                        json!({"object":object,"index":index,"x":first[0],"y":first[1],"radius":((first[0]-point[0]).powi(2)+(first[1]-point[1]).powi(2)).sqrt()}),
                    )
                };
                match self.registry.build(id, &parameters) {
                    Ok(edits) => self.transaction("Draw constrained profile", edits),
                    Err(error) => self.error = Some(error),
                }
            } else {
                self.first = Some(point);
            }
        }
        if let Some(first) = self.first
            && let Some(pos) = self.viewport.response.as_ref().and_then(|r| r.hover_pos())
        {
            let start = self.viewport.sketch_screen_point(first);
            let stroke = egui::Stroke::new(1.3_f32, Color32::from_rgb(112, 183, 177));
            if self.draw == Draw::Rectangle {
                ui.painter().rect_stroke(
                    egui::Rect::from_two_pos(start, pos),
                    0.,
                    stroke,
                    egui::StrokeKind::Middle,
                );
            } else {
                ui.painter()
                    .circle_stroke(start, start.distance(pos), stroke);
            }
        }
    }
}

impl eframe::App for Editor {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.viewport.destroy_gl();
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.busy || self.draft.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.error = Some("Finish or cancel the active feature before closing".into());
            } else if self.dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending_file = Some(FileAction::Close);
            }
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if !self.busy && ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.draft.is_some() {
                self.finish_preview(false);
            } else {
                self.draw = Draw::Select;
                self.first = None;
                self.viewport.selected = None;
            }
        }
        self.toolbar(ctx);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::from_rgb(225, 142, 138), error);
                } else {
                    ui.weak(format!(
                        "{}{} · {} · revision {}",
                        self.state["document"].as_str().unwrap_or("Part Studio"),
                        if self.dirty { " *" } else { "" },
                        self.status,
                        self.state["revision"]
                    ));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak("RMB orbit · MMB pan · wheel zoom");
                });
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(5, 7, 9)))
            .show(ctx, |ui| self.canvas(ui));
        self.panels(ctx);
        // Text editors consume their own Undo first. Button focus must not
        // disable application shortcuts, and modals must block document writes.
        if !self.busy
            && self.draft.is_none()
            && self.pending_file.is_none()
            && self.file_dialog.is_none()
            && !self.overwrite
        {
            let command = egui::Modifiers::COMMAND;
            let shifted = egui::Modifiers {
                shift: true,
                ..command
            };
            if ctx.input_mut(|i| i.consume_key(shifted, Key::S)) {
                self.file_dialog(true);
            } else if ctx.input_mut(|i| i.consume_key(command, Key::S)) {
                self.save();
            } else if ctx.input_mut(|i| i.consume_key(shifted, Key::Z)) {
                self.send("redo", json!({}));
            } else if ctx.input_mut(|i| i.consume_key(command, Key::Z)) {
                self.send("undo", json!({}));
            } else if !ctx.wants_keyboard_input()
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F))
            {
                self.viewport.fit();
            }
        }
        if !self.busy
            && let Some(changed) = self.draft.as_ref().and_then(|d| d.changed)
        {
            if changed.elapsed() >= Duration::from_millis(150) {
                self.preview();
            } else {
                ctx.request_repaint_after(Duration::from_millis(150));
            }
        }
    }
}
