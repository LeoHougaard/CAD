//! Explicit solid scopes and persistent Revolve references.
use super::*;
struct Profile {
    plane: Plane,
    source: ResolvedSketch,
    faces: Vec<FaceId>,
    sides: Vec<(String, [f64; 2], [f64; 2])>,
    edges: Vec<sketch::RegionEdge>,
}
impl Geometry {
    fn sketch_profile(&mut self, sk: &ResolvedSketch, p: &Value) -> Result<Profile> {
        let selected = match p.get("regions") {
            None | Some(Value::Null) => None,
            Some(Value::Array(values)) if !values.is_empty() => Some(
                values
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| "Sketch region selections must be identities".to_owned())
                    })
                    .collect::<Result<Vec<_>>>()?,
            ),
            Some(Value::Array(_)) => return Err("Choose at least one sketch region".into()),
            _ => return Err("Sketch region selections must be an array or null".into()),
        };
        if selected.is_none()
            && let Ok(faces) = sketch::faces(&mut self.topo, sk)
        {
            return Ok(Profile {
                plane: sk.plane.clone(),
                source: sk.clone(),
                faces,
                sides: vec![],
                edges: sketch::region_edges(sk).unwrap_or_default(),
            });
        }
        let profile =
            sketch::profile_for_regions(&mut self.topo, sk, selected.as_deref().unwrap_or(&[]))?;
        Ok(Profile {
            plane: sk.plane.clone(),
            source: sk.clone(),
            faces: profile.faces,
            sides: profile.sides,
            edges: profile.edges,
        })
    }
    fn resolve_profile(&mut self, p: &Value) -> Result<Profile> {
        let Some(reference) = p.get("profile_face").filter(|v| !v.is_null()) else {
            let sk = self
                .sketches
                .get(text(p, "sketch", ""))
                .ok_or("Choose an earlier, unsuppressed sketch or a planar face")?
                .clone();
            return self.sketch_profile(&sk, p);
        };
        let key = text(reference, "face", "");
        if key.is_empty() || key.contains("untracked:") {
            return Err("Choose a persistent planar face reference".into());
        }
        if text(reference, "part", "").is_empty() {
            return Err("Choose the part containing the profile face".into());
        }
        let i = self.target(text(reference, "part", ""))?;
        let matches: Vec<_> = self
            .face_keys(self.parts[i].solid)?
            .into_iter()
            .filter(|(_, k)| k == key)
            .collect();
        if matches.len() != 1 {
            return Err("Profile face is missing or ambiguous; reselect it".into());
        }
        let face = self.topo.face(matches[0].0).map_err(err)?;
        let FaceSurface::Plane { normal, d } = face.surface() else {
            return Err("The profile must be a planar face".into());
        };
        let normal = *normal;
        let d = *d;
        let plane = if let Some((_, plane)) = self
            .anchors
            .iter()
            .find(|(k, plane)| k == key && plane_matches(plane, normal, d))
        {
            plane.clone()
        } else {
            let n = [normal.x(), normal.y(), normal.z()];
            let seed = if n[0].abs() < 0.8 {
                [1., 0., 0.]
            } else {
                [0., 1., 0.]
            };
            let mut u = sub(seed, n.map(|x| x * dot(seed, n)));
            let length = dot(u, u).sqrt();
            u = u.map(|x| x / length);
            Plane {
                origin: n.map(|x| x * d),
                u,
                v: cross(n, u),
                normal: n,
            }
        };
        // A copy isolates any future kernel mutations from the selected source.
        let copy = ops::copy::copy_solid(&mut self.topo, self.parts[i].solid).map_err(err)?;
        let faces: Vec<_> = self
            .face_keys_scoped(copy, Some(&self.parts[i].key))?
            .into_iter()
            .filter(|(_, k)| k == key)
            .map(|(id, _)| id)
            .collect();
        let edge_keys = self.edge_keys_scoped(copy, Some(&self.parts[i].key))?;
        let mut vertices: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        let boundary = face_edges(&self.topo, faces[0]).map_err(err)?;
        for (id, key) in &edge_keys {
            let edge = self.topo.edge(*id).map_err(err)?;
            for v in [edge.start(), edge.end()] {
                vertices.entry(v.index()).or_default().push(key.clone());
            }
        }
        for keys in vertices.values_mut() {
            keys.sort();
            keys.dedup();
        }
        let local = |point: brepkit_math::vec::Point3| {
            let delta = point - p3(plane.origin);
            [delta.dot(v3(plane.u)), delta.dot(v3(plane.v))]
        };
        let mut edges = vec![];
        let mut sides = vec![];
        let source = format!("{}:{key}", self.parts[i].key);
        let vertex_key = |id: usize| {
            let keys = &vertices[&id];
            format!(
                "{}{}",
                if keys.iter().any(|key| key.contains("untracked:")) {
                    "untracked:"
                } else {
                    ""
                },
                crate::plugins::source_hash(&format!("{source}:vertex:{}", keys.join("~")))
            )
        };
        for id in boundary {
            let edge = self.topo.edge(id).map_err(err)?;
            let a = self.topo.vertex(edge.start()).map_err(err)?.point();
            let b = self.topo.vertex(edge.end()).map_err(err)?.point();
            let source_edge = &edge_keys.iter().find(|(e, _)| *e == id).unwrap().1;
            let key = format!(
                "{}face-edge:{}",
                if source_edge.contains("untracked:") {
                    "untracked:"
                } else {
                    ""
                },
                crate::plugins::source_hash(&format!("{source}:{}", source_edge))
            );
            let (lo, hi) = edge.curve().domain_with_endpoints(a, b);
            let circle = match edge.curve() {
                EdgeCurve::Line => None,
                EdgeCurve::Circle(c) => Some((local(c.center()), c.radius())),
                _ => {
                    return Err(
                        "Profile faces currently require exact straight or circular boundaries"
                            .into(),
                    );
                }
            };
            if circle.is_none() {
                sides.push((key.clone(), local(a), local(b)));
            }
            edges.push(sketch::RegionEdge {
                key,
                start_key: vertex_key(edge.start().index()),
                end_key: vertex_key(edge.end().index()),
                start: local(a),
                end: local(b),
                midpoint: local(edge.curve().evaluate_with_endpoints((lo + hi) / 2., a, b)),
                circle,
            });
        }
        Ok(Profile {
            source: ResolvedSketch {
                entities: vec![],
                plane: plane.clone(),
                dof: 0,
            },
            plane,
            faces,
            sides,
            edges,
        })
    }
    fn revolve_axis(&self, p: &Value, plane: &Plane) -> Result<(brepkit_math::vec::Point3, Vec3)> {
        let (origin, direction) = if let Some(reference) =
            p.get("axis_ref").filter(|v| !v.is_null())
        {
            match text(reference, "kind", "") {
                "origin" => {
                    let direction = match text(reference, "axis", "") {
                        "X" => [1., 0., 0.],
                        "Y" => [0., 1., 0.],
                        "Z" => [0., 0., 1.],
                        _ => return Err("Choose an origin X, Y or Z axis".into()),
                    };
                    (p3([0.; 3]), v3(direction))
                }
                "sketch_line" | "sketch_entity" => {
                    let sk = self
                        .sketches
                        .get(text(reference, "sketch", ""))
                        .ok_or("Revolve axis sketch is missing or suppressed")?;
                    let id = reference
                        .get("entity")
                        .and_then(Value::as_u64)
                        .ok_or("Choose a sketch entity identity for the revolve axis")?;
                    let entity = sk
                        .entities
                        .iter()
                        .find(|e| e.id == id)
                        .ok_or("Revolve axis entity is missing; reselect it")?;
                    let a = p3(sk.plane.at([entity.values[0], entity.values[1]]));
                    match entity.kind.as_str() {
                        "line" => {
                            let b = p3(sk.plane.at([entity.values[2], entity.values[3]]));
                            (a, b - a)
                        }
                        "circle" | "arc" => (a, v3(sk.plane.normal)),
                        _ => return Err("Choose a line, circle or arc for the revolve axis".into()),
                    }
                }
                "edge" => {
                    if text(reference, "part", "").is_empty() {
                        return Err("Choose the part containing the revolve axis edge".into());
                    }
                    let i = self.target(text(reference, "part", ""))?;
                    let key = text(reference, "edge", "");
                    if key.is_empty() || key.contains("untracked:") {
                        return Err(
                            "Revolve axis requires a persistent straight-edge reference".into()
                        );
                    }
                    let edges: Vec<_> = self
                        .edge_keys(self.parts[i].solid)?
                        .into_iter()
                        .filter(|(_, k)| k == key)
                        .collect();
                    if edges.len() != 1 {
                        return Err("Revolve axis edge is missing or ambiguous; reselect it".into());
                    }
                    analytic_edge_axis(&self.topo, edges[0].0)?
                        .ok_or("Revolve axis requires a straight or circular edge")?
                }
                _ => return Err("Unknown revolve axis reference".into()),
            }
        } else {
            let (axis, shift) = match text(p, "axis", "V") {
                "U" => (plane.u, plane.v),
                "V" => (plane.v, plane.u),
                _ => return Err("Choose a sketch U or V axis".into()),
            };
            (
                p3(std::array::from_fn(|k| {
                    plane.origin[k] + shift[k] * num(p, "axis_offset", 0.)
                })),
                v3(axis),
            )
        };
        let direction = direction
            .normalize()
            .map_err(|_| "Revolve axis has zero length".to_owned())?;
        if direction.dot(v3(plane.normal)).abs() > 1e-7
            || (origin - p3(plane.origin)).dot(v3(plane.normal)).abs() > 1e-6
        {
            return Err("Revolve axis must lie in the profile plane".into());
        }
        Ok((origin, direction))
    }
    pub(super) fn execute_revolve(&mut self, f: &Feature, ordinal: usize, p: &Value) -> Result<()> {
        let profile = self.resolve_profile(p)?;
        let (origin, axis) = self.revolve_axis(p, &profile.plane)?;
        let angle = num(p, "angle", 360.);
        if angle <= 0. || angle > 360. {
            return Err("Revolve angle must be between 0 and 360 degrees".into());
        }
        let mut solids = vec![];
        for &face in &profile.faces {
            solids.push(
                ops::revolve::revolve(&mut self.topo, face, origin, axis, angle.to_radians())
                    .map_err(err)?,
            );
        }
        self.register_revolve_references(f, &profile, &solids, origin, axis, angle)?;
        self.apply_profile_solids(f, ordinal, p, solids)
    }
    fn register_revolve_references(
        &mut self,
        f: &Feature,
        profile: &Profile,
        solids: &[SolidId],
        origin: brepkit_math::vec::Point3,
        axis: Vec3,
        angle: f64,
    ) -> Result<()> {
        if (angle - 360.).abs() > 1e-7 {
            return Ok(());
        }
        let mut vertices = BTreeMap::new();
        let mut sides: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for edge in &profile.edges {
            let a = p3(profile.plane.at(edge.start));
            let b = p3(profile.plane.at(edge.end));
            vertices.insert(edge.start_key.clone(), a);
            vertices.insert(edge.end_key.clone(), b);
            if edge.circle.is_none()
                && (b - a).length() > 1e-7
                && (b - a).cross(axis).length() < 1e-7
            {
                for key in [&edge.start_key, &edge.end_key] {
                    sides
                        .entry(key.clone())
                        .or_default()
                        .insert(format!("{}:region-side:{}", f.id, edge.key));
                }
            }
            if edge.circle.is_none() && (b - a).dot(axis).abs() < 1e-7 {
                for &solid in solids {
                    for face_id in solid_faces(&self.topo, solid).map_err(err)? {
                        let face = self.topo.face(face_id).map_err(err)?;
                        let FaceSurface::Plane { normal, d } = face.surface() else {
                            continue;
                        };
                        if normal.dot(axis).abs() < 1. - 1e-7
                            || (normal.dot(a - p3([0.; 3])) - d).abs() > 1e-7
                        {
                            continue;
                        }
                        let normal = *normal * if face.is_reversed() { -1. } else { 1. };
                        let radial = a - origin - axis * (a - origin).dot(axis);
                        let radial = if radial.length() > 1e-7 {
                            radial
                        } else {
                            b - origin - axis * (b - origin).dot(axis)
                        };
                        let Ok(u) = radial.normalize() else {
                            continue;
                        };
                        self.anchors.push((
                            format!("{}:region-side:{}", f.id, edge.key),
                            Plane {
                                origin: [a.x(), a.y(), a.z()],
                                u: [u.x(), u.y(), u.z()],
                                v: {
                                    let v = normal.cross(u);
                                    [v.x(), v.y(), v.z()]
                                },
                                normal: [normal.x(), normal.y(), normal.z()],
                            },
                        ));
                    }
                }
            }
        }
        for &solid in solids {
            for id in solid_edges(&self.topo, solid).map_err(err)? {
                let edge = self.topo.edge(id).map_err(err)?;
                let EdgeCurve::Circle(circle) = edge.curve() else {
                    continue;
                };
                if circle.normal().dot(axis).abs() < 1. - 1e-7 {
                    continue;
                }
                let candidates: Vec<_> = vertices
                    .iter()
                    .filter(|(_, point)| {
                        let center = origin + axis * (**point - origin).dot(axis);
                        (center - circle.center()).length() < 1e-7
                            && ((**point - center).length() - circle.radius()).abs() < 1e-7
                    })
                    .map(|(key, _)| key)
                    .collect();
                if candidates.len() != 1 {
                    continue;
                }
                let key = candidates[0];
                let a = self.topo.vertex(edge.start()).map_err(err)?.point();
                let b = self.topo.vertex(edge.end()).map_err(err)?.point();
                let (lo, hi) = edge.curve().domain_with_endpoints(a, b);
                self.edge_anchors.push(EdgeAnchor {
                    key: format!("{}:revolve-vertex:{key}", f.id),
                    side: sides
                        .get(key)
                        .filter(|v| v.len() == 1)
                        .and_then(|v| v.first().cloned()),
                    start: a,
                    end: b,
                    midpoint: edge.curve().evaluate_with_endpoints((lo + hi) / 2., a, b),
                    circle: Some((
                        circle.center(),
                        circle.normal(),
                        circle.radius(),
                        (a - b).length() < 1e-9,
                    )),
                });
            }
        }
        Ok(())
    }
    pub(super) fn execute_extrude(&mut self, f: &Feature, ordinal: usize, p: &Value) -> Result<()> {
        let profile = self.resolve_profile(p)?;
        let sk = &profile.source;
        let mut depth = num(p, "depth", 10.);
        if let Some(value) = f.params.get("context_up_to").filter(|v| !v.is_null()) {
            if flag(p, "symmetric") {
                return Err("Up to context face cannot use symmetric extent".into());
            }
            let plane: Plane = serde_json::from_value(value.clone())
                .map_err(|e| format!("Invalid context extent plane: {e}"))?;
            plane.validate()?;
            if dot(plane.normal, profile.plane.normal).abs() < 1. - 1e-7 {
                return Err(
                    "Up to context face currently requires a plane parallel to the profile".into(),
                );
            }
            depth = dot(
                sub(plane.origin, profile.plane.origin),
                profile.plane.normal,
            ) * if flag(p, "reverse") { -1. } else { 1. };
        }
        if depth <= 1e-7 {
            return Err("Depth must be positive; use Reverse to change direction".into());
        }
        let direction = if flag(p, "reverse") { -1. } else { 1. };
        let start = if flag(p, "symmetric") {
            -direction * depth / 2.
        } else {
            0.
        };
        self.register_profile_edges(f, sk, &profile.edges, start, start + direction * depth);
        let mut solids = vec![];
        for face in profile.faces {
            if start != 0. {
                let delta = profile.plane.normal.map(|x| x * start);
                ops::transform::transform_face(
                    &mut self.topo,
                    face,
                    &Mat4::translation(delta[0], delta[1], delta[2]),
                )
                .map_err(err)?;
            }
            solids.push(
                ops::extrude::extrude(
                    &mut self.topo,
                    face,
                    v3(profile.plane.normal) * direction,
                    depth,
                )
                .map_err(err)?,
            );
        }
        self.apply_profile_solids(f, ordinal, p, solids)?;
        self.register_extrude_anchors(f, sk, start, start + direction * depth);
        for (key, a, b) in profile.sides {
            let origin = sk.plane.at(a);
            let mut u = sub(sk.plane.at(b), origin);
            let length = dot(u, u).sqrt();
            if length < 1e-8 {
                continue;
            }
            u = u.map(|x| x / length);
            self.anchors.push((
                format!("{}:region-side:{key}", f.id),
                Plane {
                    origin,
                    u,
                    v: sk.plane.normal,
                    normal: cross(u, sk.plane.normal),
                },
            ));
        }
        Ok(())
    }
    fn apply_profile_solids(
        &mut self,
        f: &Feature,
        ordinal: usize,
        p: &Value,
        solids: Vec<SolidId>,
    ) -> Result<()> {
        let mode = text(p, "mode", "New");
        let targets = if mode == "New" {
            vec![]
        } else if p.get("targets").is_some() {
            let scope = self.part_scope(p, "targets")?;
            if scope.is_empty() {
                return Err("Choose a merge scope".into());
            }
            scope
                .into_iter()
                .map(|i| self.parts[i].key.clone())
                .collect()
        } else {
            vec![self.parts[self.target(text(p, "target", ""))?].key.clone()]
        };
        if mode == "New" {
            for (i, solid) in solids.into_iter().enumerate() {
                self.add_result(f, solid, mode, "", ordinal * 100 + i)?;
            }
        } else {
            if mode == "Remove" {
                let mut outputs = vec![];
                for target in targets {
                    for &tool in &solids {
                        let copy = ops::copy::copy_solid(&mut self.topo, tool).map_err(err)?;
                        self.add_result(f, copy, mode, &target, ordinal)?;
                    }
                    outputs.push(target);
                }
                self.record_output(f, outputs);
                return Ok(());
            }
            if mode == "Add" {
                let first = targets.first().ok_or("Choose a merge scope")?.clone();
                let index = self.target(&first)?;
                let mut result =
                    ops::copy::copy_solid(&mut self.topo, self.parts[index].solid).map_err(err)?;
                for tool in solids {
                    result = ops::boolean::boolean(
                        &mut self.topo,
                        ops::boolean::BooleanOp::Fuse,
                        result,
                        tool,
                    )
                    .map_err(err)?;
                }
                for target in &targets[1..] {
                    let i = self.target(target)?;
                    let base =
                        ops::copy::copy_solid(&mut self.topo, self.parts[i].solid).map_err(err)?;
                    result = ops::boolean::boolean(
                        &mut self.topo,
                        ops::boolean::BooleanOp::Fuse,
                        result,
                        base,
                    )
                    .map_err(err)?;
                }
                self.parts[index].solid = result;
                self.parts[index].feature = f.id.clone();
                self.parts
                    .retain(|part| part.key == first || !targets.contains(&part.key));
                self.record_output(f, vec![first]);
            } else {
                let mut tools = solids.into_iter();
                let mut tool = tools.next().ok_or("Choose at least one closed profile")?;
                for other in tools {
                    tool = ops::boolean::boolean(
                        &mut self.topo,
                        ops::boolean::BooleanOp::Fuse,
                        tool,
                        other,
                    )
                    .map_err(err)?;
                }
                let mut outputs = vec![];
                for target in targets {
                    let copy = ops::copy::copy_solid(&mut self.topo, tool).map_err(err)?;
                    self.add_result(f, copy, mode, &target, ordinal)?;
                    outputs.push(target);
                }
                self.record_output(f, outputs);
            }
        }
        Ok(())
    }
    fn part_scope(&self, p: &Value, field: &str) -> Result<Vec<usize>> {
        let list = p
            .get(field)
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{field} must be an array of part identities"))?;
        if list.len() > 100 {
            return Err("A scope may contain at most 100 parts".into());
        }
        let mut selected = Vec::with_capacity(list.len());
        for value in list {
            let key = value
                .as_str()
                .filter(|s| !s.is_empty() && *s != "@last")
                .ok_or("Choose explicit part identities for the scope")?;
            let index = self.target(key)?;
            if selected.contains(&index) {
                return Err(format!("Part '{key}' is selected more than once"));
            }
            selected.push(index);
        }
        Ok(selected)
    }
    pub(super) fn execute_boolean(&mut self, f: &Feature, p: &Value) -> Result<()> {
        let mode = text(p, "mode", "Union");
        let tools = self.part_scope(p, "tools")?;
        let keep = flag(p, "keep_tools");
        if mode == "Subtract" {
            let targets = self.part_scope(p, "targets")?;
            if targets.is_empty() || tools.is_empty() {
                return Err("Subtract requires target parts and tool parts".into());
            }
            if targets.iter().any(|i| tools.contains(i)) {
                return Err("A part cannot be both a target and a tool".into());
            }
            let mut outputs = vec![];
            for index in targets {
                let mut solid =
                    ops::copy::copy_solid(&mut self.topo, self.parts[index].solid).map_err(err)?;
                for &tool in &tools {
                    let tool = ops::copy::copy_solid(&mut self.topo, self.parts[tool].solid)
                        .map_err(err)?;
                    solid = ops::boolean::boolean(
                        &mut self.topo,
                        ops::boolean::BooleanOp::Cut,
                        solid,
                        tool,
                    )
                    .map_err(err)?;
                }
                self.parts[index].solid = solid;
                self.parts[index].feature = f.id.clone();
                outputs.push(self.parts[index].key.clone());
            }
            if !keep {
                let remove: BTreeSet<_> = tools
                    .into_iter()
                    .map(|i| self.parts[i].key.clone())
                    .collect();
                self.parts.retain(|part| !remove.contains(&part.key));
            }
            self.record_output(f, outputs);
        } else {
            let op = match mode {
                "Union" => ops::boolean::BooleanOp::Fuse,
                "Intersect" => ops::boolean::BooleanOp::Intersect,
                _ => return Err(format!("Unknown Boolean operation '{mode}'")),
            };
            if tools.len() < 2 {
                return Err("Union or Intersect requires at least two parts".into());
            }
            let first = tools[0];
            let mut solid =
                ops::copy::copy_solid(&mut self.topo, self.parts[first].solid).map_err(err)?;
            for &tool in &tools[1..] {
                let tool =
                    ops::copy::copy_solid(&mut self.topo, self.parts[tool].solid).map_err(err)?;
                solid = ops::boolean::boolean(&mut self.topo, op, solid, tool).map_err(err)?;
            }
            let key = if keep {
                let key = format!("{}:0", f.id);
                self.parts.push(Part {
                    key: key.clone(),
                    name: f.name.clone(),
                    solid,
                    feature: f.id.clone(),
                });
                key
            } else {
                let key = self.parts[first].key.clone();
                self.parts[first].solid = solid;
                self.parts[first].feature = f.id.clone();
                let remove: BTreeSet<_> = tools[1..]
                    .iter()
                    .map(|&i| self.parts[i].key.clone())
                    .collect();
                self.parts.retain(|p| !remove.contains(&p.key));
                key
            };
            self.record_output(f, vec![key]);
        }
        Ok(())
    }
}
pub(super) fn analytic_edge_axis(
    topo: &Topology,
    id: EdgeId,
) -> Result<Option<(brepkit_math::vec::Point3, Vec3)>> {
    let edge = topo.edge(id).map_err(err)?;
    let (a, mut direction) = match edge.curve() {
        EdgeCurve::Line => {
            let a = topo.vertex(edge.start()).map_err(err)?.point();
            let b = topo.vertex(edge.end()).map_err(err)?.point();
            (a, (b - a).normalize().map_err(err)?)
        }
        EdgeCurve::Circle(c) => (c.center(), c.normal()),
        _ => return Ok(None),
    };
    for x in [direction.x(), direction.y(), direction.z()] {
        if x.abs() > 1e-9 {
            if x < 0. {
                direction = -direction;
            }
            break;
        }
    }
    Ok(Some((a, direction)))
}
