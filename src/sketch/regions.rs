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
}
#[derive(Clone, Debug)]
struct Segment {
    curve: Curve,
    start: f64,
    end: f64,
    key: String,
}
impl Segment {
    fn reversed(&self) -> Self {
        Self {
            start: self.end,
            end: self.start,
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
                key: format!("c{}", e.id),
                curve: Curve::Circle {
                    center: [e.values[0], e.values[1]],
                    radius: e.values[2],
                },
                splits: vec![],
            });
        } else {
            let ps = e.points();
            for (i, pair) in ps.windows(2).enumerate() {
                if distance(pair[0], pair[1]) < EPS {
                    return Err("Sketch contains a zero-length boundary".into());
                }
                let key = if e.kind == "line" {
                    format!("l{}", e.id)
                } else {
                    format!("r{}.{i}", e.id)
                };
                result.push(Source {
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
            if discriminant.abs() <= threshold
                && roots.iter().any(|t| (-EPS..=1. + EPS).contains(t))
            {
                return Err("Tangent sketch boundaries need a small separation or a shared trimmed endpoint".into());
            }
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
            if (d - r - s).abs() < EPS || (d - (r - s).abs()).abs() < EPS {
                return Err("Tangent sketch circles need a small separation".into());
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
            let points = intersections(&sources[i].curve, &sources[j].curve)?;
            split_count += points.len() * 2;
            if split_count > MAX_EDGES * 4 {
                return Err("Sketch region subdivision exceeds the intersection limit".into());
            }
            for (branch, p) in points.into_iter().enumerate() {
                let label = format!("x{}&{}#{branch}", sources[i].key, sources[j].key);
                let a = sources[i].curve.parameter(p);
                let b = sources[j].curve.parameter(p);
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
        let circle = matches!(source.curve, Curve::Circle { .. });
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
fn cell_key(graph: &Graph, cell: &Cell) -> String {
    let mut holes: Vec<_> = cell
        .holes
        .iter()
        .map(|ids| graph.boundary(ids).key())
        .collect();
    holes.sort();
    format!(
        "region:{}|{}",
        graph.boundary(&cell.outer).key(),
        holes.join("|")
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
        let key = format!(
            "region:{}|{}",
            boundary.key(),
            inner
                .iter()
                .map(Boundary::key)
                .collect::<Vec<_>>()
                .join("|")
        );
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
    pub sides: Vec<(String, [f64; 2], [f64; 2])>,
}
pub(crate) fn profile_for_regions(
    topo: &mut Topology,
    sketch: &ResolvedSketch,
    keys: &[String],
) -> Result<RegionProfile> {
    let graph = graph(sketch)?;
    let cells = cells(&graph)?;
    let available: BTreeSet<_> = cells.iter().map(|cell| cell_key(&graph, cell)).collect();
    let mut selected = BTreeSet::new();
    if keys.len() > 4096 {
        return Err("Extrude exceeds 4096 selected regions".into());
    }
    for key in keys {
        if !selected.insert(key.clone()) {
            return Err("Extrude contains a duplicate sketch region selection".into());
        }
        if !available.contains(key) {
            return Err("A selected sketch region changed or no longer exists. Select the intended regions again.".into());
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
    let n = sketch.plane.normal;
    let d = dot3(n, sketch.plane.origin);
    let mut faces = vec![];
    let mut sides = vec![];
    for (boundary, holes) in outer {
        for b in std::iter::once(&boundary).chain(&holes) {
            for edge in &b.segments {
                if matches!(edge.curve, Curve::Line { .. }) {
                    sides.push((
                        format!(
                            "{}{}",
                            if edge.end > edge.start { "+" } else { "-" },
                            edge.key
                        ),
                        edge.curve.at(edge.start),
                        edge.curve.at(edge.end),
                    ));
                }
            }
        }
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
    Ok(RegionProfile { faces, sides })
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
