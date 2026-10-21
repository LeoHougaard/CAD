//! Build native sketch edits from solved geometry and explicit selections.
//! This module owns no document state or solver. Submit its edits atomically
//! against the revision from which the snapshot and selections were obtained.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::f64::consts::TAU;

const EPS: f64 = 1e-9;

/// Native point 0 selects a whole entity, 1/2 its endpoints, 3 its center.
/// The sketch origin is (-1, 1). Indices are scoped to a solved sketch revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SketchElement {
    pub index: i32,
    pub point: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Select,
    Point,
    Line,
    Polyline,
    Rectangle,
    Circle,
    Arc,
}

impl Tool {
    pub fn required_points(self) -> usize {
        match self {
            Self::Select => 0,
            Self::Point => 1,
            Self::Arc => 3,
            _ => 2,
        }
    }
}

fn name(object: &str) -> Result<(), String> {
    let mut chars = object.chars();
    if !chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err("Choose a native sketch object".into());
    }
    Ok(())
}

fn finite_point(point: &[f64; 3]) -> Result<(), String> {
    if point.iter().any(|value| !value.is_finite()) || point[2].abs() > EPS {
        return Err("Sketch points must be finite coordinates in the sketch plane".into());
    }
    Ok(())
}

fn delta(first: [f64; 3], second: [f64; 3]) -> [f64; 2] {
    [second[0] - first[0], second[1] - first[1]]
}

fn length(vector: [f64; 2]) -> f64 {
    vector[0].hypot(vector[1])
}

fn nonzero(value: f64, message: &str) -> Result<f64, String> {
    if !value.is_finite() || value.abs() <= EPS {
        Err(message.into())
    } else {
        Ok(value)
    }
}

fn constraint(object: &str, kind: &str, args: Value) -> Value {
    json!({"op":"constraint", "object":object, "kind":kind, "args":args})
}

/// Circle points are center/rim. Arc points are start/middle/end.
/// Rectangles get closure and horizontal/vertical constraints, with four
/// remaining degrees of freedom. Drawing never locks position or dimensions.
pub fn draw_edits(
    object: &str,
    tool: Tool,
    points: &[[f64; 3]],
    geometry_count: usize,
    construction: bool,
) -> Result<Vec<Value>, String> {
    name(object)?;
    for point in points {
        finite_point(point)?;
    }
    let expected = match tool {
        Tool::Select => return Err("Choose a drawing tool".into()),
        Tool::Point => 1,
        Tool::Line | Tool::Rectangle | Tool::Circle => 2,
        Tool::Arc => 3,
        Tool::Polyline if points.len() >= 2 => points.len(),
        Tool::Polyline => return Err("A polyline needs at least two points".into()),
    };
    if points.len() != expected {
        return Err(format!("{tool:?} needs {expected} points"));
    }
    let count = match tool {
        Tool::Rectangle => 4,
        Tool::Polyline => points.len() - 1,
        _ => 1,
    };
    let start = i32::try_from(geometry_count).map_err(|_| "Native geometry index is too large")?;
    start
        .checked_add(i32::try_from(count).map_err(|_| "Too many sketch elements")?)
        .ok_or("Native geometry index is too large")?;
    let line = |a, b| {
        json!({"op":"geometry", "object":object, "kind":"line",
        "start":a, "end":b, "construction":construction})
    };
    let mut edits = Vec::new();
    match tool {
        Tool::Point => {
            edits.push(json!({"op":"geometry", "object":object, "kind":"point",
                "position":points[0], "construction":construction}));
        }
        Tool::Line | Tool::Polyline => {
            for pair in points.windows(2) {
                nonzero(
                    length(delta(pair[0], pair[1])),
                    "Line endpoints must be distinct",
                )?;
                edits.push(line(pair[0], pair[1]));
            }
            for offset in 0..count - 1 {
                let index = start + offset as i32;
                edits.push(constraint(
                    object,
                    "Coincident",
                    json!([index, 2, index + 1, 1]),
                ));
            }
            if count > 1 && length(delta(points[0], *points.last().unwrap())) <= EPS {
                edits.push(constraint(
                    object,
                    "Coincident",
                    json!([start + count as i32 - 1, 2, start, 1]),
                ));
            }
        }
        Tool::Rectangle => {
            nonzero(
                points[1][0] - points[0][0],
                "A rectangle needs a nonzero width",
            )?;
            nonzero(
                points[1][1] - points[0][1],
                "A rectangle needs a nonzero height",
            )?;
            let (x, y) = (
                points[0][0].min(points[1][0]),
                points[0][1].min(points[1][1]),
            );
            let (right, top) = (
                points[0][0].max(points[1][0]),
                points[0][1].max(points[1][1]),
            );
            let corners = [[x, y, 0.], [right, y, 0.], [right, top, 0.], [x, top, 0.]];
            for offset in 0..4 {
                edits.push(line(corners[offset], corners[(offset + 1) % 4]));
            }
            for offset in 0..4 {
                edits.push(constraint(
                    object,
                    "Coincident",
                    json!([start + offset, 2, start + (offset + 1) % 4, 1]),
                ));
            }
            for (offset, kind) in ["Horizontal", "Vertical", "Horizontal", "Vertical"]
                .into_iter()
                .enumerate()
            {
                edits.push(constraint(object, kind, json!([start + offset as i32])));
            }
        }
        Tool::Circle => {
            let radius = nonzero(
                length(delta(points[0], points[1])),
                "A circle needs a positive radius",
            )?;
            edits.push(json!({"op":"geometry", "object":object, "kind":"circle",
                "center":points[0], "radius":radius, "construction":construction}));
        }
        Tool::Arc => {
            let first = delta(points[0], points[1]);
            let second = delta(points[0], points[2]);
            let scale = length(first) * length(second);
            if !scale.is_finite()
                || scale <= EPS * EPS
                || (first[0] * second[1] - first[1] * second[0]).abs() <= 1e-10 * scale
            {
                return Err("An arc needs three distinct, non-collinear points".into());
            }
            edits.push(json!({"op":"geometry", "object":object, "kind":"arc",
                "start":points[0], "mid":points[1], "end":points[2], "construction":construction}));
        }
        Tool::Select => unreachable!(),
    }
    Ok(edits)
}

struct Sketch<'a> {
    name: &'a str,
    geometry: &'a [Value],
    constraints: usize,
}

impl<'a> Sketch<'a> {
    fn new(snapshot: &'a Value) -> Result<Self, String> {
        let object_name = snapshot["name"].as_str().ok_or("Choose a native sketch")?;
        name(object_name)?;
        let geometry = snapshot["sketch"]["geometry"]
            .as_array()
            .ok_or("Choose a native sketch")?;
        let constraints = snapshot["sketch"]["constraint_count"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("Sketch constraint count is unavailable")?;
        Ok(Self {
            name: object_name,
            geometry,
            constraints,
        })
    }

    fn geometry(&self, element: SketchElement) -> Result<&Value, String> {
        self.geometry
            .iter()
            .find(|geo| geo["index"].as_i64() == Some(i64::from(element.index)))
            .ok_or_else(|| "Selected geometry is no longer in this sketch".into())
    }

    fn kind(&self, element: SketchElement) -> Result<&str, String> {
        self.geometry(element)?["kind"]
            .as_str()
            .ok_or_else(|| "Unsupported sketch geometry".into())
    }

    fn point(&self, element: SketchElement) -> Result<[f64; 3], String> {
        if element
            == (SketchElement {
                index: -1,
                point: 1,
            })
        {
            return Ok([0.; 3]);
        }
        let geo = self.geometry(element)?;
        let key = match (self.kind(element)?, element.point) {
            ("line" | "arc", 1) => "start",
            ("line" | "arc", 2) => "end",
            ("arc" | "circle", 3) => "center",
            ("point", 1) => "position",
            _ => return Err("Select an endpoint, circle center or sketch origin".into()),
        };
        vector(&geo[key])
    }

    fn entity(&self, element: SketchElement) -> Result<&str, String> {
        if element.point != 0 {
            return Err("Select whole sketch entities for this constraint".into());
        }
        let kind = self.kind(element)?;
        if !["line", "circle", "arc"].contains(&kind) {
            return Err("Select a line, circle or arc".into());
        }
        Ok(kind)
    }

    fn line(&self, element: SketchElement) -> Result<[f64; 2], String> {
        if self.entity(element)? != "line" {
            return Err("Select straight lines for this constraint".into());
        }
        let geo = self.geometry(element)?;
        let result = delta(vector(&geo["start"])?, vector(&geo["end"])?);
        nonzero(length(result), "Selected line has zero length")?;
        Ok(result)
    }
}

fn vector(value: &Value) -> Result<[f64; 3], String> {
    let values = value
        .as_array()
        .filter(|values| values.len() == 3)
        .ok_or("Solved sketch coordinates are unavailable")?;
    let mut result = [0.; 3];
    for (index, value) in values.iter().enumerate() {
        result[index] = value
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or("Solved sketch coordinates must be finite")?;
    }
    Ok(result)
}

fn pair(selection: &[SketchElement]) -> Result<(SketchElement, SketchElement), String> {
    if selection.len() != 2 {
        return Err("Select two sketch elements".into());
    }
    Ok((selection[0], selection[1]))
}

fn point_args(first: SketchElement, second: SketchElement) -> Value {
    json!([first.index, first.point, second.index, second.point])
}

/// Build a constraint from the current solved object snapshot. Dimensional
/// constraints start at the measured native value, then an optional `datum`
/// edit applies the user's explicit unit string in the same transaction.
pub fn constrain(
    snapshot: &Value,
    kind: &str,
    selection: &[SketchElement],
    value: Option<&str>,
) -> Result<Vec<Value>, String> {
    let sketch = Sketch::new(snapshot)?;
    if selection.is_empty() {
        return Err("Select sketch geometry or points first".into());
    }
    for (index, element) in selection.iter().enumerate() {
        if selection[..index].contains(element) {
            return Err("Select distinct sketch elements".into());
        }
        if element.point == 0 {
            sketch.entity(*element)?;
        } else {
            sketch.point(*element)?;
        }
    }
    let dimensional = [
        "Distance",
        "DistanceX",
        "DistanceY",
        "Radius",
        "Diameter",
        "Angle",
    ]
    .contains(&kind);
    if value.is_some() && !dimensional {
        return Err("This geometric constraint does not take a dimension value".into());
    }
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err("Enter a dimension with units, for example 10 mm or 45 deg".into());
    }
    let args = match kind {
        "Horizontal" | "Vertical" => {
            if selection.len() == 1 {
                sketch.line(selection[0])?;
                json!([selection[0].index])
            } else {
                let (first, second) = pair(selection)?;
                sketch.point(first)?;
                sketch.point(second)?;
                point_args(first, second)
            }
        }
        "Coincident" => {
            let (first, second) = pair(selection)?;
            sketch.point(first)?;
            sketch.point(second)?;
            point_args(first, second)
        }
        "PointOnObject" => {
            let (mut first, mut second) = pair(selection)?;
            if first.point == 0 {
                std::mem::swap(&mut first, &mut second);
            }
            sketch.point(first)?;
            sketch.entity(second)?;
            if first.index == second.index {
                return Err("Select a point and a different curve".into());
            }
            json!([first.index, first.point, second.index])
        }
        "Parallel" | "Equal" => {
            let (first, second) = pair(selection)?;
            let first_kind = sketch.entity(first)?;
            let second_kind = sketch.entity(second)?;
            if kind == "Parallel" {
                sketch.line(first)?;
                sketch.line(second)?;
            } else if (first_kind == "line") != (second_kind == "line") {
                return Err("Equal needs two lines, or two circles/arcs".into());
            }
            json!([first.index, second.index])
        }
        "Perpendicular" | "Tangent" => {
            let (mut first, mut second) = pair(selection)?;
            if first.point == 0 && second.point != 0 {
                std::mem::swap(&mut first, &mut second);
            }
            if first.index == second.index || first.index < 0 || second.index < 0 {
                return Err("Select two distinct curves".into());
            }
            if first.point == 3 || second.point == 3 {
                return Err("Select curve endpoints or whole curves, rather than centers".into());
            }
            if kind == "Tangent" && sketch.kind(first)? == "line" && sketch.kind(second)? == "line"
            {
                return Err("Use Parallel for two straight lines".into());
            }
            match (first.point, second.point) {
                (0, 0) => json!([first.index, second.index]),
                (_, 0) => json!([first.index, first.point, second.index]),
                _ => point_args(first, second),
            }
        }
        "Symmetric" => {
            if selection.len() != 3 {
                return Err(
                    "Select two points and an axis line, or three points with the center last"
                        .into(),
                );
            }
            let points: Vec<_> = selection
                .iter()
                .copied()
                .filter(|element| element.point != 0)
                .collect();
            if points.len() == 2 {
                let axis = *selection.iter().find(|element| element.point == 0).unwrap();
                sketch.line(axis)?;
                json!([
                    points[0].index,
                    points[0].point,
                    points[1].index,
                    points[1].point,
                    axis.index
                ])
            } else if points.len() == 3 {
                json!([
                    points[0].index,
                    points[0].point,
                    points[1].index,
                    points[1].point,
                    points[2].index,
                    points[2].point
                ])
            } else {
                return Err("Symmetry needs two points and a line, or three points".into());
            }
        }
        "Block" => {
            let mut edits = Vec::new();
            for element in selection {
                if element.point == 0 {
                    edits.push(constraint(sketch.name, "Block", json!([element.index])));
                } else {
                    if element.index < 0 {
                        return Err("The sketch origin is already fixed".into());
                    }
                    let point = sketch.point(*element)?;
                    edits.push(constraint(
                        sketch.name,
                        "DistanceX",
                        json!([element.index, element.point, point[0]]),
                    ));
                    edits.push(constraint(
                        sketch.name,
                        "DistanceY",
                        json!([element.index, element.point, point[1]]),
                    ));
                }
            }
            return Ok(edits);
        }
        "Radius" | "Diameter" => {
            if selection.len() != 1 || !["circle", "arc"].contains(&sketch.entity(selection[0])?) {
                return Err("Select one circle or arc".into());
            }
            let radius = sketch.geometry(selection[0])?["radius"]
                .as_f64()
                .ok_or("Solved radius is unavailable")?;
            let measure = nonzero(radius, "Selected curve has an invalid radius")?
                * if kind == "Diameter" { 2. } else { 1. };
            if radius <= 0. || !measure.is_finite() {
                return Err("Selected curve has an invalid radius".into());
            }
            json!([selection[0].index, measure])
        }
        "Angle" => {
            let first = sketch.line(selection[0])?;
            if selection.len() == 1 {
                json!([selection[0].index, first[1].atan2(first[0]).rem_euclid(TAU)])
            } else {
                let (_, second) = pair(selection)?;
                let second_line = sketch.line(second)?;
                let angle = (first[0] * second_line[1] - first[1] * second_line[0])
                    .atan2(first[0] * second_line[0] + first[1] * second_line[1])
                    .rem_euclid(TAU);
                json!([selection[0].index, second.index, angle])
            }
        }
        "Distance" | "DistanceX" | "DistanceY" => {
            let (mut args, measure) = if selection.len() == 1 && selection[0].point == 0 {
                let vector = sketch.line(selection[0])?;
                (vec![json!(selection[0].index)], measure(kind, vector))
            } else if selection.len() == 1 {
                let point = sketch.point(selection[0])?;
                let args = if kind == "Distance" {
                    vec![
                        json!(-1),
                        json!(1),
                        json!(selection[0].index),
                        json!(selection[0].point),
                    ]
                } else {
                    vec![json!(selection[0].index), json!(selection[0].point)]
                };
                (args, measure(kind, [point[0], point[1]]))
            } else {
                let (mut first, mut second) = pair(selection)?;
                if kind == "Distance" && (first.point == 0 || second.point == 0) {
                    if first.point == 0 {
                        std::mem::swap(&mut first, &mut second);
                    }
                    let point = sketch.point(first)?;
                    let direction = sketch.line(second)?;
                    let start = vector(&sketch.geometry(second)?["start"])?;
                    let offset = delta(start, point);
                    let distance = (direction[0] * offset[1] - direction[1] * offset[0]).abs()
                        / length(direction);
                    (
                        vec![json!(first.index), json!(first.point), json!(second.index)],
                        distance,
                    )
                } else {
                    let delta = delta(sketch.point(first)?, sketch.point(second)?);
                    (
                        vec![
                            json!(first.index),
                            json!(first.point),
                            json!(second.index),
                            json!(second.point),
                        ],
                        measure(kind, delta),
                    )
                }
            };
            if !measure.is_finite() {
                return Err("Dimension exceeds finite native coordinates".into());
            }
            if kind == "Distance" {
                nonzero(measure, "Use Coincident for points at zero distance")?;
            }
            args.push(json!(measure));
            Value::Array(args)
        }
        _ => return Err(format!("Unsupported sketch constraint: {kind}")),
    };
    let mut edits = vec![constraint(sketch.name, kind, args)];
    if let Some(value) = value {
        edits.push(json!({"op":"datum", "object":sketch.name, "index":sketch.constraints, "value":value.trim()}));
    }
    Ok(edits)
}

fn measure(kind: &str, vector: [f64; 2]) -> f64 {
    match kind {
        "DistanceX" => vector[0],
        "DistanceY" => vector[1],
        _ => length(vector),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sketch() -> Value {
        json!({"name":"Sketch", "sketch":{"constraint_count":7,"geometry":[
            {"index":0,"kind":"line","start":[1,2,0],"end":[5,5,0]},
            {"index":1,"kind":"line","start":[0,0,0],"end":[0,10,0]},
            {"index":2,"kind":"circle","center":[4,6,0],"radius":3},
            {"index":3,"kind":"arc","start":[10,0,0],"mid":[0,10,0],"end":[-10,0,0],"center":[0,0,0],"radius":10}
        ]}})
    }
    fn e(index: i32, point: i32) -> SketchElement {
        SketchElement { index, point }
    }

    #[test]
    fn drawing_preserves_freedom_and_native_indices() {
        let rectangle = draw_edits(
            "Sketch",
            Tool::Rectangle,
            &[[5., 6., 0.], [-2., -3., 0.]],
            4,
            false,
        )
        .unwrap();
        assert_eq!(rectangle.len(), 12);
        assert_eq!(rectangle[4]["args"], json!([4, 2, 5, 1]));
        assert!(rectangle.iter().all(|edit| {
            !["Distance", "DistanceX", "DistanceY", "Block"]
                .contains(&edit["kind"].as_str().unwrap_or_default())
        }));
        let polyline = draw_edits(
            "Sketch",
            Tool::Polyline,
            &[[0., 0., 0.], [5., 0., 0.], [5., 5., 0.], [0., 0., 0.]],
            0,
            true,
        )
        .unwrap();
        assert_eq!(polyline.len(), 6);
        assert_eq!(polyline[5]["args"], json!([2, 2, 0, 1]));
        assert_eq!(polyline[0]["construction"], true);
    }

    #[test]
    fn typed_dimensions_follow_measured_native_seeds() {
        let line = constrain(&sketch(), "Distance", &[e(0, 0)], Some("25 mm")).unwrap();
        assert_eq!(line[0]["args"], json!([0, 5.]));
        assert_eq!(
            line[1],
            json!({"op":"datum","object":"Sketch","index":7,"value":"25 mm"})
        );
        let radius = constrain(&sketch(), "Diameter", &[e(2, 0)], Some("8 mm")).unwrap();
        assert_eq!(radius[0]["args"], json!([2, 6.]));
        let projected = constrain(&sketch(), "DistanceY", &[e(0, 1), e(0, 2)], None).unwrap();
        assert_eq!(projected[0]["args"], json!([0, 1, 0, 2, 3.]));
        let center = constrain(&sketch(), "DistanceX", &[e(2, 3)], None).unwrap();
        assert_eq!(center[0]["args"], json!([2, 3, 4.]));
    }

    #[test]
    fn geometric_selection_forms_map_to_native_conventions() {
        let tangent = constrain(&sketch(), "Tangent", &[e(0, 0), e(3, 1)], None).unwrap();
        assert_eq!(tangent[0]["args"], json!([3, 1, 0]));
        let symmetric =
            constrain(&sketch(), "Symmetric", &[e(1, 0), e(0, 1), e(0, 2)], None).unwrap();
        assert_eq!(symmetric[0]["args"], json!([0, 1, 0, 2, 1]));
        let fixed = constrain(&sketch(), "Block", &[e(2, 3)], None).unwrap();
        assert_eq!(fixed[1]["args"], json!([2, 3, 6.]));
        let origin = constrain(&sketch(), "Coincident", &[e(0, 1), e(-1, 1)], None).unwrap();
        assert_eq!(origin[0]["args"], json!([0, 1, -1, 1]));
    }

    #[test]
    fn rejects_ambiguous_selection_and_degenerate_geometry() {
        assert!(draw_edits("Sketch", Tool::Line, &[[0.; 3], [0.; 3]], 0, false).is_err());
        assert!(
            draw_edits(
                "Sketch",
                Tool::Arc,
                &[[0.; 3], [1., 1., 0.], [2., 2., 0.]],
                0,
                false
            )
            .is_err()
        );
        assert!(constrain(&sketch(), "Parallel", &[e(0, 0), e(2, 0)], None).is_err());
        assert!(constrain(&sketch(), "Coincident", &[e(0, 1), e(0, 1)], None).is_err());
        assert!(constrain(&sketch(), "Radius", &[e(0, 0)], Some("3 mm")).is_err());
        assert!(constrain(&sketch(), "Horizontal", &[e(0, 0)], Some("3 mm")).is_err());
        assert!(constrain(&sketch(), "Distance", &[e(99, 0)], None).is_err());
    }
}
