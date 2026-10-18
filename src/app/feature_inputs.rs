//! Selection belongs to one active feature input, never every compatible field.
use super::*;

pub(super) fn selectable(kind: &str) -> bool {
    matches!(
        kind,
        "regions" | "profile_face" | "axis" | "edges" | "faces" | "body" | "bodies"
    )
}

pub(super) fn visible(field: &Field, feature: &Feature) -> bool {
    let p = &feature.params;
    !((matches!(field.kind.as_str(), "body" | "bodies") && p["mode"] == "New")
        || (p.get("profile_face").is_some_and(|v| !v.is_null())
            && matches!(field.kind.as_str(), "sketch" | "regions"))
        || (feature.plugin == "boolean" && field.key == "targets" && p["mode"] != "Subtract")
        || (matches!(feature.plugin.as_str(), "extrude" | "revolve")
            && ((field.key == "target" && p.get("targets").is_some())
                || (field.key == "targets" && p.get("targets").is_none())))
        || (feature.plugin == "revolve"
            && matches!(field.key.as_str(), "axis" | "axis_offset")
            && p.get("axis_ref").is_some_and(|v| !v.is_null())))
}

fn input<'a>(edit: &Editor, registry: &'a Registry) -> Option<&'a Field> {
    let fields = &registry.plugins.get(&edit.feature.plugin)?.manifest.fields;
    edit.input_field
        .as_ref()
        .and_then(|key| {
            fields
                .iter()
                .find(|f| &f.key == key && visible(f, &edit.feature))
        })
        .or_else(|| {
            [
                "regions",
                "profile_face",
                "edges",
                "faces",
                "bodies",
                "axis",
                "body",
            ]
            .into_iter()
            .find_map(|kind| {
                fields
                    .iter()
                    .find(|f| f.kind == kind && visible(f, &edit.feature))
            })
        })
}

pub(super) fn active_key<'a>(edit: &Editor, registry: &'a Registry) -> Option<&'a str> {
    input(edit, registry).map(|f| f.key.as_str())
}

pub(super) fn referenced_sketches<'a>(feature: &'a Feature, registry: &Registry) -> Vec<&'a str> {
    let p = &feature.params;
    let mut ids = Vec::new();
    if feature.sketch().is_some() {
        ids.push(feature.id.as_str());
    }
    if let Some(plugin) = registry.plugins.get(&feature.plugin) {
        for field in &plugin.manifest.fields {
            match field.kind.as_str() {
                "sketch" => ids.extend(p[&field.key].as_str().filter(|s| !s.is_empty())),
                "sketches" => ids.extend(
                    p[&field.key]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str),
                ),
                _ => (),
            }
        }
    }
    if matches!(
        p["axis_ref"]["kind"].as_str(),
        Some("sketch_line" | "sketch_entity")
    ) {
        ids.extend(p["axis_ref"]["sketch"].as_str());
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub(super) fn input_ui(ui: &mut egui::Ui, field: &Field, value: &mut Value, scene: &Scene) -> bool {
    let before = value.clone();
    match field.kind.as_str() {
        "bodies" => {
            let mut selected: Vec<String> = value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            egui::ScrollArea::vertical()
                .id_salt((&field.key, "parts"))
                .max_height(120.)
                .show(ui, |ui| {
                    for mesh in &scene.meshes {
                        let mut checked = selected.contains(&mesh.part);
                        if ui.checkbox(&mut checked, &mesh.name).changed() {
                            selected.retain(|s| s != &mesh.part);
                            if checked {
                                selected.push(mesh.part.clone());
                            }
                        }
                    }
                    selected.retain(|key| {
                        if scene.meshes.iter().any(|m| &m.part == key) {
                            return true;
                        }
                        let mut keep = true;
                        ui.horizontal(|ui| {
                            ui.colored_label(RED, "Missing part").on_hover_text(key);
                            if ui.small_button("Remove").clicked() {
                                keep = false;
                            }
                        });
                        keep
                    });
                });
            if ui.small_button("Clear").clicked() {
                selected.clear();
            }
            *value = json!(selected);
        }
        "profile_face" | "axis" => {
            let mut options = vec![(
                Value::Null,
                if field.kind == "axis" {
                    "Sketch axis".into()
                } else {
                    "None".into()
                },
            )];
            if field.kind == "profile_face" {
                for mesh in &scene.meshes {
                    for (index, face) in mesh
                        .faces
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| f.plane.is_some())
                    {
                        options.push((
                            json!({"part":mesh.part,"face":face.key}),
                            format!("{} / face {}", mesh.name, index + 1),
                        ));
                    }
                }
            } else {
                for axis in ["X", "Y", "Z"] {
                    options.push((
                        json!({"kind":"origin","axis":axis}),
                        format!("Origin {axis}"),
                    ));
                }
                for (id, sketch) in &scene.sketches {
                    for entity in sketch
                        .entities
                        .iter()
                        .filter(|e| matches!(e.kind.as_str(), "line" | "circle" | "arc"))
                    {
                        options.push((
                            json!({"kind":"sketch_entity","sketch":id,"entity":entity.id}),
                            format!("{id} / {} {}", entity.kind, entity.id),
                        ));
                    }
                }
                for mesh in &scene.meshes {
                    for (index, edge) in mesh
                        .edges
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| !e.seam && e.axis.is_some())
                    {
                        options.push((
                            json!({"kind":"edge","part":mesh.part,"edge":edge.key}),
                            format!("{} / edge {}", mesh.name, index + 1),
                        ));
                    }
                }
            }
            let label = options
                .iter()
                .find(|(v, _)| v == value)
                .map(|(_, s)| s.as_str())
                .unwrap_or("Missing reference");
            egui::ComboBox::from_id_salt(&field.key)
                .width(ui.available_width())
                .selected_text(label)
                .truncate()
                .show_ui(ui, |ui| {
                    for (candidate, name) in options {
                        ui.selectable_value(value, candidate, name);
                    }
                });
            if !value.is_null() && ui.small_button("Clear").clicked() {
                *value = Value::Null;
            }
        }
        _ => (),
    }
    *value != before
}

pub(super) fn seed(feature: &mut Feature, targets: &[Target], scene: &Scene) -> Option<String> {
    if feature.plugin == "boolean" {
        feature.params["tools"] = json!(parts(targets));
        return Some("tools".into());
    }
    let selected_parts = parts(targets);
    if feature.params.get("targets").is_some() && !selected_parts.is_empty() {
        feature.params["targets"] = json!(selected_parts);
        feature.params["target"] = json!("");
    }
    if feature.params.get("profile_face").is_some() {
        if let Some(Target::Region { sketch, .. }) = targets
            .iter()
            .rev()
            .find(|t| matches!(t, Target::Region { .. }))
        {
            feature.params["sketch"] = json!(sketch);
            feature.params["regions"] = json!(
                targets
                    .iter()
                    .filter_map(|t| match t {
                        Target::Region { sketch: id, key } if id == sketch => Some(key),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            );
        } else if let Some(Target::Face { part, key }) = targets
            .iter()
            .rev()
            .find(|t| matches!(t, Target::Face { .. }))
            && scene.meshes.iter().any(|m| {
                &m.part == part && m.faces.iter().any(|f| &f.key == key && f.plane.is_some())
            })
        {
            feature.params["profile_face"] = json!({"part":part,"face":key});
            return Some("profile_face".into());
        }
    }
    None
}

pub(super) fn filter(edit: &Editor, registry: &Registry) -> Filter {
    match input(edit, registry).map(|f| f.kind.as_str()) {
        Some("regions") => Filter::Regions,
        Some("profile_face" | "faces") => Filter::Faces,
        Some("axis" | "edges") => Filter::Edges,
        Some("body" | "bodies") => Filter::Parts,
        _ => Filter::All,
    }
}

fn parts(targets: &[Target]) -> Vec<String> {
    let mut result = vec![];
    for target in targets {
        if let Target::Part(part) | Target::Face { part, .. } | Target::Edge { part, .. } = target
            && !result.contains(part)
        {
            result.push(part.clone());
        }
    }
    result
}

fn apply_selection(params: &mut Value, field: &Field, targets: &[Target], scene: &Scene) {
    if !targets.is_empty()
        && !targets.iter().any(|t| match (field.kind.as_str(), t) {
            ("body" | "bodies", Target::Part(_) | Target::Face { .. } | Target::Edge { .. })
            | ("regions", Target::Region { .. })
            | ("faces", Target::Face { .. })
            | ("edges", Target::Edge { .. }) => true,
            ("profile_face", Target::Face { part, key }) => scene.meshes.iter().any(|m| {
                &m.part == part && m.faces.iter().any(|f| &f.key == key && f.plane.is_some())
            }),
            ("axis", Target::Edge { part, key }) => scene.meshes.iter().any(|m| {
                &m.part == part && m.edges.iter().any(|e| &e.key == key && e.axis.is_some())
            }),
            _ => false,
        })
    {
        return;
    }
    params[&field.key] = match field.kind.as_str() {
        "bodies" => json!(parts(targets)),
        "body" => json!(parts(targets).last().cloned().unwrap_or_default()),
        "edges" => json!(
            targets
                .iter()
                .filter_map(|t| if let Target::Edge { key, .. } = t {
                    Some(key)
                } else {
                    None
                })
                .collect::<Vec<_>>()
        ),
        "faces" => json!(
            targets
                .iter()
                .filter_map(|t| if let Target::Face { key, .. } = t {
                    Some(key)
                } else {
                    None
                })
                .collect::<Vec<_>>()
        ),
        "profile_face" => targets
            .iter()
            .rev()
            .find_map(|t| match t {
                Target::Face { part, key }
                    if scene.meshes.iter().any(|m| {
                        &m.part == part
                            && m.faces.iter().any(|f| &f.key == key && f.plane.is_some())
                    }) =>
                {
                    Some(json!({"part":part,"face":key}))
                }
                _ => None,
            })
            .unwrap_or(Value::Null),
        "axis" => targets
            .iter()
            .rev()
            .find_map(|t| match t {
                Target::Edge { part, key }
                    if scene.meshes.iter().any(|m| {
                        &m.part == part && m.edges.iter().any(|e| &e.key == key && e.axis.is_some())
                    }) =>
                {
                    Some(json!({"kind":"edge","part":part,"edge":key}))
                }
                _ => None,
            })
            .unwrap_or(Value::Null),
        "regions" => {
            if let Some(sketch) = targets.iter().rev().find_map(|t| {
                if let Target::Region { sketch, .. } = t {
                    Some(sketch)
                } else {
                    None
                }
            }) {
                params["sketch"] = json!(sketch);
                if params.get("profile_face").is_some() {
                    params["profile_face"] = Value::Null;
                }
            }
            let sketch = params["sketch"].as_str().unwrap_or_default();
            json!(
                targets
                    .iter()
                    .filter_map(|t| match t {
                        Target::Region { sketch: id, key } if id == sketch => Some(key),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            )
        }
        _ => return,
    };
}

impl CadApp {
    pub(super) fn paint_feature_axis(&self, ui: &egui::Ui, scene: &Scene, rect: Rect) {
        let Some(edit) = &self.editor else { return };
        if edit.feature.plugin != "revolve" {
            return;
        }
        let p = &edit.feature.params;
        let reference = &p["axis_ref"];
        let frame = match reference["kind"].as_str() {
            Some("origin") => {
                let axis = match reference["axis"].as_str() {
                    Some("X") => [1., 0., 0.],
                    Some("Y") => [0., 1., 0.],
                    Some("Z") => [0., 0., 1.],
                    _ => return,
                };
                Some(([0.; 3], axis))
            }
            Some("sketch_line" | "sketch_entity") => scene
                .sketches
                .get(reference["sketch"].as_str().unwrap_or_default())
                .and_then(|sk| {
                    sk.entities
                        .iter()
                        .find(|e| Some(e.id) == reference["entity"].as_u64())
                        .map(|e| (sk, e))
                })
                .map(|(sk, e)| {
                    if e.kind == "line" {
                        let points = e.points();
                        (sk.plane.at(points[0]), sk.plane.at(points[1]))
                    } else {
                        let center = e
                            .arc()
                            .map(|a| a.center)
                            .unwrap_or([e.values[0], e.values[1]]);
                        let a = sk.plane.at(center);
                        (a, std::array::from_fn(|n| a[n] + sk.plane.normal[n]))
                    }
                }),
            Some("edge") => scene
                .meshes
                .iter()
                .find(|m| Some(m.part.as_str()) == reference["part"].as_str())
                .and_then(|m| {
                    m.edges
                        .iter()
                        .find(|e| Some(e.key.as_str()) == reference["edge"].as_str())
                })
                .and_then(|e| {
                    e.axis
                        .map(|(a, d)| (a, std::array::from_fn(|n| a[n] + d[n])))
                }),
            _ => scene
                .sketches
                .get(p["sketch"].as_str().unwrap_or_default())
                .map(|sk| {
                    let offset = Numbers::new(&self.document.variables)
                        .and_then(|n| n.eval(p["axis_offset"].as_str().unwrap_or("0")))
                        .unwrap_or(0.);
                    if p["axis"] == "U" {
                        (sk.plane.at([0., offset]), sk.plane.at([1., offset]))
                    } else {
                        (sk.plane.at([offset, 0.]), sk.plane.at([offset, 1.]))
                    }
                }),
        };
        let Some((a, b)) = frame else { return };
        let a = glam::Vec3::from_array(a.map(|v| v as f32));
        let d = glam::Vec3::from_array(b.map(|v| v as f32)) - a;
        if d.length_squared() < 1e-12 {
            return;
        }
        let d = d.normalize() * self.camera.span;
        ui.painter().line_segment(
            [
                self.camera.project((a - d).to_array().map(f64::from), rect),
                self.camera.project((a + d).to_array().map(f64::from), rect),
            ],
            Stroke::new(2_f32, rgb(self.settings.appearance.purple)),
        );
    }
    pub(super) fn route_feature_selection(&mut self, ctx: &egui::Context) {
        let scene = self.source_scene.as_ref().unwrap_or(&self.scene);
        let Some(edit) = &mut self.editor else { return };
        let Some(field) = input(edit, &self.registry) else {
            return;
        };
        let before = edit.feature.params.clone();
        apply_selection(
            &mut edit.feature.params,
            field,
            self.selection.items(),
            scene,
        );
        // A blend's edge/face input determines its single owning part. Boolean
        // target/tool lists remain independent because only the active input runs.
        if matches!(field.kind.as_str(), "edges" | "faces") {
            let owners = parts(self.selection.items());
            if owners.len() == 1
                && let Some(plugin) = self.registry.plugins.get(&edit.feature.plugin)
            {
                for body in plugin.manifest.fields.iter().filter(|f| f.kind == "body") {
                    edit.feature.params[&body.key] = json!(owners[0]);
                }
            }
        }
        if before != edit.feature.params {
            edit.undo.push(before);
            edit.redo.clear();
            edit.changed = true;
            self.schedule(ctx);
        }
    }

    pub(super) fn restore_feature_selection(&mut self) {
        let Some(edit) = &self.editor else { return };
        let Some(field) = input(edit, &self.registry) else {
            return;
        };
        self.selection.clear();
        let value = &edit.feature.params[&field.key];
        let scene = self.source_scene.as_ref().unwrap_or(&self.scene);
        let strings = || {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
        };
        match field.kind.as_str() {
            "body" => {
                if let Some(part) = value.as_str().filter(|s| !s.is_empty()) {
                    self.selection.add(Target::Part(part.into()));
                }
            }
            "bodies" => {
                for part in strings() {
                    self.selection.add(Target::Part(part.into()));
                }
            }
            "profile_face" => {
                if let (Some(part), Some(key)) = (value["part"].as_str(), value["face"].as_str()) {
                    self.selection.add(Target::Face {
                        part: part.into(),
                        key: key.into(),
                    });
                }
            }
            "axis" => {
                if value["kind"] == "edge"
                    && let (Some(part), Some(key)) =
                        (value["part"].as_str(), value["edge"].as_str())
                {
                    self.selection.add(Target::Edge {
                        part: part.into(),
                        key: key.into(),
                    });
                }
            }
            "regions" => {
                let sketch = edit.feature.params["sketch"].as_str().unwrap_or_default();
                if let Some(regions) = scene.regions.get(sketch) {
                    for r in regions.iter().filter(|r| {
                        value.as_array().map_or(r.legacy_selected, |a| {
                            a.iter().any(|v| v.as_str() == Some(&r.key))
                        })
                    }) {
                        self.selection.add(Target::Region {
                            sketch: sketch.into(),
                            key: r.key.clone(),
                        });
                    }
                }
            }
            "edges" | "faces" => {
                for mesh in &scene.meshes {
                    for key in strings() {
                        if field.kind == "edges" && mesh.edges.iter().any(|e| e.key == key) {
                            self.selection.add(Target::Edge {
                                part: mesh.part.clone(),
                                key: key.into(),
                            });
                        } else if field.kind == "faces" && mesh.faces.iter().any(|f| f.key == key) {
                            self.selection.add(Target::Face {
                                part: mesh.part.clone(),
                                key: key.into(),
                            });
                        }
                    }
                }
            }
            _ => (),
        }
    }

    pub(super) fn pick_sketch_axis(
        &mut self,
        pos: egui::Pos2,
        rect: Rect,
        ctx: &egui::Context,
    ) -> bool {
        let Some(edit) = &self.editor else {
            return false;
        };
        if input(edit, &self.registry).is_none_or(|f| f.kind != "axis") {
            return false;
        }
        let scene = self.source_scene.as_ref().unwrap_or(&self.scene);
        let line = self
            .region_sketches(scene)
            .into_iter()
            .flat_map(|(sketch, s)| {
                s.entities
                    .iter()
                    .filter(|e| matches!(e.kind.as_str(), "line" | "circle" | "arc"))
                    .map(move |e| (sketch, s, e))
            })
            .filter_map(|(sketch, s, e)| {
                let distance = e
                    .points()
                    .windows(2)
                    .map(|points| {
                        let a = self.camera.project(s.plane.at(points[0]), rect);
                        let b = self.camera.project(s.plane.at(points[1]), rect);
                        render::point_segment_distance(pos, a, b)
                    })
                    .fold(f32::INFINITY, f32::min);
                (distance <= 7.).then_some((distance, sketch.to_owned(), e.id))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((distance, sketch, entity)) = line else {
            return false;
        };
        if render::pick_edges(scene, &self.camera, pos, rect, &self.hidden)
            .iter()
            .any(|edge| {
                edge.pixels <= distance + 0.1
                    && scene.meshes.iter().any(|m| {
                        m.part == edge.part
                            && m.edges
                                .iter()
                                .any(|e| e.key == edge.key && e.axis.is_some())
                    })
            })
        {
            return false;
        }
        let edit = self.editor.as_mut().unwrap();
        let key = input(edit, &self.registry).unwrap().key.clone();
        let before = edit.feature.params.clone();
        edit.feature.params[&key] = json!({"kind":"sketch_entity","sketch":sketch,"entity":entity});
        if before != edit.feature.params {
            edit.undo.push(before);
            edit.redo.clear();
            edit.changed = true;
            self.selection.clear();
            self.schedule(ctx);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(key: &str, kind: &str) -> Field {
        Field {
            key: key.into(),
            label: key.into(),
            kind: kind.into(),
            default: Value::Null,
            options: vec![],
        }
    }
    #[test]
    fn legacy_single_scope_is_not_silently_converted_to_empty_explicit_scope() {
        let registry = Registry::builtin().unwrap();
        for plugin in ["extrude", "revolve"] {
            let mut document = Document::default();
            let mut feature = registry.create(plugin, &mut document).unwrap();
            feature.params["mode"] = json!("Remove");
            let single = field("target", "body");
            let multiple = field("targets", "bodies");
            assert!(!visible(&single, &feature));
            assert!(visible(&multiple, &feature));
            feature.params.as_object_mut().unwrap().remove("targets");
            assert!(visible(&single, &feature));
            assert!(!visible(&multiple, &feature));
            feature.params["targets"] = json!([]);
            assert!(!visible(&single, &feature));
            assert!(visible(&multiple, &feature));
        }
    }
    #[test]
    fn body_inputs_never_overwrite_other_scopes() {
        let mut params = json!({"targets":["base"],"tools":[]});
        let picked = vec![
            Target::Face {
                part: "cutter".into(),
                key: "face".into(),
            },
            Target::Part("cutter".into()),
            Target::Part("second".into()),
        ];
        apply_selection(
            &mut params,
            &field("tools", "bodies"),
            &picked,
            &Scene::default(),
        );
        assert_eq!(
            params,
            json!({"targets":["base"],"tools":["cutter","second"]})
        );
        apply_selection(
            &mut params,
            &field("tools", "bodies"),
            &[],
            &Scene::default(),
        );
        assert_eq!(params["targets"], json!(["base"]));
        assert_eq!(params["tools"], json!([]));
    }
    #[test]
    fn unrelated_picks_cannot_clear_a_valid_input() {
        let mut params = json!({"axis_ref":{"kind":"origin","axis":"Z"},"regions":["cell"]});
        let before = params.clone();
        apply_selection(
            &mut params,
            &field("axis_ref", "axis"),
            &[Target::Part("body".into())],
            &Scene::default(),
        );
        apply_selection(
            &mut params,
            &field("regions", "regions"),
            &[Target::Face {
                part: "body".into(),
                key: "face".into(),
            }],
            &Scene::default(),
        );
        assert_eq!(params, before);
    }
}
