//! Exact solid sweeps and ruled lofts. Unsupported curve combinations fail
//! before entering kernel paths that would polygonize the source geometry.
use super::*;
use brepkit_math::{nurbs::surface::NurbsSurface, vec::Point3};
use brepkit_topology::{builder::make_polygon_wire, face::Face};
use sketch::RegionLoop;

const EPS: f64 = 1e-7;
const ANGLE_EPS: f64 = 1e-12;

impl Geometry {
    fn input_sketch(&self, id: &str) -> Result<ResolvedSketch> {
        self.sketches.get(id).cloned().ok_or_else(|| {
            format!("Sketch '{id}' is missing. Choose an earlier, unsuppressed sketch.")
        })
    }

    pub(super) fn execute_loft(&mut self, f: &Feature, ordinal: usize, p: &Value) -> Result<()> {
        let ids = p
            .get("profiles")
            .and_then(Value::as_array)
            .ok_or("Choose an ordered list of loft profiles")?;
        if !(2..=32).contains(&ids.len()) {
            return Err("Loft needs between 2 and 32 profile sketches".into());
        }
        let mut unique = std::collections::BTreeSet::new();
        let mut profiles = vec![];
        let mut regions = vec![];
        for id in ids {
            let id = id
                .as_str()
                .ok_or("Loft profile references must be sketch IDs")?;
            if !unique.insert(id) {
                return Err("A loft profile cannot be selected twice".into());
            }
            let sk = self.input_sketch(id)?;
            if sk
                .entities
                .iter()
                .any(|e| !e.construction && e.kind == "arc")
            {
                return Err("Loft with arc boundaries is not supported yet; the curved profile was preserved".into());
            }
            let mut loops = sketch::loops(&sk)?;
            if loops.len() != 1 {
                return Err(
                    "Each loft sketch must contain one closed profile without holes".into(),
                );
            }
            regions.push(loops.remove(0));
            profiles.push(sk);
        }
        let axis = v3(profiles[0].plane.normal);
        let first = p3(profiles[0].plane.origin);
        let mut direction = 0.;
        for pair in profiles.windows(2) {
            if v3(pair[1].plane.normal).cross(axis).length() > ANGLE_EPS {
                return Err("Loft profile planes must be parallel".into());
            }
            let separation = (p3(pair[1].plane.origin) - p3(pair[0].plane.origin)).dot(axis);
            if separation.abs() < EPS || direction * separation < 0. {
                return Err(
                    "Loft profiles must advance through distinct planes in list order".into(),
                );
            }
            direction = separation.signum();
        }
        let mut polygon_sections = None;
        let circular = matches!(regions[0], RegionLoop::Circle { .. });
        match &regions[0] {
            RegionLoop::Circle { center, .. } => {
                let center0 = p3(profiles[0].plane.at(*center));
                let RegionLoop::Circle { center: next, .. } = &regions[1] else {
                    return Err(
                        "Mixing circular and polygon loft profiles would approximate curves".into(),
                    );
                };
                let center_axis = (p3(profiles[1].plane.at(*next)) - center0)
                    .normalize()
                    .map_err(err)?;
                for (sk, region) in profiles.iter().zip(&regions) {
                    let RegionLoop::Circle { center, .. } = region else {
                        return Err(
                            "Mixing circular and polygon loft profiles would approximate curves"
                                .into(),
                        );
                    };
                    let delta = p3(sk.plane.at(*center)) - center0;
                    if v3(sk.plane.normal).dot(center_axis).abs() <= 1. - 1e-12
                        || (delta - center_axis * delta.dot(center_axis)).length() > EPS
                    {
                        return Err("Circular loft profiles must share one axis".into());
                    }
                }
            }
            RegionLoop::Polygon { points, .. } => {
                for region in &regions {
                    let RegionLoop::Polygon { points: other, .. } = region else {
                        return Err(
                            "Mixing polygon and circular loft profiles would approximate curves"
                                .into(),
                        );
                    };
                    if points.len() != other.len() {
                        return Err("Polygon loft profiles need matching vertex counts".into());
                    }
                }
                let normal = axis * direction;
                let mut rings = vec![];
                for (sk, region) in profiles.iter().zip(&regions) {
                    let RegionLoop::Polygon { points, entity } = region else {
                        unreachable!()
                    };
                    let mut ring = points
                        .iter()
                        .map(|p| p3(sk.plane.at(*p)))
                        .collect::<Vec<_>>();
                    orient_polygon(&mut ring, normal);
                    // Rectangle primitives have an implicit local-min corner,
                    // which changes physical corner on a reversed plane. Match
                    // those corners in the first sketch's frame. Line polygons
                    // retain their explicit first vertex and correspondence.
                    if sk
                        .entities
                        .iter()
                        .any(|e| e.id == *entity && e.kind == "rectangle")
                    {
                        let u = v3(profiles[0].plane.u);
                        let v = v3(profiles[0].plane.v);
                        let index = (0..ring.len())
                            .min_by(|a, b| {
                                let delta = ring[*a] - ring[*b];
                                let x = delta.dot(u);
                                if x.abs() > EPS {
                                    x.total_cmp(&0.)
                                } else {
                                    delta.dot(v).total_cmp(&0.)
                                }
                            })
                            .unwrap();
                        ring.rotate_left(index);
                    }
                    rings.push(ring);
                }
                convex_transitions(&rings, normal)?;
                polygon_sections = Some(rings);
            }
        }
        let mut faces = vec![];
        for (i, sk) in profiles.iter().enumerate() {
            let built = if let Some(rings) = &polygon_sections {
                vec![polygon_face(
                    &mut self.topo,
                    rings[i].clone(),
                    axis * direction,
                )?]
            } else {
                sketch::faces(&mut self.topo, sk)?
            };
            if built.len() != 1
                || !self
                    .topo
                    .face(built[0])
                    .map_err(err)?
                    .inner_wires()
                    .is_empty()
            {
                return Err("Loft profiles cannot contain holes or separate regions".into());
            }
            faces.push(built[0]);
        }
        let solid = ruled_loft(&mut self.topo, &faces)?;
        if circular {
            let result = solid_faces(&self.topo, solid).map_err(err)?;
            let mut curved = 0;
            for id in result {
                match self.topo.face(id).map_err(err)?.surface() {
                    FaceSurface::Cylinder(_) | FaceSurface::Cone(_) => curved += 1,
                    FaceSurface::Plane { .. } => (),
                    _ => {
                        return Err(
                            "An exact analytic circular loft could not be constructed".into()
                        );
                    }
                }
            }
            if curved == 0 {
                return Err("Circular loft would polygonize the profiles".into());
            }
        }
        self.add_result(
            f,
            solid,
            text(p, "mode", "New"),
            text(p, "target", ""),
            ordinal * 100,
        )?;
        let end = p3(profiles.last().unwrap().plane.origin);
        let mut end_plane = profiles.last().unwrap().plane.clone();
        if v3(end_plane.normal).dot(axis) < 0. {
            end_plane.normal = end_plane.normal.map(|x| -x);
        }
        end_plane.v = cross(end_plane.normal, end_plane.u);
        self.cap_anchors(
            f,
            &profiles[0].plane,
            &end_plane,
            (end - first).dot(axis).signum(),
        );
        Ok(())
    }

    pub(super) fn execute_sweep(&mut self, f: &Feature, ordinal: usize, p: &Value) -> Result<()> {
        let profile = self.input_sketch(text(p, "profile", ""))?;
        let path = self.input_sketch(text(p, "path", ""))?;
        if profile
            .entities
            .iter()
            .chain(&path.entities)
            .any(|e| !e.construction && e.kind == "arc")
        {
            return Err("Sweep with arc boundaries or paths is not supported yet; the curved sketches were preserved".into());
        }
        if text(p, "profile", "") == text(p, "path", "") {
            return Err("Sweep profile and path must be different sketches".into());
        }
        let entities: Vec<_> = path.entities.iter().filter(|e| !e.construction).collect();
        if entities.is_empty() {
            return Err("Sweep path is empty".into());
        }
        let faces = sketch::faces(&mut self.topo, &profile)?;
        let regions = sketch::loops(&profile)?;
        let mut caps: Option<(Plane, Plane, f64)> = None;
        let solids = if entities.len() == 1 && entities[0].kind == "circle" {
            let e = entities[0];
            let center = p3(path.plane.at([e.values[0], e.values[1]]));
            let axis = v3(path.plane.normal);
            let n = v3(profile.plane.normal);
            if n.dot(axis).abs() > ANGLE_EPS
                || (p3(profile.plane.origin) - center).dot(n).abs() > EPS
            {
                return Err(
                    "Place the circular sweep profile on a radial plane through the path axis"
                        .into(),
                );
            }
            let outer = regions
                .iter()
                .max_by(|a, b| a.area().total_cmp(&b.area()))
                .ok_or("Sweep profile is empty")?;
            if faces.len() != 1 {
                return Err("A circular sweep needs one connected profile".into());
            }
            let centroid = p3(profile.plane.at(region_centroid(outer)));
            let delta = centroid - center;
            if delta.dot(axis).abs() > EPS || (delta.length() - e.values[2]).abs() > EPS {
                return Err("Place the profile center on the circular path; use a shared radius dimension to keep them together".into());
            }
            let radial = axis.cross(n).normalize().map_err(err)?;
            let mut interval = (f64::INFINITY, f64::NEG_INFINITY);
            for region in &regions {
                match region {
                    RegionLoop::Circle {
                        center: c, radius, ..
                    } => {
                        let x = (p3(profile.plane.at(*c)) - center).dot(radial);
                        interval.0 = interval.0.min(x - radius);
                        interval.1 = interval.1.max(x + radius);
                    }
                    RegionLoop::Polygon { points, .. } => {
                        for point in points {
                            let x = (p3(profile.plane.at(*point)) - center).dot(radial);
                            interval.0 = interval.0.min(x);
                            interval.1 = interval.1.max(x);
                        }
                    }
                }
            }
            if interval.0 <= EPS && interval.1 >= -EPS {
                return Err(
                    "Circular sweep profile reaches or crosses its axis and would self-intersect"
                        .into(),
                );
            }
            faces
                .iter()
                .map(|face| {
                    ops::revolve::revolve(
                        &mut self.topo,
                        *face,
                        center,
                        axis,
                        std::f64::consts::TAU,
                    )
                    .map_err(err)
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            let points = ordered_path(&path, &profile)?;
            let tangent = (points[1] - points[0]).normalize().map_err(err)?;
            if v3(profile.plane.normal).cross(tangent).length() > ANGLE_EPS
                || (p3(profile.plane.origin) - points[0]).dot(tangent).abs() > EPS
            {
                return Err(
                    "Place the sweep profile perpendicular to the start of its path".into(),
                );
            }
            if points.len() == 2 {
                let length = (points[1] - points[0]).length();
                let signed = length * tangent.dot(v3(profile.plane.normal)).signum();
                caps = Some((
                    profile.plane.clone(),
                    profile.plane.shifted(signed),
                    signed.signum(),
                ));
                faces
                    .iter()
                    .map(|face| {
                        ops::extrude::extrude(&mut self.topo, *face, tangent, length).map_err(err)
                    })
                    .collect::<Result<Vec<_>>>()?
            } else {
                if regions.len() != 1 || !matches!(regions[0], RegionLoop::Polygon { .. }) {
                    return Err("Sharp-corner sweeps currently need one polygon profile without holes; use a circular path for a curved tube".into());
                }
                let RegionLoop::Polygon {
                    points: polygon, ..
                } = &regions[0]
                else {
                    unreachable!()
                };
                let (sections, start, end) = miter_sections(&profile, &path, &points, polygon)?;
                let built = sections
                    .into_iter()
                    .map(|(vertices, normal)| polygon_face(&mut self.topo, vertices, normal))
                    .collect::<Result<Vec<_>>>()?;
                caps = Some((start, end, 1.));
                vec![ruled_loft(&mut self.topo, &built)?]
            }
        };
        for (i, solid) in solids.into_iter().enumerate() {
            self.add_result(
                f,
                solid,
                text(p, "mode", "New"),
                text(p, "target", ""),
                ordinal * 100 + i,
            )?;
        }
        if let Some((start, end, sign)) = caps {
            self.cap_anchors(f, &start, &end, sign);
        }
        Ok(())
    }

    fn cap_anchors(&mut self, f: &Feature, start: &Plane, end: &Plane, sign: f64) {
        let mut a = start.clone();
        let mut b = end.clone();
        a.normal = a.normal.map(|x| -sign * x);
        a.v = cross(a.normal, a.u);
        b.normal = b.normal.map(|x| sign * x);
        b.v = cross(b.normal, b.u);
        self.anchors.push((format!("{}:start", f.id), a));
        self.anchors.push((format!("{}:end", f.id), b));
    }
}

// Kernel polygon loft sides are planar by default. A warped quadrilateral is
// instead the exact bilinear ruled patch through its four boundary vertices.
fn ruled_loft(topo: &mut Topology, profiles: &[FaceId]) -> Result<SolidId> {
    let solid = ops::loft::loft(topo, profiles).map_err(err)?;
    for id in solid_faces(topo, solid).map_err(err)? {
        let face = topo.face(id).map_err(err)?;
        if !matches!(face.surface(), FaceSurface::Plane { .. }) {
            continue;
        }
        let wire = topo.wire(face.outer_wire()).map_err(err)?;
        if wire.edges().len() != 4 {
            continue;
        }
        let mut p = vec![];
        for oe in wire.edges() {
            let edge = topo.edge(oe.edge()).map_err(err)?;
            if !matches!(edge.curve(), EdgeCurve::Line) {
                break;
            }
            p.push(topo.vertex(oe.oriented_start(edge)).map_err(err)?.point());
        }
        if p.len() != 4 {
            continue;
        }
        let normal = (p[1] - p[0]).cross(p[3] - p[0]).normalize().map_err(err)?;
        if normal.dot(p[2] - p[0]).abs() <= EPS {
            continue;
        }
        let surface = NurbsSurface::new(
            1,
            1,
            vec![0., 0., 1., 1.],
            vec![0., 0., 1., 1.],
            vec![vec![p[0], p[3]], vec![p[1], p[2]]],
            vec![vec![1., 1.], vec![1., 1.]],
        )
        .map_err(err)?;
        topo.face_mut(id)
            .map_err(err)?
            .set_surface(FaceSurface::Nurbs(surface));
    }
    Ok(solid)
}

fn ordered_path(path: &ResolvedSketch, profile: &ResolvedSketch) -> Result<Vec<Point3>> {
    let mut points: Vec<Point3> = vec![];
    let mut edges = vec![];
    for e in path.entities.iter().filter(|e| !e.construction) {
        if e.kind != "line" {
            return Err("Sweep path must be connected lines or one circle".into());
        }
        if edges.len() >= 128 {
            return Err("Sweep path exceeds 128 line segments".into());
        }
        let mut nodes = [0; 2];
        for (i, node) in nodes.iter_mut().enumerate() {
            let p = p3(path.plane.at([e.values[i * 2], e.values[i * 2 + 1]]));
            *node = if let Some(n) = points.iter().position(|q| (*q - p).length() <= EPS) {
                n
            } else {
                points.push(p);
                points.len() - 1
            };
        }
        if nodes[0] == nodes[1]
            || edges
                .iter()
                .any(|old: &[usize; 2]| *old == nodes || *old == [nodes[1], nodes[0]])
        {
            return Err("Sweep path has a zero-length or duplicate segment".into());
        }
        edges.push(nodes);
    }
    let degree = |i| edges.iter().filter(|e| e.contains(&i)).count();
    if (0..points.len()).any(|i| degree(i) > 2) {
        return Err("Sweep path branches; choose one unbranched chain".into());
    }
    let ends: Vec<_> = (0..points.len()).filter(|i| degree(*i) == 1).collect();
    if ends.len() != 2 {
        return Err("Line sweep paths must form one open chain".into());
    }
    let normal = v3(profile.plane.normal);
    let starts: Vec<_> = ends
        .iter()
        .copied()
        .filter(|i| {
            if (points[*i] - p3(profile.plane.origin)).dot(normal).abs() > EPS {
                return false;
            }
            let edge = edges.iter().find(|e| e.contains(i)).unwrap();
            let other = if edge[0] == *i { edge[1] } else { edge[0] };
            (points[other] - points[*i])
                .normalize()
                .is_ok_and(|t| t.cross(normal).length() <= ANGLE_EPS)
        })
        .collect();
    let mut current = if starts.len() == 1 {
        starts[0]
    } else {
        ends[0]
    };
    let mut ordered = vec![points[current]];
    let mut used = vec![false; edges.len()];
    while let Some((i, e)) = edges
        .iter()
        .enumerate()
        .find(|(i, e)| !used[*i] && e.contains(&current))
    {
        used[i] = true;
        current = if e[0] == current { e[1] } else { e[0] };
        ordered.push(points[current]);
    }
    if used.iter().any(|u| !*u) {
        return Err("Sweep path contains disconnected chains".into());
    }
    Ok(ordered)
}

type MiterSections = Vec<(Vec<Point3>, Vec3)>;
fn miter_sections(
    profile: &ResolvedSketch,
    path: &ResolvedSketch,
    points: &[Point3],
    polygon: &[[f64; 2]],
) -> Result<(MiterSections, Plane, Plane)> {
    let normal = v3(path.plane.normal);
    let tangents = points
        .windows(2)
        .map(|p| (p[1] - p[0]).normalize().map_err(err))
        .collect::<Result<Vec<_>>>()?;
    let side = normal.cross(tangents[0]);
    let coords: Vec<_> = polygon
        .iter()
        .map(|p| {
            let d = p3(profile.plane.at(*p)) - points[0];
            (d.dot(side), d.dot(normal))
        })
        .collect();
    let mut sections = vec![];
    let mut max_stretch = 1_f64;
    for (i, origin) in points.iter().enumerate() {
        let (tangent, stretch) = if i == 0 {
            (tangents[0], 1.)
        } else if i == points.len() - 1 {
            (*tangents.last().unwrap(), 1.)
        } else {
            let tangent = (tangents[i - 1] + tangents[i])
                .normalize()
                .map_err(|_| "Sweep path reverses direction at a corner")?;
            let cosine = tangent.dot(tangents[i]);
            if cosine < 0.25 {
                return Err("Sweep corner exceeds the miter limit".into());
            }
            (tangent, 1. / cosine)
        };
        max_stretch = max_stretch.max(stretch);
        let side = normal.cross(tangent);
        let vertices: Vec<Point3> = coords
            .iter()
            .map(|(x, y)| *origin + side * (*x * stretch) + normal * *y)
            .collect();
        sections.push((vertices, tangent));
    }
    for (i, pair) in sections.windows(2).enumerate() {
        for (a, b) in pair[0].0.iter().zip(&pair[1].0) {
            if (*b - *a).dot(tangents[i]) <= EPS {
                return Err(
                    "Sweep miters overlap; lengthen the path segments or reduce the profile".into(),
                );
            }
        }
    }
    let reach = coords.iter().map(|(x, _)| x.abs()).fold(0_f64, f64::max) * max_stretch;
    for i in 0..tangents.len() {
        for j in i + 2..tangents.len() {
            if segment_distance(points[i], points[i + 1], points[j], points[j + 1], normal)
                <= 2. * reach + EPS
            {
                return Err(
                    "Nonadjacent sweep path segments are too close for this profile".into(),
                );
            }
        }
    }
    let plane = |i: usize| {
        let tangent = sections[i].1;
        let side = normal.cross(tangent);
        Plane {
            origin: [points[i].x(), points[i].y(), points[i].z()],
            u: [side.x(), side.y(), side.z()],
            v: path.plane.normal,
            normal: [tangent.x(), tangent.y(), tangent.z()],
        }
    };
    let start = plane(0);
    let end = plane(points.len() - 1);
    Ok((sections, start, end))
}

fn region_centroid(region: &RegionLoop) -> [f64; 2] {
    match region {
        RegionLoop::Circle { center, .. } => *center,
        RegionLoop::Polygon { points, .. } => {
            let mut area2 = 0.;
            let mut sum = [0., 0.];
            for (i, a) in points.iter().enumerate() {
                let b = points[(i + 1) % points.len()];
                let cross = a[0] * b[1] - b[0] * a[1];
                area2 += cross;
                sum[0] += (a[0] + b[0]) * cross;
                sum[1] += (a[1] + b[1]) * cross;
            }
            [sum[0] / (3. * area2), sum[1] / (3. * area2)]
        }
    }
}

fn segment_distance(a: Point3, b: Point3, c: Point3, d: Point3, normal: Vec3) -> f64 {
    let ab = b - a;
    let cd = d - c;
    let cross = normal.dot(ab.cross(cd));
    if cross.abs() > EPS * EPS {
        let t = normal.dot((c - a).cross(cd)) / cross;
        let u = normal.dot((c - a).cross(ab)) / cross;
        if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&u) {
            return 0.;
        }
    }
    let point_segment = |p: Point3, x: Point3, y: Point3| {
        let v = y - x;
        let t = ((p - x).dot(v) / v.dot(v)).clamp(0., 1.);
        (p - (x + v * t)).length()
    };
    point_segment(a, c, d)
        .min(point_segment(b, c, d))
        .min(point_segment(c, a, b))
        .min(point_segment(d, a, b))
}

fn polygon_face(topo: &mut Topology, mut points: Vec<Point3>, normal: Vec3) -> Result<FaceId> {
    orient_polygon(&mut points, normal);
    let d = normal.dot(points[0] - Point3::new(0., 0., 0.));
    let wire = make_polygon_wire(topo, &points, EPS).map_err(err)?;
    Ok(topo.add_face(Face::new(wire, vec![], FaceSurface::Plane { normal, d })))
}

fn orient_polygon(points: &mut [Point3], normal: Vec3) {
    let winding = points
        .iter()
        .enumerate()
        .fold(Vec3::new(0., 0., 0.), |sum, (i, p)| {
            sum + (*p - points[0]).cross(points[(i + 1) % points.len()] - points[0])
        });
    if winding.dot(normal) < 0. {
        points[1..].reverse();
    }
}

// Signed turns are quadratic in the section parameter. Check their true
// minimum, not a few sampled sections, so even a collapse at t=1/3 is rejected.
fn convex_transitions(rings: &[Vec<Point3>], normal: Vec3) -> Result<()> {
    for pair in rings.windows(2) {
        let n = pair[0].len();
        for i in 0..n {
            let edge = |ring: &[Point3], j: usize| ring[(j + 1) % n] - ring[j % n];
            let a = edge(&pair[0], i);
            let b = edge(&pair[0], i + 1);
            let da = edge(&pair[1], i) - a;
            let db = edge(&pair[1], i + 1) - b;
            let c0 = normal.dot(a.cross(b));
            let c1 = normal.dot(da.cross(b) + a.cross(db));
            let c2 = normal.dot(da.cross(db));
            let mut minimum = c0.min(c0 + c1 + c2);
            if c2 > 0. {
                let t = -c1 / (2. * c2);
                if (0. ..1.).contains(&t) {
                    minimum = minimum.min(c0 + c1 * t + c2 * t * t);
                }
            }
            let scale = a
                .length()
                .max(b.length())
                .max((a + da).length())
                .max((b + db).length());
            if minimum <= EPS * scale {
                return Err("Polygon loft sections must stay strictly convex without collapsing or crossing between profiles".into());
            }
        }
    }
    Ok(())
}

/// Exact divergence integral for complete bilinear patches and planar polygon
/// faces. Trimmed patches and every other surface family use the kernel measure.
pub(super) fn exact_ruled_volume(topo: &Topology, solid: SolidId) -> Option<f64> {
    let faces = solid_faces(topo, solid).ok()?;
    if !faces.iter().any(|id| {
        matches!(
            topo.face(*id).map(|f| f.surface()),
            Ok(FaceSurface::Nurbs(_))
        )
    }) {
        return None;
    }
    let first = topo.face(*faces.first()?).ok()?;
    let wire = topo.wire(first.outer_wire()).ok()?;
    let edge = topo.edge(wire.edges().first()?.edge()).ok()?;
    let about = topo.vertex(edge.start()).ok()?.point();
    let mut volume = 0.;
    for id in faces {
        let face = topo.face(id).ok()?;
        let sign = if face.is_reversed() { -1. } else { 1. };
        match face.surface() {
            FaceSurface::Plane { normal, d } => {
                let unit = normal.normalize().ok()?;
                let area = |wire_id| -> Option<f64> {
                    let wire = topo.wire(wire_id).ok()?;
                    let mut points = vec![];
                    for oe in wire.edges() {
                        let e = topo.edge(oe.edge()).ok()?;
                        if !matches!(e.curve(), EdgeCurve::Line) {
                            return None;
                        }
                        points.push(topo.vertex(oe.oriented_start(e)).ok()?.point());
                    }
                    if points.len() < 3 {
                        return None;
                    }
                    if points
                        .iter()
                        .any(|p| (normal.dot(*p - Point3::new(0., 0., 0.)) - d).abs() > EPS)
                    {
                        return None;
                    }
                    Some(
                        (1..points.len() - 1)
                            .map(|i| {
                                unit.dot((points[i] - points[0]).cross(points[i + 1] - points[0]))
                            })
                            .sum::<f64>()
                            .abs()
                            / 2.,
                    )
                };
                let mut a = area(face.outer_wire())?;
                for inner in face.inner_wires() {
                    a -= area(*inner)?;
                }
                volume +=
                    sign * (d / normal.length() - unit.dot(about - Point3::new(0., 0., 0.))) * a
                        / 3.;
            }
            FaceSurface::Nurbs(surface) => {
                let c = surface.control_points();
                if surface.degree_u() != 1
                    || surface.degree_v() != 1
                    || c.len() != 2
                    || c.iter().any(|r| r.len() != 2)
                    || surface.knots_u() != [0., 0., 1., 1.]
                    || surface.knots_v() != [0., 0., 1., 1.]
                    || surface
                        .weights()
                        .iter()
                        .flatten()
                        .any(|w| (*w - 1.).abs() > f64::EPSILON)
                    || !face.inner_wires().is_empty()
                {
                    return None;
                }
                let corners = [c[0][0], c[1][0], c[1][1], c[0][1]];
                let wire = topo.wire(face.outer_wire()).ok()?;
                if wire.edges().len() != 4 {
                    return None;
                }
                let mut boundary = vec![];
                for oe in wire.edges() {
                    let e = topo.edge(oe.edge()).ok()?;
                    if !matches!(e.curve(), EdgeCurve::Line) {
                        return None;
                    }
                    boundary.push(topo.vertex(oe.oriented_start(e)).ok()?.point());
                }
                let full = (0..4).any(|offset| {
                    [1_i32, -1].into_iter().any(|step| {
                        (0..4).all(|i| {
                            (boundary[i]
                                - corners[(offset + step * i as i32).rem_euclid(4) as usize])
                                .length()
                                <= EPS
                        })
                    })
                });
                if !full {
                    return None;
                }
                let p00 = c[0][0] - about;
                let p10 = c[1][0] - about;
                let p01 = c[0][1] - about;
                let p11 = c[1][1] - about;
                let nodes = [0.5 - 3_f64.sqrt() / 6., 0.5 + 3_f64.sqrt() / 6.];
                for u in nodes {
                    for v in nodes {
                        let p = p00 * ((1. - u) * (1. - v))
                            + p10 * (u * (1. - v))
                            + p01 * ((1. - u) * v)
                            + p11 * (u * v);
                        let du = (p10 - p00) * (1. - v) + (p11 - p01) * v;
                        let dv = (p01 - p00) * (1. - u) + (p11 - p10) * u;
                        volume += sign * p.dot(du.cross(dv)) / 12.;
                    }
                }
            }
            _ => return None,
        }
    }
    Some(volume.abs())
}
