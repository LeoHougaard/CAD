use crate::{
    model::{Entity, Plane, Result, Sketch},
    plugins::Numbers,
};
use brepkit_math::vec::{Point3, Vec3};
use brepkit_sketch::{Constraint, GcsSystem, PointData};
use brepkit_topology::{
    Topology,
    builder::{make_circle_edge_with_ref, make_polygon_wire},
    face::{Face, FaceId, FaceSurface},
    wire::{OrientedEdge, Wire},
};
use std::collections::{BTreeMap, BTreeSet};

mod regions;
mod trim;
pub(crate) use regions::{RegionEdge, profile_for_regions, region_edges};
pub use regions::{ResolvedRegion, TrimPiece, faces_for_regions, regions, trim_entity};
pub use trim::{TrimEdit, apply_trim};

#[derive(Clone, Debug)]
pub struct ResolvedEntity {
    pub id: u64,
    pub kind: String,
    pub values: Vec<f64>,
    pub construction: bool,
    pub fixed: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct ArcGeometry {
    pub center: [f64; 2],
    pub radius: f64,
    pub start: f64,
    pub sweep: f64,
}
pub fn arc_geometry(values: &[f64]) -> Result<ArcGeometry> {
    if values.len() != 7 || values.iter().any(|v| !v.is_finite()) || values[6].abs() != 1. {
        return Err("Arc needs center, start, end and a direction of +1 or -1".into());
    }
    let center = [values[0], values[1]];
    let radius = (values[2] - center[0]).hypot(values[3] - center[1]);
    let end_radius = (values[4] - center[0]).hypot(values[5] - center[1]);
    if radius <= 1e-7 || end_radius <= 1e-7 {
        return Err("Arc radius must be positive".into());
    }
    let start = (values[3] - center[1]).atan2(values[2] - center[0]);
    let end = (values[5] - center[1]).atan2(values[4] - center[0]);
    let sweep = if values[6] > 0. {
        (end - start).rem_euclid(std::f64::consts::TAU)
    } else {
        -(start - end).rem_euclid(std::f64::consts::TAU)
    };
    if sweep.abs() < 1e-7 {
        return Err("Arc endpoints must be distinct; use a circle for a full turn".into());
    }
    Ok(ArcGeometry {
        center,
        radius,
        start,
        sweep,
    })
}
impl ResolvedEntity {
    pub fn control_points(&self) -> Vec<[f64; 2]> {
        match self.kind.as_str() {
            "rectangle" => self.points().into_iter().take(4).collect(),
            "circle" => vec![[self.values[0], self.values[1]]],
            "arc" => vec![
                [self.values[0], self.values[1]],
                [self.values[2], self.values[3]],
                [self.values[4], self.values[5]],
            ],
            _ => vec![
                [self.values[0], self.values[1]],
                [self.values[2], self.values[3]],
            ],
        }
    }
    pub fn arc(&self) -> Option<ArcGeometry> {
        (self.kind == "arc")
            .then(|| arc_geometry(&self.values).ok())
            .flatten()
    }
    pub fn points(&self) -> Vec<[f64; 2]> {
        let p = &self.values;
        match self.kind.as_str() {
            "rectangle" => vec![
                [p[0], p[1]],
                [p[0] + p[2], p[1]],
                [p[0] + p[2], p[1] + p[3]],
                [p[0], p[1] + p[3]],
                [p[0], p[1]],
            ],
            "arc" => self
                .arc()
                .map(|arc| {
                    let count = (arc.sweep.abs() / std::f64::consts::TAU * 128.)
                        .ceil()
                        .max(2.) as usize;
                    (0..=count)
                        .map(|i| {
                            let angle = arc.start + arc.sweep * i as f64 / count as f64;
                            [
                                arc.center[0] + arc.radius * angle.cos(),
                                arc.center[1] + arc.radius * angle.sin(),
                            ]
                        })
                        .collect()
                })
                .unwrap_or_default(),
            "circle" => (0..=96)
                .map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / 96.;
                    [p[0] + p[2] * a.cos(), p[1] + p[2] * a.sin()]
                })
                .collect(),
            _ => vec![[p[0], p[1]], [p[2], p[3]]],
        }
    }
}
#[derive(Clone, Debug)]
pub struct ResolvedSketch {
    pub entities: Vec<ResolvedEntity>,
    pub plane: Plane,
    pub dof: usize,
}

pub fn seed_from_resolved(sketch: &mut Sketch, resolved: Option<&ResolvedSketch>) {
    if let Some(sk) = resolved {
        for e in &mut sketch.entities {
            if let Some(s) = sk.entities.iter().find(|s| s.id == e.id) {
                for (value, &v) in e.values.iter_mut().zip(&s.values) {
                    if value.parse::<f64>().is_ok() {
                        *value = crate::model::fmt(v);
                    }
                }
            }
        }
    }
}
fn constraints_with_expressions(sketch: &Sketch) -> Vec<crate::model::SketchConstraint> {
    let mut constraints = sketch.constraints.clone();
    for entity in &sketch.entities {
        let labels = match entity.kind.as_str() {
            "rectangle" => vec!["X", "Y", "Width", "Height"],
            "circle" => vec!["X", "Y", "Radius"],
            "arc" => vec!["X", "Y", "StartX", "StartY", "EndX", "EndY"],
            _ => vec!["X", "Y", "EndX", "EndY"],
        };
        for (label, value) in labels.into_iter().zip(&entity.values) {
            if value.parse::<f64>().is_err()
                && !constraints
                    .iter()
                    .any(|c| c.a == entity.id && c.kind == label)
            {
                constraints.push(crate::model::SketchConstraint {
                    kind: label.into(),
                    a: entity.id,
                    value: value.clone(),
                    ..Default::default()
                });
            }
        }
    }
    constraints
}

pub fn resolve(sketch: &Sketch, plane: Plane, numbers: &Numbers) -> Result<ResolvedSketch> {
    resolve_with_targets(sketch, plane, numbers, &[])
}

/// Temporary mouse targets have lower priority than every persisted constraint.
pub fn resolve_with_targets(
    sketch: &Sketch,
    plane: Plane,
    numbers: &Numbers,
    targets: &[crate::model::SketchConstraint],
) -> Result<ResolvedSketch> {
    if sketch.entities.len() > 1000 {
        return Err("Sketch exceeds 1000 entities".into());
    }
    let mut entities = Vec::new();
    let mut seen = BTreeSet::new();
    for entity in &sketch.entities {
        if !seen.insert(entity.id) {
            return Err("Duplicate sketch entity identity".into());
        }
        let values: Vec<f64> = entity
            .values
            .iter()
            .map(|s| numbers.eval(s))
            .collect::<Result<_>>()?;
        let needed = match entity.kind.as_str() {
            "circle" => 3,
            "arc" => 7,
            _ => 4,
        };
        if values.len() != needed {
            return Err("Invalid sketch entity parameter count".into());
        }
        if entity.kind == "rectangle" && (values[2] <= 1e-7 || values[3] <= 1e-7) {
            return Err("Rectangle width and height must be positive".into());
        }
        if entity.kind == "circle" && values[2] <= 1e-7 {
            return Err("Circle radius must be positive".into());
        }
        if entity.kind == "arc" {
            arc_geometry(&values)?;
        }
        if !["rectangle", "circle", "line", "arc"].contains(&entity.kind.as_str()) {
            return Err(format!("Unsupported sketch entity '{}'", entity.kind));
        }
        entities.push(ResolvedEntity {
            id: entity.id,
            kind: entity.kind.clone(),
            values,
            construction: entity.construction,
            fixed: false,
        });
    }
    let mut sys = GcsSystem::new();
    let origin = sys.add_point(PointData {
        x: 0.,
        y: 0.,
        fixed: true,
    });
    let mut points: BTreeMap<u64, Vec<brepkit_sketch::PointId>> =
        BTreeMap::from([(0, vec![origin])]);
    let mut lines: BTreeMap<u64, Vec<brepkit_sketch::LineId>> = BTreeMap::new();
    let mut circles = BTreeMap::new();
    let mut arcs = BTreeMap::new();
    for entity in &entities {
        let fixed = sketch
            .constraints
            .iter()
            .any(|c| c.kind == "Fix" && c.a == entity.id);
        let v = &entity.values;
        let coords = match entity.kind.as_str() {
            "rectangle" => vec![
                [v[0], v[1]],
                [v[0] + v[2], v[1]],
                [v[0] + v[2], v[1] + v[3]],
                [v[0], v[1] + v[3]],
            ],
            "circle" => vec![[v[0], v[1]]],
            "arc" => vec![[v[0], v[1]], [v[2], v[3]], [v[4], v[5]]],
            _ => vec![[v[0], v[1]], [v[2], v[3]]],
        };
        let ps: Vec<_> = coords
            .into_iter()
            .map(|p| {
                sys.add_point(PointData {
                    x: p[0],
                    y: p[1],
                    fixed,
                })
            })
            .collect();
        if entity.kind == "arc" {
            let arc = sys
                .add_arc(ps[0], ps[1], ps[2])
                .map_err(|e| e.to_string())?;
            arcs.insert(entity.id, arc);
        } else if entity.kind == "circle" {
            let c = sys.add_circle(ps[0], v[2]).map_err(|e| e.to_string())?;
            circles.insert(entity.id, c);
            if fixed {
                constrain(&mut sys, Constraint::Radius(c, v[2]))?;
            }
        } else {
            let count = if entity.kind == "rectangle" { 4 } else { 1 };
            let mut ls = vec![];
            for n in 0..count {
                let line = sys
                    .add_line(ps[n], ps[(n + 1) % ps.len()])
                    .map_err(|e| e.to_string())?;
                if entity.kind == "rectangle" && !fixed {
                    constrain(
                        &mut sys,
                        if n % 2 == 0 {
                            Constraint::Horizontal(line)
                        } else {
                            Constraint::Vertical(line)
                        },
                    )?;
                }
                ls.push(line);
            }
            lines.insert(entity.id, ls);
        }
        points.insert(entity.id, ps);
    }
    let point = |entity, index| {
        points
            .get(&entity)
            .and_then(|p| p.get(index))
            .copied()
            .ok_or_else(|| format!("Point reference {entity}:{index} is missing"))
    };
    let line = |entity, index| {
        lines
            .get(&entity)
            .and_then(|ls| ls.get(if ls.len() == 1 { 0 } else { index }))
            .copied()
            .ok_or_else(|| format!("Line reference {entity}:{index} is missing"))
    };
    let circle = |entity| {
        circles
            .get(&entity)
            .copied()
            .ok_or_else(|| format!("Circle {entity} is missing"))
    };
    let mut coincidences = BTreeSet::new();
    // Retain legacy inferred line joins; new joins are stored explicitly.
    let line_entities: Vec<_> = entities.iter().filter(|e| e.kind == "line").collect();
    for (i, a) in line_entities.iter().enumerate() {
        for b in &line_entities[i + 1..] {
            for &pa in &points[&a.id] {
                for &pb in &points[&b.id] {
                    let p = sys.point(pa).unwrap();
                    let q = sys.point(pb).unwrap();
                    if (p.x - q.x).hypot(p.y - q.y) < 1e-6 {
                        coincidences
                            .insert((pa.index().min(pb.index()), pa.index().max(pb.index())));
                        constrain(&mut sys, Constraint::Coincident(pa, pb))?;
                    }
                }
            }
        }
    }
    let constraints = constraints_with_expressions(sketch);
    for (index, c) in constraints.iter().enumerate() {
        if c.reference {
            continue;
        }
        let dim = || numbers.eval(&c.value);
        let pa = || point(c.a, c.points[0]);
        let pb = || point(c.b, c.points[1]);
        let la = || line(c.a, c.points[0]);
        let lb = || line(c.b, c.points[1]);
        let positive = || -> Result<f64> {
            let d = dim()?;
            if d <= 1e-7 {
                Err("Dimension must be positive".into())
            } else {
                Ok(d)
            }
        };
        let relation = match c.kind.as_str() {
            "Fix" => {
                if !points.contains_key(&c.a) {
                    return Err("Fix references a missing entity".into());
                }
                continue;
            }
            "Coincident" => {
                let (a, b) = if let Some((a, b)) = c.value.split_once(':') {
                    (
                        point(c.a, a.parse().map_err(|_| "Invalid endpoint")?)?,
                        point(c.b, b.parse().map_err(|_| "Invalid endpoint")?)?,
                    )
                } else {
                    (pa()?, pb()?)
                };
                if !coincidences.insert((a.index().min(b.index()), a.index().max(b.index()))) {
                    continue;
                }
                Constraint::Coincident(a, b)
            }
            "Origin" => {
                constrain(&mut sys, Constraint::FixX(pa()?, 0.))?;
                Constraint::FixY(pa()?, 0.)
            }
            "X" => Constraint::FixX(pa()?, dim()?),
            "Y" => Constraint::FixY(pa()?, dim()?),
            "StartX" => Constraint::FixX(point(c.a, 1)?, dim()?),
            "StartY" => Constraint::FixY(point(c.a, 1)?, dim()?),
            "EndX" => Constraint::FixX(
                point(c.a, if arcs.contains_key(&c.a) { 2 } else { 1 })?,
                dim()?,
            ),
            "EndY" => Constraint::FixY(
                point(c.a, if arcs.contains_key(&c.a) { 2 } else { 1 })?,
                dim()?,
            ),
            "Width" => Constraint::DistanceX(point(c.a, 0)?, point(c.a, 1)?, positive()?),
            "Height" => Constraint::DistanceY(point(c.a, 0)?, point(c.a, 3)?, positive()?),
            "Radius" | "Diameter" => {
                let radius = positive()? / if c.kind == "Diameter" { 2. } else { 1. };
                if arcs.contains_key(&c.a) {
                    Constraint::Distance(point(c.a, 0)?, point(c.a, 1)?, radius)
                } else {
                    Constraint::Radius(circle(c.a)?, radius)
                }
            }
            "Length" => {
                let id = la()?;
                let l = sys.line(id).unwrap();
                Constraint::Distance(l.p1, l.p2, positive()?)
            }
            "Distance" => Constraint::Distance(pa()?, pb()?, positive()?),
            "DistanceX" => Constraint::DistanceX(pa()?, pb()?, dim()?),
            "DistanceY" => Constraint::DistanceY(pa()?, pb()?, dim()?),
            "Horizontal" => Constraint::Horizontal(la()?),
            "Vertical" => Constraint::Vertical(la()?),
            "HorizontalPoints" => Constraint::DistanceY(pa()?, pb()?, 0.),
            "VerticalPoints" => Constraint::DistanceX(pa()?, pb()?, 0.),
            "Parallel" => Constraint::Parallel(la()?, lb()?),
            "Perpendicular" => Constraint::Perpendicular(la()?, lb()?),
            "Angle" => Constraint::Angle(la()?, lb()?, dim()?.to_radians()),
            "Equal" => {
                if let (Some(&a), Some(&b)) = (arcs.get(&c.a), arcs.get(&c.b)) {
                    Constraint::EqualRadiusArcArc(a, b)
                } else if let (Some(&a), Some(&b)) = (arcs.get(&c.a), circles.get(&c.b)) {
                    Constraint::EqualRadiusArcCircle(a, b)
                } else if let (Some(&a), Some(&b)) = (arcs.get(&c.b), circles.get(&c.a)) {
                    Constraint::EqualRadiusArcCircle(a, b)
                } else if circles.contains_key(&c.a) && circles.contains_key(&c.b) {
                    Constraint::EqualRadius(circle(c.a)?, circle(c.b)?)
                } else {
                    Constraint::EqualLength(la()?, lb()?)
                }
            }
            "Concentric" => Constraint::Coincident(point(c.a, 0)?, point(c.b, 0)?),
            "PointOnLine" => Constraint::PointLineDistance(pa()?, lb()?, 0.),
            "PointOnCircle" => {
                if let Some(&arc) = arcs.get(&c.b) {
                    Constraint::PointOnArc(pa()?, arc)
                } else {
                    Constraint::PointOnCircle(pa()?, circle(c.b)?)
                }
            }
            "LineDistance" => {
                constrain(&mut sys, Constraint::Parallel(la()?, lb()?))?;
                let l = sys.line(lb()?).unwrap();
                Constraint::PointLineDistance(
                    l.p1,
                    la()?,
                    dim()? * if c.negative { -1. } else { 1. },
                )
            }
            "PointLineDistance" => Constraint::PointLineDistance(
                pa()?,
                lb()?,
                dim()? * if c.negative { -1. } else { 1. },
            ),
            "Tangent" if arcs.contains_key(&c.a) || arcs.contains_key(&c.b) => {
                let (arc_id, other) = if arcs.contains_key(&c.a) {
                    (c.a, c.b)
                } else {
                    (c.b, c.a)
                };
                let edge = c
                    .value
                    .strip_prefix("edge:")
                    .map(|v| v.parse::<usize>().map_err(|_| "Invalid tangent edge"))
                    .transpose()?
                    .unwrap_or(0);
                let other_indices = if arcs.contains_key(&other) {
                    vec![1, 2]
                } else if points[&other].len() == 4 {
                    vec![edge % 4, (edge + 1) % 4]
                } else {
                    vec![0, 1]
                };
                let (arc_endpoint, other_endpoint) = if arc_id == c.a {
                    (c.points[0], c.points[1])
                } else {
                    (c.points[1], c.points[0])
                };
                let mut shared =
                    if [1, 2].contains(&arc_endpoint) && other_indices.contains(&other_endpoint) {
                        Some((point(arc_id, arc_endpoint)?, point(other, other_endpoint)?))
                    } else {
                        None
                    };
                for i in [1, 2] {
                    for &j in &other_indices {
                        let a = sys.point(point(arc_id, i)?).unwrap();
                        let b = sys.point(point(other, j)?).unwrap();
                        if shared.is_none() && (a.x - b.x).hypot(a.y - b.y) < 1e-5 {
                            shared = Some((point(arc_id, i)?, point(other, j)?));
                        }
                    }
                }
                let (a, b) = shared.ok_or("Arc tangency requires a shared endpoint")?;
                constrain(&mut sys, Constraint::Coincident(a, b))?;
                if let Some(&other_arc) = arcs.get(&other) {
                    Constraint::TangentArcArc(arcs[&arc_id], other_arc, a)
                } else {
                    Constraint::TangentLineArc(line(other, edge)?, arcs[&arc_id], a)
                }
            }
            "Tangent" => {
                let (l, circle_id) = if circles.contains_key(&c.a) {
                    (lb()?, circle(c.a)?)
                } else {
                    (la()?, circle(c.b)?)
                };
                Constraint::TangentLineCircle(l, circle_id, if c.negative { -1. } else { 1. })
            }
            _ => return Err(format!("Unknown constraint {}", c.kind)),
        };
        constrain(&mut sys, relation)
            .map_err(|e| format!("Constraint {} ({}): {e}", index + 1, c.kind))?;
    }
    let requests = targets
        .iter()
        .map(|c| {
            let value = numbers.eval(&c.value)?;
            match c.kind.as_str() {
                "X" => Ok(Constraint::FixX(point(c.a, c.points[0])?, value)),
                "Y" => Ok(Constraint::FixY(point(c.a, c.points[0])?, value)),
                "CurvePoint" => {
                    let p = c.position.ok_or("A curve drag needs a pointer position")?;
                    let mouse = sys.add_point(PointData {
                        x: p[0],
                        y: p[1],
                        fixed: true,
                    });
                    if let Some(&arc) = arcs.get(&c.a) {
                        Ok(Constraint::PointOnArc(mouse, arc))
                    } else {
                        Ok(Constraint::PointOnCircle(mouse, circle(c.a)?))
                    }
                }
                "Radius" => {
                    if arcs.contains_key(&c.a) {
                        Ok(Constraint::Distance(point(c.a, 0)?, point(c.a, 1)?, value))
                    } else {
                        Ok(Constraint::Radius(circle(c.a)?, value))
                    }
                }
                _ => Err("Unsupported temporary sketch target".into()),
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let outcome = sys
        .solve_with_targets(&requests, 100, 1e-8)
        .map_err(|e| e.to_string())?;
    if !outcome.converged {
        return Err(format!(
            "Sketch constraints conflict or failed to converge (residual {:.3e}). Edit or remove a dimension/constraint.",
            outcome.max_residual
        ));
    }
    for e in &mut entities {
        let ps = &points[&e.id];
        let a = sys.point(ps[0]).unwrap();
        e.values = match e.kind.as_str() {
            "circle" => vec![a.x, a.y, sys.circle(circles[&e.id]).unwrap().radius],
            "arc" => {
                let start = sys.point(ps[1]).unwrap();
                let end = sys.point(ps[2]).unwrap();
                vec![a.x, a.y, start.x, start.y, end.x, end.y, e.values[6]]
            }
            "rectangle" => {
                let b = sys.point(ps[1]).unwrap();
                let d = sys.point(ps[3]).unwrap();
                vec![a.x, a.y, b.x - a.x, d.y - a.y]
            }
            _ => {
                let b = sys.point(ps[1]).unwrap();
                vec![a.x, a.y, b.x, b.y]
            }
        };
        if e.kind == "arc" {
            arc_geometry(&e.values)?;
        }
        if e.values.iter().any(|v| !v.is_finite())
            || e.kind == "circle" && e.values[2] <= 1e-7
            || e.kind == "rectangle" && (e.values[2] <= 1e-7 || e.values[3] <= 1e-7)
        {
            return Err("Constraints produced degenerate or inverted geometry".into());
        }
    }
    let (analysis, moving_points, moving_circles) = sys.mobility();
    for e in &mut entities {
        e.fixed = points[&e.id].iter().all(|p| !moving_points.contains(p))
            && circles
                .get(&e.id)
                .is_none_or(|c| !moving_circles.contains(c));
    }
    Ok(ResolvedSketch {
        entities,
        plane,
        dof: analysis.dof,
    })
}
fn constrain(sys: &mut GcsSystem, c: Constraint) -> Result<()> {
    sys.add_constraint(c).map(|_| ()).map_err(|e| e.to_string())
}

#[derive(Clone, Debug)]
pub enum RegionLoop {
    Polygon {
        entity: u64,
        points: Vec<[f64; 2]>,
    },
    Circle {
        entity: u64,
        center: [f64; 2],
        radius: f64,
    },
}
impl RegionLoop {
    pub fn area(&self) -> f64 {
        match self {
            Self::Circle { radius, .. } => std::f64::consts::PI * radius * radius,
            Self::Polygon { points, .. } => signed_area(points).abs(),
        }
    }
    pub fn sample(&self) -> Vec<[f64; 2]> {
        match self {
            Self::Polygon { points, .. } => points.clone(),
            Self::Circle { center, radius, .. } => (0..96)
                .map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / 96.;
                    [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
                })
                .collect(),
        }
    }
    pub fn contains(&self, p: [f64; 2]) -> bool {
        match self {
            Self::Circle { center, radius, .. } => {
                (p[0] - center[0]).hypot(p[1] - center[1]) < *radius - 1e-7
            }
            Self::Polygon { points, .. } => inside_polygon(points, p),
        }
    }
    pub fn entity(&self) -> u64 {
        match self {
            Self::Polygon { entity, .. } | Self::Circle { entity, .. } => *entity,
        }
    }
    fn wire(
        &self,
        topo: &mut Topology,
        plane: &Plane,
        hole: bool,
    ) -> Result<brepkit_topology::wire::WireId> {
        match self {
            Self::Polygon { points, .. } => {
                let mut pts = points.clone();
                if (signed_area(&pts) > 0.) == hole {
                    pts.reverse();
                }
                let ps: Vec<_> = pts
                    .iter()
                    .map(|p| {
                        let q = plane.at(*p);
                        Point3::new(q[0], q[1], q[2])
                    })
                    .collect();
                make_polygon_wire(topo, &ps, 1e-8).map_err(|e| e.to_string())
            }
            Self::Circle { center, radius, .. } => {
                let c = plane.at(*center);
                let e = make_circle_edge_with_ref(
                    topo,
                    Point3::new(c[0], c[1], c[2]),
                    v3(plane.normal),
                    *radius,
                    v3(plane.u),
                    1e-8,
                )
                .map_err(|e| e.to_string())?;
                let wire = Wire::new(vec![OrientedEdge::new(e, !hole)], true)
                    .map_err(|e| e.to_string())?;
                Ok(topo.add_wire(wire))
            }
        }
    }
}
pub fn v3(p: [f64; 3]) -> Vec3 {
    Vec3::new(p[0], p[1], p[2])
}
pub fn p3(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}
pub fn signed_area(p: &[[f64; 2]]) -> f64 {
    (0..p.len())
        .map(|i| {
            let j = (i + 1) % p.len();
            p[i][0] * p[j][1] - p[j][0] * p[i][1]
        })
        .sum::<f64>()
        / 2.
}
pub fn inside_polygon(ps: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    for i in 0..ps.len() {
        let a = ps[i];
        let b = ps[(i + 1) % ps.len()];
        if ((a[1] > p[1]) != (b[1] > p[1]))
            && (p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0])
        {
            inside = !inside;
        }
    }
    inside
}
fn close(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}
fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn crossing(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let on = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        orient(p, q, r).abs() < 1e-8
            && r[0] >= p[0].min(q[0]) - 1e-7
            && r[0] <= p[0].max(q[0]) + 1e-7
            && r[1] >= p[1].min(q[1]) - 1e-7
            && r[1] <= p[1].max(q[1]) + 1e-7
    };
    (orient(a, b, c) * orient(a, b, d) < -1e-12 && orient(c, d, a) * orient(c, d, b) < -1e-12)
        || on(a, b, c)
        || on(a, b, d)
        || on(c, d, a)
        || on(c, d, b)
}
fn boundaries_touch(a: &RegionLoop, b: &RegionLoop) -> bool {
    match (a, b) {
        (
            RegionLoop::Circle {
                center: a,
                radius: r,
                ..
            },
            RegionLoop::Circle {
                center: b,
                radius: s,
                ..
            },
        ) => {
            let distance = (a[0] - b[0]).hypot(a[1] - b[1]);
            distance >= (r - s).abs() - 1e-7 && distance <= r + s + 1e-7
        }
        (RegionLoop::Polygon { points, .. }, RegionLoop::Circle { center, radius, .. })
        | (RegionLoop::Circle { center, radius, .. }, RegionLoop::Polygon { points, .. }) => {
            points.iter().enumerate().any(|(i, &a)| {
                let b = points[(i + 1) % points.len()];
                let v = [b[0] - a[0], b[1] - a[1]];
                let t = (((center[0] - a[0]) * v[0] + (center[1] - a[1]) * v[1])
                    / (v[0] * v[0] + v[1] * v[1]))
                    .clamp(0., 1.);
                let closest = (a[0] + t * v[0] - center[0]).hypot(a[1] + t * v[1] - center[1]);
                let far = (a[0] - center[0])
                    .hypot(a[1] - center[1])
                    .max((b[0] - center[0]).hypot(b[1] - center[1]));
                closest <= radius + 1e-7 && far >= radius - 1e-7
            })
        }
        (RegionLoop::Polygon { points: a, .. }, RegionLoop::Polygon { points: b, .. }) => {
            a.iter().enumerate().any(|(i, &p)| {
                b.iter()
                    .enumerate()
                    .any(|(j, &q)| crossing(p, a[(i + 1) % a.len()], q, b[(j + 1) % b.len()]))
            })
        }
    }
}
pub fn loops(sketch: &ResolvedSketch) -> Result<Vec<RegionLoop>> {
    let mut loops = vec![];
    let mut line_entities = vec![];
    for e in &sketch.entities {
        if !e.construction {
            let v = &e.values;
            match e.kind.as_str() {
                "rectangle" => {
                    let mut ps = e.points();
                    ps.pop();
                    loops.push(RegionLoop::Polygon {
                        entity: e.id,
                        points: ps,
                    });
                }
                "circle" => loops.push(RegionLoop::Circle {
                    entity: e.id,
                    center: [v[0], v[1]],
                    radius: v[2],
                }),
                "line" => line_entities.push(e),
                _ => (),
            }
        }
    }
    let mut used = BTreeSet::new();
    for e in &line_entities {
        if used.contains(&e.id) {
            continue;
        }
        used.insert(e.id);
        let mut points = vec![[e.values[0], e.values[1]], [e.values[2], e.values[3]]];
        while !close(points[0], *points.last().unwrap()) {
            let end = *points.last().unwrap();
            let candidates: Vec<_> = line_entities
                .iter()
                .filter(|x| {
                    !used.contains(&x.id)
                        && (close(end, [x.values[0], x.values[1]])
                            || close(end, [x.values[2], x.values[3]]))
                })
                .collect();
            if candidates.len() != 1 {
                return Err("Profile has open or branching lines. Close each loop or mark unused lines as construction.".into());
            }
            let next = candidates[0];
            used.insert(next.id);
            let a = [next.values[0], next.values[1]];
            let b = [next.values[2], next.values[3]];
            points.push(if close(end, a) { b } else { a });
            if points.len() > 1001 {
                return Err("Profile loop exceeds the size limit".into());
            }
        }
        points.pop();
        if points.len() < 3 || signed_area(&points).abs() < 1e-8 {
            return Err("Degenerate sketch region".into());
        }
        loops.push(RegionLoop::Polygon {
            entity: e.id,
            points,
        });
    }
    if loops.is_empty() {
        return Err("Sketch has no closed regions".into());
    }
    // Reject self-crossing and overlapping region boundaries before building topology.
    for (i, a) in loops.iter().enumerate() {
        let pa = a.sample();
        for j in 0..pa.len() {
            for k in j + 2..pa.len() {
                if j == 0 && k + 1 == pa.len() {
                    continue;
                }
                if crossing(pa[j], pa[(j + 1) % pa.len()], pa[k], pa[(k + 1) % pa.len()]) {
                    return Err("Sketch profile crosses itself".into());
                }
            }
        }
        for b in &loops[i + 1..] {
            if boundaries_touch(a, b) {
                return Err(
                    "Sketch region boundaries overlap or touch. Trim them before making a solid."
                        .into(),
                );
            }
        }
    }
    Ok(loops)
}

pub fn faces(topo: &mut Topology, sketch: &ResolvedSketch) -> Result<Vec<FaceId>> {
    if sketch
        .entities
        .iter()
        .any(|e| e.kind == "arc" && !e.construction)
    {
        return faces_for_regions(topo, sketch, &[]);
    }
    let loops = loops(sketch)?;
    let parents: Vec<Option<usize>> = loops
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let p = l.sample()[0];
            loops
                .iter()
                .enumerate()
                .filter(|(j, x)| *j != i && x.area() > l.area() + 1e-7 && x.contains(p))
                .min_by(|(_, a), (_, b)| a.area().total_cmp(&b.area()))
                .map(|(j, _)| j)
        })
        .collect();
    let mut result = vec![];
    for (i, l) in loops.iter().enumerate() {
        let mut depth = 0;
        let mut parent = parents[i];
        while let Some(j) = parent {
            depth += 1;
            parent = parents[j];
        }
        if depth % 2 != 0 {
            continue;
        }
        let outer = l.wire(topo, &sketch.plane, false)?;
        let inner = loops
            .iter()
            .enumerate()
            .filter(|(j, _)| parents[*j] == Some(i))
            .map(|(_, hole)| hole.wire(topo, &sketch.plane, true))
            .collect::<Result<Vec<_>>>()?;
        let n = sketch.plane.normal;
        let d = (0..3).map(|k| n[k] * sketch.plane.origin[k]).sum();
        result.push(topo.add_face(Face::new(
            outer,
            inner,
            FaceSurface::Plane { normal: v3(n), d },
        )));
    }
    Ok(result)
}

pub fn entity_from_values(id: u64, kind: &str, values: &[f64]) -> Entity {
    Entity {
        id,
        kind: kind.into(),
        values: values.iter().map(|v| crate::model::fmt(*v)).collect(),
        construction: false,
    }
}
