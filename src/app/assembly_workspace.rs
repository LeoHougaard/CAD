use super::*;
use crate::{
    assembly::{BindingKind, Context, ContextBinding, Transform},
    assembly_ui::{Action, AssemblyUi, Host},
};

struct PartBackup {
    document: Document,
    saved: Document,
    path: Option<PathBuf>,
    past: VecDeque<Document>,
    future: Vec<Document>,
    scene: Arc<Scene>,
    source_scene: Option<Arc<Scene>>,
    editor: Option<Editor>,
    sketch_ui: SketchUi,
    selection: Selection,
    filter: Filter,
    selected_feature: Option<usize>,
    selected_plane: Option<String>,
    hidden: BTreeSet<String>,
    hidden_sketches: BTreeSet<String>,
    show_sketches: bool,
    show_planes: bool,
    choosing_plane: bool,
    camera: Camera,
    error: Option<String>,
    notice: String,
}
pub(super) struct ContextEdit {
    context: Context,
    pub(super) ghosts: Arc<Scene>,
    backup: PartBackup,
    world_camera: Camera,
    pub(super) picking: Option<BindingKind>,
    offset: f64,
    pending_bind: Option<Document>,
    save_after: bool,
    display: Option<((usize, u64), Arc<Scene>)>,
    source_display: Option<((usize, u64), Arc<Scene>)>,
    ghost_parts: BTreeSet<String>,
}
impl PartBackup {
    fn take(app: &mut CadApp) -> Self {
        Self {
            document: std::mem::take(&mut app.document),
            saved: std::mem::take(&mut app.saved),
            path: app.path.take(),
            past: std::mem::take(&mut app.past),
            future: std::mem::take(&mut app.future),
            scene: std::mem::replace(&mut app.scene, Arc::new(Scene::default())),
            source_scene: app.source_scene.take(),
            editor: app.editor.take(),
            sketch_ui: std::mem::take(&mut app.sketch_ui),
            selection: std::mem::take(&mut app.selection),
            filter: app.selection_filter,
            selected_feature: app.selected_feature.take(),
            selected_plane: app.selected_plane.take(),
            hidden: std::mem::take(&mut app.hidden),
            hidden_sketches: std::mem::take(&mut app.hidden_sketches),
            show_sketches: app.show_sketches,
            show_planes: app.show_planes,
            choosing_plane: app.choosing_plane,
            camera: app.assembly_camera.clone(),
            error: app.last_error.take(),
            notice: std::mem::take(&mut app.notice),
        }
    }
    fn restore(self, app: &mut CadApp) {
        app.document = self.document;
        app.saved = self.saved;
        app.path = self.path;
        app.past = self.past;
        app.future = self.future;
        app.scene = self.scene;
        app.source_scene = self.source_scene;
        app.editor = self.editor;
        app.sketch_ui = self.sketch_ui;
        app.selection = self.selection;
        app.selection_filter = self.filter;
        app.selected_feature = self.selected_feature;
        app.selected_plane = self.selected_plane;
        app.hidden = self.hidden;
        app.hidden_sketches = self.hidden_sketches;
        app.show_sketches = self.show_sketches;
        app.show_planes = self.show_planes;
        app.choosing_plane = self.choosing_plane;
        app.assembly_camera = self.camera;
        app.last_error = self.error;
        app.notice = self.notice;
        app.pending_rebuild = None;
        app.fit_when_ready = false;
        app.align_when_ready = false;
    }
}
fn local_camera(camera: &Camera, transform: Transform) -> Camera {
    let inverse = transform.inverse();
    let mut local = camera.clone();
    local.center = glam::Vec3::from_array(
        inverse
            .point(camera.center.to_array().map(f64::from))
            .map(|v| v as f32),
    );
    local.pivot = glam::Vec3::from_array(
        inverse
            .point(camera.pivot.to_array().map(f64::from))
            .map(|v| v as f32),
    );
    local.rotation =
        (glam::Quat::from_array(inverse.rotation.map(|v| v as f32)) * camera.rotation).normalize();
    local
}
impl CadApp {
    fn ensure_assembly(&mut self, ctx: &egui::Context) {
        if self.assembly_workspace.is_none() {
            self.assembly_workspace = Some(AssemblyUi::new(ctx, self.registry.clone()));
        }
    }
    pub fn open_assembly_path(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.switch_to_assembly(ctx);
        if let Some(assembly) = &mut self.assembly_workspace {
            assembly.open(path);
        }
    }
    pub(super) fn switch_to_assembly(&mut self, ctx: &egui::Context) {
        if self.assembly_context.is_some() {
            self.notice = "Commit or cancel the current context first".into();
            return;
        }
        self.ensure_assembly(ctx);
        if !self.assembly_mode {
            std::mem::swap(&mut self.camera, &mut self.assembly_camera);
            self.assembly_mode = true;
        }
    }
    fn switch_to_part_studio(&mut self) {
        if self.assembly_mode {
            std::mem::swap(&mut self.camera, &mut self.assembly_camera);
            self.assembly_mode = false;
        }
    }
    pub(super) fn poll_assembly(&mut self, ctx: &egui::Context) {
        let actions = self
            .assembly_workspace
            .as_mut()
            .map(AssemblyUi::poll)
            .unwrap_or_default();
        for action in actions {
            self.assembly_action(ctx, action);
        }
    }
    fn assembly_action(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::PartStudio => self.switch_to_part_studio(),
            Action::Settings => self.open_settings(),
            Action::VisibleEdges(value) => {
                self.shaded_edges = value;
                self.settings.shaded_edges = value;
            }
            Action::BeginContext {
                context,
                document,
                ghosts,
            } => {
                if self.assembly_context.is_some() {
                    self.last_error = Some("Another context is already being edited".into());
                    return;
                }
                let world_camera = self.camera.clone();
                let camera = local_camera(&world_camera, context.primary_transform);
                let ghost_parts = ghosts.meshes.iter().map(|m| m.part.clone()).collect();
                let backup = PartBackup::take(self);
                self.assembly_context = Some(ContextEdit {
                    context,
                    ghosts,
                    backup,
                    world_camera,
                    picking: None,
                    offset: 0.,
                    pending_bind: None,
                    save_after: false,
                    display: None,
                    source_display: None,
                    ghost_parts,
                });
                self.document = document.clone();
                self.saved = document;
                self.path = None;
                self.camera = camera;
                self.assembly_mode = false;
                self.show_sketches = false;
                self.show_planes = false;
                self.choosing_plane = false;
                self.pending_rebuild = None;
                self.fit_when_ready = false;
                self.align_when_ready = false;
                self.serial += 1;
                self.rebuild();
            }
            Action::BoundContext { context, document } => {
                let previous = self
                    .assembly_context
                    .as_mut()
                    .and_then(|s| s.pending_bind.take());
                if previous.as_ref() != Some(&self.preview_document()) {
                    self.notice="The feature changed while its reference was being checked. Pick the face again.".into();
                    return;
                }
                if let Some(edit) = &mut self.editor
                    && let Some(feature) =
                        document.features.iter().find(|f| f.id == edit.feature.id)
                {
                    edit.record(edit.feature.params.clone(), self.choosing_plane);
                    edit.feature.params = feature.params.clone();
                    edit.changed = true;
                }
                if let Some(session) = &mut self.assembly_context {
                    session.context = context;
                    session.picking = None;
                }
                self.choosing_plane = false;
                self.align_when_ready = self.editor.as_ref().is_some_and(|e| e.sketch.is_some());
                self.rebuild();
            }
            Action::CommittedContext => {
                if let Some(session) = self.assembly_context.take() {
                    let save_after = session.save_after;
                    let camera = session.world_camera;
                    session.backup.restore(self);
                    self.camera = camera;
                    self.assembly_mode = true;
                    self.rebuild();
                    if save_after && let Some(assembly) = &mut self.assembly_workspace {
                        assembly.save_workspace();
                    }
                }
            }
        }
        ctx.request_repaint();
    }
    pub(super) fn assembly_workspace_ui(&mut self, ctx: &egui::Context) {
        self.ensure_assembly(ctx);
        let ready = !self.work.busy() && self.editor.is_none() && self.scene.error.is_none();
        let actions = self.assembly_workspace.as_mut().unwrap().show(
            ctx,
            Host {
                camera: &mut self.camera,
                space: &mut self.space_mouse,
                settings: &self.settings,
                renderer: self.renderer.clone(),
                registry: self.registry.clone(),
                document: &self.document,
                path: self.path.as_deref(),
                scene: &self.scene,
                part_ready: ready,
                shaded_edges: self.shaded_edges,
                blocked: self.settings_open || self.close_pending,
            },
        );
        for action in actions {
            self.assembly_action(ctx, action);
        }
    }
    pub(super) fn context_toolbar(&mut self, ctx: &egui::Context) {
        if self.assembly_context.is_none() {
            return;
        }
        let busy = self
            .assembly_workspace
            .as_ref()
            .is_some_and(AssemblyUi::busy);
        let valid = !busy
            && !self.work.busy()
            && self.pending_rebuild.is_none()
            && self.editor.is_none()
            && self.last_error.is_none();
        let mut commit = false;
        let mut cancel = false;
        egui::TopBottomPanel::top("context_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Editing in assembly context");
                commit = ui
                    .add_enabled(valid, egui::Button::new("Commit context"))
                    .clicked();
                cancel = ui
                    .add_enabled(!busy, egui::Button::new("Cancel context"))
                    .clicked();
                if self.editor.is_some() {
                    ui.weak("Finish the active feature to commit");
                }
                if let Some(error) = self.assembly_workspace.as_ref().and_then(AssemblyUi::error) {
                    ui.colored_label(RED, error);
                }
            });
        });
        if commit {
            self.commit_context(false);
        }
        if cancel {
            self.cancel_context();
        }
    }
    pub(super) fn commit_context(&mut self, save_after: bool) {
        if self.editor.is_some()
            || self.work.busy()
            || self.pending_rebuild.is_some()
            || self.last_error.is_some()
        {
            self.notice =
                "Finish the active feature before committing and saving the assembly".into();
            return;
        }
        let Some(session) = &mut self.assembly_context else {
            return;
        };
        if self
            .assembly_workspace
            .as_ref()
            .is_some_and(AssemblyUi::busy)
        {
            return;
        }
        session.context.bindings = self
            .document
            .features
            .iter()
            .filter_map(|feature| {
                let value = feature.params.get("context_ref")?;
                if value.get("context")?.as_str()? != session.context.id {
                    return None;
                }
                Some(ContextBinding {
                    feature: feature.id.clone(),
                    instance: value.get("instance")?.as_str()?.into(),
                    face: value.get("face")?.as_str()?.into(),
                    kind: serde_json::from_value(value.get("kind")?.clone()).ok()?,
                    offset: value.get("offset").and_then(Value::as_f64).unwrap_or(0.),
                })
            })
            .collect();
        session.save_after = save_after;
        if let Some(assembly) = &mut self.assembly_workspace {
            assembly.commit_context(session.context.clone(), self.document.clone());
        }
    }
    fn cancel_context(&mut self) {
        if let Some(session) = self.assembly_context.take() {
            let camera = session.world_camera;
            session.backup.restore(self);
            self.camera = camera;
            self.assembly_mode = true;
            self.serial += 1;
            self.rebuild();
        }
    }
    pub(super) fn context_picking(&self) -> bool {
        self.assembly_context
            .as_ref()
            .is_some_and(|s| s.picking.is_some())
    }
    pub(super) fn context_reference_controls(
        &mut self,
        ui: &mut egui::Ui,
        edit: &mut Editor,
    ) -> bool {
        let mut changed = false;
        let bound = edit
            .feature
            .params
            .get("context_ref")
            .filter(|v| !v.is_null())
            .cloned();
        let frozen_plane = edit
            .feature
            .params
            .get("context_plane")
            .is_some_and(|v| !v.is_null());
        let extent = edit
            .feature
            .params
            .get("context_up_to")
            .is_some_and(|v| !v.is_null());
        if bound.is_some() || frozen_plane || extent {
            let name = bound
                .as_ref()
                .and_then(|r| r["instance"].as_str())
                .map(|id| {
                    self.assembly_context
                        .as_ref()
                        .and_then(|s| s.context.instances.iter().find(|i| i.id == id))
                        .map(|i| i.name.as_str())
                        .unwrap_or(id)
                });
            ui.label(match (extent, name) {
                (true, Some(name)) => format!("Up to face: {name}"),
                (true, None) => "Frozen up-to face".into(),
                (false, Some(name)) => format!("Plane on: {name}"),
                _ => "Frozen plane".into(),
            });
            if bound.is_some() || extent {
                if ui
                    .small_button("Detach reference")
                    .on_hover_text("Keep the current geometry and remove the assembly dependency")
                    .clicked()
                {
                    let plane = edit.feature.params["sketch"]
                        .as_str()
                        .and_then(|id| self.scene.sketches.get(id))
                        .map(|s| &s.plane);
                    match detach_reference(&mut edit.feature.params, plane) {
                        Ok(()) => changed = true,
                        Err(error) => self.notice = error,
                    }
                }
            } else if frozen_plane && ui.small_button("Choose new plane").clicked() {
                self.choosing_plane = true;
                self.show_planes = true;
            }
            ui.separator();
        }
        let Some(session) = &mut self.assembly_context else {
            return changed;
        };
        let kind = if edit.sketch.is_some() {
            Some(BindingKind::SketchPlane)
        } else if edit.feature.plugin == "extrude" {
            Some(BindingKind::ExtrudeUpToFace)
        } else {
            None
        };
        if let Some(kind) = kind {
            let busy = self
                .assembly_workspace
                .as_ref()
                .is_some_and(AssemblyUi::busy);
            if ui
                .add_enabled(
                    !busy,
                    egui::Button::new(match kind {
                        BindingKind::SketchPlane => "Pick external sketch plane",
                        BindingKind::ExtrudeUpToFace => "Pick external up-to face",
                    }),
                )
                .clicked()
            {
                session.picking = Some(kind);
                self.fit_when_ready = true;
            }
            ui.horizontal(|ui| {
                ui.label("Reference offset");
                ui.add(egui::DragValue::new(&mut session.offset).speed(0.1));
            });
            if session.picking.is_some() {
                ui.label("Choose a planar face on another instance");
                if ui.small_button("Cancel pick").clicked() {
                    session.picking = None;
                }
            }
        }
        changed
    }
    pub(super) fn pick_context_reference(&mut self, pos: egui::Pos2, rect: Rect) {
        let Some(session) = &self.assembly_context else {
            return;
        };
        let Some(kind) = session.picking else {
            return;
        };
        let Some(hit) = render::pick(&session.ghosts, &self.camera, pos, rect, &BTreeSet::new())
        else {
            return;
        };
        if hit.plane.is_none() {
            self.notice = "Choose a planar external face".into();
            return;
        }
        let Some(edit) = &self.editor else {
            return;
        };
        let feature = edit.feature.id.clone();
        let document = self.preview_document();
        let face = hit
            .face
            .split_once("::")
            .map_or(hit.face.as_str(), |(_, face)| face)
            .to_owned();
        let session = self.assembly_context.as_mut().unwrap();
        let binding = ContextBinding {
            feature,
            instance: hit.part,
            face,
            kind,
            offset: session.offset,
        };
        session.pending_bind = Some(document.clone());
        session.picking = None;
        if let Some(assembly) = &mut self.assembly_workspace {
            assembly.bind_context(session.context.clone(), document, binding);
        }
    }
    pub(super) fn context_display_scene(&mut self, base: Arc<Scene>, source: bool) -> Arc<Scene> {
        let Some(session) = &mut self.assembly_context else {
            return base;
        };
        let pointer = (Arc::as_ptr(&base) as usize, self.render_revision);
        let cache = if source {
            &mut session.source_display
        } else {
            &mut session.display
        };
        if let Some((old, scene)) = cache
            && *old == pointer
        {
            return scene.clone();
        }
        let mut scene = (*base).clone();
        let mut id = scene
            .meshes
            .iter()
            .flat_map(|m| m.faces.iter().map(|f| f.global_id))
            .max()
            .unwrap_or(0)
            + 1;
        for mesh in &session.ghosts.meshes {
            let mut mesh = mesh.clone();
            for face in &mut mesh.faces {
                face.global_id = id;
                id += 1;
            }
            scene.meshes.push(mesh);
        }
        let scene = Arc::new(scene);
        *cache = Some((pointer, scene.clone()));
        scene
    }
    pub(super) fn context_ghost_parts(&self) -> BTreeSet<String> {
        self.assembly_context
            .as_ref()
            .map(|s| s.ghost_parts.clone())
            .unwrap_or_default()
    }
}

fn detach_reference(params: &mut Value, sketch_plane: Option<&Plane>) -> crate::model::Result<()> {
    if let Some(value) = params.get("context_up_to").filter(|v| !v.is_null()) {
        let target: Plane = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        let sketch = sketch_plane.ok_or("Rebuild this feature before detaching its up-to face")?;
        let depth = (0..3)
            .map(|n| (target.origin[n] - sketch.origin[n]) * sketch.normal[n])
            .sum::<f64>()
            * if params["reverse"].as_bool().unwrap_or(false) {
                -1.
            } else {
                1.
            };
        if !depth.is_finite() || depth <= 1e-7 {
            return Err(
                "The current up-to extent cannot be preserved; repair the feature first".into(),
            );
        }
        params["depth"] = json!(depth.to_string());
    }
    if let Some(object) = params.as_object_mut() {
        object.remove("context_ref");
        object.remove("context_up_to");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detach_preserves_explicit_sketch_plane_and_converts_extent_to_current_depth() {
        let plane = Plane::named("Top", 23.);
        let mut params = json!({"context_plane":plane,"context_ref":{"context":"c"}});
        detach_reference(&mut params, None).unwrap();
        assert_eq!(params["context_plane"], json!(plane));
        assert!(params.get("context_ref").is_none());
        let mut params = json!({"depth":"5","context_up_to":Plane::named("Top",23.),"context_ref":{"context":"c"}});
        detach_reference(&mut params, Some(&Plane::named("Top", 3.))).unwrap();
        assert_eq!(params["depth"], "20");
        assert!(params.get("context_up_to").is_none());
    }
}
