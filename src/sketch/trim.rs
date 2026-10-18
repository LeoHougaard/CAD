//! Exact trimming with endpoint-based constraint preservation.
use super::*;
use crate::{dimensions, model::SketchConstraint};

pub struct TrimEdit {
    pub sketch: Sketch,
    pub resolved: ResolvedSketch,
    pub removed_constraints: usize,
}

/// Build and validate an entire edit before replacing any caller-owned data.
pub fn apply_trim(
    original: &Sketch,
    resolved: &ResolvedSketch,
    entity: u64,
    edge: usize,
    point: [f64; 2],
    numbers: &Numbers,
) -> Result<TrimEdit> {
    let pieces = super::trim_entity(resolved, entity, edge, point)?;
    let index = original
        .entities
        .iter()
        .position(|e| e.id == entity)
        .ok_or("Trim entity is missing")?;
    let old = resolved
        .entities
        .iter()
        .find(|e| e.id == entity)
        .ok_or("Trim geometry is missing")?;
    let mut sketch = original.clone();
    sketch.next_id = sketch
        .next_id
        .max(original.entities.iter().map(|e| e.id).max().unwrap_or(0));
    let mut replacements = Vec::new();
    for (n, piece) in pieces.iter().enumerate() {
        let id = if n == 0 { entity } else { sketch.allocate_id() };
        let mut replacement = entity_from_values(id, &piece.kind, &piece.values);
        replacement.construction = original.entities[index].construction;
        replacements.push(replacement);
    }
    sketch
        .entities
        .splice(index..=index, replacements.iter().cloned());
    let children = pieces
        .iter()
        .zip(&replacements)
        .map(|(piece, e)| {
            (
                piece.source_edge,
                ResolvedEntity {
                    id: e.id,
                    kind: e.kind.clone(),
                    values: piece.values.clone(),
                    construction: e.construction,
                    fixed: false,
                },
            )
        })
        .collect::<Vec<_>>();
    let point_map = |index: usize| -> Option<(u64, usize)> {
        let p = dimensions::point(resolved, entity, index).ok()?;
        children.iter().find_map(|(_, child)| {
            control_points(child)
                .into_iter()
                .enumerate()
                .find(|(_, q)| near(p, *q))
                .map(|(index, _)| (child.id, index))
        })
    };
    let curve_map = |index: usize| -> Option<u64> {
        children
            .iter()
            .find(|(source, _)| *source == index || old.kind != "rectangle")
            .map(|(_, e)| e.id)
    };
    let whole_curve = |index: usize| -> Option<u64> {
        let line = dimensions::line(resolved, entity, index).ok()?;
        children
            .iter()
            .find(|(source, e)| {
                *source == index
                    && e.kind == "line"
                    && near(line[0], [e.values[0], e.values[1]])
                    && near(line[1], [e.values[2], e.values[3]])
            })
            .map(|(_, e)| e.id)
    };
    let mut retained = Vec::new();
    let mut removed = 0;
    for (constraint_index, mut c) in super::constraints_with_expressions(original)
        .into_iter()
        .enumerate()
    {
        if constraint_index >= original.constraints.len() && c.a != entity {
            continue;
        }
        if c.a != entity && c.b != entity {
            retained.push(c);
            continue;
        }
        if children.is_empty() {
            removed += 1;
            continue;
        }
        let mut keep = true;
        let mut point_ref = |id: &mut u64, index: &mut usize| {
            if *id == entity {
                if let Some((replacement, point)) = point_map(*index) {
                    *id = replacement;
                    *index = point;
                } else {
                    keep = false;
                }
            }
        };
        match c.kind.as_str() {
            "Fix" => {
                retained.extend(children.iter().map(|(_, e)| SketchConstraint {
                    a: e.id,
                    ..c.clone()
                }));
                continue;
            }
            "Origin" | "X" | "Y" => point_ref(&mut c.a, &mut c.points[0]),
            "StartX" | "StartY" | "EndX" | "EndY" => {
                c.points[0] = if c.kind.starts_with("End") && old.kind == "arc" {
                    2
                } else {
                    1
                };
                point_ref(&mut c.a, &mut c.points[0]);
                c.kind = if c.kind.ends_with('X') { "X" } else { "Y" }.into();
            }
            "Coincident" | "Distance" | "DistanceX" | "DistanceY" | "HorizontalPoints"
            | "VerticalPoints" => {
                if c.kind == "Coincident"
                    && let Some((a, b)) = c.value.split_once(':')
                {
                    c.points = [
                        a.parse().map_err(|_| "Invalid endpoint")?,
                        b.parse().map_err(|_| "Invalid endpoint")?,
                    ];
                    c.value.clear();
                }
                point_ref(&mut c.a, &mut c.points[0]);
                point_ref(&mut c.b, &mut c.points[1]);
            }
            "Radius" | "Diameter" | "Concentric" => {
                if children[0].1.arc().is_none() && children[0].1.kind != "circle" {
                    keep = false;
                }
                if c.a == entity {
                    c.a = children[0].1.id;
                }
                if c.b == entity {
                    c.b = children[0].1.id;
                }
            }
            "Length" | "Width" | "Height" => {
                let index = match c.kind.as_str() {
                    "Width" => 0,
                    "Height" => 3,
                    _ => c.points[0],
                };
                if let Some(id) = whole_curve(index) {
                    c.a = id;
                    c.points[0] = 0;
                    c.kind = "Length".into();
                } else {
                    keep = false;
                }
            }
            "Horizontal" | "Vertical" | "Parallel" | "Perpendicular" | "Angle" | "LineDistance"
            | "Equal" => {
                let (first, second) = c.points.split_at_mut(1);
                for (id, index) in [(&mut c.a, &mut first[0]), (&mut c.b, &mut second[0])] {
                    if *id != entity {
                        continue;
                    }
                    let mapped =
                        if c.kind == "Equal" && matches!(old.kind.as_str(), "line" | "rectangle") {
                            whole_curve(*index)
                        } else {
                            curve_map(*index)
                        };
                    if let Some(child) = mapped {
                        *id = child;
                        *index = 0;
                    } else {
                        keep = false;
                    }
                }
            }
            "PointOnLine" | "PointOnCircle" | "PointLineDistance" => {
                let source = dimensions::point(resolved, c.a, c.points[0]).ok();
                point_ref(&mut c.a, &mut c.points[0]);
                if c.b == entity {
                    let child = children.iter().find(|(_, child)| {
                        c.kind == "PointLineDistance" || source.is_some_and(|p| on_curve(child, p))
                    });
                    if let Some((_, child)) = child {
                        c.b = child.id;
                        c.points[1] = 0;
                    } else {
                        keep = false;
                    }
                }
            }
            "Tangent" => {
                if old.kind == "circle" {
                    keep = false;
                } else if old.kind == "arc" {
                    point_ref(&mut c.a, &mut c.points[0]);
                    point_ref(&mut c.b, &mut c.points[1]);
                } else if resolved
                    .entities
                    .iter()
                    .any(|e| [c.a, c.b].contains(&e.id) && e.kind == "arc")
                {
                    let side = usize::from(c.b == entity);
                    let endpoint = dimensions::point(resolved, entity, c.points[side])?;
                    let edge = c
                        .value
                        .strip_prefix("edge:")
                        .map(|v| v.parse::<usize>().map_err(|_| "Invalid tangent edge"))
                        .transpose()?
                        .unwrap_or(0);
                    let replacement = children
                        .iter()
                        .filter(|(source, _)| old.kind != "rectangle" || *source == edge)
                        .find_map(|(_, child)| {
                            control_points(child)
                                .iter()
                                .position(|&p| near(p, endpoint))
                                .map(|index| (child.id, index))
                        });
                    if let Some((id, index)) = replacement {
                        if side == 0 {
                            c.a = id;
                        } else {
                            c.b = id;
                        }
                        c.points[side] = index;
                        c.value.clear();
                    } else {
                        keep = false;
                    }
                } else {
                    if c.a == entity {
                        if let Some(id) = curve_map(c.points[0]) {
                            c.a = id;
                            c.points[0] = 0;
                        } else {
                            keep = false;
                        }
                    }
                    if c.b == entity {
                        if let Some(id) = curve_map(c.points[1]) {
                            c.b = id;
                            c.points[1] = 0;
                        } else {
                            keep = false;
                        }
                    }
                }
            }
            _ => {
                return Err(format!(
                    "Remove the {} constraint before trimming this entity",
                    c.kind
                ));
            }
        }
        if keep {
            retained.push(c);
        } else {
            removed += 1;
        }
    }
    // Preserve each original curve's geometry and joins, including the rectangle's
    // formerly implicit horizontal/vertical edges. New trim ends follow bounds.
    for (i, (source, a)) in children.iter().enumerate() {
        if old.kind == "rectangle" {
            retained.push(SketchConstraint {
                kind: if source % 2 == 0 {
                    "Horizontal"
                } else {
                    "Vertical"
                }
                .into(),
                a: a.id,
                ..Default::default()
            });
        }
        for (index, p) in control_points(a).into_iter().enumerate() {
            if a.kind == "arc" && index == 0 || control_points(old).iter().any(|&q| near(p, q)) {
                continue;
            }
            for bound in resolved.entities.iter().filter(|e| e.id != entity) {
                if !on_curve(bound, p) {
                    continue;
                }
                let mut constraint = SketchConstraint {
                    a: a.id,
                    b: bound.id,
                    points: [index, 0],
                    ..Default::default()
                };
                if let Some(endpoint) = control_points(bound).iter().position(|&q| near(p, q)) {
                    constraint.kind = "Coincident".into();
                    constraint.points[1] = endpoint;
                } else if matches!(bound.kind.as_str(), "circle" | "arc") {
                    constraint.kind = "PointOnCircle".into();
                } else {
                    constraint.kind = "PointOnLine".into();
                    if bound.kind == "rectangle" {
                        let points = bound.points();
                        constraint.points[1] = points
                            .windows(2)
                            .position(|pair| on_segment(pair[0], pair[1], p))
                            .unwrap();
                    }
                }
                retained.push(constraint);
            }
        }
        for (other, b) in &children[i + 1..] {
            if source == other {
                for kind in if a.kind == "line" {
                    vec!["Parallel", "PointOnLine"]
                } else {
                    vec!["Concentric", "Equal"]
                } {
                    retained.push(SketchConstraint {
                        kind: kind.into(),
                        a: b.id,
                        b: a.id,
                        ..Default::default()
                    });
                }
            }
            for (ia, p) in control_points(a).into_iter().enumerate() {
                for (ib, q) in control_points(b).into_iter().enumerate() {
                    if near(p, q) && !(a.kind == "arc" && b.kind == "arc" && ia == 0 && ib == 0) {
                        retained.push(SketchConstraint {
                            kind: "Coincident".into(),
                            a: a.id,
                            b: b.id,
                            points: [ia, ib],
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }
    sketch.constraints = retained;
    let resolved = resolve(&sketch, resolved.plane.clone(), numbers)?;
    seed_from_resolved(&mut sketch, Some(&resolved));
    Ok(TrimEdit {
        sketch,
        resolved,
        removed_constraints: removed,
    })
}
fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}
fn control_points(e: &ResolvedEntity) -> Vec<[f64; 2]> {
    e.control_points()
}
fn on_curve(e: &ResolvedEntity, p: [f64; 2]) -> bool {
    if let Some(arc) = e.arc() {
        let angle = (p[1] - arc.center[1]).atan2(p[0] - arc.center[0]);
        let span = if arc.sweep > 0. {
            (angle - arc.start).rem_euclid(std::f64::consts::TAU)
        } else {
            (arc.start - angle).rem_euclid(std::f64::consts::TAU)
        };
        ((p[0] - arc.center[0]).hypot(p[1] - arc.center[1]) - arc.radius).abs() < 1e-6
            && (span <= arc.sweep.abs() + 1e-7 || std::f64::consts::TAU - span < 1e-7)
    } else if e.kind == "circle" {
        ((p[0] - e.values[0]).hypot(p[1] - e.values[1]) - e.values[2]).abs() < 1e-6
    } else if e.kind == "rectangle" {
        e.points()
            .windows(2)
            .any(|pair| on_segment(pair[0], pair[1], p))
    } else if e.kind == "line" {
        on_segment([e.values[0], e.values[1]], [e.values[2], e.values[3]], p)
    } else {
        false
    }
}

fn on_segment(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> bool {
    let u = [b[0] - a[0], b[1] - a[1]];
    let v = [p[0] - a[0], p[1] - a[1]];
    let length = u[0] * u[0] + u[1] * u[1];
    let t = (u[0] * v[0] + u[1] * v[1]) / length;
    (-1e-7..=1. + 1e-7).contains(&t) && (u[0] * v[1] - u[1] * v[0]).abs() < 1e-6 * length.sqrt()
}
