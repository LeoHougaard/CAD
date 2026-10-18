//! Selectable planar cells. Display contours are sampled; topology uses exact
//! line segments and circle arcs throughout subdivision and extrusion.
use super::{ResolvedSketch, p3, v3};
use crate::model::{Plane, Result};
use brepkit_math::curves::Circle3D;
use brepkit_topology::{
    Topology,
    edge::{Edge, EdgeCurve},
    face::{Face, FaceId, FaceSurface},
    vertex::Vertex,
    wire::{OrientedEdge, Wire},
};
use std::collections::{BTreeMap, BTreeSet};

const EPS: f64 = 1e-7;
const TAU: f64 = std::f64::consts::TAU;
const MAX_EDGES: usize = 8192;

#[derive(Clone, Debug)]
pub struct ResolvedRegion {
    /// Entity-based identity, independent of coordinates and tessellation.
    pub key: String,
    /// Display-only contours in sketch coordinates. They are not model geometry.
    pub outer: Vec<[f64; 2]>,
    pub holes: Vec<Vec<[f64; 2]>>,
    /// Display fill, cached by the geometry worker. Never used for solid topology.
    pub triangles: Vec<[[f64; 2]; 3]>,
    /// Whole-sketch selection retains the legacy alternating nesting rule.
    pub legacy_selected: bool,
    boundary: Boundary,
    inner: Vec<Boundary>,
}
impl ResolvedRegion {
    pub fn contains(&self, point: [f64; 2]) -> bool {
        self.boundary.contains(point) && !self.inner.iter().any(|b| b.contains(point))
    }
}

#[derive(Clone, Copy, Debug)]
enum Curve {
    Line { a: [f64; 2], b: [f64; 2] },
    Circle { center: [f64; 2], radius: f64 },
}
impl Curve {
    fn at(&self, t: f64) -> [f64; 2] {
        match *self {
            Self::Line { a, b } => [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])],
            Self::Circle { center, radius } => {
                [center[0] + radius * t.cos(), center[1] + radius * t.sin()]
            }
        }
    }
    fn tangent(&self, t: f64) -> [f64; 2] {
        match *self {
            Self::Line { a, b } => sub(b, a),
            Self::Circle { radius, .. } => [-radius * t.sin(), radius * t.cos()],
        }
    }
    fn parameter(&self, p: [f64; 2]) -> f64 {
        match *self {
            Self::Line { a, b } => dot(sub(p, a), sub(b, a)) / dot(sub(b, a), sub(b, a)),
            Self::Circle { center, .. } => {
                (p[1] - center[1]).atan2(p[0] - center[0]).rem_euclid(TAU)
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Source {
    key: String,
    curve: Curve,
    splits: Vec<(f64, String)>,
    /// A bounded CCW interval on a circle; None denotes a complete circle.
    arc: Option<(f64, f64)>,
}
impl Source {
    fn parameter(&self, p: [f64; 2]) -> Option<f64> {
        let t = self.curve.parameter(p);
        if let Some((lo, hi)) = self.arc {
            let t = lo + (t - lo).rem_euclid(TAU);
            if (t - lo - TAU).abs() < EPS {
                return Some(lo);
            }
            (t <= hi + EPS).then_some(t.clamp(lo, hi))
        } else {
            Some(t)
        }
    }
    fn endpoint(&self, p: [f64; 2]) -> bool {
        match (self.curve, self.arc) {
            (Curve::Line { a, b }, _) => distance(a, p) < EPS || distance(b, p) < EPS,
            (_, Some((lo, hi))) => {
                distance(self.curve.at(lo), p) < EPS || distance(self.curve.at(hi), p) < EPS
            }
            _ => false,
        }
    }
}
#[derive(Clone, Debug)]
struct Segment {
    curve: Curve,
    start: f64,
    end: f64,
    key: String,
    start_key: String,
    end_key: String,
}
impl Segment {
    fn reversed(&self) -> Self {
        Self {
            start: self.end,
            end: self.start,
            start_key: self.end_key.clone(),
            end_key: self.start_key.clone(),
            ..self.clone()
        }
    }
    fn area(&self) -> f64 {
        let a = self.curve.at(self.start);
        let b = self.curve.at(self.end);
        match self.curve {
            Curve::Line { .. } => cross(a, b) / 2.,
            Curve::Circle { center, radius } => {
                (radius * center[0] * (self.end.sin() - self.start.sin())
                    + radius * center[1] * (self.start.cos() - self.end.cos())
                    + radius * radius * (self.end - self.start))
                    / 2.
            }
        }
    }
    fn tangent_angle(&self) -> f64 {
        let mut t = self.curve.tangent(self.start);
        if self.end < self.start {
            t = [-t[0], -t[1]];
        }
        t[1].atan2(t[0])
    }
}
#[derive(Clone, Debug)]
struct Boundary {
    segments: Vec<Segment>,
}
impl Boundary {
    fn area(&self) -> f64 {
        self.segments.iter().map(Segment::area).sum()
    }
    fn sample(&self) -> Vec<[f64; 2]> {
        let mut points = vec![];
        for e in &self.segments {
            let n = match e.curve {
                Curve::Line { .. } => 1,
                Curve::Circle { .. } => {
                    ((e.end - e.start).abs() / TAU * 128.).ceil().max(1.) as usize
                }
            };
            for i in 0..n {
                points.push(
                    e.curve
                        .at(e.start + (e.end - e.start) * i as f64 / n as f64),
                );
            }
        }
        points
    }
    fn contains(&self, p: [f64; 2]) -> bool {
        let mut winding = 0i32;
        for e in &self.segments {
            match e.curve {
                Curve::Line { .. } => {
                    let a = e.curve.at(e.start);
                    let b = e.curve.at(e.end);
                    if (a[1] > p[1]) != (b[1] > p[1]) {
                        let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
                        if x > p[0] {
                            winding += if b[1] > a[1] { 1 } else { -1 };
                        }
                    }
                }
                Curve::Circle { center, radius } => {
                    let y = (p[1] - center[1]) / radius;
                    if y.abs() >= 1. {
                        continue;
                    }
                    let angle = y.asin();
                    for a in [angle, std::f64::consts::PI - angle] {
                        let span = e.end - e.start;
                        let mut offset = if span > 0. {
                            (a - e.start).rem_euclid(TAU)
                        } else {
                            (e.start - a).rem_euclid(TAU)
                        };
                        if TAU - offset < 1e-12 {
                            offset = 0.;
                        }
                        let upward = a.cos() * span > 0.;
                        let crosses = if span.abs() > TAU - 1e-12 {
                            true
                        } else if offset < 1e-12 {
                            upward
                        } else if (offset - span.abs()).abs() < 1e-12 {
                            !upward
                        } else {
                            offset < span.abs()
                        };
                        // Count lower endpoints and exclude upper endpoints,
                        // just as for line crossings. Excluding every arc end
                        // double-counts extrema where different circles meet.
                        if crosses && center[0] + radius * a.cos() > p[0] {
                            winding += if upward { 1 } else { -1 };
                        }
                    }
                }
            }
        }
        winding != 0
    }
    fn key(&self) -> String {
        let tokens: Vec<_> = self
            .segments
            .iter()
            .map(|e| format!("{}{}", if e.end > e.start { "+" } else { "-" }, e.key))
            .collect();
        let Some(first) = (0..tokens.len()).min_by(|&a, &b| {
            tokens[a..]
                .iter()
                .chain(&tokens[..a])
                .cmp(tokens[b..].iter().chain(&tokens[..b]))
        }) else {
            return String::new();
        };
        tokens[first..]
            .iter()
            .chain(&tokens[..first])
            .cloned()
            .collect::<Vec<_>>()
            .join(";")
    }
    fn midpoint(&self) -> [f64; 2] {
        let e = &self.segments[0];
        e.curve.at((e.start + e.end) / 2.)
    }
    fn wire(&self, topo: &mut Topology, plane: &Plane) -> Result<brepkit_topology::wire::WireId> {
        let vertices: Vec<_> = self
            .segments
            .iter()
            .map(|e| topo.add_vertex(Vertex::new(p3(plane.at(e.curve.at(e.start))), 1e-8)))
            .collect();
        let mut edges = vec![];
        for (i, e) in self.segments.iter().enumerate() {
            let curve = match e.curve {
                Curve::Line { .. } => EdgeCurve::Line,
                Curve::Circle { center, radius } => {
                    let normal = if e.end > e.start {
                        plane.normal
                    } else {
                        plane.normal.map(|v| -v)
                    };
                    let reference = if (e.end - e.start).abs() > TAU - 1e-10 {
                        std::array::from_fn(|i| {
                            plane.u[i] * e.start.cos() + plane.v[i] * e.start.sin()
                        })
                    } else {
                        plane.u
                    };
                    EdgeCurve::Circle(
                        Circle3D::new_with_ref(
                            p3(plane.at(center)),
                            v3(normal),
                            radius,
                            v3(reference),
                        )
                        .map_err(|e| e.to_string())?,
                    )
                }
            };
            edges.push(OrientedEdge::new(
                topo.add_edge(Edge::new(
                    vertices[i],
                    vertices[(i + 1) % vertices.len()],
                    curve,
                )),
                true,
            ));
        }
        Ok(topo.add_wire(Wire::new(edges, true).map_err(|e| e.to_string())?))
    }
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn sources(sketch: &ResolvedSketch) -> Result<Vec<Source>> {
    let mut result = vec![];
    for e in sketch.entities.iter().filter(|e| !e.construction) {
        if e.kind == "circle" {
            result.push(Source {
                key: e.curve_key(0),
                curve: Curve::Circle {
                    center: [e.values[0], e.values[1]],
                    radius: e.values[2],
                },
                splits: vec![],
                arc: None,
            });
        } else if let Some(arc) = e.arc() {
            let lo = (arc.start + arc.sweep.min(0.)).rem_euclid(TAU);
            let hi = lo + arc.sweep.abs();
            let key = e.curve_key(0);
            let (a, b) = if arc.sweep > 0. { (1, 2) } else { (2, 1) };
            result.push(Source {
                curve: Curve::Circle {
                    center: arc.center,
                    radius: arc.radius,
                },
                splits: vec![(lo, format!("{key}:{a}")), (hi, format!("{key}:{b}"))],
                key,
                arc: Some((lo, hi)),
            });
        } else {
            let ps = e.points();
            for (i, pair) in ps.windows(2).enumerate() {
                if distance(pair[0], pair[1]) < EPS {
                    return Err("Sketch contains a zero-length boundary".into());
                }
                let key = e.curve_key(i);
                result.push(Source {
                    arc: None,
                    splits: vec![(0., format!("{key}:0")), (1., format!("{key}:1"))],
                    key,
                    curve: Curve::Line {
                        a: pair[0],
                        b: pair[1],
                    },
                });
            }
        }
    }
    if result.len() > 2000 {
        return Err("Sketch region subdivision exceeds 2000 boundary curves".into());
    }
    result.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(result)
}

/// Exact intersections. Collinear line endpoints split shared edges; coincident
/// circles and tangent contacts are rejected instead of producing sliver faces.
fn intersections(a: &Curve, b: &Curve) -> Result<Vec<[f64; 2]>> {
    match (*a, *b) {
        (Curve::Line { a: p, b: q }, Curve::Line { a: r, b: s }) => {
            let u = sub(q, p);
            let v = sub(s, r);
            let denominator = cross(u, v);
            if denominator.abs() < EPS * EPS {
                if cross(sub(r, p), u).abs() > EPS * dot(u, u).sqrt() {
                    return Ok(vec![]);
                }
                let mut points = vec![];
                for x in [p, q, r, s] {
                    let t = dot(sub(x, p), u) / dot(u, u);
                    let z = dot(sub(x, r), v) / dot(v, v);
                    if (-EPS..=1. + EPS).contains(&t)
                        && (-EPS..=1. + EPS).contains(&z)
                        && !points.iter().any(|&y| distance(x, y) < EPS)
                    {
                        points.push(x)
                    }
                }
                Ok(points)
            } else {
                let t = cross(sub(r, p), v) / denominator;
                let z = cross(sub(r, p), u) / denominator;
                Ok(
                    if (-EPS..=1. + EPS).contains(&t) && (-EPS..=1. + EPS).contains(&z) {
                        vec![a.at(t.clamp(0., 1.))]
                    } else {
                        vec![]
                    },
                )
            }
        }
        (Curve::Line { a: p, b: q }, Curve::Circle { center, radius }) => {
            let d = sub(q, p);
            let offset = sub(p, center);
            let aa = dot(d, d);
            let bb = 2. * dot(offset, d);
            let cc = dot(offset, offset) - radius * radius;
            let discriminant = bb * bb - 4. * aa * cc;
            let threshold = EPS * EPS * aa;
            if discriminant < -threshold {
                return Ok(vec![]);
            }
            let roots = [
                (-bb - discriminant.max(0.).sqrt()) / (2. * aa),
                (-bb + discriminant.max(0.).sqrt()) / (2. * aa),
            ];
            Ok(roots
                .into_iter()
                .filter(|t| (-EPS..=1. + EPS).contains(t))
                .map(|t| a.at(t.clamp(0., 1.)))
                .collect())
        }
        (Curve::Circle { .. }, Curve::Line { .. }) => intersections(b, a),
        (
            Curve::Circle {
                center: p,
                radius: r,
            },
            Curve::Circle {
                center: q,
                radius: s,
            },
        ) => {
            let d = distance(p, q);
            if d < EPS && (r - s).abs() < EPS {
                return Err("Duplicate circular sketch boundaries are ambiguous".into());
            }
            if d > r + s + EPS || d < (r - s).abs() - EPS {
                return Ok(vec![]);
            }
            if d < EPS {
                return Ok(vec![]);
            }
            let x = (r * r - s * s + d * d) / (2. * d);
            let h = (r * r - x * x).max(0.).sqrt();
            let u = [(q[0] - p[0]) / d, (q[1] - p[1]) / d];
            Ok(vec![
                [p[0] + x * u[0] - h * u[1], p[1] + x * u[1] + h * u[0]],
                [p[0] + x * u[0] + h * u[1], p[1] + x * u[1] - h * u[0]],
            ])
        }
    }
}

// The endpoint is a known root, so the other root needs no discriminant.
fn contact_line_circle(line: &Curve, circle: &Curve, p: [f64; 2]) -> Vec<[f64; 2]> {
    let (Curve::Line { a, b }, Curve::Circle { center, .. }) = (*line, *circle) else {
        unreachable!()
    };
    let d = sub(b, a);
    let t = -2. * dot(sub(p, center), d) / dot(d, d);
    let q = [p[0] + t * d[0], p[1] + t * d[1]];
    let mut points = vec![p];
    if (-EPS..=1. + EPS).contains(&line.parameter(q)) {
        points.push(q);
    }
    points.sort_by(|&p, &q| line.parameter(p).total_cmp(&line.parameter(q)));
    points
}

fn source_intersections(a: &Source, b: &Source) -> Result<Vec<[f64; 2]>> {
    let coincident = matches!((a.curve,b.curve), (Curve::Circle {center:p,radius:r}, Curve::Circle {center:q,radius:s}) if distance(p,q)<EPS && (r-s).abs()<EPS);
    // Factor intersections around a known common endpoint. The ordinary
    // quadratic loses precision at tangency and can invent two nearby roots.
    let ends = match (a.curve, a.arc) {
        (Curve::Line { a, b }, _) => Some([a, b]),
        (_, Some((lo, hi))) => Some([a.curve.at(lo), a.curve.at(hi)]),
        _ => None,
    };
    let contacts: Vec<_> = ends
        .into_iter()
        .flatten()
        .filter(|&p| b.endpoint(p))
        .collect();
    let mut points = if coincident {
        let (Some((lo, hi)), Some((u, v))) = (a.arc, b.arc) else {
            return Err("Duplicate circular sketch boundaries are ambiguous".into());
        };
        // Adjacent arcs on one circle are valid; overlapping spans are not.
        let interior = |source: &Source, p| source.parameter(p).is_some() && !source.endpoint(p);
        if [u, (u + v) / 2., v]
            .into_iter()
            .any(|t| interior(a, b.curve.at(t)))
            || [lo, (lo + hi) / 2., hi]
                .into_iter()
                .any(|t| interior(b, a.curve.at(t)))
        {
            return Err("Overlapping circular sketch arcs are ambiguous".into());
        }
        vec![a.curve.at(lo), a.curve.at(hi), b.curve.at(u), b.curve.at(v)]
    } else if let Some(&p) = contacts.first() {
        match (a.curve, b.curve) {
            (Curve::Line { .. }, Curve::Circle { .. }) => {
                contact_line_circle(&a.curve, &b.curve, p)
            }
            (Curve::Circle { .. }, Curve::Line { .. }) => {
                contact_line_circle(&b.curve, &a.curve, p)
            }
            (Curve::Circle { center: u, .. }, Curve::Circle { center: v, .. }) => {
                let d = sub(v, u);
                let delta = sub(p, u);
                let projection = dot(delta, d) / dot(d, d);
                let q = [
                    2. * (u[0] + projection * d[0]) - p[0],
                    2. * (u[1] + projection * d[1]) - p[1],
                ];
                if cross(d, delta) >= 0. {
                    vec![p, q]
                } else {
                    vec![q, p]
                }
            }
            _ => intersections(&a.curve, &b.curve)?,
        }
    } else {
        intersections(&a.curve, &b.curve)?
    };
    for p in contacts {
        if !points.iter().any(|&q| distance(p, q) < EPS) {
            points.push(p);
        }
    }
    let mut result = Vec::new();
    for p in points {
        let (Some(u), Some(v)) = (a.parameter(p), b.parameter(p)) else {
            continue;
        };
        if !coincident
            && (matches!(a.curve, Curve::Circle { .. }) || matches!(b.curve, Curve::Circle { .. }))
        {
            let x = a.curve.tangent(u);
            let y = b.curve.tangent(v);
            if cross(x, y).abs() <= EPS * dot(x, x).sqrt() * dot(y, y).sqrt()
                && !(a.endpoint(p) && b.endpoint(p))
            {
                return Err(
                    "Tangent sketch boundaries must meet at shared trimmed endpoints".into(),
                );
            }
        }
        if !result.iter().any(|&q| distance(p, q) < EPS) {
            result.push(p);
        }
    }
    Ok(result)
}

#[derive(Clone, Debug)]
pub struct TrimPiece {
    pub kind: String,
    pub values: Vec<f64>,
    pub source_edge: usize,
}

/// Remove the interval under the pointer. Display sampling never defines cuts.
pub fn trim_entity(
    sketch: &ResolvedSketch,
    entity: u64,
    edge: usize,
    point: [f64; 2],
) -> Result<Vec<TrimPiece>> {
    let original = sketch
        .entities
        .iter()
        .find(|e| e.id == entity)
        .ok_or("Trim entity is missing")?;
    let mut all = sketch.clone();
    for e in &mut all.entities {
        e.construction = false;
    }
    let boundaries = sources(&all)?;
    let key = original.curve_key(edge);
    let target = boundaries
        .iter()
        .find(|s| s.key == key)
        .ok_or("Trim edge is missing")?;
    let mut cuts = Vec::new();
    for other in boundaries.iter().filter(|s| s.key != key) {
        for p in intersections(&target.curve, &other.curve)? {
            if let (Some(t), Some(_)) = (target.parameter(p), other.parameter(p)) {
                cuts.push(t);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
    let circle = matches!(target.curve, Curve::Circle { .. }) && target.arc.is_none();
    let (lo, hi) = target
        .arc
        .unwrap_or(if circle { (0., TAU) } else { (0., 1.) });
    let cursor = target.parameter(point).unwrap_or_else(|| {
        if distance(point, target.curve.at(lo)) < distance(point, target.curve.at(hi)) {
            lo
        } else {
            hi
        }
    });
    let mut intervals = vec![];
    if circle {
        if cuts.len() >= 2 {
            let index = cuts.partition_point(|&t| t <= cursor);
            let start = cuts[(index + cuts.len() - 1) % cuts.len()];
            let mut end = cuts[index % cuts.len()];
            if end <= start {
                end += TAU;
            }
            if end - start < TAU - 1e-10 {
                intervals.push((end, start + TAU));
            }
        }
    } else {
        cuts.retain(|&t| t > lo + 1e-10 && t < hi - 1e-10);
        let left = cuts.iter().copied().rfind(|&t| t <= cursor).unwrap_or(lo);
        let right = cuts.iter().copied().find(|&t| t > cursor).unwrap_or(hi);
        if left > lo + 1e-10 {
            intervals.push((lo, left));
        }
        if right < hi - 1e-10 {
            intervals.push((right, hi));
        }
    }
    let mut result = vec![];
    if original.kind == "rectangle" {
        let points = original.points();
        for (index, pair) in points.windows(2).enumerate().filter(|(i, _)| *i != edge) {
            result.push(TrimPiece {
                kind: "line".into(),
                values: pair.iter().flatten().copied().collect(),
                source_edge: index,
            });
        }
    }
    for (start, end) in intervals {
        let (mut a, mut b) = (target.curve.at(start), target.curve.at(end));
        if distance(a, b) < EPS {
            continue;
        }
        let (kind, values) = match target.curve {
            Curve::Line { .. } => ("line", vec![a[0], a[1], b[0], b[1]]),
            Curve::Circle { center, .. } => {
                let direction = if original.kind == "arc" {
                    original.values[6]
                } else {
                    1.
                };
                if direction < 0. {
                    std::mem::swap(&mut a, &mut b);
                }
                (
                    "arc",
                    vec![center[0], center[1], a[0], a[1], b[0], b[1], direction],
                )
            }
        };
        result.push(TrimPiece {
            kind: kind.into(),
            values,
            source_edge: edge,
        });
    }
    Ok(result)
}

#[derive(Clone)]
struct GraphEdge {
    segment: Segment,
    start: usize,
    end: usize,
}
struct Graph {
    edges: Vec<GraphEdge>,
    outgoing: Vec<Vec<usize>>,
    vertices: Vec<[f64; 2]>,
}
impl Graph {
    fn remove_bridges(&mut self) {
        // Open lines may connect otherwise separate closed contours. Their
        // bridges do not bound a cell and must be removed before containment,
        // rather than leaving a wire that visits one vertex twice.
        let mut time = 0;
        let mut entered = vec![usize::MAX; self.vertices.len()];
        let mut low = entered.clone();
        let mut parent = vec![None; self.vertices.len()];
        let mut bridges = BTreeSet::new();
        for root in 0..self.vertices.len() {
            if entered[root] != usize::MAX {
                continue;
            }
            entered[root] = time;
            low[root] = time;
            time += 1;
            let mut stack = vec![(root, 0usize)];
            while let Some((v, next)) = stack.last_mut() {
                let vertex = *v;
                if *next < self.outgoing[vertex].len() {
                    let h = self.outgoing[vertex][*next];
                    *next += 1;
                    if parent[vertex] == Some(h / 2) {
                        continue;
                    }
                    let to = self.end(h);
                    if entered[to] == usize::MAX {
                        parent[to] = Some(h / 2);
                        entered[to] = time;
                        low[to] = time;
                        time += 1;
                        stack.push((to, 0));
                    } else {
                        low[vertex] = low[vertex].min(entered[to]);
                    }
                } else {
                    stack.pop();
                    if let Some(edge) = parent[vertex] {
                        let e = &self.edges[edge];
                        let previous = if e.start == vertex { e.end } else { e.start };
                        if low[vertex] > entered[previous] {
                            bridges.insert(edge);
                        }
                        low[previous] = low[previous].min(low[vertex]);
                    }
                }
            }
        }
        if bridges.is_empty() {
            return;
        }
        self.edges = std::mem::take(&mut self.edges)
            .into_iter()
            .enumerate()
            .filter_map(|(i, e)| (!bridges.contains(&i)).then_some(e))
            .collect();
        self.outgoing.iter_mut().for_each(Vec::clear);
        for (i, e) in self.edges.iter().enumerate() {
            self.outgoing[e.start].push(2 * i);
            self.outgoing[e.end].push(2 * i + 1);
        }
        for outgoing in &mut self.outgoing {
            outgoing.sort_by(|&a, &b| {
                let angle = |h: usize| {
                    if h.is_multiple_of(2) {
                        self.edges[h / 2].segment.tangent_angle()
                    } else {
                        self.edges[h / 2].segment.reversed().tangent_angle()
                    }
                };
                angle(a).total_cmp(&angle(b))
            });
        }
    }
    fn directed(&self, h: usize) -> Segment {
        let s = &self.edges[h / 2].segment;
        if h.is_multiple_of(2) {
            s.clone()
        } else {
            s.reversed()
        }
    }
    fn start(&self, h: usize) -> usize {
        let e = &self.edges[h / 2];
        if h.is_multiple_of(2) { e.start } else { e.end }
    }
    fn end(&self, h: usize) -> usize {
        self.start(h ^ 1)
    }
    fn boundary(&self, ids: &[usize]) -> Boundary {
        Boundary {
            segments: ids.iter().map(|&h| self.directed(h)).collect(),
        }
    }
    fn cycles(&self, allowed: &BTreeSet<usize>) -> Result<Vec<Vec<usize>>> {
        let mut used = BTreeSet::new();
        let mut cycles = vec![];
        for &first in allowed {
            if used.contains(&first) {
                continue;
            }
            let mut ids = vec![];
            let mut h = first;
            loop {
                if !used.insert(h) {
                    return Err("Sketch region boundary is not a closed manifold".into());
                }
                ids.push(h);
                let outgoing = &self.outgoing[self.end(h)];
                let at = outgoing.iter().position(|&x| x == h ^ 1).unwrap();
                h = (1..=outgoing.len())
                    .map(|i| outgoing[(at + outgoing.len() - i) % outgoing.len()])
                    .find(|x| allowed.contains(x))
                    .ok_or("Open sketch region boundary")?;
                if h == first {
                    break;
                }
                if ids.len() > MAX_EDGES * 2 {
                    return Err("Sketch region boundary exceeds the size limit".into());
                }
            }
            cycles.push(ids);
        }
        Ok(cycles)
    }
}

fn graph(sketch: &ResolvedSketch) -> Result<Graph> {
    let mut sources = sources(sketch)?;
    let mut split_count = sources.iter().map(|s| s.splits.len()).sum::<usize>();
    for i in 0..sources.len() {
        for j in i + 1..sources.len() {
            let points = source_intersections(&sources[i], &sources[j])?;
            split_count += points.len() * 2;
            if split_count > MAX_EDGES * 4 {
                return Err("Sketch region subdivision exceeds the intersection limit".into());
            }
            for (branch, p) in points.into_iter().enumerate() {
                let label = format!("x{}&{}#{branch}", sources[i].key, sources[j].key);
                let a = sources[i].parameter(p).unwrap();
                let b = sources[j].parameter(p).unwrap();
                sources[i].splits.push((a, label.clone()));
                sources[j].splits.push((b, label));
            }
        }
    }
    let mut vertices = vec![];
    let mut labels: Vec<BTreeSet<String>> = vec![];
    let mut parts = vec![];
    for source in &mut sources {
        if source.splits.is_empty() {
            source.splits.push((0., format!("{}:seam", source.key)))
        }
        source.splits.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut cuts: Vec<(f64, usize)> = vec![];
        for (t, label) in &source.splits {
            let p = source.curve.at(*t);
            let index = vertices
                .iter()
                .position(|&q| distance(p, q) < EPS)
                .unwrap_or_else(|| {
                    vertices.push(p);
                    labels.push(BTreeSet::new());
                    vertices.len() - 1
                });
            labels[index].insert(label.clone());
            if cuts.last().is_none_or(|&(_, v)| v != index) {
                cuts.push((*t, index))
            }
        }
        let circle = matches!(source.curve, Curve::Circle { .. }) && source.arc.is_none();
        let count = if circle {
            cuts.len()
        } else {
            cuts.len().saturating_sub(1)
        };
        for i in 0..count {
            let (start, a) = cuts[i];
            let (mut end, b) = cuts[(i + 1) % cuts.len()];
            if circle && i + 1 == cuts.len() {
                end += TAU
            }
            if end - start < 1e-12 {
                continue;
            }
            parts.push((
                source.key.clone(),
                Segment {
                    curve: source.curve,
                    start,
                    end,
                    key: String::new(),
                    start_key: String::new(),
                    end_key: String::new(),
                },
                a,
                b,
            ));
            if parts.len() > MAX_EDGES {
                return Err("Sketch region subdivision exceeds 8192 edges".into());
            }
        }
    }
    let mut edges: Vec<GraphEdge> = vec![];
    for (source, mut segment, a, b) in parts {
        // Shared collinear portions are one topological edge with all source
        // identities retained. Circular arcs cannot be duplicate here.
        let duplicate = if matches!(segment.curve, Curve::Line { .. }) {
            edges.iter_mut().find(|e| {
                matches!(e.segment.curve, Curve::Line { .. })
                    && ((e.start == a && e.end == b) || (e.start == b && e.end == a))
            })
        } else {
            None
        };
        if let Some(e) = duplicate {
            e.segment.key = format!("{}&{}", e.segment.key, source);
            continue;
        }
        segment.key = source;
        edges.push(GraphEdge {
            segment,
            start: a,
            end: b,
        });
    }
    for e in &mut edges {
        e.segment.start_key = labels[e.start]
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        e.segment.end_key = labels[e.end].iter().cloned().collect::<Vec<_>>().join(",");
        if e.start != e.end {
            e.segment.key = format!(
                "{}[{}>{}]",
                e.segment.key,
                labels[e.start]
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(","),
                labels[e.end].iter().cloned().collect::<Vec<_>>().join(",")
            );
        }
    }
    let mut outgoing = vec![vec![]; vertices.len()];
    for (i, e) in edges.iter().enumerate() {
        outgoing[e.start].push(2 * i);
        outgoing[e.end].push(2 * i + 1)
    }
    for list in &mut outgoing {
        list.sort_by(|&a, &b| {
            let angle = |h: usize| {
                if h.is_multiple_of(2) {
                    edges[h / 2].segment.tangent_angle()
                } else {
                    edges[h / 2].segment.reversed().tangent_angle()
                }
            };
            angle(a).total_cmp(&angle(b))
        });
    }
    let mut graph = Graph {
        edges,
        outgoing,
        vertices,
    };
    graph.remove_bridges();
    Ok(graph)
}

struct Cell {
    outer: Vec<usize>,
    holes: Vec<Vec<usize>>,
    component: usize,
    depth: usize,
}
fn boundary_key(boundary: &Boundary, holes: &[Boundary]) -> String {
    let mut holes: Vec<_> = holes.iter().map(Boundary::key).collect();
    holes.sort();
    format!("region:{}|{}", boundary.key(), holes.join("|"))
}
fn cell_key(graph: &Graph, cell: &Cell) -> String {
    boundary_key(
        &graph.boundary(&cell.outer),
        &cell
            .holes
            .iter()
            .map(|ids| graph.boundary(ids))
            .collect::<Vec<_>>(),
    )
}

fn cells(graph: &Graph) -> Result<Vec<Cell>> {
    let mut components = vec![usize::MAX; graph.vertices.len()];
    let mut component = 0;
    for first in 0..components.len() {
        if components[first] != usize::MAX {
            continue;
        }
        let mut stack = vec![first];
        components[first] = component;
        while let Some(v) = stack.pop() {
            for &h in &graph.outgoing[v] {
                let next = graph.end(h);
                if components[next] == usize::MAX {
                    components[next] = component;
                    stack.push(next)
                }
            }
        }
        component += 1;
    }
    let cycles = graph.cycles(&(0..graph.edges.len() * 2).collect())?;
    let mut positive = vec![];
    let mut negative = vec![];
    for ids in cycles {
        // Remove bridges: dangling lines have both directions on the same face
        // and do not enclose a region.
        let set: BTreeSet<_> = ids.iter().copied().collect();
        let ids: Vec<_> = ids
            .into_iter()
            .filter(|h| !set.contains(&(h ^ 1)))
            .collect();
        if ids.is_empty() {
            continue;
        }
        let area = graph.boundary(&ids).area();
        if area.abs() < EPS * EPS {
            continue;
        }
        let comp = components[graph.start(ids[0])];
        if area > 0. {
            positive.push(Cell {
                outer: ids,
                holes: vec![],
                component: comp,
                depth: 0,
            })
        } else {
            negative.push((ids, comp))
        }
    }
    if positive.is_empty() {
        return Err("Sketch has no closed regions".into());
    }
    let mut parents = BTreeMap::new();
    for (ids, comp) in negative {
        let boundary = graph.boundary(&ids);
        let p = boundary.midpoint();
        let parent = positive
            .iter()
            .enumerate()
            .filter(|(_, c)| c.component != comp && graph.boundary(&c.outer).contains(p))
            .min_by(|(_, a), (_, b)| {
                graph
                    .boundary(&a.outer)
                    .area()
                    .total_cmp(&graph.boundary(&b.outer).area())
            })
            .map(|(i, _)| i);
        if let Some(parent) = parent {
            positive[parent].holes.push(ids);
            parents.insert(comp, positive[parent].component);
        }
    }
    for c in &mut positive {
        let mut comp = c.component;
        while let Some(&parent) = parents.get(&comp) {
            c.depth += 1;
            comp = parent;
            if c.depth > components.len() {
                return Err("Cyclic sketch region containment".into());
            }
        }
    }
    Ok(positive)
}

/// All bounded cells, including enclosed discs and regions separated by shared
/// lines. Recompute after a sketch edit; identities fail closed on topology changes.
pub fn regions(sketch: &ResolvedSketch) -> Result<Vec<ResolvedRegion>> {
    let graph = graph(sketch)?;
    let mut result = vec![];
    for cell in cells(&graph)? {
        let boundary = graph.boundary(&cell.outer);
        let mut inner: Vec<_> = cell.holes.iter().map(|ids| graph.boundary(ids)).collect();
        inner.sort_by_key(Boundary::key);
        let key = boundary_key(&boundary, &inner);
        let mut region = ResolvedRegion {
            key,
            outer: boundary.sample(),
            holes: inner.iter().map(Boundary::sample).collect(),
            triangles: vec![],
            legacy_selected: cell.depth % 2 == 0,
            boundary,
            inner,
        };
        region.triangles = display_triangles(&region).unwrap_or_default();
        result.push(region);
    }
    result.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(result)
}

/// Build the union of explicitly selected cells. Empty keys preserve whole-sketch
/// parity selection for existing documents. Adjacent selected cells lose their
/// internal seam before any solid operation, avoiding coincident separate bodies.
pub fn faces_for_regions(
    topo: &mut Topology,
    sketch: &ResolvedSketch,
    keys: &[String],
) -> Result<Vec<FaceId>> {
    Ok(profile_for_regions(topo, sketch, keys)?.faces)
}

pub(crate) struct RegionProfile {
    pub faces: Vec<FaceId>,
    pub face_keys: Vec<String>,
    pub face_edges: Vec<Vec<RegionEdge>>,
}
#[derive(Clone)]
pub(crate) struct RegionEdge {
    pub key: String,
    pub start_key: String,
    pub end_key: String,
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub midpoint: [f64; 2],
    pub circle: Option<([f64; 2], f64)>,
}
impl From<&Segment> for RegionEdge {
    fn from(edge: &Segment) -> Self {
        Self {
            key: format!(
                "{}{}",
                if edge.end > edge.start { "+" } else { "-" },
                edge.key
            ),
            start_key: edge.start_key.clone(),
            end_key: edge.end_key.clone(),
            start: edge.curve.at(edge.start),
            end: edge.curve.at(edge.end),
            midpoint: edge.curve.at((edge.start + edge.end) / 2.),
            circle: match edge.curve {
                Curve::Circle { center, radius } => Some((center, radius)),
                Curve::Line { .. } => None,
            },
        }
    }
}
pub(crate) fn region_edges(sketch: &ResolvedSketch) -> Result<Vec<RegionEdge>> {
    Ok(selected_boundaries(sketch, &[])?
        .iter()
        .flat_map(|(boundary, holes)| {
            std::iter::once(boundary)
                .chain(holes)
                .flat_map(|b| b.segments.iter().map(RegionEdge::from))
        })
        .collect())
}
pub(crate) fn region_profile_keys(sketch: &ResolvedSketch, keys: &[String]) -> Result<Vec<String>> {
    Ok(selected_boundaries(sketch, keys)?.iter()
        .map(|(boundary, holes)| boundary_key(boundary, holes)).collect())
}

pub(crate) fn profile_for_regions(
    topo: &mut Topology,
    sketch: &ResolvedSketch,
    keys: &[String],
) -> Result<RegionProfile> {
    let outer = selected_boundaries(sketch, keys)?;
    let n = sketch.plane.normal;
    let d = dot3(n, sketch.plane.origin);
    let mut faces = vec![];
    let mut face_keys = vec![];
    let mut face_edges = vec![];
    for (boundary, holes) in outer {
        face_keys.push(boundary_key(&boundary, &holes));
        face_edges.push(
            std::iter::once(&boundary)
                .chain(&holes)
                .flat_map(|b| b.segments.iter().map(RegionEdge::from))
                .collect(),
        );
        let outer = boundary.wire(topo, &sketch.plane)?;
        let inner = holes
            .iter()
            .map(|b| b.wire(topo, &sketch.plane))
            .collect::<Result<Vec<_>>>()?;
        faces.push(topo.add_face(Face::new(
            outer,
            inner,
            FaceSurface::Plane { normal: v3(n), d },
        )));
    }
    if faces.is_empty() {
        return Err("Choose at least one closed sketch region".into());
    }
    Ok(RegionProfile {
        faces,
        face_keys,
        face_edges,
    })
}

fn selected_boundaries(
    sketch: &ResolvedSketch,
    keys: &[String],
) -> Result<Vec<(Boundary, Vec<Boundary>)>> {
    let graph = graph(sketch)?;
    let cells = cells(&graph)?;
    let available: BTreeSet<_> = cells.iter().map(|cell| cell_key(&graph, cell)).collect();
    let mut selected = BTreeSet::new();
    if keys.len() > 4096 {
        return Err("Extrude exceeds 4096 selected regions".into());
    }
    for key in keys {
        let resolved = if let Some(exact) = available.get(key) {
            exact
        } else {
            let mut matches = available.iter().filter(|candidate| crate::model::reference_matches(candidate, key));
            let first = matches.next();
            if first.is_none() || matches.next().is_some() {
                return Err("A selected sketch region changed or no longer exists. Select the intended regions again.".into());
            }
            first.unwrap()
        };
        if !selected.insert(resolved.clone()) {
            return Err("Extrude contains a duplicate sketch region selection".into());
        }
    }
    let mut allowed = BTreeSet::new();
    for cell in &cells {
        let key = cell_key(&graph, cell);
        let take = if keys.is_empty() {
            cell.depth % 2 == 0
        } else {
            selected.contains(&key)
        };
        if !take {
            continue;
        }
        for &h in cell.outer.iter().chain(cell.holes.iter().flatten()) {
            if !allowed.remove(&(h ^ 1)) {
                allowed.insert(h);
            }
        }
    }
    let mut outer = vec![];
    let mut holes = vec![];
    for ids in graph.cycles(&allowed)? {
        let boundary = graph.boundary(&ids);
        if boundary.area() > EPS * EPS {
            outer.push((boundary, vec![]))
        } else if boundary.area() < -EPS * EPS {
            holes.push(boundary)
        }
    }
    for hole in holes {
        let point = hole.midpoint();
        let at = outer
            .iter()
            .enumerate()
            .filter(|(_, (b, _))| b.contains(point))
            .min_by(|(_, (a, _)), (_, (b, _))| a.area().total_cmp(&b.area()))
            .map(|(i, _)| i)
            .ok_or("Selected sketch regions have an invalid hole boundary")?;
        outer[at].1.push(hole);
    }
    Ok(outer)
}
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn display_triangles(region: &ResolvedRegion) -> Result<Vec<[[f64; 2]; 3]>> {
    use brepkit_math::{cdt::Cdt, vec::Point2};
    let contours: Vec<_> = std::iter::once(&region.outer)
        .chain(&region.holes)
        .collect();
    let points: Vec<_> = contours
        .iter()
        .flat_map(|c| c.iter().map(|p| Point2::new(p[0], p[1])))
        .collect();
    if points.len() > 16_384 {
        return Err("Sketch region display fill exceeds the size limit".into());
    }
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in &points {
        lo[0] = lo[0].min(p.x());
        lo[1] = lo[1].min(p.y());
        hi[0] = hi[0].max(p.x());
        hi[1] = hi[1].max(p.y());
    }
    let mut cdt = Cdt::with_capacity(
        (Point2::new(lo[0], lo[1]), Point2::new(hi[0], hi[1])),
        points.len(),
    );
    let ids = cdt
        .insert_points_hilbert(&points)
        .map_err(|e| e.to_string())?;
    let mut offset = 0;
    for contour in contours {
        for i in 0..contour.len() {
            cdt.insert_constraint(ids[offset + i], ids[offset + (i + 1) % contour.len()])
                .map_err(|e| e.to_string())?;
        }
        offset += contour.len();
    }
    let vertices = cdt.vertices();
    Ok(cdt
        .triangles()
        .into_iter()
        .filter_map(|(a, b, c)| {
            let tri = [a, b, c].map(|i| [vertices[i].x(), vertices[i].y()]);
            let center = [
                (tri[0][0] + tri[1][0] + tri[2][0]) / 3.,
                (tri[0][1] + tri[1][1] + tri[2][1]) / 3.,
            ];
            region.contains(center).then_some(tri)
        })
        .collect())
}

#[cfg(test)]
mod arc_tests {
    use super::*;
    use crate::sketch::ResolvedEntity;
    fn entity(id: u64, kind: &str, values: &[f64]) -> ResolvedEntity {
        ResolvedEntity {
            id,
            kind: kind.into(),
            curve_identity: None,
            values: values.to_vec(),
            construction: false,
            fixed: false,
        }
    }
    fn sketch(entities: Vec<ResolvedEntity>) -> ResolvedSketch {
        ResolvedSketch {
            entities,
            plane: Plane::named("Top", 0.),
            dof: 0,
        }
    }
    #[test]
    fn trim_uses_exact_intersections_and_returns_all_remaining_pieces() {
        let sk = sketch(vec![
            entity(1, "line", &[0., 0., 10., 0.]),
            entity(2, "line", &[2., -2., 2., 2.]),
            entity(3, "line", &[8., -2., 8., 2.]),
        ]);
        let pieces = trim_entity(&sk, 1, 0, [5., 0.]).unwrap();
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].values, vec![0., 0., 2., 0.]);
        assert_eq!(pieces[1].values, vec![8., 0., 10., 0.]);
        let sk = sketch(vec![
            entity(1, "circle", &[0., 0., 5.]),
            entity(2, "line", &[-6., 0., 6., 0.]),
        ]);
        let pieces = trim_entity(&sk, 1, 0, [0., 5.]).unwrap();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].kind, "arc");
        let arc = super::super::arc_geometry(&pieces[0].values).unwrap();
        assert!((arc.sweep - std::f64::consts::PI).abs() < 1e-12);
        assert!(arc.start.sin().abs() < 1e-12);
        assert!(arc.start.cos() < 0.);
    }
    #[test]
    fn trim_preserves_clockwise_orientation_and_other_rectangle_sides() {
        let sk = sketch(vec![
            entity(1, "arc", &[0., 0., 5., 0., 0., 5., -1.]),
            entity(2, "line", &[0., -6., 0., 6.]),
        ]);
        let pieces = trim_entity(&sk, 1, 0, [-5., 0.]).unwrap();
        assert_eq!(pieces.len(), 1);
        let arc = super::super::arc_geometry(&pieces[0].values).unwrap();
        assert!((arc.sweep + std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        let sk = sketch(vec![
            entity(1, "rectangle", &[0., 0., 10., 10.]),
            entity(2, "line", &[5., -5., 5., 15.]),
        ]);
        let pieces = trim_entity(&sk, 1, 0, [2., 0.]).unwrap();
        assert_eq!(pieces.len(), 4);
        assert_eq!(pieces.iter().filter(|p| p.source_edge != 0).count(), 3);
        assert!(
            pieces
                .iter()
                .any(|p| p.source_edge == 0 && p.values == vec![5., 0., 10., 0.])
        );
        assert!(
            trim_entity(
                &sketch(vec![entity(1, "circle", &[0., 0., 5.])]),
                1,
                0,
                [0., 5.]
            )
            .unwrap()
            .is_empty()
        );
    }
    #[test]
    fn bounded_semicircle_is_exact_and_clockwise_selects_the_other_half() {
        for (direction, sign) in [(1., 1.), (-1., -1.)] {
            let sk = sketch(vec![
                entity(1, "arc", &[0., 0., 5., 0., -5., 0., direction]),
                entity(2, "line", &[-5., 0., 5., 0.]),
            ]);
            let cells = regions(&sk).unwrap();
            assert_eq!(cells.len(), 1);
            assert!((cells[0].boundary.area() - std::f64::consts::PI * 12.5).abs() < 1e-9);
            assert!(cells[0].contains([0., sign * 2.]));
            assert!(!cells[0].contains([0., -sign * 2.]));
            let mut topo = Topology::new();
            let faces = faces_for_regions(&mut topo, &sk, &[]).unwrap();
            assert_eq!(faces.len(), 1);
            assert!(
                cells[0]
                    .boundary
                    .segments
                    .iter()
                    .any(|e| matches!(e.curve, Curve::Circle { .. }))
            );
        }
    }
    #[test]
    fn tangent_arc_line_endpoints_make_an_exact_stadium() {
        let sk = sketch(vec![
            entity(1, "arc", &[-10., 0., -10., 5., -10., -5., 1.]),
            entity(2, "line", &[-10., -5., 10., -5.]),
            entity(3, "arc", &[10., 0., 10., -5., 10., 5., 1.]),
            entity(4, "line", &[10., 5., -10., 5.]),
        ]);
        let cells = regions(&sk).unwrap();
        assert_eq!(cells.len(), 1);
        assert!((cells[0].boundary.area() - (200. + std::f64::consts::PI * 25.)).abs() < 1e-8);
        for p in [[-14., 0.], [14., 0.], [0., 4.]] {
            assert!(cells[0].contains(p));
        }
        assert!(!cells[0].contains([14., 4.]));
        let old_key = crate::model::canonical_reference(&cells[0].key);
        assert_ne!(old_key, cells[0].key);
        let mut topo = Topology::new();
        assert_eq!(faces_for_regions(&mut topo, &sk, std::slice::from_ref(&old_key)).unwrap().len(), 1);
        assert!(faces_for_regions(&mut topo, &sk, &[old_key, cells[0].key.clone()]).is_err());
    }
    #[test]
    fn complementary_arcs_close_one_circle_and_overlaps_reject() {
        let mut sk = sketch(vec![
            entity(1, "arc", &[0., 0., 5., 0., -5., 0., 1.]),
            entity(2, "arc", &[0., 0., -5., 0., 5., 0., 1.]),
        ]);
        let cells = regions(&sk).unwrap();
        assert_eq!(cells.len(), 1);
        assert!((cells[0].boundary.area() - std::f64::consts::PI * 25.).abs() < 1e-9);
        sk.entities[1].values = sk.entities[0].values.clone();
        assert!(regions(&sk).unwrap_err().contains("Overlapping"));
    }
    #[test]
    fn off_span_intersections_do_not_split_an_arc() {
        let sk = sketch(vec![
            entity(1, "arc", &[0., 0., 5., 0., -5., 0., 1.]),
            entity(2, "line", &[-5., 0., 5., 0.]),
            entity(3, "line", &[-8., -2., 8., -2.]),
        ]);
        let cells = regions(&sk).unwrap();
        assert_eq!(cells.len(), 1);
        assert!((cells[0].boundary.area() - std::f64::consts::PI * 12.5).abs() < 1e-9);
        assert_eq!(cells[0].boundary.segments.len(), 2);
    }
}
