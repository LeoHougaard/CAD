//! The UI owns interaction state. The worker owns all document state and history.

mod assembly;
mod bodies;
mod constraints;
mod drawing;
mod exchange;
mod file_browser;
mod history;
mod measurements;
mod profile_references;
mod properties;
mod recovery;
mod sketch;
mod sketch_creation;
mod sketch_tools;
#[cfg(test)]
mod tests;
mod transforms;
mod units;
mod view;
mod workflow_guide;

use crate::body_commands::display_objects;
use crate::sketch_commands::Tool as Draw;
use bodies::BodyUi;
use sketch::SketchUi;

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

fn consume_exact_key(ctx: &egui::Context, modifiers: egui::Modifiers, key: Key) -> bool {
    ctx.input_mut(|input| {
        let exact = input.events.iter().any(|event| {
            matches!(event,
            egui::Event::Key { key: actual, pressed: true, modifiers: actual_modifiers, .. }
                if *actual == key && actual_modifiers.matches_exact(modifiers))
        });
        exact && input.consume_key(modifiers, key)
    })
}

struct Job {
    op: String,
    parameters: Value,
}
struct Reply {
    op: String,
    accepted: bool,
    state: Value,
    scenes: Vec<(Value, Value)>,
    error: Option<String>,
    elapsed: Duration,
    data: Value,
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
                scenes: Vec::new(),
                error: Some(error.to_string()),
                elapsed: Duration::ZERO,
                data: Value::Null,
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
        let recovery_action = matches!(
            job.op.as_str(),
            "close_dependent" | "remove_dependent_references"
        );
        let result = if recovery_action {
            // The UI action is bound to the inspected unavailable owner and an
            // exact dependent revision. The worker still receives ordinary Close.
            let listed = current["close_dependents"]
                .as_array()
                .is_some_and(|owners| {
                    owners.iter().any(|owner| {
                        owner["document"] == job.parameters["document"]
                            && owner["revision"] == job.parameters["expected_revision"]
                            && (job.op == "close_dependent"
                                || owner["reference_removals"].is_array()
                                    && owner["reference_removals"] == job.parameters["edits"])
                    })
                });
            if current["available"] == false && listed {
                call(
                    &mut worker,
                    if job.op == "close_dependent" {
                        "close"
                    } else {
                        "transaction"
                    },
                    job.parameters,
                )
            } else {
                Err("Recovery dependent changed; inspect the unavailable document again".into())
            }
        } else {
            call(&mut worker, &job.op, job.parameters)
        };
        if matches!(
            job.op.as_str(),
            "feature_schema" | "measure" | "quantities" | "assembly_sources"
        ) {
            let (accepted, data, error) = match result {
                Ok(data) => (true, data, None),
                Err(error) => (false, Value::Null, Some(error)),
            };
            if output
                .send(Reply {
                    op: job.op,
                    accepted,
                    state: current.clone(),
                    scenes: Vec::new(),
                    error,
                    elapsed: start.elapsed(),
                    data,
                })
                .is_err()
            {
                break;
            }
            ctx.request_repaint();
            continue;
        }
        let accepted = result.is_ok();
        let mut error = None;
        match result {
            Ok(result) => {
                if let Some(message) = result["cleanup_error"].as_str() {
                    error = Some(message.to_owned());
                }
                if recovery_action {
                    // Recovering a dependent does not replace the UI's owner.
                } else if job.op == "close" {
                    current = Value::Null;
                } else if result.get("state").is_some() {
                    current = result["state"].clone();
                } else if result.get("objects").is_some() && result.get("document").is_some() {
                    current = result;
                } else if result.get("revision").is_some() {
                    current["revision"] = result["revision"].clone();
                    if job.op == "save" {
                        current["path"] = result["path"].clone();
                        current["save_recovery"] = result["save_recovery"].clone();
                        current["recovery_only"] = result["recovery_only"].clone();
                    }
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
        if (!accepted || recovery_action)
            && let Some(document) = current["document"].as_str()
        {
            match call(&mut worker, "inspect", json!({"document":document})) {
                Ok(state) => current = state,
                Err(message) => {
                    error.get_or_insert(message);
                }
            }
        }
        let mut scenes = Vec::new();
        let mut display_failed = false;
        for object in display_objects(&current)
            .into_iter()
            .filter(|_| current["available"] != false)
        {
            if current["sketch_edit"]["object"] == object {
                continue;
            }
            let parameters =
                json!({"document":current["document"],"object":object,"deflection":0.03});
            match (
                call(&mut worker, "mesh", parameters.clone()),
                call(&mut worker, "topology", parameters),
            ) {
                (Ok(m), Ok(t))
                    if m["revision"] == current["revision"]
                        && t["revision"] == current["revision"]
                        && m["object"] == object
                        && t["object"] == object =>
                {
                    scenes.push((m, t));
                }
                (Err(message), _) | (_, Err(message)) => {
                    error = Some(message);
                    display_failed = true;
                }
                _ => {
                    error = Some("Display object or revision changed".into());
                    display_failed = true;
                }
            }
        }
        if display_failed {
            scenes.clear();
        }
        let reply = Reply {
            op: job.op,
            accepted,
            state: current.clone(),
            scenes,
            error,
            elapsed: start.elapsed(),
            data: Value::Null,
        };
        if output.send(reply).is_err() {
            break;
        }
        ctx.request_repaint();
    }
}

fn feature_edits(registry: &Registry, draft: &Draft, schema: &Value) -> Result<Vec<Value>, String> {
    if draft.id == "native.assembly" {
        crate::assembly_commands::build(&draft.parameters["state"], &draft.parameters["request"])
    } else if draft.id == "native.history" {
        crate::history_commands::build(&draft.parameters["state"], &draft.parameters["request"])
    } else if draft.id == "native.properties" {
        crate::property_commands::build(&crate::hole_commands::active_parameters(
            schema,
            &draft.parameters,
        ))
    } else if draft.id == "native.property" {
        Ok(vec![
            json!({"op":"set","object":draft.parameters["object"],"property":draft.parameters["property"],"value":draft.parameters["value"]}),
        ])
    } else if matches!(draft.id.as_str(), "native.boolean" | "body.boolean")
        && draft.parameters["tools"].is_array()
    {
        let p = &draft.parameters;
        let tools = p["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "Choose native tool Bodies".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut edits = crate::body_commands::boolean_edits(
            p["name"].as_str().unwrap_or_default(),
            p["target"].as_str().unwrap_or_default(),
            &tools,
            p["operation"].as_str().unwrap_or_default(),
        )?;
        if draft.id == "native.boolean" {
            edits.remove(0);
        }
        Ok(edits)
    } else {
        registry.build(&draft.id, &draft.parameters)
    }
}

fn objects(state: &Value) -> &[Value] {
    state["objects"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

struct Draft {
    id: String,
    name: String,
    token: String,
    parameters: Value,
    changed: Option<Instant>,
    valid: bool,
    all_parameters: bool,
    deferred: bool,
    committing: bool,
}
#[derive(Clone, Copy)]
enum FileAction {
    New,
    Open,
    Close,
    Recover,
    RecoverDependent {
        index: usize,
        owner_revision: u64,
        revision: u64,
        remove_references: bool,
    },
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
    sketch_ui: SketchUi,
    sketch_creation: Option<sketch_creation::SketchCreation>,
    body_ui: BodyUi,
    property_ui: properties::PropertyUi,
    measurement_ui: measurements::MeasurementUi,
    units_ui: units::UnitsUi,
    transform_ui: transforms::TransformUi,
    exchange_ui: exchange::ExchangeUi,
    draft: Option<Draft>,
    sequence: u64,
    error: Option<String>,
    status: String,
    path: String,
    file_input: String,
    file_dialog: Option<bool>,
    file_browser: file_browser::FileBrowser,
    overwrite: bool,
    dirty: bool,
    pending_file: Option<FileAction>,
    allow_close: bool,
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
            sketch_ui: SketchUi::default(),
            sketch_creation: None,
            body_ui: BodyUi::default(),
            property_ui: properties::PropertyUi::default(),
            measurement_ui: measurements::MeasurementUi::default(),
            units_ui: Default::default(),
            transform_ui: transforms::TransformUi::default(),
            exchange_ui: exchange::ExchangeUi::default(),
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
            file_browser: file_browser::FileBrowser::default(),
            overwrite: false,
            dirty: false,
            pending_file: None,
            allow_close: false,
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
        if self.busy || self.units_ui.active() && op != "document_settings" {
            return;
        }
        if self.state["recovery_only"] == true
            && !matches!(
                op,
                "save" | "close" | "inspect" | "assembly_sources" | "new" | "open"
            )
            && !(op == "transaction"
                && parameters["edits"].as_array().is_some_and(|edits| {
                    !edits.is_empty() && edits.iter().all(|e| e["op"] == "assembly_relink")
                }))
        {
            self.error=Some("Native links need recovery. Retry Save or Update / relink external source after correcting the native error.".into());
            return;
        }
        if !matches!(op, "feature_schema" | "quantities") {
            self.measurement_invalidate();
        }
        if !matches!(
            op,
            "new" | "open" | "close_dependent" | "remove_dependent_references"
        ) {
            parameters["document"] = self.state["document"].clone();
            parameters["expected_revision"] = self.state["revision"].clone();
            if let Some(token) = &self.sketch_ui.session {
                parameters["sketch_token"] = json!(token);
            }
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
        self.body_ui
            .active
            .as_ref()
            .and_then(|name| {
                objects(&self.state)
                    .iter()
                    .find(|o| o["name"] == *name && o["type"] == "PartDesign::Body")
            })
            .or_else(|| {
                objects(&self.state)
                    .iter()
                    .find(|o| o["type"] == "PartDesign::Body")
            })
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
            self.sketch_ui.tool = Draw::Select;
            self.sketch_ui.clear_points();
        }
        if self
            .selected_object
            .as_ref()
            .is_some_and(|name| !objects(&self.state).iter().any(|o| o["name"] == *name))
        {
            self.selected_object = None;
        }
        self.viewport.set_sketch(snapshot);
    }
    fn poll(&mut self) {
        while let Ok(reply) = self.replies.try_recv() {
            self.busy = false;
            if reply.op == "measure" {
                self.measurement_reply(reply);
                continue;
            }
            let success = reply.accepted;
            if reply.op == "feature_schema" {
                self.error = reply.error;
                if self.transform_ui.awaiting_schema() {
                    self.transform_ui.schema_reply(reply.data, success);
                } else {
                    self.native_schema_reply(reply.data, success);
                }
                continue;
            }
            if reply.op == "quantities" {
                self.error = reply.error;
                if success {
                    self.quantity_reply(&reply.data);
                } else {
                    self.units_ui.quantity_failed();
                }
                continue;
            }
            if reply.op == "assembly_sources" {
                self.error = reply.error;
                if success {
                    self.assembly_source_reply(reply.data);
                }
                continue;
            }
            let units_changed = self.state["units"]["index"] != reply.state["units"]["index"];
            self.error = reply.error;
            self.state = reply.state;
            if reply.op == "save" && !success && self.state["save_recovery"].is_object() {
                self.path = self.state["path"].as_str().unwrap_or_default().into();
                self.file_input = self.path.clone();
                self.dirty = true;
            }
            self.viewport.set_scenes(reply.scenes);
            self.viewport.set_references(&self.state);
            if success
                && self.fit_next
                && (reply.op == "preview"
                    || reply.op == "transaction"
                        && self.draft.as_ref().is_some_and(|d| d.committing))
            {
                self.viewport.fit();
                self.fit_next = false;
            }
            self.status = format!(
                "{} | {:.0} ms",
                if success { "Ready" } else { "Edit rejected" },
                reply.elapsed.as_secs_f64() * 1000.
            );
            if success {
                match reply.op.as_str() {
                    "transaction" | "preview_commit" | "undo" | "redo" => self.dirty = true,
                    "new" | "open" | "close" => {
                        self.dirty = false;
                        self.reset_document_interaction();
                        self.viewport.fit();
                        if reply.op != "open" {
                            self.path.clear();
                            self.file_input.clear();
                        } else if !self.file_input.is_empty() {
                            self.path = self.file_input.clone();
                        }
                    }
                    "document_settings" => {
                        self.dirty |= units_changed;
                        self.units_ui = units::UnitsUi::default();
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
                    if d.id == "native.properties" {
                        self.sync_native_schema();
                        if let Some(draft) = &mut self.draft {
                            self.transform_ui.sync(&mut draft.parameters, &self.state);
                        }
                    }
                }
                if matches!(reply.op.as_str(), "preview_commit" | "preview_cancel")
                    && self.draft.is_some()
                {
                    if self.state["preview"].is_null() {
                        self.draft = None;
                    } else if let Some(draft) = &mut self.draft {
                        draft.valid = false;
                    }
                }
            }
            if reply.op == "transaction" && self.draft.as_ref().is_some_and(|d| d.committing) {
                if success {
                    self.draft = None;
                } else if let Some(d) = &mut self.draft {
                    d.committing = false;
                }
            }
            self.exchange_reply(&reply.op, success);
            self.sync_bodies(&reply.op, success);
            self.sync_sketch();
            self.sketch_creation_reply(&reply.op, success);
            self.sketch_reply(&reply.op, success);
            if matches!(reply.op.as_str(), "undo" | "redo" | "new" | "open") {
                self.sketch_ui.clear_points();
                self.sketch_ui.selected_constraint = None;
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
    fn start_feature(&mut self, id: &str) {
        self.start_native_feature(match id {
            "feature.pad" => "PartDesign::Pad",
            "feature.pocket" => "PartDesign::Pocket",
            "feature.revolution" => "PartDesign::Revolution",
            "feature.fillet" => "PartDesign::Fillet",
            other => other,
        });
    }
    fn preview(&mut self) {
        if self.package_quantities(true) {
            return;
        }
        if let Some(draft) = &self.draft
            && let Some(type_id) = self.transform_ui.schema_request(&draft.parameters)
        {
            let values = draft.parameters["transformations"]
                .as_array()
                .and_then(|steps| steps.iter().find(|step| step["type"] == type_id))
                .map(|step| step["properties"].clone())
                .unwrap_or_default();
            self.send("feature_schema", json!({"type":type_id,"values":values}));
            return;
        }
        if self.transform_ui.awaiting_schema() {
            return;
        }
        let Some(d) = &mut self.draft else {
            return;
        };
        d.changed = None;
        let edits = feature_edits(&self.registry, d, &self.property_ui.schema);
        match edits {
            Ok(edits) => {
                if d.deferred {
                    d.valid = true;
                    return;
                }
                let params = json!({"label":d.name,"token":d.token,"edits":edits});
                self.send("preview", params);
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn edit_feature(&mut self, name: &str) {
        if self.edit_assembly_object(name) {
            return;
        }
        self.edit_native_properties(name);
    }
    fn start_package(&mut self, id: &str) {
        if id == "body.boolean" {
            self.start_boolean(None);
            return;
        }
        let name = self.unique("Feature");
        let package = self.registry.get(id).unwrap();
        let mut parameters = serde_json::Map::new();
        for (key, p) in &package.parameters {
            let value = p
                .default
                .clone()
                .or_else(|| {
                    p.choices
                        .as_ref()
                        .and_then(|choices| choices.first())
                        .cloned()
                })
                .unwrap_or_else(|| match key.as_str() {
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
        self.units_ui.fields.clear();
        self.draft = Some(Draft {
            id: id.into(),
            name: package.label.clone(),
            token: format!("editor-{}-{}", std::process::id(), self.sequence),
            parameters: Value::Object(parameters),
            changed: Some(Instant::now()),
            valid: false,
            all_parameters: true,
            deferred: self.state["redo_count"].as_u64().unwrap_or(0) > 0,
            committing: false,
        });
        self.sketch_ui.tool = Draw::Select;
        self.sketch_ui.clear_points();
        self.package_quantities(false);
    }
    fn file_dialog(&mut self, save: bool) {
        self.file_input = self.path.clone();
        self.file_dialog = Some(save);
        let path = self.path.clone();
        self.browse_file(
            if save {
                file_browser::Purpose::Save
            } else {
                file_browser::Purpose::Open
            },
            &path,
        );
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
        if let Some(d) = &mut self.draft
            && d.deferred
        {
            if !commit {
                self.draft = None;
                return;
            }
            let edits = feature_edits(&self.registry, d, &self.property_ui.schema);
            match edits {
                Ok(edits) => {
                    d.committing = true;
                    let label = d.name.clone();
                    self.transaction(&label, edits);
                }
                Err(error) => self.error = Some(error),
            }
            return;
        }
        if let Some(d) = &self.draft {
            if self.state["preview"].is_null() {
                if !commit {
                    self.fit_next = false;
                    self.draft = None;
                    self.property_ui = properties::PropertyUi::default();
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
            FileAction::Recover => {
                if self.unavailable() {
                    self.send("close", json!({}));
                }
            }
            FileAction::RecoverDependent {
                index,
                owner_revision,
                revision,
                remove_references,
            } => {
                let owner = &self.state["close_dependents"][index];
                if self.unavailable()
                    && self.state["revision"].as_u64() == Some(owner_revision)
                    && owner["revision"].as_u64() == Some(revision)
                {
                    self.send(
                        if remove_references {
                            "remove_dependent_references"
                        } else {
                            "close_dependent"
                        },
                        json!({
                            "document":owner["document"], "expected_revision":revision,
                            "edits":owner["reference_removals"]
                        }),
                    );
                } else {
                    self.error = Some(
                        "Recovery dependent changed; inspect the unavailable document again".into(),
                    );
                }
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
                let idle = self.sketch_ui.task.is_none()
                    && self.sketch_ui.drag.is_none()
                    && self.sketch_ui.dimension.is_none()
                    && !self.busy
                    && (!self.unavailable() || self.source_recovery())
                    && self.draft.is_none()
                    && self.sketch_creation.is_none()
                    && self.pending_file.is_none()
                    && self.file_dialog.is_none()
                    && !self.overwrite
                    && !self.exchange_ui.active()
                    && !self.file_browser.active();
                ui.add_enabled_ui(idle, |ui| {
                    if self.sketch_ui.session.is_none()
                        && ui
                            .add_enabled(!self.unavailable(), egui::Button::new("New"))
                            .clicked()
                    {
                        self.request_file(FileAction::New);
                    }
                    if self.sketch_ui.session.is_none()
                        && ui
                            .add_enabled(!self.unavailable(), egui::Button::new("Open"))
                            .clicked()
                    {
                        self.request_file(FileAction::Open);
                    }
                    if self.sketch_ui.session.is_none() && ui.button("Save").clicked() {
                        self.save();
                    }
                    if self.sketch_ui.session.is_none() && ui.button("Save as").clicked() {
                        self.file_dialog(true);
                    }
                    ui.add_enabled_ui(!self.unavailable(), |ui| self.exchange_controls(ui));
                    ui.separator();
                    if ui
                        .add_enabled(
                            !self.unavailable()
                                && self.state["undo_count"].as_u64().unwrap_or(0) > 0,
                            egui::Button::new("Undo"),
                        )
                        .clicked()
                    {
                        self.send("undo", json!({}));
                    }
                    if ui
                        .add_enabled(
                            !self.unavailable()
                                && self.state["redo_count"].as_u64().unwrap_or(0) > 0,
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
                    self.view_controls(ui);
                    if ui
                        .button("Help")
                        .on_hover_text("Workflow guide (F1)")
                        .clicked()
                    {
                        workflow_guide::open(ctx);
                    }
                    self.measurement_controls(ui);
                    self.units_controls(ui);
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(
                    !self.busy
                        && (!self.unavailable() || self.source_recovery())
                        && self.sketch_ui.drag.is_none()
                        && self.sketch_ui.dimension.is_none()
                        && self.sketch_ui.task.is_none()
                        && self.draft.is_none()
                        && self.sketch_creation.is_none()
                        && self.pending_file.is_none()
                        && self.file_dialog.is_none()
                        && !self.exchange_ui.active()
                        && !self.file_browser.active(),
                    |ui| {
                        if self.source_recovery() {
                            self.assembly_toolbar(ui);
                            return;
                        }
                        if self.sketch_ui.session.is_none() {
                            self.body_toolbar(ui);
                            self.assembly_toolbar(ui);
                        }
                        if self.sketch_ui.session.is_none() {
                            let sketch = ui.button("Sketch");
                            if sketch.clicked() {
                                self.new_sketch();
                            }
                            sketch.context_menu(|ui| {
                                if ui.button("Create sketch with options").clicked() {
                                    self.new_sketch_with_options();
                                    ui.close();
                                }
                            });
                        }
                        if self.sketch.is_some() {
                            self.sketch_toolbar(ui);
                        }
                        ui.separator();
                        if ui.button("Extrude").clicked() {
                            self.feature_from_sketch("feature.pad");
                        }
                        if ui.button("Pocket").clicked() {
                            self.feature_from_sketch("feature.pocket");
                        }
                        if self.sketch_ui.session.is_none() && ui.button("Fillet").clicked() {
                            self.start_feature("feature.fillet");
                        }
                        ui.add_enabled_ui(self.sketch_ui.session.is_none(), |ui| {
                            self.native_feature_menu(ui);
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
        if !self.source_recovery() {
            self.units_dialog(ctx);
            self.body_tree(ctx);
            self.sketch_panel(ctx);
        }
        if self.draft.is_some() {
            let mut commit = false;
            let mut cancel = false;
            egui::Window::new("Feature")
                .default_pos([180., 85.])
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    let boolean = self.draft.as_ref().is_some_and(|d| {
                        matches!(d.id.as_str(), "body.boolean" | "native.boolean")
                    });
                    if boolean {
                        ui.label(self.draft.as_ref().unwrap().name.clone());
                        self.boolean_fields(ui);
                    }
                    let native = self
                        .draft
                        .as_ref()
                        .is_some_and(|d| d.id == "native.properties");
                    if native {
                        egui::ScrollArea::vertical()
                            .id_salt("feature_native_task")
                            .max_height((ctx.content_rect().height() - 220.).clamp(150., 650.))
                            .show(ui, |ui| self.native_property_fields(ui));
                    }
                    let assembly = self
                        .draft
                        .as_ref()
                        .is_some_and(|d| d.id == "native.assembly");
                    if assembly {
                        self.assembly_fields(ui);
                    }
                    let history = self
                        .draft
                        .as_ref()
                        .is_some_and(|d| d.id == "native.history");
                    if history {
                        self.history_fields(ui);
                    }
                    let draft = self.draft.as_mut().unwrap();
                    if !boolean && !native && !history && !assembly {
                        ui.label(&draft.name);
                    }
                    if draft.deferred {
                        ui.weak("Apply updates the model. Cancel keeps Redo.");
                    }
                    let fields: Vec<_> = draft
                        .parameters
                        .as_object()
                        .unwrap()
                        .keys()
                        .filter(|_| !boolean && !native && !history && !assembly)
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
                                let choices = self
                                    .registry
                                    .get(&draft.id)
                                    .and_then(|p| p.parameters.get(&field))
                                    .and_then(|p| p.choices.as_ref());
                                if let Some(choices) = choices {
                                    egui::ComboBox::from_id_salt(("package_choice", &field))
                                        .selected_text(
                                            current
                                                .as_str()
                                                .map(str::to_owned)
                                                .unwrap_or_else(|| current.to_string()),
                                        )
                                        .show_ui(ui, |ui| {
                                            for value in choices {
                                                changed |= ui
                                                    .selectable_value(
                                                        current,
                                                        value.clone(),
                                                        value
                                                            .as_str()
                                                            .map(str::to_owned)
                                                            .unwrap_or_else(|| value.to_string()),
                                                    )
                                                    .changed();
                                            }
                                        });
                                } else if let Some(mut value) = current.as_bool() {
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
                                } else if self
                                    .registry
                                    .get(&draft.id)
                                    .and_then(|p| p.parameters.get(&field))
                                    .is_some_and(|p| p.unit.is_some())
                                {
                                    let meta =
                                        self.units_ui.fields.get(&field).unwrap_or(&Value::Null);
                                    changed = units::quantity_field(ui, current, meta);
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
    }

    fn canvas(&mut self, ui: &mut egui::Ui, sketch_input_blocked: bool) {
        let axis_profile = self.property_ui.axis_profile.as_deref();
        let axis_active = !self.unavailable()
            && self.property_ui.axis_pick
            && self.draft.as_ref().is_some_and(|draft| {
                draft.parameters["type"] == "PartDesign::Revolution"
                    && draft.parameters["properties"]["Profile"]["link"].as_str() == axis_profile
                    && axis_profile.is_some()
            });
        if axis_active {
            let sketch = objects(&self.state)
                .iter()
                .find(|o| o["name"].as_str() == axis_profile)
                .cloned();
            if !self.viewport.sketch_matches(&sketch) {
                self.viewport.set_sketch(sketch);
            }
        } else if self.property_ui.axis_profile.is_some() {
            let sketch = self.sketch_snapshot();
            if !self.viewport.sketch_matches(&sketch) {
                self.viewport.set_sketch(sketch);
            }
        }
        self.viewport.solid_select_in_sketch = self.sketch_ui.external;
        self.viewport.sketch_select_enabled = !self.sketch_ui.external
            && self.sketch_ui.dimension_tool.is_none()
            && self.sketch_ui.tool == Draw::Select
            && self.sketch_ui.drag.is_none()
            && !self.sketch_ui.trim
            && !self.sketch_ui.split
            && self.sketch_ui.task.is_none();
        let before = self.viewport.selected.clone();
        let picked = self.viewport.ui(ui).is_some();
        if self.viewport.selected != before {
            if !self.measurement_keeps_selection(ui.input(|i| i.modifiers.shift)) {
                self.body_ui.selected.clear();
            }
            self.selected_object = self
                .viewport
                .selected
                .as_ref()
                .map(|selection| selection.object.clone());
        }
        if axis_active
            && !self.busy
            && self.draft.is_some()
            && self.viewport.response.as_ref().is_some_and(|r| r.clicked())
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
            && let Some(hit) = self.viewport.sketch_hit(pos)
            && hit.point == 0
            && let Some(profile) = self.property_ui.axis_profile.clone()
        {
            let subelement = match hit.index {
                -1 => Some("H_Axis".to_owned()),
                -2 => Some("V_Axis".to_owned()),
                _ => None,
            };
            if subelement.is_none() {
                self.error = Some("Choose a native sketch horizontal or vertical axis".into());
            }
            if let Some(subelement) = subelement
                && let Some(draft) = &mut self.draft
            {
                draft.parameters["properties"]["ReferenceAxis"] =
                    json!({"link":profile,"subelements":[subelement]});
                draft.changed = Some(Instant::now());
                draft.valid = false;
                self.property_ui.axis_pick = false;
                self.viewport.set_sketch(None);
            }
        }
        if !self.unavailable() {
            self.sketch_canvas(ui, picked, sketch_input_blocked);
        }
    }
}

impl eframe::App for Editor {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.viewport.destroy_gl();
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let was_busy = self.busy;
        let shortcut_blocked = self.busy
            || egui::Popup::is_any_open(ctx)
            || self.unavailable()
            || self.units_ui.active()
            || self.measurement_is_open()
            || self.pending_file.is_some()
            || self.file_dialog.is_some()
            || self.overwrite
            || self.exchange_ui.active()
            || self.file_browser.active()
            || self.sketch_creation.is_some()
            || self.draft.is_some()
            || self.sketch_ui.drag.is_some()
            || self.sketch_ui.dimension.is_some()
            || self.sketch_ui.task.is_some();
        self.poll();
        if self.draft.is_none() && self.property_ui.axis_profile.is_some() {
            self.property_ui.axis_pick = false;
            self.property_ui.axis_profile = None;
            self.viewport.set_sketch(self.sketch_snapshot());
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F1)) {
            workflow_guide::open(ctx);
        }
        let guide_was_open = workflow_guide::active(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.unavailable() && !self.source_recovery() && !self.busy {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending_file = Some(FileAction::Recover);
            } else if self.busy
                || self.draft.is_some()
                || self.sketch_creation.is_some()
                || self.sketch_ui.session.is_some()
                || self.exchange_ui.active()
                || self.file_browser.active()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.error = Some("Finish or cancel the active operation before closing".into());
            } else if self.dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending_file = Some(FileAction::Close);
            }
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if (workflow_guide::active(ctx) || !self.units_ui.active())
            && ctx.input(|i| i.key_pressed(Key::Escape))
        {
            if workflow_guide::active(ctx) {
                workflow_guide::close(ctx);
                ctx.input_mut(|i| i.consume_key(i.modifiers, Key::Escape));
            } else if self.viewport.dismiss_other_menu(ctx) {
                ctx.input_mut(|i| i.consume_key(i.modifiers, Key::Escape));
            } else if egui::Popup::is_any_open(ctx) {
                egui::Popup::close_all(ctx);
                ctx.input_mut(|i| i.consume_key(i.modifiers, Key::Escape));
            } else if self.measurement_escape() {
                ctx.input_mut(|i| i.consume_key(i.modifiers, Key::Escape));
            } else if self.unavailable() && !self.source_recovery() {
                self.pending_file = None;
            } else if self.file_browser.active() {
                self.cancel_file_browser();
            } else if self.exchange_ui.active() {
                self.exchange_ui.cancel();
            } else if self.file_dialog.is_some() {
                self.file_dialog = None;
            } else if self.overwrite {
                self.overwrite = false;
            } else if self.pending_file.is_some() {
                self.pending_file = None;
            } else if self.sketch_creation.is_some() && !self.busy {
                self.sketch_creation = None;
            } else if self.draft.is_some() && !self.busy {
                self.finish_preview(false);
            } else {
                self.sketch_escape();
            }
        }
        if !was_busy
            && !self.busy
            && !self.units_ui.active()
            && !self.measurement_is_open()
            && self.pending_file.is_none()
            && self.file_dialog.is_none()
            && !self.overwrite
            && !self.exchange_ui.active()
            && !self.file_browser.active()
            && !workflow_guide::active(ctx)
            && !egui::Popup::is_any_open(ctx)
            && !ctx.wants_keyboard_input()
            && self.property_ui.axis_profile.is_some()
            && self.draft.is_some()
            && consume_exact_key(ctx, egui::Modifiers::NONE, Key::Tab)
        {
            self.property_ui.axis_pick = true;
        }
        self.toolbar(ctx);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            if self.unavailable() && !self.source_recovery() {
                ui.horizontal(|ui| self.recovery_controls(ui));
            }
            if self.sketch_ui.session.is_some()
                && let Some(hint) = self.sketch_ui.hint()
            {
                ui.label(hint);
            }

            ui.horizontal(|ui| {
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::from_rgb(225, 142, 138), error);
                } else {
                    ui.weak(format!(
                        "{}{} | {} | revision {}",
                        self.state["document"].as_str().unwrap_or("Part Studio"),
                        if self.dirty { " *" } else { "" },
                        self.status,
                        self.state["revision"]
                    ));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak("RMB orbit | MMB pan | wheel zoom");
                });
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(5, 7, 9)))
            .show(ctx, |ui| {
                ui.add_enabled_ui(!guide_was_open && !workflow_guide::active(ctx), |ui| {
                    self.canvas(ui, shortcut_blocked)
                });
            });
        if self.source_recovery() && !guide_was_open && !workflow_guide::active(ctx) {
            self.panels(ctx);
            self.file_browser_ui(ctx);
        } else if !self.unavailable() && !guide_was_open && !workflow_guide::active(ctx) {
            if !self.exchange_ui.active() && !self.file_browser.active() {
                self.panels(ctx);
            }
            self.sketch_creation_dialog(ctx);
            self.exchange_dialog(ctx);
            self.file_browser_ui(ctx);
            self.sketch_pump(ctx, shortcut_blocked || egui::Popup::is_any_open(ctx));
        }
        if !guide_was_open && !workflow_guide::active(ctx) {
            self.measurement_ui(ctx, shortcut_blocked || egui::Popup::is_any_open(ctx));
            self.file_action_dialog(ctx);
        }
        workflow_guide::show(ctx);
        let guide_is_open = workflow_guide::active(ctx);
        if guide_was_open || guide_is_open {
            self.sketch_help_input(ctx, !guide_is_open);
        }
        if !was_busy
            && !self.busy
            && !self.units_ui.active()
            && !self.measurement_is_open()
            && self.pending_file.is_none()
            && self.file_dialog.is_none()
            && !self.overwrite
            && !self.exchange_ui.active()
            && !self.file_browser.active()
            && !guide_was_open
            && !guide_is_open
            && !egui::Popup::is_any_open(ctx)
            && !ctx.wants_keyboard_input()
            && !self.property_ui.axis_pick
            && self.draft.as_ref().is_some_and(|d| {
                d.valid
                    && d.changed.is_none()
                    && matches!(
                        d.parameters["type"].as_str(),
                        Some("PartDesign::Pad" | "PartDesign::Revolution")
                    )
            })
            && consume_exact_key(ctx, egui::Modifiers::NONE, Key::Enter)
        {
            self.finish_preview(true);
        }
        // Text editors consume their own Undo first. Button focus must not
        // disable application shortcuts, and modals must block document writes.
        if !self.busy
            && !guide_was_open
            && !workflow_guide::active(ctx)
            && (!self.unavailable() || self.source_recovery())
            && self.draft.is_none()
            && self.sketch_creation.is_none()
            && self.pending_file.is_none()
            && self.file_dialog.is_none()
            && !self.overwrite
            && !self.exchange_ui.active()
            && !self.file_browser.active()
            && self.sketch_ui.drag.is_none()
            && self.sketch_ui.dimension.is_none()
            && self.sketch_ui.task.is_none()
        {
            let command = egui::Modifiers::COMMAND;
            let shifted = egui::Modifiers {
                shift: true,
                ..command
            };
            if self.sketch_ui.session.is_none() && ctx.input_mut(|i| i.consume_key(shifted, Key::S))
            {
                self.file_dialog(true);
            } else if self.sketch_ui.session.is_none()
                && ctx.input_mut(|i| i.consume_key(command, Key::S))
            {
                self.save();
            } else if ctx.input_mut(|i| i.consume_key(shifted, Key::Z)) {
                self.send("redo", json!({}));
            } else if ctx.input_mut(|i| i.consume_key(command, Key::Z)) {
                self.send("undo", json!({}));
            } else if !shortcut_blocked
                && !egui::Popup::is_any_open(ctx)
                && !ctx.wants_keyboard_input()
                && self.sketch_ui.session.is_none()
                && consume_exact_key(ctx, egui::Modifiers::SHIFT, Key::S)
            {
                self.new_sketch();
            } else if !shortcut_blocked
                && !egui::Popup::is_any_open(ctx)
                && !ctx.wants_keyboard_input()
                && consume_exact_key(ctx, egui::Modifiers::SHIFT, Key::E)
            {
                self.feature_from_sketch("feature.pad");
            } else if !shortcut_blocked
                && !egui::Popup::is_any_open(ctx)
                && !ctx.wants_keyboard_input()
                && consume_exact_key(ctx, egui::Modifiers::SHIFT, Key::W)
            {
                self.feature_from_sketch("feature.revolution");
            } else if !shortcut_blocked
                && !egui::Popup::is_any_open(ctx)
                && !ctx.wants_keyboard_input()
                && self.sketch_ui.session.is_some()
                && consume_exact_key(ctx, egui::Modifiers::NONE, Key::N)
            {
                self.viewport.align_sketch();
            } else if !ctx.wants_keyboard_input()
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F))
            {
                self.viewport.fit();
            }
        }
        if !self.busy
            && (!self.unavailable() || self.source_recovery())
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
