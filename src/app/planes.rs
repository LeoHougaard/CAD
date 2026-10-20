//! Origin and feature planes share one drawing, picking and attachment path.
use super::*;

pub(super) fn support_overrides_origin(params: &Value) -> bool {
    params.get("context_plane").is_some_and(|v| !v.is_null())
        || params.get("support_ref").is_some_and(|v| !v.is_null())
        || params["support"].as_str().is_some_and(|s| !s.is_empty())
}

enum SupportAction { Change, Detach }
fn support_controls(ui: &mut egui::Ui, label: &str, choosing: bool, can_detach: bool) -> Option<SupportAction> {
    ui.label(RichText::new("Sketch support").small().color(MUTED));
    if choosing {
        ui.label("Select a plane or planar face");
        return None;
    }
    ui.add(egui::Label::new(label).truncate());
    ui.horizontal(|ui| {
        if ui.small_button("Change").clicked() { return Some(SupportAction::Change); }
        if ui.add_enabled(can_detach, egui::Button::new("Detach").small()).clicked() { return Some(SupportAction::Detach); }
        None
    }).inner
}

// Highlight the same support that regeneration will resolve. An unattached
// draft is handled by the caller; a missing reference never becomes Top.
pub(super) fn support_target(params: &Value, scene: &Scene) -> Option<Target> {
    if params.get("context_plane").is_some_and(|v| !v.is_null()) { return None; }
    if let Some(reference) = params.get("support_ref").filter(|v| !v.is_null()) {
        return match serde_json::from_value::<crate::model::PlaneReference>(reference.clone()).ok()? {
            crate::model::PlaneReference::Origin { plane } => Some(Target::Plane(plane)),
            crate::model::PlaneReference::Datum { feature } => Some(Target::Plane(feature)),
            crate::model::PlaneReference::Face { part, face } => Some(Target::Face { part, key: face }),
        };
    }
    if let Some(support) = params["support"].as_str().filter(|s| !s.is_empty()) {
        if scene.planes.contains_key(support) { return Some(Target::Plane(support.into())); }
        let mut matches = scene.meshes.iter().flat_map(|m| m.faces.iter().filter(move |f| reference_matches(&f.key, support))
            .map(move |f| Target::Face { part: m.part.clone(), key: f.key.clone() }));
        let first = matches.next()?;
        return matches.next().is_none().then_some(first);
    }
    params["plane"].as_str().filter(|plane| matches!(*plane, "Top" | "Front" | "Right"))
        .map(|plane| Target::Plane(plane.into()))
}

pub(super) fn attach(params: &mut Value, id: &str) -> crate::model::Result<()> {
    let key = if matches!(id, "Top" | "Front" | "Right") { "plane" } else { "support" };
    crate::model::set_sketch_support(params, key, json!(id))
        .map(|_| ())
}

pub(super) fn attach_face(params: &mut Value, part: &str, face: &str) -> crate::model::Result<()> {
    let reference = crate::model::PlaneReference::Face {
        part: part.into(), face: face.into(),
    };
    crate::model::set_sketch_support(params, "support_ref", json!(reference))
        .map(|_| ())
}

pub(super) fn detach(params: &mut Value, evaluated: &Plane) {
    params["context_plane"] = json!(evaluated);
    params["offset"] = json!("0");
    params["support"] = json!("");
    if let Some(object) = params.as_object_mut() {
        object.remove("context_ref");
        object.remove("support_ref");
    }
}

impl CadApp {
    pub(super) fn sketch_support_ui(&mut self, ui: &mut egui::Ui, edit: &mut Editor) -> bool {
        let frozen = edit.feature.params.get("context_plane").is_some_and(|v| !v.is_null());
        let label = if frozen {
            if edit.feature.params.get("context_ref").is_some_and(|v| !v.is_null()) { "In-context plane".into() }
            else { "Detached plane".into() }
        } else {
            support_target(&edit.feature.params, self.source_scene.as_deref().unwrap_or(&self.scene))
                .map(|target| self.target_label(&target)).unwrap_or_else(|| "Missing support".into())
        };
        let can_detach = !frozen && !self.work.busy() && self.pending_rebuild.is_none()
            && self.last_error.is_none() && self.scene.sketches.contains_key(&edit.feature.id);
        match support_controls(ui, &label, self.choosing_plane, can_detach) {
            Some(SupportAction::Change) => {
                self.choosing_plane = true;
                self.show_planes = true;
                self.align_when_ready = false;
                self.selection.clear();
                self.selected_plane = None;
                self.sketch_ui.cancel_gesture();
                true
            }
            Some(SupportAction::Detach) => {
                let sketch = &self.scene.sketches[&edit.feature.id];
                detach(&mut edit.feature.params, &sketch.plane);
                self.selection.clear();
                self.selected_plane = None;
                true
            }
            None => false,
        }
    }
    pub(super) fn plane_frame(&self, id: &str) -> Option<Plane> {
        if matches!(id, "Top" | "Front" | "Right") {
            return Some(Plane::named(id, 0.));
        }
        self.scene
            .planes
            .get(id)
            .or_else(|| self.source_scene.as_ref().and_then(|s| s.planes.get(id)))
            .cloned()
    }
    pub(super) fn plane_name(&self, id: &str) -> String {
        self.document
            .features
            .iter()
            .find(|f| f.id == id)
            .map_or_else(|| id.to_owned(), |f| f.name.clone())
    }
    pub(super) fn selected_plane_id(&self) -> Option<String> {
        self.selection
            .items()
            .iter()
            .rev()
            .find_map(|target| match target {
                Target::Plane(id) | Target::Feature(id) if self.plane_frame(id).is_some() => {
                    Some(id.clone())
                }
                _ => None,
            })
            .or_else(|| {
                self.selected_plane
                    .as_ref()
                    .filter(|id| self.plane_frame(id).is_some())
                    .cloned()
            })
    }
    pub(super) fn selected_sketch_support(&self) -> Option<Target> {
        self.selection
            .items()
            .iter()
            .rev()
            .find_map(|target| match target {
                Target::Plane(id) | Target::Feature(id) if self.plane_frame(id).is_some() => {
                    Some(Target::Plane(id.clone()))
                }
                Target::Face { .. } => Some(target.clone()),
                _ => None,
            })
    }
    pub(super) fn selected_support_plane(&self) -> Option<Plane> {
        match self.selected_sketch_support()? {
            Target::Plane(id) => self.plane_frame(&id),
            Target::Face { part, key } => self
                .source_scene
                .as_ref()
                .unwrap_or(&self.scene)
                .meshes
                .iter()
                .find(|m| m.part == part)?
                .faces
                .iter()
                .find(|f| f.key == key)?
                .plane
                .clone(),
            _ => None,
        }
    }
    pub(super) fn plane_input_active(&self) -> bool {
        self.editor.as_ref().is_some_and(|edit| {
            feature_inputs::active_key(edit, &self.registry)
                .and_then(|key| {
                    self.registry
                        .plugins
                        .get(&edit.feature.plugin)?
                        .manifest
                        .fields
                        .iter()
                        .find(|f| f.key == key)
                })
                .is_some_and(|f| f.kind == "plane_ref")
        })
    }
    fn displayed_planes(&self) -> Vec<(String, Plane)> {
        if !self.show_planes {
            return vec![];
        }
        // Aligned parallel planes share projected labels. During sketch editing,
        // only the chosen support is relevant; choosing a support still shows all.
        if let Some(edit) = &self.editor
            && edit.sketch.is_some()
            && !self.choosing_plane
        {
            let Some(Target::Plane(id)) = support_target(&edit.feature.params,
                self.source_scene.as_deref().unwrap_or(&self.scene)) else { return vec![]; };
            return self
                .plane_frame(&id)
                .filter(|_| !self.hidden_sketches.contains(&id))
                .map(|plane| vec![(id, plane)])
                .unwrap_or_default();
        }
        let selecting = self.choosing_plane || self.plane_input_active();
        let selected = self.selected_plane_id();
        let mut planes = BTreeMap::new();
        for id in ["Top", "Front", "Right"] {
            if (selecting || self.scene.meshes.is_empty() || selected.as_deref() == Some(id))
                && !self.hidden_sketches.contains(id)
            {
                planes.insert(id.to_owned(), Plane::named(id, 0.));
            }
        }
        if let Some(source) = &self.source_scene {
            planes.extend(source.planes.clone());
        }
        planes.extend(self.scene.planes.clone());
        planes.retain(|id, _| !self.hidden_sketches.contains(id));
        planes.into_iter().collect()
    }
    fn plane_intersections(&self, pos: egui::Pos2, rect: Rect) -> Vec<(f32, String)> {
        let (origin, ray) = self.camera.ray(pos, rect);
        let mut hits = vec![];
        for (id, plane) in self.displayed_planes() {
            if self.plane_input_active() && self.editor.as_ref().is_some_and(|e| e.feature.id == id)
            {
                continue;
            }
            if let Some(p) = self.camera.plane_point(pos, rect, &plane)
                && p.into_iter()
                    .all(|v| v.abs() <= render::ORIGIN_PLANE_HALF_SIZE)
            {
                let point = glam::Vec3::from_array(plane.at(p).map(|v| v as f32));
                let distance = (point - origin).dot(ray);
                if distance > 0. {
                    hits.push((distance, id));
                }
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        hits
    }
    pub(super) fn plane_hits(&self, pos: egui::Pos2, rect: Rect) -> Vec<String> {
        self.plane_intersections(pos, rect)
            .into_iter()
            .map(|(_, id)| id)
            .collect()
    }
    pub(super) fn sketch_support_hit(
        &self,
        scene: &Scene,
        pos: egui::Pos2,
        rect: Rect,
    ) -> Option<Target> {
        let face = render::pick(scene, &self.camera, pos, rect, &self.hidden);
        if let Some((distance, id)) = self.plane_intersections(pos, rect).into_iter().next()
            && face.as_ref().is_none_or(|f| distance <= f.distance + 1e-4)
        {
            return Some(Target::Plane(id));
        }
        face.filter(|f| f.plane.is_some()).map(|f| Target::Face {
            part: f.part,
            key: f.face,
        })
    }
    pub(super) fn paint_planes(&self, ui: &egui::Ui, rect: Rect, pointer: Option<egui::Pos2>) {
        let hover = pointer.and_then(|p| self.plane_hits(p, rect).into_iter().next());
        let mut planes = self.displayed_planes();
        planes.sort_by_key(|(id, _)| {
            std::cmp::Reverse(
                self.editor.as_ref().is_some_and(|e| e.feature.id == *id) as u8 * 2
                    + (self.selection.contains(&Target::Plane(id.clone()))
                        || self.selection.contains(&Target::Feature(id.clone())))
                        as u8,
            )
        });
        let mut labels: Vec<(Rect, egui::epaint::Mesh)> = Vec::new();
        for (id, plane) in planes {
            let selected = self.selection.contains(&Target::Plane(id.clone()))
                || self.selection.contains(&Target::Feature(id.clone()))
                || self.selected_plane.as_ref() == Some(&id);
            let input = self.editor.as_ref().is_some_and(|edit| {
                edit.feature.id == id
                    || edit.feature.params["support"] == id
                    || ["reference", "second"].iter().any(|key| {
                        let value = &edit.feature.params[key];
                        value["feature"] == id || value["plane"] == id
                    })
            });
            let color = if selected {
                rgb(self.settings.appearance.turquoise)
            } else if input || hover.as_ref() == Some(&id) {
                rgb(self.settings.appearance.purple)
            } else {
                MUTED
            };
            let points = render::origin_plane_points(&plane)
                .into_iter()
                .map(|p| self.camera.project(p, rect))
                .collect();
            ui.painter().add(egui::Shape::convex_polygon(
                points,
                color.gamma_multiply(if input || selected { 0.10 } else { 0.04 }),
                Stroke::new(1_f32, color),
            ));
            if let Some(mesh) = render::plane_label_mesh(
                ui,
                &self.camera,
                &plane,
                rect,
                &self.plane_name(&id),
                color,
            ) {
                let bounds = mesh.vertices.iter().fold(Rect::NOTHING, |bounds, vertex| {
                    bounds.union(Rect::from_min_max(vertex.pos, vertex.pos))
                });
                if !labels
                    .iter()
                    .any(|(other, _)| other.expand(4.).intersects(bounds))
                {
                    labels.push((bounds, mesh));
                }
            }
        }
        for (_, mesh) in labels {
            ui.painter().add(egui::Shape::mesh(mesh));
        }
    }
    pub(super) fn choose_plane(&mut self, id: &str) {
        if self.plane_frame(id).is_none() {
            return;
        }
        self.selected_plane = Some(id.into());
        if let Some(edit) = &mut self.editor
            && edit.sketch.is_some()
        {
            let before = edit.feature.params.clone();
            if let Err(error) = attach(&mut edit.feature.params, id) {
                self.last_error = Some(error);
                return;
            }
            if before != edit.feature.params || self.choosing_plane {
                edit.record(before, self.choosing_plane);
            }
            self.choosing_plane = false;
            self.align_when_ready = true;
            self.rebuild();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bound_support_row_never_presents_the_inactive_origin_choice() {
        let mut scene = Scene::default();
        scene.planes.insert("f8".into(), Plane::named("Front", 12.));
        for params in [
            json!({"plane":"Top","support_ref":{"kind":"datum","feature":"f8"}}),
            json!({"plane":"Top","support":"f8"}),
            json!({"plane":"Top","support_ref":{"kind":"face","part":"f2:0","face":"f2:end"}}),
            json!({"plane":"Top","context_plane":Plane::named("Front",12.)}),
        ] {
            assert!(support_overrides_origin(&params));
            assert_ne!(support_target(&params,&scene), Some(Target::Plane("Top".into())));
        }
        assert!(!support_overrides_origin(&json!({"plane":"Top","support":"","support_ref":null})));
        let params = json!({"plane":"Top","support_ref":{"kind":"datum","feature":"f8"}});
        assert_eq!(support_target(&params,&scene),Some(Target::Plane("f8".into())));
        let ctx = egui::Context::default();
        let output = ctx.run(egui::RawInput::default(),|ctx| {
            egui::CentralPanel::default().show(ctx,|ui| { let _ = support_controls(ui,"Plane 1",false,true); });
        });
        let text:Vec<_> = output.shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.as_str()), _ => None,
        }).collect();
        assert!(text.contains(&"Plane 1"),"{text:?}");
        assert!(!text.contains(&"Top"),"The actual support row must not contain an origin selector");
        assert!(text.contains(&"Change") && text.contains(&"Detach"));
    }
    #[test]
    fn support_highlight_uses_the_binding_instead_of_an_obsolete_origin_plane() {
        let scene = Scene::default();
        let mut params = json!({"plane":"Top","support":"missing-face"});
        assert_eq!(support_target(&params, &scene), None);
        attach_face(&mut params, "f2:0", "f2:end").unwrap();
        assert_eq!(support_target(&params, &scene), Some(Target::Face { part: "f2:0".into(), key: "f2:end".into() }));
        attach(&mut params, "Right").unwrap();
        assert_eq!(support_target(&params, &scene), Some(Target::Plane("Right".into())));
        detach(&mut params, &Plane::named("Front", 7.));
        assert_eq!(support_target(&params, &scene), None, "A detached frame has no selected source plane");
    }
    #[test]
    fn datum_attachment_is_explicit_and_detach_preserves_evaluated_frame() {
        let mut params = json!({"plane":"Top","offset":"3","support":"","context_ref":{},"support_ref":{"kind":"face","part":"f2:0","face":"f2:end"}});
        attach(&mut params, "f8").unwrap();
        assert_eq!(params["support"], "f8");
        assert!(params.get("context_ref").is_none());
        assert!(params.get("support_ref").is_none());
        attach_face(&mut params, "f2:0", "f2:end:patch:0123456789abcdef").unwrap();
        assert_eq!(params["support_ref"], json!({"kind":"face","part":"f2:0","face":"f2:end:patch:0123456789abcdef"}));
        assert!(params.get("context_plane").is_none());
        let frame = Plane::named("Right", 27.);
        detach(&mut params, &frame);
        assert_eq!(params["context_plane"], json!(frame));
        assert_eq!(params["offset"], "0");
        assert!(params.get("support_ref").is_none());
        assert_eq!(params["support"], "");
        attach_face(&mut params, "f2:0", "f2:end").unwrap();
        attach(&mut params, "Front").unwrap();
        assert_eq!(params["plane"], "Front");
        assert!(params.get("support_ref").is_none());
        assert!(params.get("context_plane").is_none());
        let before = params.clone();
        assert!(attach_face(&mut params, "f2:0", "untracked:face").is_err());
        assert_eq!(params, before);
    }
}
