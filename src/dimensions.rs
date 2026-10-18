//! Geometry selection and smart dimension inference, independent of the UI.
use crate::{
    model::{Result, SketchConstraint, fmt},
    sketch::ResolvedSketch,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sub {
    Point(usize),
    Edge(usize),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub entity: u64,
    pub sub: Sub,
}
impl Selection {
    pub fn point(entity: u64, index: usize) -> Self {
        Self {
            entity,
            sub: Sub::Point(index),
        }
    }
    pub fn edge(entity: u64, index: usize) -> Self {
        Self {
            entity,
            sub: Sub::Edge(index),
        }
    }
    pub fn index(self) -> usize {
        match self.sub {
            Sub::Point(n) | Sub::Edge(n) => n,
        }
    }
}
pub fn point(sk: &ResolvedSketch, id: u64, index: usize) -> Result<[f64; 2]> {
    if id == 0 {
        return Ok([0., 0.]);
    }
    let e = sk
        .entities
        .iter()
        .find(|e| e.id == id)
        .ok_or("Selected entity is missing")?;
    e.control_points()
        .get(index)
        .copied()
        .ok_or("Selected point is missing".into())
}
pub fn line(sk: &ResolvedSketch, id: u64, index: usize) -> Result<[[f64; 2]; 2]> {
    let e = sk
        .entities
        .iter()
        .find(|e| e.id == id)
        .ok_or("Selected entity is missing")?;
    match e.kind.as_str() {
        "line" => Ok([point(sk, id, 0)?, point(sk, id, 1)?]),
        "rectangle" => Ok([point(sk, id, index % 4)?, point(sk, id, (index + 1) % 4)?]),
        _ => Err("Select a line or rectangle edge".into()),
    }
}
pub fn is_dimension(kind: &str) -> bool {
    matches!(
        kind,
        "Length"
            | "Width"
            | "Height"
            | "Radius"
            | "Diameter"
            | "Distance"
            | "DistanceX"
            | "DistanceY"
            | "X"
            | "Y"
            | "StartX"
            | "StartY"
            | "EndX"
            | "EndY"
            | "Angle"
            | "LineDistance"
            | "PointLineDistance"
    )
}
pub fn signed_distance(p: [f64; 2], l: [[f64; 2]; 2]) -> f64 {
    let d = [l[1][0] - l[0][0], l[1][1] - l[0][1]];
    (d[0] * (p[1] - l[0][1]) - d[1] * (p[0] - l[0][0])) / d[0].hypot(d[1]).max(1e-12)
}
pub fn measure(sk: &ResolvedSketch, c: &SketchConstraint) -> Result<f64> {
    let p = || point(sk, c.a, c.points[0]);
    let q = || point(sk, c.b, c.points[1]);
    match c.kind.as_str() {
        "Radius" | "Diameter" => {
            let e = sk
                .entities
                .iter()
                .find(|e| e.id == c.a && matches!(e.kind.as_str(), "circle" | "arc"))
                .ok_or("Circle is missing")?;
            let radius = e.arc().map_or(e.values[2], |a| a.radius);
            Ok(radius * if c.kind == "Diameter" { 2. } else { 1. })
        }
        "X" => Ok(p()?[0]),
        "Y" => Ok(p()?[1]),
        "StartX" | "StartY" | "EndX" | "EndY" => {
            let index = if c.kind.starts_with("End")
                && sk.entities.iter().any(|e| e.id == c.a && e.kind == "arc")
            {
                2
            } else {
                1
            };
            Ok(point(sk, c.a, index)?[usize::from(c.kind.ends_with('Y'))])
        }
        "Width" | "Height" => {
            let e = sk
                .entities
                .iter()
                .find(|e| e.id == c.a)
                .ok_or("Rectangle is missing")?;
            Ok(e.values[if c.kind == "Width" { 2 } else { 3 }])
        }
        "Length" => {
            let l = line(sk, c.a, c.points[0])?;
            Ok((l[1][0] - l[0][0]).hypot(l[1][1] - l[0][1]))
        }
        "Distance" => Ok((q()?[0] - p()?[0]).hypot(q()?[1] - p()?[1])),
        "DistanceX" => Ok(q()?[0] - p()?[0]),
        "DistanceY" => Ok(q()?[1] - p()?[1]),
        "LineDistance" => {
            Ok(signed_distance(line(sk, c.b, c.points[1])?[0], line(sk, c.a, c.points[0])?).abs())
        }
        "PointLineDistance" => Ok(signed_distance(p()?, line(sk, c.b, c.points[1])?).abs()),
        "Angle" => {
            let a = line(sk, c.a, c.points[0])?;
            let b = line(sk, c.b, c.points[1])?;
            let u = [a[1][0] - a[0][0], a[1][1] - a[0][1]];
            let v = [b[1][0] - b[0][0], b[1][1] - b[0][1]];
            Ok((u[0] * v[1] - u[1] * v[0])
                .atan2(u[0] * v[0] + u[1] * v[1])
                .to_degrees())
        }
        _ => Err("This is a geometric relationship".into()),
    }
}
fn label_anchor(sk: &ResolvedSketch, c: &SketchConstraint) -> [f64; 2] {
    let a = point(sk, c.a, c.points[0]).unwrap_or([0., 0.]);
    let midpoint = |a: [f64; 2], b: [f64; 2]| [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.];
    match c.kind.as_str() {
        "Width" | "Height" | "Length" => line(sk, c.a, c.points[0])
            .map(|l| midpoint(l[0], l[1]))
            .unwrap_or(a),
        "Radius" | "Diameter" => point(sk, c.a, 0).unwrap_or(a),
        "X" | "Y" | "StartX" | "StartY" | "EndX" | "EndY" => midpoint(
            [0., 0.],
            point(
                sk,
                c.a,
                if c.kind.starts_with("End")
                    && sk.entities.iter().any(|e| e.id == c.a && e.kind == "arc")
                {
                    2
                } else if c.kind.starts_with("End") || c.kind.starts_with("Start") {
                    1
                } else {
                    c.points[0]
                },
            )
            .unwrap_or(a),
        ),
        "Distance" | "DistanceX" | "DistanceY" => {
            midpoint(a, point(sk, c.b, c.points[1]).unwrap_or(a))
        }
        _ => a,
    }
}
pub fn label_position(sk: &ResolvedSketch, c: &SketchConstraint) -> [f64; 2] {
    let anchor = label_anchor(sk, c);
    match c.position {
        Some(p) if c.relative_position => [anchor[0] + p[0], anchor[1] + p[1]],
        Some(p) => p,
        None => [anchor[0] + 12., anchor[1] + 12.],
    }
}
pub fn set_label_position(sk: &ResolvedSketch, c: &mut SketchConstraint, position: [f64; 2]) {
    let anchor = label_anchor(sk, c);
    c.position = Some([position[0] - anchor[0], position[1] - anchor[1]]);
    c.relative_position = true;
}

pub fn infer(
    sk: &ResolvedSketch,
    selected: &[Selection],
    position: [f64; 2],
) -> Result<SketchConstraint> {
    let a = *selected.first().ok_or("Select geometry to dimension")?;
    let b = selected.get(1).copied();
    let kind = |s: Selection| {
        sk.entities
            .iter()
            .find(|e| e.id == s.entity)
            .map(|e| e.kind.as_str())
            .unwrap_or("origin")
    };
    let mut c = SketchConstraint {
        a: a.entity,
        points: [a.index(), b.map_or(0, Selection::index)],
        b: b.map_or(0, |b| b.entity),
        position: Some(position),
        reference: sk.dof == 0,
        ..Default::default()
    };
    match (a.sub, b) {
        (Sub::Edge(_), None) => {
            c.kind = match kind(a) {
                "circle" => "Diameter",
                "arc" => "Radius",
                "rectangle" => {
                    if a.index() % 2 == 0 {
                        "Width"
                    } else {
                        "Height"
                    }
                }
                _ => "Length",
            }
            .into();
        }
        (Sub::Point(_), Some(b)) if matches!(b.sub, Sub::Point(_)) => {
            let p = point(sk, a.entity, a.index())?;
            let q = point(sk, b.entity, b.index())?;
            let middle = [(p[0] + q[0]) / 2., (p[1] + q[1]) / 2.];
            let dx = (position[0] - middle[0]).abs();
            let dy = (position[1] - middle[1]).abs();
            c.kind = if dy > dx * 1.5 {
                "DistanceX"
            } else if dx > dy * 1.5 {
                "DistanceY"
            } else {
                "Distance"
            }
            .into();
            if c.kind == "DistanceX" && q[0] < p[0] || c.kind == "DistanceY" && q[1] < p[1] {
                std::mem::swap(&mut c.a, &mut c.b);
                c.points.swap(0, 1);
            }
        }
        (Sub::Edge(_), Some(b)) if matches!(b.sub, Sub::Edge(_)) => {
            if matches!(kind(a), "circle" | "arc") && matches!(kind(b), "circle" | "arc") {
                c.kind = "Distance".into();
                c.points = [0, 0];
            } else if matches!(kind(a), "circle" | "arc") || matches!(kind(b), "circle" | "arc") {
                let (circle, edge) = if matches!(kind(a), "circle" | "arc") {
                    (a, b)
                } else {
                    (b, a)
                };
                c.kind = "PointLineDistance".into();
                c.a = circle.entity;
                c.b = edge.entity;
                c.points = [0, edge.index()];
                c.negative = signed_distance(point(sk, c.a, 0)?, line(sk, c.b, c.points[1])?) < 0.;
            } else {
                let x = line(sk, a.entity, a.index())?;
                let y = line(sk, b.entity, b.index())?;
                let u = [x[1][0] - x[0][0], x[1][1] - x[0][1]];
                let v = [y[1][0] - y[0][0], y[1][1] - y[0][1]];
                let cross = (u[0] * v[1] - u[1] * v[0]).abs()
                    / (u[0].hypot(u[1]) * v[0].hypot(v[1])).max(1e-12);
                c.kind = if cross < 1e-5 {
                    "LineDistance"
                } else {
                    "Angle"
                }
                .into();
                c.negative = signed_distance(y[0], x) < 0.;
            }
        }
        (_, Some(b)) => {
            let (p, e) = if matches!(a.sub, Sub::Point(_)) {
                (a, b)
            } else {
                (b, a)
            };
            c.a = p.entity;
            c.b = e.entity;
            c.points = [p.index(), e.index()];
            if matches!(kind(e), "circle" | "arc") {
                c.kind = "Distance".into();
                c.points[1] = 0;
            } else {
                c.kind = "PointLineDistance".into();
                c.negative =
                    signed_distance(point(sk, c.a, c.points[0])?, line(sk, c.b, c.points[1])?) < 0.;
            }
        }
        _ => return Err("Select another point, then click to place the dimension".into()),
    }
    c.value = fmt(measure(sk, &c)?);
    set_label_position(sk, &mut c, position);
    Ok(c)
}
