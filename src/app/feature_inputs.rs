//! Selection belongs to one active feature input, never every compatible field.
use super::*;
use crate::model::{ProfileReference, reference_matches};

pub(super) fn selectable(kind: &str) -> bool {
    matches!(
        kind,
        "profiles" | "regions" | "profile_face" | "plane_ref" | "axis" | "edges" | "faces" | "body" | "bodies" | "sketch" | "sketches"
    )
}

pub(super) fn visible(field: &Field, feature: &Feature) -> bool {
    let p = &feature.params;
    !field.hidden && !((feature.plugin == "plane"
        && match field.key.as_str() {
            "second" | "flip_alignment" => p["mode"] != "Midplane",
            "axis_ref" | "angle" => p["mode"] != "Angle",
            "offset" => p["mode"] != "Offset",
            _ => false,
        })
        || (matches!(field.kind.as_str(), "body" | "bodies") && p["mode"] == "New")
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
        .or_else(|| primary_input(&edit.feature, registry))
}

fn primary_input<'a>(feature: &Feature, registry: &'a Registry) -> Option<&'a Field> {
    let fields = &registry.plugins.get(&feature.plugin)?.manifest.fields;
            [
                "profiles",
                "regions",
                "profile_face",
                "plane_ref",
                "edges",
                "faces",
                "sketches",
                "sketch",
                "bodies",
                "axis",
                "body",
            ]
            .into_iter()
            .find_map(|kind| {
                fields
                    .iter()
                    .find(|f| f.kind == kind && visible(f, feature))
            })
}

pub(super) fn active_key<'a>(edit: &Editor, registry: &'a Registry) -> Option<&'a str> {
    input(edit, registry).map(|f| f.key.as_str())
}

// UI commands start with empty references. Manifest defaults remain compatible
// with saved documents and agent scripts that intentionally use legacy inputs.
pub(super) fn prepare(feature: &mut Feature, registry: &Registry) {
    let Some(plugin) = registry.plugins.get(&feature.plugin) else { return };
    for field in plugin.manifest.fields.iter().filter(|f| !f.hidden && selectable(&f.kind)) {
        feature.params[&field.key] = match field.kind.as_str() {
            "profiles" | "regions" | "edges" | "faces" | "bodies" | "sketches" => json!([]),
            "axis" => json!({}),
            "plane_ref" | "profile_face" => Value::Null,
            _ => json!(""),
        };
    }
}

pub(super) fn missing(edit: &Editor, registry: &Registry) -> Option<String> {
    if edit.sketch.is_some() { return None; }
    let plugin = registry.plugins.get(&edit.feature.plugin)?;
    plugin.manifest.fields.iter().filter(|f| visible(f, &edit.feature) && selectable(&f.kind))
        .find(|field| {
            let value = &edit.feature.params[&field.key];
            match field.kind.as_str() {
                "profiles" => ProfileReference::from_params(&edit.feature.params).map_or(true, |p| p.is_empty()),
                "regions" => value.as_array().is_some_and(Vec::is_empty),
                "axis" => !value.is_null() && value["kind"].as_str().is_none(),
                "edges" | "faces" | "bodies" | "sketches" => value.as_array().is_none_or(Vec::is_empty),
                "plane_ref" | "profile_face" => value.is_null(),
                _ => value.as_str().is_none_or(str::is_empty),
            }
        })
        .map(|f| format!("Select {}", f.label.to_lowercase()))
}

pub(super) fn cycle(edit: &mut Editor, registry: &Registry, reverse: bool) {
    let Some(plugin) = registry.plugins.get(&edit.feature.plugin) else { return };
    let fields: Vec<_> = plugin.manifest.fields.iter().filter(|f| visible(f, &edit.feature) && selectable(&f.kind)).collect();
    if fields.is_empty() { return; }
    let current = active_key(edit, registry).and_then(|key| fields.iter().position(|f| f.key == key)).unwrap_or(0);
    let next = (current + if reverse { fields.len() - 1 } else { 1 }) % fields.len();
    edit.input_field = Some(fields[next].key.clone());
}

pub(super) fn referenced_sketches<'a>(feature: &'a Feature, registry: &Registry) -> Vec<&'a str> {
    let p = &feature.params;
    let mut ids = Vec::new();
    if feature.params.get("geometry").is_some_and(Value::is_object) {
        ids.push(feature.id.as_str());
    }
    if let Some(plugin) = registry.plugins.get(&feature.plugin) {
        for field in &plugin.manifest.fields {
            match field.kind.as_str() {
                "profiles" => ids.extend(p[&field.key].as_array().into_iter().flatten().filter_map(|profile| profile["sketch"].as_str())),
                "sketch" if !p["profiles"].is_array() => ids.extend(p[&field.key].as_str().filter(|s| !s.is_empty())),
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

// Exposure belongs to the editing session, not its mutable selected values.
pub(super) fn exposed_sketches<'a>(edit: &'a Editor, document: &'a Document, registry: &Registry) -> Vec<&'a str> {
    let mut ids = referenced_sketches(&edit.feature, registry);
    if !edit.is_new && let Some(original) = document.features.get(edit.index) {
        ids.extend(referenced_sketches(original, registry));
    }
    ids.extend(edit.workspace.selection.iter().filter_map(|target| match target {
        Target::Feature(id) | Target::Region { sketch: id, .. } | Target::SketchEntity { sketch: id, .. } => Some(id.as_str()),
        _ => None,
    }));
    if let Some(feature) = edit.workspace.selected_feature.and_then(|i| document.features.get(i)) {
        ids.push(feature.id.as_str());
    }
    ids.sort_unstable(); ids.dedup(); ids
}

pub(super) fn input_ui(
    ui: &mut egui::Ui,
    field: &Field,
    value: &mut Value,
    scene: &Scene,
    document: &Document,
) -> bool {
    let before = value.clone();
    match field.kind.as_str() {
        "axis" => {
            let label = match value["kind"].as_str() {
                Some("origin") => format!("Origin {}",value["axis"].as_str().unwrap_or("?")),
                Some("sketch_line" | "sketch_entity") => {
                    let id = value["sketch"].as_str().unwrap_or_default();
                    let name = document.features.iter().find(|f|f.id == id).map_or(id,|f|f.name.as_str());
                    let entity = scene.sketches.get(id).and_then(|s|s.entities.iter().find(|e|Some(e.id) == value["entity"].as_u64()));
                    entity.map_or_else(||"Missing axis".into(),|e| if e.kind == "rectangle" {
                        format!("{name} / rectangle {} / Edge {}",e.id,value["edge"].as_u64().unwrap_or(0)+1)
                    } else {format!("{name} / {} {}",e.kind,e.id)})
                }
                Some("edge") => scene.meshes.iter().find(|m|Some(m.part.as_str()) == value["part"].as_str())
                    .and_then(|m|m.edges.iter().position(|e|value["edge"].as_str().is_some_and(|key|reference_matches(&e.key,key))).map(|i|format!("{} / Edge {}",m.name,i+1)))
                    .unwrap_or_else(||"Missing axis".into()),
                _ if value.is_null() => "Sketch axis (legacy)".into(),
                _ => "Select a line or edge".into(),
            };
            ui.add(egui::Label::new(label).truncate());
            ui.horizontal(|ui| {
                ui.menu_button("Origin axis",|ui| {
                    for axis in ["X","Y","Z"] {
                        if ui.selectable_label(value["kind"] == "origin" && value["axis"] == axis,format!("Origin {axis}")).clicked() {
                            *value = json!({"kind":"origin","axis":axis}); ui.close();
                        }
                    }
                });
                if ui.small_button("Clear").clicked() { *value = json!({}); }
            });
        }
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
        "profile_face" | "plane_ref" => {
            let mut options = vec![(
                Value::Null,
                "Select in viewport".into(),
            )];
            if field.kind == "plane_ref" {
                for plane in ["Top", "Front", "Right"] {
                    options.push((json!({"kind":"origin","plane":plane}), plane.into()));
                }
                for id in scene.planes.keys() {
                    let name = document
                        .features
                        .iter()
                        .find(|f| &f.id == id)
                        .map_or(id.as_str(), |f| f.name.as_str());
                    options.push((json!({"kind":"datum","feature":id}), name.into()));
                }
            }
            if matches!(field.kind.as_str(), "profile_face" | "plane_ref") {
                for mesh in &scene.meshes {
                    for (index, face) in mesh
                        .faces
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| f.plane.is_some())
                    {
                        options.push((
                            if field.kind == "plane_ref" {
                                json!({"kind":"face","part":mesh.part,"face":face.key})
                            } else {
                                json!({"part":mesh.part,"face":face.key})
                            },
                            format!("{} / face {}", mesh.name, index + 1),
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
    if feature.plugin == "plane" {
        let references: Vec<_> = targets
            .iter()
            .filter_map(|t| plane_value(t, scene))
            .collect();
        if let [reference] = references.as_slice() {
            feature.params["reference"] = reference.clone();
        }
        return Some("reference".into());
    }
    if feature.plugin == "boolean" {
        feature.params["tools"] = json!(parts(targets));
        return Some("tools".into());
    }
    let selected_parts = parts(targets);
    if feature.params.get("targets").is_some() && !selected_parts.is_empty() {
        feature.params["targets"] = json!(selected_parts);
        feature.params["target"] = json!("");
    }
    if feature.params.get("profiles").is_some() {
        let picked = profiles_from_targets(targets, scene);
        feature.params["profiles"] = json!(picked);
        return Some("profiles".into());
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
                &m.part == part && m.faces.iter().any(|f| reference_matches(&f.key, key) && f.plane.is_some())
            })
        {
            feature.params["profile_face"] = json!({"part":part,"face":key});
            return Some("profile_face".into());
        }
    }
    None
}

pub(super) fn seed_inputs(feature: &mut Feature, targets: &[Target], scene: &Scene, registry: &Registry) -> Option<String> {
    prepare(feature, registry);
    let plugin = registry.plugins.get(&feature.plugin)?;
    if matches!(feature.plugin.as_str(),"plane" | "boolean") || plugin.manifest.fields.iter().any(|f|f.kind == "profiles") {
        return seed(feature, targets, scene);
    }
    let field = primary_input(feature, registry)?;
    let ambiguous = if field.kind == "body" { parts(targets).len() > 1 }
        else if field.kind == "sketch" { targets.iter().filter_map(|t|match t {
            Target::Feature(id) | Target::Region {sketch:id,..} if scene.sketches.contains_key(id) => Some(id), _ => None
        }).collect::<BTreeSet<_>>().len() > 1 } else { false };
    if !ambiguous { apply_selection(&mut feature.params, field, targets, scene); }
    if matches!(field.kind.as_str(), "edges" | "faces") {
        let owners = parts(targets);
        if owners.len() == 1 {
            for body in plugin.manifest.fields.iter().filter(|f| f.kind == "body") {
                feature.params[&body.key] = json!(owners[0]);
            }
        }
    }
    Some(field.key.clone())
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
pub(super) fn filters(edit: &Editor, registry: &Registry) -> &'static [Filter] {
    match input(edit,registry).map(|f|f.kind.as_str()) {
        Some("profiles") => &[Filter::All,Filter::Faces,Filter::Regions],
        Some("axis" | "edges") => &[Filter::Edges],
        Some("faces" | "profile_face") => &[Filter::Faces],
        Some("body" | "bodies") => &[Filter::Parts],
        Some("regions") => &[Filter::Regions],
        _ => &[Filter::All],
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

fn plane_value(target: &Target, scene: &Scene) -> Option<Value> {
    match target {
        Target::Plane(name) if matches!(name.as_str(), "Top" | "Front" | "Right") => {
            Some(json!({"kind":"origin","plane":name}))
        }
        Target::Plane(id) | Target::Feature(id) if scene.planes.contains_key(id) => {
            Some(json!({"kind":"datum","feature":id}))
        }
        Target::Face { part, key }
            if scene.meshes.iter().any(|m| {
                &m.part == part && m.faces.iter().any(|f| reference_matches(&f.key, key) && f.plane.is_some())
            }) =>
        {
            Some(json!({"kind":"face","part":part,"face":key}))
        }
        _ => None,
    }
}

pub(super) fn profile_input(edit: &Editor, registry: &Registry) -> bool {
    input(edit, registry).is_some_and(|f| f.kind == "profiles")
}
pub(super) fn axis_input(edit: &Editor, registry: &Registry) -> bool {
    input(edit, registry).is_some_and(|f| f.kind == "axis")
}
pub(super) fn sketch_input(edit: &Editor, registry: &Registry) -> bool {
    input(edit, registry).is_some_and(|f| matches!(f.kind.as_str(), "sketch" | "sketches"))
}
pub(super) fn axis_candidates(
    scene: &Scene, camera: &Camera, pos: egui::Pos2, rect: Rect,
    hidden: &BTreeSet<String>, sketches: &[(&str, &crate::sketch::ResolvedSketch)], through: bool,
) -> Vec<Target> {
    let front = render::pick(scene, camera, pos, rect, hidden).map(|hit| hit.distance);
    let projector = camera.projector(rect);
    let (origin, ray) = camera.ray(pos, rect);
    let visible = |depth: f32| depth >= 0. && (through || front.is_none_or(|d| depth <= d + camera.span / rect.height() * 5.));
    let mut candidates = vec![];
    for &(id, sketch) in sketches {
        for entity in sketch.entities.iter().filter(|e| matches!(e.kind.as_str(), "line" | "rectangle" | "circle" | "arc")) {
            let points = entity.points();
            for edge in 0..if entity.kind == "rectangle" {4} else {1} {
            let axis_points = if entity.kind == "rectangle" {&points[edge..edge+2]} else {points.as_slice()};
            let hit = axis_points.windows(2).filter_map(|p| {
                let a = projector.project(sketch.plane.at(p[0]), rect);
                let b = projector.project(sketch.plane.at(p[1]), rect);
                let pixels = render::point_segment_distance(pos, a, b);
                let delta = b - a;
                let t = ((pos - a).dot(delta) / delta.length_sq().max(1e-8)).clamp(0.,1.);
                let a = glam::Vec3::from_array(sketch.plane.at(p[0]).map(|v|v as f32));
                let b = glam::Vec3::from_array(sketch.plane.at(p[1]).map(|v|v as f32));
                let depth = (a.lerp(b,t) - origin).dot(ray);
                // Source sketches are painted over the solid while choosing an
                // axis, so their visible lines must remain selectable there.
                (pixels <= 7. && depth >= 0.).then_some((pixels,depth))
            }).min_by(|a,b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
            if let Some((pixels,depth)) = hit { candidates.push((pixels,depth,Target::SketchEntity {sketch:id.into(),entity:entity.id,edge})); }
            }
        }
    }
    for edge in render::pick_edges(scene,camera,pos,rect,hidden) {
        if visible(edge.distance) && scene.meshes.iter().any(|m|m.part == edge.part && m.edges.iter().any(|e| reference_matches(&e.key,&edge.key) && e.axis.is_some())) {
            candidates.push((edge.pixels,edge.distance,Target::Edge {part:edge.part,key:edge.key}));
        }
    }
    candidates.sort_by(|a,b|a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    candidates.into_iter().map(|(_,_,target)|target).collect()
}
pub(super) fn sketch_candidate(
    scene: &Scene, camera: &Camera, pos: egui::Pos2, rect: Rect,
    hidden: &BTreeSet<String>, sketches: &[(&str, &crate::sketch::ResolvedSketch)], overlay: bool,
) -> Option<String> {
    let projector = camera.projector(rect);
    let (origin, ray) = camera.ray(pos, rect);
    let front = if overlay { None } else { render::pick(scene, camera, pos, rect, hidden).map(|h| h.distance) };
    let mut closest = 7.0f32;
    let mut result = None;
    for &(id, sketch) in sketches {
        for entity in &sketch.entities {
            for points in entity.points().windows(2) {
                let a = sketch.plane.at(points[0]); let b = sketch.plane.at(points[1]);
                let sa = projector.project(a, rect); let sb = projector.project(b, rect);
                let distance = render::point_segment_distance(pos, sa, sb);
                if distance >= closest { continue; }
                let delta = sb - sa;
                let t = ((pos - sa).dot(delta) / delta.length_sq().max(1e-8)).clamp(0., 1.);
                let point = glam::Vec3::from_array(a.map(|v| v as f32))
                    .lerp(glam::Vec3::from_array(b.map(|v| v as f32)), t);
                let depth = (point - origin).dot(ray);
                if depth >= 0. && front.is_none_or(|d| depth <= d + camera.span / rect.height() * 2.) {
                    closest = distance; result = Some(id.to_owned());
                }
            }
        }
    }
    result
}

pub(super) fn profile_candidates(
    scene: &Scene, camera: &Camera, pointer: (egui::Pos2, Rect, Filter, bool),
    hidden: &BTreeSet<String>, sketches: &[(&str, &crate::sketch::ResolvedSketch)],
) -> Vec<Target> {
    let (pos, rect, filter, through) = pointer;
    let faces = render::picks(scene, camera, pos, rect, hidden);
    let mut result = vec![];
    if matches!(filter, Filter::All | Filter::Regions) {
        let (origin, direction) = camera.ray(pos, rect);
        let mut regions = vec![];
        for &(id, sketch) in sketches {
            let Some(point) = camera.plane_point(pos, rect, &sketch.plane) else { continue; };
            let world = glam::Vec3::from_array(sketch.plane.at(point).map(|v| v as f32));
            let distance = (world - origin).dot(direction);
            // These sketches are painted over solids. Region picking follows
            // that same visible set, independent of selected profile values.
            if distance < 0. { continue; }
            for region in scene.regions.get(id).into_iter().flatten().filter(|r| r.contains(point)) {
                regions.push((distance, Target::Region { sketch: id.into(), key: region.key.clone() }));
            }
        }
        regions.sort_by(|a, b| a.0.total_cmp(&b.0));
        result.extend(regions.into_iter().map(|(_, target)| target));
    }
    if matches!(filter, Filter::All | Filter::Faces) {
        result.extend(faces.into_iter().take(if through { usize::MAX } else { 1 })
            .filter(|f| f.plane.is_some() && !f.face.contains("untracked:"))
            .map(|f| Target::Face { part: f.part, key: f.face }));
    }
    result
}
fn profiles_from_targets(targets: &[Target], scene: &Scene) -> Vec<ProfileReference> {
    let mut profiles: Vec<ProfileReference> = vec![];
    for target in targets {
        let profile = match target {
            Target::Region { sketch, key } => Some(ProfileReference::Region { sketch: sketch.clone(), region: key.clone() }),
            Target::Face { part, key } if scene.meshes.iter().any(|m| m.part == *part
                && m.faces.iter().any(|f| reference_matches(&f.key, key) && f.plane.is_some())) =>
                Some(ProfileReference::Face { part: part.clone(), face: key.clone() }),
            Target::Feature(sketch) if scene.sketches.contains_key(sketch) => Some(ProfileReference::Sketch { sketch: sketch.clone() }),
            _ => None,
        };
        if let Some(profile) = profile && !profiles.iter().any(|p| p.matches(&profile)) { profiles.push(profile); }
    }
    expanded_profiles(profiles, scene)
}
fn expanded_profiles(profiles: Vec<ProfileReference>, scene: &Scene) -> Vec<ProfileReference> {
    let mut result: Vec<ProfileReference> = vec![];
    for profile in profiles {
        if let ProfileReference::Sketch { sketch } = &profile
            && let Some(regions) = scene.regions.get(sketch) {
            for region in regions.iter().filter(|r| r.legacy_selected) {
                let item = ProfileReference::Region { sketch: sketch.clone(), region: region.key.clone() };
                if !result.iter().any(|p| p.matches(&item)) { result.push(item); }
            }
        } else if !result.iter().any(|p| p.matches(&profile)) { result.push(profile); }
    }
    result
}
fn field_profiles(params: &Value, field: &Field) -> crate::model::Result<Vec<ProfileReference>> {
    if field.key == "profiles" { ProfileReference::from_params(params) }
    else { ProfileReference::from_params(&json!({"profiles":params.get(&field.key).unwrap_or(&field.default)})) }
}
pub(super) fn profiles_ui(ui: &mut egui::Ui, field: &Field, params: &mut Value, scene: &Scene, document: &Document) -> bool {
    let before = params.get(&field.key).cloned();
    let Ok(selected) = field_profiles(params, field) else {
        ui.colored_label(RED, "Invalid profile reference");
        if ui.small_button("Clear").clicked() { params[&field.key] = json!([]); }
        return before != params.get(&field.key).cloned();
    };
    let mut selected = expanded_profiles(selected, scene);
    let name = |id: &str| document.features.iter().find(|f| f.id == id).map_or_else(|| id.to_owned(), |f| f.name.clone());
    let label = |profile: &ProfileReference| match profile {
        ProfileReference::Region { sketch, region } => format!("{} / Region {}", name(sketch), scene.regions.get(sketch).and_then(|r| r.iter().position(|r| reference_matches(&r.key, region))).map_or_else(|| "missing".into(), |i| (i + 1).to_string())),
        ProfileReference::Sketch { sketch } => format!("{} / Whole sketch", name(sketch)),
        ProfileReference::Face { part, face } => scene.meshes.iter().find(|m| &m.part == part).map_or_else(|| "Missing face".into(), |m| format!("{} / Face {}", m.name, m.faces.iter().position(|f| reference_matches(&f.key, face)).map_or_else(|| "missing".into(), |i| (i+1).to_string()))),
    };
    let mut changed = false;
    egui::ScrollArea::vertical().id_salt("profiles_selected").max_height(130.).show(ui, |ui| {
        selected.retain(|profile| {
            let mut keep = true;
            ui.horizontal(|ui| { ui.label(label(profile)); if ui.small_button("Remove").clicked() { keep = false; changed = true; } });
            keep
        });
    });
    ui.horizontal(|ui| {
        ui.menu_button("Add", |ui| {
            for (sketch, regions) in &scene.regions {
                for region in regions {
                    let profile = ProfileReference::Region { sketch: sketch.clone(), region: region.key.clone() };
                    if ui.selectable_label(selected.iter().any(|p| p.matches(&profile)), label(&profile)).clicked() {
                        let whole = ProfileReference::Sketch { sketch: sketch.clone() };
                        if selected.contains(&whole) {
                            selected.retain(|p| p != &whole);
                            selected.extend(regions.iter().filter(|r| r.legacy_selected).map(|r| ProfileReference::Region { sketch: sketch.clone(), region: r.key.clone() }));
                        }
                        if selected.iter().any(|p| p.matches(&profile)) { selected.retain(|p| !p.matches(&profile)); } else { selected.push(profile); }
                        changed = true;
                    }
                }
            }
            for mesh in &scene.meshes { for face in mesh.faces.iter().filter(|f| f.plane.is_some() && !f.key.contains("untracked:")) {
                let profile = ProfileReference::Face { part: mesh.part.clone(), face: face.key.clone() };
                if ui.selectable_label(selected.iter().any(|p| p.matches(&profile)), label(&profile)).clicked() {
                    if selected.iter().any(|p| p.matches(&profile)) { selected.retain(|p| !p.matches(&profile)); } else { selected.push(profile); }
                    changed = true;
                }
            }}
        });
        if ui.small_button("Clear").clicked() { selected.clear(); changed = true; }
    });
    if changed { params[&field.key] = json!(selected); }
    before != params.get(&field.key).cloned()
}

fn apply_selection(params: &mut Value, field: &Field, targets: &[Target], scene: &Scene) {
    if !targets.is_empty()
        && !targets.iter().any(|t| match (field.kind.as_str(), t) {
            ("body" | "bodies", Target::Part(_) | Target::Face { .. } | Target::Edge { .. })
            | ("regions", Target::Region { .. })
            | ("faces", Target::Face { .. })
            | ("edges", Target::Edge { .. }) => true,
            ("profiles", _) => !profiles_from_targets(std::slice::from_ref(t), scene).is_empty(),
            ("sketch" | "sketches", Target::Feature(id)) => scene.sketches.contains_key(id),
            ("sketch" | "sketches", Target::Region { sketch: id, .. }) => scene.sketches.contains_key(id),
            ("axis", Target::SketchEntity { sketch, entity, .. }) => scene.sketches.get(sketch).is_some_and(|s| s.entities.iter().any(|e| e.id == *entity && matches!(e.kind.as_str(), "line" | "rectangle" | "circle" | "arc"))),
            ("plane_ref", t) => plane_value(t, scene).is_some(),
            ("profile_face", Target::Face { part, key }) => scene.meshes.iter().any(|m| {
                &m.part == part && m.faces.iter().any(|f| reference_matches(&f.key, key) && f.plane.is_some())
            }),
            ("axis", Target::Edge { part, key }) => scene.meshes.iter().any(|m| {
                &m.part == part && m.edges.iter().any(|e| reference_matches(&e.key, key) && e.axis.is_some())
            }),
            _ => false,
        })
    {
        return;
    }
    params[&field.key] = match field.kind.as_str() {
        "profiles" => json!(profiles_from_targets(targets, scene)),
        "sketch" | "sketches" => {
            let mut selected = vec![];
            for target in targets {
                let id = match target { Target::Feature(id) | Target::Region { sketch: id, .. } => id, _ => continue };
                if scene.sketches.contains_key(id) && !selected.contains(id) { selected.push(id.clone()); }
            }
            if field.kind == "sketch" { json!(selected.last().cloned().unwrap_or_default()) } else { json!(selected) }
        }
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
        "plane_ref" => targets
            .iter()
            .rev()
            .find_map(|t| plane_value(t, scene))
            .unwrap_or(Value::Null),
        "profile_face" => targets
            .iter()
            .rev()
            .find_map(|t| match t {
                Target::Face { part, key }
                    if scene.meshes.iter().any(|m| {
                        &m.part == part
                            && m.faces.iter().any(|f| reference_matches(&f.key, key) && f.plane.is_some())
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
                        &m.part == part && m.edges.iter().any(|e| reference_matches(&e.key, key) && e.axis.is_some())
                    }) =>
                {
                    Some(json!({"kind":"edge","part":part,"edge":key}))
                }
                Target::SketchEntity { sketch, entity, edge } if scene.sketches.get(sketch).is_some_and(|s| s.entities.iter().any(|e| e.id == *entity)) => Some(json!({"kind":"sketch_entity","sketch":sketch,"entity":entity,"edge":edge})),
                _ => None,
            })
            .unwrap_or_else(|| json!({})),
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
        if edit.feature.plugin != "revolve"
            && !(edit.feature.plugin == "plane" && edit.feature.params["mode"] == "Angle")
        {
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
                .and_then(|(sk, e)| {
                    if matches!(e.kind.as_str(), "line" | "rectangle") {
                        let points = e.points();
                        let edge = if e.kind == "rectangle" {reference["edge"].as_u64().unwrap_or(0) as usize} else {0};
                        points.get(edge..edge+2).map(|p|(sk.plane.at(p[0]),sk.plane.at(p[1])))
                    } else {
                        let center = e
                            .arc()
                            .map(|a| a.center)
                            .unwrap_or([e.values[0], e.values[1]]);
                        let a = sk.plane.at(center);
                        Some((a, std::array::from_fn(|n| a[n] + sk.plane.normal[n])))
                    }
                }),
            Some("edge") => scene
                .meshes
                .iter()
                .find(|m| Some(m.part.as_str()) == reference["part"].as_str())
                .and_then(|m| {
                    m.edges
                        .iter()
                        .find(|e| reference["edge"].as_str().is_some_and(|key| reference_matches(&e.key, key)))
                })
                .and_then(|e| {
                    e.axis
                        .map(|(a, d)| (a, std::array::from_fn(|n| a[n] + d[n])))
                }),
            _ if reference.is_null() => ProfileReference::from_params(p).ok().and_then(|profiles| profiles.first().and_then(|profile| match profile {
                ProfileReference::Region { sketch, .. } | ProfileReference::Sketch { sketch } => scene.sketches.get(sketch).map(|s| s.plane.clone()),
                ProfileReference::Face { part, face } => scene.meshes.iter().find(|m| &m.part == part).and_then(|m| m.faces.iter().find(|f| reference_matches(&f.key, face))).and_then(|f| f.plane.clone()),
            })).map(|plane| {
                    let expression = p["axis_offset"].as_str().unwrap_or("0");
                    let offset = p["axis_offset"].as_f64().or_else(|| expression.parse().ok())
                        .unwrap_or_else(|| Numbers::for_document(&self.document).and_then(|n| n.eval(expression)).unwrap_or(0.));
                    if p["axis"] == "U" {
                        (plane.at([0., offset]), plane.at([1., offset]))
                    } else {
                        (plane.at([offset, 0.]), plane.at([offset, 1.]))
                    }
                }),
            _ => None,
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
            Stroke::new(2_f32, rgb(if axis_input(edit,&self.registry) {self.settings.appearance.turquoise} else {self.settings.appearance.purple})),
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
        self.restore_feature_selection();
    }

    pub(super) fn restore_feature_selection(&mut self) {
        let Some(edit) = &self.editor else { return };
        let scene = self.source_scene.as_ref().unwrap_or(&self.scene);
        if let Some(selection) = Self::selection_from_input(edit, &self.registry, scene) {
            self.selection = selection;
        }
    }

    fn selection_from_input(edit: &Editor, registry: &Registry, scene: &Scene) -> Option<Selection> {
        let field = input(edit, registry)?;
        let mut selection = Selection::default();
        let value = &edit.feature.params[&field.key];
        let strings = || {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
        };
        match field.kind.as_str() {
            "profiles" => {
                for profile in expanded_profiles(field_profiles(&edit.feature.params, field).unwrap_or_default(), scene) {
                    match profile {
                        ProfileReference::Region { sketch, region } => selection.add(Target::Region { sketch, key: region }),
                        ProfileReference::Face { part, face } => selection.add(Target::Face { part, key: face }),
                        ProfileReference::Sketch { sketch } => selection.add(Target::Feature(sketch)),
                    }
                }
            }
            "body" => {
                if let Some(part) = value.as_str().filter(|s| !s.is_empty()) {
                    selection.add(Target::Part(part.into()));
                }
            }
            "bodies" => {
                for part in strings() {
                    selection.add(Target::Part(part.into()));
                }
            }
            "plane_ref" => match value["kind"].as_str() {
                Some("origin") => {
                    if let Some(name) = value["plane"].as_str() {
                        selection.add(Target::Plane(name.into()));
                    }
                }
                Some("datum") => {
                    if let Some(id) = value["feature"].as_str() {
                        selection.add(Target::Plane(id.into()));
                    }
                }
                Some("face") => {
                    if let (Some(part), Some(key)) =
                        (value["part"].as_str(), value["face"].as_str())
                    {
                        selection.add(Target::Face {
                            part: part.into(),
                            key: key.into(),
                        });
                    }
                }
                _ => (),
            },
            "profile_face" => {
                if let (Some(part), Some(key)) = (value["part"].as_str(), value["face"].as_str()) {
                    selection.add(Target::Face {
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
                    selection.add(Target::Edge {
                        part: part.into(),
                        key: key.into(),
                    });
                } else if matches!(value["kind"].as_str(), Some("sketch_line" | "sketch_entity"))
                    && let (Some(sketch),Some(entity)) = (value["sketch"].as_str(),value["entity"].as_u64()) {
                    selection.add(Target::SketchEntity {sketch:sketch.into(),entity,edge:value["edge"].as_u64().unwrap_or(0) as usize});
                }
            }
            "sketch" => { if let Some(id) = value.as_str().filter(|id|!id.is_empty()) { selection.add(Target::Feature(id.into())); } }
            "sketches" => { for id in strings() { selection.add(Target::Feature(id.into())); } }
            "regions" => {
                let sketch = edit.feature.params["sketch"].as_str().unwrap_or_default();
                if let Some(regions) = scene.regions.get(sketch) {
                    for r in regions.iter().filter(|r| {
                        value.as_array().map_or(r.legacy_selected, |a| {
                            a.iter().any(|v| v.as_str().is_some_and(|key| reference_matches(key, &r.key)))
                        })
                    }) {
                        selection.add(Target::Region {
                            sketch: sketch.into(),
                            key: r.key.clone(),
                        });
                    }
                }
            }
            "edges" | "faces" => {
                for mesh in &scene.meshes {
                    for key in strings() {
                        if field.kind == "edges" && mesh.edges.iter().any(|e| reference_matches(&e.key, key)) {
                            selection.add(Target::Edge {
                                part: mesh.part.clone(),
                                key: key.into(),
                            });
                        } else if field.kind == "faces" && mesh.faces.iter().any(|f| reference_matches(&f.key, key)) {
                            selection.add(Target::Face {
                                part: mesh.part.clone(),
                                key: key.into(),
                            });
                        }
                    }
                }
            }
            _ => (),
        }
        Some(selection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile_editor(feature: Feature) -> Editor {
        Editor {
            workspace: EditWorkspace {
                camera: Camera::default(), automatic_camera: None, scene: Arc::new(Scene::default()), hidden: BTreeSet::new(),
                hidden_sketches: BTreeSet::new(), selected_plane: None, selected_feature: None,
                selection: vec![], selection_filter: Filter::All, show_planes: true, show_sketches: false,
            },
            index: 0, is_new: false, feature, sketch: None, changed: false,
            input_field: Some("profile_face".into()), undo: vec![], redo: vec![],
        }
    }
    fn profile_scene() -> (Document, Scene, Feature, Registry) {
        let registry = Registry::builtin().unwrap(); let mut doc = Document::default();
        let mut sketch = Sketch::default(); sketch.add_rect([0.,0.],[6.,4.]); sketch.add_rect([10.,0.],[14.,4.]);
        sketch.add_line([30.,-5.],[30.,10.]); sketch.entities.last_mut().unwrap().construction = true;
        let mut feature = registry.create("sketch", &mut doc).unwrap(); feature.set_sketch(&sketch); doc.features.push(feature);
        let mut feature = registry.create("extrude", &mut doc).unwrap(); feature.params["sketch"] = json!("f1"); feature.params["depth"] = json!("2"); doc.features.push(feature);
        let mut sketch = Sketch::default(); sketch.add_circle([24.,2.],2.);
        let mut feature = registry.create("sketch", &mut doc).unwrap(); feature.set_sketch(&sketch); doc.features.push(feature);
        let result = Runtime::new(Registry::builtin().unwrap()).evaluate(&doc).unwrap(); assert!(result.scene.error.is_none());
        let feature = registry.create("extrude", &mut doc).unwrap();
        (doc, (*result.scene).clone(), feature, registry)
    }
    #[test]
    fn face_seed_clear_then_viewport_region_click_uses_the_same_unified_input() {
        let (_, scene, mut feature, registry) = profile_scene();
        let face = scene.meshes[0].faces.iter().find(|f| f.key == "f2:end").unwrap();
        let target = Target::Face { part: face.part.clone(), key: face.key.clone() };
        assert_eq!(seed(&mut feature, &[target], &scene), Some("profiles".into()));
        let mut edit = profile_editor(feature);
        assert_eq!(active_key(&edit, &registry), Some("profiles"));
        assert_eq!(filter(&edit, &registry), Filter::All);
        for legacy in registry.plugins["extrude"].manifest.fields.iter().filter(|f| matches!(f.key.as_str(), "profile_face" | "sketch" | "regions")) { assert!(!visible(legacy, &edit.feature)); }
        apply_selection(&mut edit.feature.params, &field("profiles", "profiles"), &[], &scene);
        assert_eq!(edit.feature.params["profiles"], json!([]));
        let mut camera = Camera::default(); camera.align(&Plane::named("Top", 0.));
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.,700.));
        let point = camera.project([3.,2.,0.], rect);
        let sketches: Vec<_> = scene.sketches.iter().map(|(id,s)| (id.as_str(),s)).collect();
        let picked = profile_candidates(&scene, &camera, (point,rect,Filter::All,false), &BTreeSet::new(), &sketches);
        assert!(matches!(picked.first(), Some(Target::Region { sketch, .. }) if sketch == "f1"), "{picked:?}");
        apply_selection(&mut edit.feature.params, &field("profiles", "profiles"), &picked[..1], &scene);
        assert_eq!(edit.feature.params["profiles"][0]["kind"], "region");
        assert_eq!(active_key(&edit, &registry), Some("profiles"));
        assert!(profile_input(&edit, &registry));
    }
    #[test]
    fn clear_and_toggle_preserve_visible_source_regions_behind_a_body() {
        let (doc, scene, _, registry) = profile_scene();
        let mut edit = profile_editor(doc.features[1].clone());
        edit.index = 1;
        edit.workspace.hidden_sketches.insert("f1".into());
        let mut camera = Camera::default(); camera.align(&Plane::named("Top", 0.));
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let pos = camera.project([3., 2., 0.], rect);
        apply_selection(&mut edit.feature.params, &field("profiles", "profiles"), &[], &scene);
        assert!(referenced_sketches(&edit.feature, &registry).is_empty());
        let mut selection = Selection::default();
        for count in [1, 0, 1] {
            let exposure = exposed_sketches(&edit, &doc, &registry);
            assert_eq!(exposure, vec!["f1"], "Clear preserves the original source, without exposing unrelated f3");
            let visible: Vec<_> = scene.sketches.iter().filter(|(id, _)| exposure.contains(&id.as_str()))
                .map(|(id, sketch)| (id.as_str(), sketch)).collect();
            let picks = profile_candidates(&scene, &camera, (pos, rect, Filter::Regions, false), &BTreeSet::new(), &visible);
            let target = picks.first().expect("The painted sketch remains pickable through the source body");
            assert!(matches!(target, Target::Region {sketch, ..} if sketch == "f1"));
            selection.toggle(target.clone());
            apply_selection(&mut edit.feature.params, &field("profiles", "profiles"), selection.items(), &scene);
            assert_eq!(edit.feature.params["profiles"].as_array().unwrap().len(), count);
        }
        let faces = |through| profile_candidates(&scene, &camera, (pos, rect, Filter::Faces, through), &BTreeSet::new(), &[]);
        assert_eq!(faces(false).len(), 1, "Solid faces still obey ordinary depth selection");
        assert!(faces(true).len() > 1, "Select other still exposes occluded solid faces");
    }
    #[test]
    fn open_sketch_paths_follow_the_overlay_policy_only_while_choosing_sketches() {
        let (mut doc, _, _, registry) = profile_scene();
        let mut path = Sketch::default(); path.add_line([1., 1.], [5., 3.]);
        let mut feature = registry.create("sketch", &mut doc).unwrap();
        feature.set_sketch(&path); let id = feature.id.clone(); doc.features.push(feature);
        let result = Runtime::new(registry).evaluate(&doc).unwrap();
        assert!(result.scene.error.is_none());
        assert!(result.scene.regions.get(&id).is_none_or(Vec::is_empty), "An open sweep path has no region-picking fallback");
        let mut camera = Camera::default(); camera.align(&Plane::named("Top", 0.));
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let point = camera.project([3., 2., 0.], rect);
        let visible = [(id.as_str(), &result.scene.sketches[&id])];
        assert_eq!(sketch_candidate(&result.scene, &camera, point, rect, &BTreeSet::new(), &visible, false), None);
        assert_eq!(sketch_candidate(&result.scene, &camera, point, rect, &BTreeSet::new(), &visible, true), Some(id));
        assert_eq!(sketch_candidate(&result.scene, &camera, point, rect, &BTreeSet::new(), &[], true), None);
    }

    #[test]
    fn new_feature_exposure_uses_entry_selection_not_the_insertion_slots_old_feature() {
        let (doc, _, feature, registry) = profile_scene();
        let mut edit = profile_editor(feature); edit.is_new = true; edit.index = 1;
        edit.workspace.selection = vec![Target::Region { sketch: "f3".into(), key: "region".into() }];
        assert_eq!(exposed_sketches(&edit, &doc, &registry), vec!["f3"]);
        edit.workspace.selected_feature = Some(0);
        assert_eq!(exposed_sketches(&edit, &doc, &registry), vec!["f1", "f3"]);
        edit.workspace.selection.clear();
        assert_eq!(exposed_sketches(&edit, &doc, &registry), vec!["f1"]);
    }

    #[test]
    fn mixed_profile_picks_preserve_other_sketches_and_face_entries_until_removed() {
        let (_, scene, mut feature, _) = profile_scene();
        let a = scene.regions["f1"].iter().find(|r| r.contains([3.,2.])).unwrap().key.clone();
        let b = scene.regions["f3"][0].key.clone();
        let mut selected = Selection::default();
        let face = Target::Face { part: "f2:0".into(), key: "f2:end".into() };
        for target in [Target::Region {sketch:"f1".into(),key:a}, Target::Region {sketch:"f3".into(),key:b}, face.clone()] {
            selected.toggle(target);
            apply_selection(&mut feature.params, &field("profiles", "profiles"), selected.items(), &scene);
            assert_eq!(feature.params["profiles"].as_array().unwrap().len(), selected.items().len());
        }
        assert_eq!(referenced_sketches(&feature, &Registry::builtin().unwrap()), vec!["f1", "f3"]);
        selected.toggle(face);
        apply_selection(&mut feature.params, &field("profiles", "profiles"), selected.items(), &scene);
        assert_eq!(feature.params["profiles"].as_array().unwrap().len(), 2);
        assert!(feature.params["profiles"].as_array().unwrap().iter().all(|p|p["kind"] == "region"));
    }
    #[test]
    fn commands_do_not_guess_references_from_existing_geometry() {
        let (mut doc,scene,_,registry) = profile_scene();
        for id in ["extrude","revolve","fillet","chamfer","shell","sweep","loft","transform","pattern","mirror","plane","boolean"] {
            let mut feature = registry.create(id,&mut doc).unwrap();
            seed_inputs(&mut feature,&[],&scene,&registry);
            let mut edit = profile_editor(feature);
            edit.is_new = true;
            assert!(missing(&edit,&registry).is_some(),"{id} must request its input instead of using a recent sketch/part");
            for field in registry.plugins[id].manifest.fields.iter().filter(|f|selectable(&f.kind) && !f.hidden) {
                let value = &edit.feature.params[&field.key];
                assert!(value.is_null() || value == &json!({}) || value == &json!([]) || value == "", "{id}/{} = {value}",field.key);
            }
        }
    }
    #[test]
    fn deliberate_sketch_handoff_and_face_preselection_are_the_only_profile_seeds() {
        let (_,scene,mut feature,registry) = profile_scene();
        seed_inputs(&mut feature,&[Target::Feature("f3".into())],&scene,&registry);
        assert_eq!(feature.params["profiles"],json!([{"kind":"region","sketch":"f3","region":scene.regions["f3"][0].key}]));
        assert_eq!(feature.params["sketch"],"");
        let target = Target::Face {part:"f2:0".into(),key:"f2:end".into()};
        seed_inputs(&mut feature,&[target],&scene,&registry);
        assert_eq!(feature.params["profiles"],json!([{"kind":"face","part":"f2:0","face":"f2:end"}]));
    }
    #[test]
    fn sweep_roles_and_ordered_loft_profiles_use_their_declared_kinds() {
        let (mut doc,scene,_,registry) = profile_scene();
        let picked = [Target::Feature("f3".into()),Target::Feature("f1".into())];
        let mut loft = registry.create("loft",&mut doc).unwrap();
        assert_eq!(seed_inputs(&mut loft,&picked,&scene,&registry),Some("profiles".into()));
        assert_eq!(loft.params["profiles"],json!(["f3","f1"]));
        let mut sweep = registry.create("sweep",&mut doc).unwrap();
        seed_inputs(&mut sweep,&picked[1..],&scene,&registry);
        assert_eq!(sweep.params["profile"],"f1");
        assert_eq!(sweep.params["path"],"");
        apply_selection(&mut sweep.params,&field("path","sketch"),&picked[..1],&scene);
        assert_eq!(sweep.params["profile"],"f1");
        assert_eq!(sweep.params["path"],"f3");
        apply_selection(&mut sweep.params,&field("path","sketch"),&[],&scene);
        assert_eq!(sweep.params["path"],"");
        assert_eq!(sweep.params["profile"],"f1");
        seed_inputs(&mut sweep,&picked,&scene,&registry);
        assert_eq!(sweep.params["profile"],"","Two compatible preselected sketches do not imply which is profile vs path");
        let mut transform = registry.create("transform",&mut doc).unwrap();
        seed_inputs(&mut transform,&[Target::Part("f2:0".into()),Target::Part("f2:1".into())],&scene,&registry);
        assert_eq!(transform.params["target"],"","A single-part feature cannot quietly keep only the last preselected part");
    }
    #[test]
    fn revolve_profile_and_axis_are_independent_and_clear_never_uses_a_default_axis() {
        let (mut doc,scene,_,registry) = profile_scene();
        let mut feature = registry.create("revolve",&mut doc).unwrap();
        seed_inputs(&mut feature,&[Target::Feature("f3".into())],&scene,&registry);
        let profiles = feature.params["profiles"].clone();
        let mut edit = profile_editor(feature);
        edit.input_field = Some("profiles".into());
        assert_eq!(missing(&edit,&registry).as_deref(),Some("Select revolve axis"));
        cycle(&mut edit,&registry,false);
        assert_eq!(active_key(&edit,&registry),Some("axis_ref"));
        let axis = Target::SketchEntity {sketch:"f1".into(),entity:scene.sketches["f1"].entities[0].id,edge:0};
        apply_selection(&mut edit.feature.params,&field("axis_ref","axis"),&[axis],&scene);
        assert_eq!(edit.feature.params["profiles"],profiles);
        assert_eq!(missing(&edit,&registry),None);
        apply_selection(&mut edit.feature.params,&field("axis_ref","axis"),&[],&scene);
        assert_eq!(edit.feature.params["axis_ref"],json!({}));
        assert_eq!(edit.feature.params["profiles"],profiles);
        assert_eq!(missing(&edit,&registry).as_deref(),Some("Select revolve axis"));
        cycle(&mut edit,&registry,true);
        assert_eq!(active_key(&edit,&registry),Some("profiles"));
    }
    #[test]
    fn axis_hover_and_selection_use_the_exact_visible_sketch_entity() {
        let (_,scene,_,_) = profile_scene();
        let mut camera = Camera::default(); camera.align(&Plane::named("Top",0.));
        let rect = Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1000.,700.));
        let pos = camera.project([3.,0.,0.],rect);
        let sketches = vec![("f1",&scene.sketches["f1"])];
        let picks = axis_candidates(&scene,&camera,pos,rect,&BTreeSet::new(),&sketches,false);
        let expected = scene.sketches["f1"].entities[0].id;
        assert!(picks.contains(&Target::SketchEntity {sketch:"f1".into(),entity:expected,edge:0}),"{picks:?}");
        let pos = camera.project([30.,2.,0.],rect);
        let picks = axis_candidates(&scene,&camera,pos,rect,&BTreeSet::new(),&sketches,false);
        let expected = scene.sketches["f1"].entities.iter().find(|e|e.construction).unwrap().id;
        assert_eq!(picks.first(),Some(&Target::SketchEntity {sketch:"f1".into(),entity:expected,edge:0}));
    }
    #[test]
    fn selected_rectangle_side_axis_survives_roundtrip_and_revolves_that_side() {
        let (mut doc,scene,_,registry) = profile_scene();
        let region = scene.regions["f1"].iter().find(|r|r.contains([3.,2.])).unwrap();
        let mut feature = registry.create("revolve",&mut doc).unwrap();
        seed_inputs(&mut feature,&[Target::Region {sketch:"f1".into(),key:region.key.clone()}],&scene,&registry);
        let axis = Target::SketchEntity {sketch:"f1".into(),entity:scene.sketches["f1"].entities[0].id,edge:0};
        apply_selection(&mut feature.params,&field("axis_ref","axis"),&[axis],&scene);
        assert_eq!(feature.params["axis_ref"]["edge"],0);
        doc.features.push(feature);
        let doc: Document = serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
        let result = Runtime::new(registry).evaluate(&doc).unwrap();
        assert!(result.scene.error.is_none(),"{:?}",result.scene.error);
        assert!((result.scene.meshes.last().unwrap().volume - 96.*std::f64::consts::PI).abs() < 1e-6);
    }
    #[test]
    fn merge_scope_is_required_only_for_operations_that_use_it() {
        let (_,_,mut feature,registry) = profile_scene();
        feature.params["profiles"] = json!([{"kind":"sketch","sketch":"f1"}]);
        let mut edit = profile_editor(feature);
        assert_eq!(missing(&edit,&registry),None);
        for mode in ["Add","Remove","Intersect"] {
            edit.feature.params["mode"] = json!(mode);
            assert_eq!(missing(&edit,&registry).as_deref(),Some("Select merge scope"));
            edit.feature.params["targets"] = json!(["f2:0"]);
            assert_eq!(missing(&edit,&registry),None);
            edit.feature.params["targets"] = json!([]);
        }
    }
    #[test]
    fn saved_inputs_are_reprojected_from_the_source_scene_without_mutating_references() {
        let (mut document, source, _, registry) = profile_scene();
        let mesh = &source.meshes[0];
        for (plugin, key, kind, reference, expected) in [
            ("fillet", "edges", "edges", mesh.edges[0].key.clone(),
                Target::Edge { part: mesh.part.clone(), key: mesh.edges[0].key.clone() }),
            ("chamfer", "edges", "edges", mesh.edges[0].key.clone(),
                Target::Edge { part: mesh.part.clone(), key: mesh.edges[0].key.clone() }),
            ("shell", "faces", "faces", mesh.faces[0].key.clone(),
                Target::Face { part: mesh.part.clone(), key: mesh.faces[0].key.clone() }),
        ] {
            let mut feature = registry.create(plugin, &mut document).unwrap();
            feature.params["target"] = json!(mesh.part);
            feature.params[key] = json!([reference]);
            let mut edit = profile_editor(feature);
            edit.input_field = Some(key.into());
            let saved = edit.feature.params.clone();
            // An edited result can consume the selected topology. An empty
            // projection is display state, never a replacement for saved input.
            let stale = CadApp::selection_from_input(&edit, &registry, &Scene::default()).unwrap();
            assert!(stale.items().is_empty());
            assert_eq!(edit.feature.params, saved);
            let restored = CadApp::selection_from_input(&edit, &registry, &source).unwrap();
            assert_eq!(restored.items(), &[expected]);
            apply_selection(&mut edit.feature.params, &field(key, kind), restored.items(), &source);
            assert_eq!(edit.feature.params, saved, "Restored inputs round-trip after the prefix arrives");
        }
    }
    fn field(key: &str, kind: &str) -> Field {
        Field {
            key: key.into(),
            label: key.into(),
            kind: kind.into(),
            default: Value::Null,
            options: vec![],
            hidden: false,
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
    #[test]
    fn plane_inputs_pick_origin_datum_and_planar_face_independently() {
        let mut scene = Scene::default();
        scene.planes.insert("f3".into(), Plane::named("Front", 10.));
        let mut params = json!({"reference":{"kind":"origin","plane":"Top"},"second":null});
        apply_selection(
            &mut params,
            &field("second", "plane_ref"),
            &[Target::Feature("f3".into())],
            &scene,
        );
        assert_eq!(params["reference"], json!({"kind":"origin","plane":"Top"}));
        assert_eq!(params["second"], json!({"kind":"datum","feature":"f3"}));
        apply_selection(
            &mut params,
            &field("reference", "plane_ref"),
            &[Target::Plane("Right".into())],
            &scene,
        );
        assert_eq!(
            params["reference"],
            json!({"kind":"origin","plane":"Right"})
        );
        let before = params.clone();
        apply_selection(
            &mut params,
            &field("reference", "plane_ref"),
            &[Target::Part("p".into())],
            &scene,
        );
        assert_eq!(params, before);
        apply_selection(&mut params, &field("second", "plane_ref"), &[], &scene);
        assert!(params["second"].is_null());
    }
    #[test]
    fn midplane_second_input_routes_history_selection_without_overwriting_reference() {
        let mut scene = Scene::default();
        scene.planes.insert("f1".into(),Plane::named("Top",10.));
        scene.planes.insert("f2".into(),Plane::named("Top",30.));
        let mut params = json!({"mode":"Midplane","reference":{"kind":"datum","feature":"f2"},"second":{"kind":"origin","plane":"Top"}});
        let mut selected = crate::selection::Selection::default();
        selected.add(Target::Plane("Top".into()));
        selected.toggle(Target::Feature("f1".into()));
        apply_selection(&mut params,&field("second","plane_ref"),selected.items(),&scene);
        assert_eq!(params["second"],json!({"kind":"datum","feature":"f1"}));
        assert_eq!(params["reference"],json!({"kind":"datum","feature":"f2"}));
    }
    #[test]
    fn construction_plane_mode_fields_and_active_input_follow_one_manifest() {
        let registry = Registry::builtin().unwrap();
        let mut doc = Document::default();
        let mut feature = registry.create("plane", &mut doc).unwrap();
        for (mode, shown, hidden) in [
            (
                "Offset",
                vec!["reference", "offset", "flip_normal"],
                vec!["second", "angle", "axis_ref", "flip_alignment"],
            ),
            (
                "Midplane",
                vec!["reference", "second", "flip_alignment", "flip_normal"],
                vec!["offset", "angle", "axis_ref"],
            ),
            (
                "Angle",
                vec!["reference", "axis_ref", "angle", "flip_normal"],
                vec!["second", "offset", "flip_alignment"],
            ),
        ] {
            feature.params["mode"] = json!(mode);
            for field in &registry.plugins["plane"].manifest.fields {
                if shown.contains(&field.key.as_str()) {
                    assert!(visible(field, &feature), "{mode}/{}", field.key);
                }
                if hidden.contains(&field.key.as_str()) {
                    assert!(!visible(field, &feature), "{mode}/{}", field.key);
                }
            }
        }
    }
}
