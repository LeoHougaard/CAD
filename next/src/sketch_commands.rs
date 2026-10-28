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
    CenterRectangle,
    Circle,
    Arc,
    Ellipse,
    EllipseArc,
    #[serde(rename = "bspline")]
    BSpline,
    #[serde(rename = "periodic_bspline")]
    PeriodicBSpline,
    HyperbolaArc,
    ParabolaArc,
}

impl Tool {
    pub fn required_points(self) -> usize {
        match self {
            Self::Select => 0,
            Self::Point => 1,
            Self::Arc | Self::Ellipse | Self::PeriodicBSpline => 3,
            Self::EllipseArc | Self::HyperbolaArc => 5,
            Self::ParabolaArc => 4,
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
    if crate::sketch_curve_commands::advanced(tool) {
        return crate::sketch_curve_commands::draw(object, tool, points, construction);
    }
    let expected = match tool {
        Tool::Select => return Err("Choose a drawing tool".into()),
        Tool::Point => 1,
        Tool::Line | Tool::Rectangle | Tool::CenterRectangle | Tool::Circle => 2,
        Tool::Arc => 3,
        Tool::Polyline if points.len() >= 2 => points.len(),
        Tool::Polyline => return Err("A polyline needs at least two points".into()),
        _ => unreachable!(),
    };
    if points.len() != expected {
        return Err(format!("{tool:?} needs {expected} points"));
    }
    let count = match tool {
        Tool::Rectangle | Tool::CenterRectangle => 4,
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
        Tool::Rectangle | Tool::CenterRectangle => {
            nonzero(
                points[1][0] - points[0][0],
                "A rectangle needs a nonzero width",
            )?;
            nonzero(
                points[1][1] - points[0][1],
                "A rectangle needs a nonzero height",
            )?;
            let opposite = if tool == Tool::CenterRectangle {
                [
                    2. * points[0][0] - points[1][0],
                    2. * points[0][1] - points[1][1],
                    0.,
                ]
            } else {
                points[0]
            };
            finite_point(&opposite)?;
            let (x, y) = (opposite[0].min(points[1][0]), opposite[1].min(points[1][1]));
            let (right, top) = (opposite[0].max(points[1][0]), opposite[1].max(points[1][1]));
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
        _ => unreachable!(),
    }
    Ok(edits)
}

/// Add snapped references using the same click interpretation as the editor.
/// Native Sketcher numbers clockwise arc endpoints in CCW-XY order, opposite
/// the start/end order accepted by Part.ArcOfCircle's three-point constructor.
pub fn draw_snapped_edits(
    object: &str,
    tool: Tool,
    points: &[[f64; 3]],
    geometry_count: usize,
    construction: bool,
    snaps: &[Option<SketchElement>],
) -> Result<Vec<Value>, String> {
    let mut edits = draw_edits(object, tool, points, geometry_count, construction)?;
    if snaps.len() != points.len() {
        return Err("Each input point must have a corresponding optional snap".into());
    }
    if snaps
        .iter()
        .flatten()
        .any(|target| !matches!(target.point, 0..=3) || target.index == -2000)
    {
        return Err("Snap targets must identify native geometry or point handles".into());
    }
    let refs: Vec<(usize, usize, i32)> = match tool {
        Tool::Point => vec![(0, geometry_count, 1)],
        Tool::Line => {
            vec![
                (0, geometry_count, 1),
                (points.len() - 1, geometry_count, 2),
            ]
        }
        Tool::Polyline => (0..points.len())
            .map(|click| {
                if click == 0 {
                    (click, geometry_count, 1)
                } else {
                    (click, geometry_count + click - 1, 2)
                }
            })
            .collect(),
        Tool::Arc => {
            let first = delta(points[0], points[1]);
            let second = delta(points[0], points[2]);
            let clockwise = first[0] * second[1] - first[1] * second[0] < 0.;
            let (start, end) = if clockwise { (2, 1) } else { (1, 2) };
            vec![(0, geometry_count, start), (2, geometry_count, end)]
        }
        Tool::Circle | Tool::Ellipse | Tool::EllipseArc | Tool::HyperbolaArc => {
            vec![(0, geometry_count, 3)]
        }
        Tool::BSpline => vec![
            (0, geometry_count, 1),
            (points.len() - 1, geometry_count, 2),
        ],
        Tool::CenterRectangle => {
            if let Some(target) = snaps[0] {
                if target.point == 0 {
                    return Err(
                        "Center rectangle cannot yet constrain its center to an edge".into(),
                    );
                }
                let start = i32::try_from(geometry_count)
                    .map_err(|_| "Native geometry index is too large")?;
                edits.push(constraint(
                    object,
                    "Symmetric",
                    json!([start, 1, start + 2, 1, target.index, target.point]),
                ));
            }
            let opposite = [
                2. * points[0][0] - points[1][0],
                2. * points[0][1] - points[1][1],
                0.,
            ];
            let minx = opposite[0].min(points[1][0]);
            let miny = opposite[1].min(points[1][1]);
            let corner = match (points[1][0] > minx, points[1][1] > miny) {
                (false, false) => 0,
                (true, false) => 1,
                (true, true) => 2,
                (false, true) => 3,
            };
            vec![(1, geometry_count + corner, 1)]
        }
        Tool::Rectangle => {
            let minx = points[0][0].min(points[1][0]);
            let miny = points[0][1].min(points[1][1]);
            (0..2)
                .map(|click| {
                    let corner = match (points[click][0] > minx, points[click][1] > miny) {
                        (false, false) => 0,
                        (true, false) => 1,
                        (true, true) => 2,
                        (false, true) => 3,
                    };
                    (click, geometry_count + corner, 1)
                })
                .collect()
        }
        _ => vec![],
    };
    for (click, index, point) in refs {
        if let Some(target) = snaps[click] {
            let index = i32::try_from(index).map_err(|_| "Native geometry index is too large")?;
            edits.push(if target.point == 0 {
                constraint(object, "PointOnObject", json!([index, point, target.index]))
            } else {
                constraint(
                    object,
                    "Coincident",
                    json!([index, point, target.index, target.point]),
                )
            });
        }
    }
    if tool == Tool::Circle
        && let Some(target) = snaps[1].filter(|target| target.point != 0)
    {
        let index =
            i32::try_from(geometry_count).map_err(|_| "Native geometry index is too large")?;
        edits.push(constraint(
            object,
            "PointOnObject",
            json!([target.index, target.point, index]),
        ));
    }
    Ok(edits)
}

struct Sketch<'a> {
    name: &'a str,
    geometry: &'a [Value],
    externals: &'a [Value],
    axes: &'a [Value],
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
            externals: snapshot["sketch"]["externals"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default(),
            axes: snapshot["sketch"]["axes"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default(),
            constraints,
        })
    }

    fn geometry(&self, element: SketchElement) -> Result<&Value, String> {
        self.all_geometry()
            .find(|geo| geo["index"].as_i64() == Some(i64::from(element.index)))
            .ok_or_else(|| "Selected geometry is no longer in this sketch".into())
    }

    fn all_geometry(&self) -> impl Iterator<Item = &'a Value> + '_ {
        self.geometry
            .iter()
            .filter(|geometry| geometry["index"].as_i64().is_some_and(|index| index >= 0))
            .chain(self.axes.iter().filter(|geometry| {
                matches!(geometry["index"].as_i64(), Some(-1 | -2))
                    && geometry["kind"] == "line"
                    && geometry["handles"].as_array().is_some_and(Vec::is_empty)
            }))
            .chain(self.externals.iter().filter_map(|wrapper| {
                // Flattened native cache indices differ from source link indices.
                // Only the explicitly paired signed solver index is selectable.
                let flat = i32::try_from(wrapper["index"].as_i64()?).ok()?;
                let index = i32::try_from(wrapper["geometry_index"].as_i64()?).ok()?;
                let geometry = &wrapper["geometry"];
                (flat >= 0
                    && Some(index) == (-3_i32).checked_sub(flat)
                    && geometry["index"].as_i64() == Some(i64::from(index)))
                .then_some(geometry)
            }))
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
        if element.point <= 0 {
            return Err("Select a native sketch point or curve handle".into());
        }
        let mut position = None;
        for geometry in self.all_geometry() {
            for handle in geometry["handles"].as_array().into_iter().flatten() {
                let index = handle.get("index").unwrap_or(&geometry["index"]);
                if index.as_i64() != Some(i64::from(element.index))
                    || handle["point"].as_i64() != Some(i64::from(element.point))
                {
                    continue;
                }
                if geometry["index"].as_i64().is_some_and(|index| index <= -3) && element.index >= 0
                {
                    return Err("External handles must retain their native solver indices".into());
                }
                let found = vector(&handle["position"])?;
                if position.is_some_and(|previous| previous != found) {
                    return Err("Native point handle positions disagree; refresh the sketch".into());
                }
                position = Some(found);
            }
        }
        if let Some(position) = position {
            // Drag permission is independent of constraint reference permission.
            return Ok(position);
        }
        let geo = self.geometry(element)?;
        if geo.get("handles").is_some() {
            return Err("The native snapshot does not expose this point handle".into());
        }
        let key = match (self.kind(element)?, element.point) {
            ("line" | "arc", 1) => "start",
            ("line" | "arc", 2) => "end",
            ("arc" | "circle", 3) => "center",
            ("point", 1) => "position",
            _ => {
                return Err("Select an exposed native point, curve handle or sketch origin".into());
            }
        };
        vector(&geo[key])
    }

    fn entity(&self, element: SketchElement) -> Result<&str, String> {
        if element.point != 0 {
            return Err("Select whole sketch entities for this constraint".into());
        }
        let kind = self.kind(element)?;
        if ![
            "line",
            "circle",
            "arc",
            "ellipse",
            "ellipse_arc",
            "bspline",
            "hyperbola_arc",
            "parabola_arc",
        ]
        .contains(&kind)
        {
            return Err("Select a native sketch curve".into());
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
    let angle_via_point = kind == "AngleViaPoint" || kind == "Angle" && selection.len() == 3;
    if matches!(kind, "Tangent" | "Perpendicular") && selection.len() == 3 || angle_via_point {
        if value.is_some() && !angle_via_point {
            return Err("Via-point tangency and perpendicularity do not take a datum".into());
        }
        let curves: Vec<_> = selection.iter().copied().filter(|e| e.point == 0).collect();
        let points: Vec<_> = selection.iter().copied().filter(|e| e.point != 0).collect();
        if curves.len() != 2 || points.len() != 1 {
            return Err(
                "Select two whole curves and one exposed native point for a via-point constraint"
                    .into(),
            );
        }
        let fixed = |e: SketchElement| {
            e.index < 0
                || snapshot["sketch"]["constraints"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|c| {
                        c["type"] == "Block"
                            && c["first"].as_i64() == Some(i64::from(e.index))
                            && c["first_pos"].as_i64() == Some(0)
                    })
        };
        let reference = curves.iter().all(|e| fixed(*e));
        if reference && !angle_via_point {
            return Err(
                "Cannot constrain two fixed curves; external and blocked curves are fixed".into(),
            );
        }
        if curves.iter().any(|e| {
            snapshot["sketch"]["spline_controls"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|c| c["control_index"].as_i64() == Some(i64::from(e.index)))
        }) {
            return Err("A B-spline weight circle cannot be a via-point curve".into());
        }
        if angle_via_point {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err("Enter an angle with units".into());
            }
            // Preparation changes the geometry. Only native Sketcher can measure
            // the constructor seed and choose its curve order after solving.
            let mut edit = json!({"op":"via_point","object":sketch.name,"kind":"AngleViaPoint",
                "args":[curves[0].index,curves[1].index,points[0].index,points[0].point,0.0],"driving":!reference});
            if let Some(value) = value {
                edit["value"] = json!(value.trim());
            }
            return Ok(vec![edit]);
        }
        return Ok(vec![json!({"op":"via_point","object":sketch.name,
            "kind":format!("{kind}ViaPoint"),"args":[curves[0].index,curves[1].index,points[0].index,points[0].point]})]);
    }
    if kind == "SnellsLaw" {
        let rays: Vec<_> = selection.iter().copied().filter(|e| e.point != 0).collect();
        let boundaries: Vec<_> = selection.iter().copied().filter(|e| e.point == 0).collect();
        if rays.len() != 2 || boundaries.len() != 1 {
            return Err("Refraction needs two line endpoints in n1, n2 selection order and one whole boundary edge".into());
        }
        for ray in &rays {
            if sketch.kind(*ray)? != "line" || !matches!(ray.point, 1 | 2) {
                return Err("Refraction rays must be straight line endpoints".into());
            }
        }
        let boundary = boundaries[0];
        if rays[0].index == rays[1].index || rays.iter().any(|r| r.index == boundary.index) {
            return Err("Refraction needs two distinct rays and a different boundary curve".into());
        }
        if selection.iter().all(|e| e.index < 0) {
            return Err("Refraction cannot use external geometry only".into());
        }
        if value.is_some_and(|v| v.trim().is_empty()) {
            return Err("Enter a refraction ratio n2/n1".into());
        }
        // Native point-on-curve tests must run between native additions, after
        // each solve. The worker prepares the missing relations atomically.
        return Ok(vec![json!({"op":"refraction", "object":sketch.name,
            "args":[rays[0].index,rays[0].point,rays[1].index,rays[1].point,boundary.index,1.0],
            "value":value.unwrap_or("1").trim()})]);
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
    if dimensional && value.is_some() && selection.iter().all(|element| element.index < 0) {
        return Err("Dimensions between fixed references are measured; create a reference dimension without a driving value".into());
    }
    let circular = crate::sketch_dimensions::measure(snapshot, kind, selection)?;
    let args = if let Some(measurement) = circular {
        measurement.args
    } else {
        match kind {
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
                } else if !equal_compatible(first_kind, second_kind) {
                    return Err(
                        "Equal needs two compatible lines, circular curves or conics".into(),
                    );
                }
                json!([first.index, second.index])
            }
            "Perpendicular" | "Tangent" => {
                let (mut first, mut second) = pair(selection)?;
                if first.point == 0 && second.point != 0 {
                    std::mem::swap(&mut first, &mut second);
                }
                if first.index == second.index {
                    return Err("Select two distinct curves".into());
                }
                sketch.kind(first)?;
                sketch.kind(second)?;
                if !matches!(first.point, 0..=2) || !matches!(second.point, 0..=2) {
                    return Err(
                        "Select curve endpoints or whole curves, rather than centers".into(),
                    );
                }
                if kind == "Tangent"
                    && sketch.kind(first)? == "line"
                    && sketch.kind(second)? == "line"
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
                    if element.index < 0 {
                        return Err(if element.index == -1 {
                            "The sketch origin is already fixed"
                        } else {
                            "External geometry is controlled by its source and cannot be blocked"
                        }
                        .into());
                    }
                    if element.point == 0 {
                        edits.push(constraint(sketch.name, "Block", json!([element.index])));
                    } else {
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
                if selection.len() != 1
                    || !["circle", "arc"].contains(&sketch.entity(selection[0])?)
                {
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
        }
    };
    let mut edits = vec![constraint(sketch.name, kind, args)];
    if let Some(value) = value {
        edits.push(json!({"op":"datum", "object":sketch.name, "index":sketch.constraints, "value":value.trim()}));
    }
    Ok(edits)
}

/// Compile native datum, formula and reference options at the native-owned index.
pub fn angle_via_point_edits(
    snapshot: &Value,
    selection: &[SketchElement],
    value: Option<&str>,
    expression: Option<&str>,
    driving: Option<bool>,
) -> Result<Vec<Value>, String> {
    if value.is_some() && expression.is_some() {
        return Err("Choose an angle datum or an expression".into());
    }
    if expression.is_some_and(|expression| expression.trim().is_empty()) {
        return Err("Enter a native angle expression".into());
    }
    let mut edits = constrain(snapshot, "AngleViaPoint", selection, value)?;
    if let Some(expression) = expression {
        edits[0]["expression"] = json!(expression.trim());
    }
    if driving == Some(false) {
        edits[0]["driving"] = json!(false);
    }
    Ok(edits)
}

/// Compile existing native datum/expression edits from the same snapshot used by the UI.
pub fn datum_edits(
    snapshot: &Value,
    index: usize,
    value: &str,
    expression: bool,
) -> Result<Vec<Value>, String> {
    let sketch = Sketch::new(snapshot)?;
    let constraint = snapshot["sketch"]["constraints"]
        .as_array()
        .and_then(|cs| {
            cs.iter()
                .find(|c| c["index"].as_u64() == Some(index as u64))
        })
        .ok_or("Choose an existing native constraint")?;
    if !matches!(
        constraint["type"].as_str().unwrap_or_default(),
        "Distance"
            | "DistanceX"
            | "DistanceY"
            | "Radius"
            | "Diameter"
            | "Angle"
            | "Weight"
            | "SnellsLaw"
    ) {
        return Err("This constraint has no editable datum".into());
    }
    if constraint["driving"] == false {
        return Err("Reference constraints do not accept driving values".into());
    }
    let value = value.trim();
    if value.is_empty() {
        return Err("Enter a datum value or expression".into());
    }
    let path = format!("Constraints[{index}]");
    if expression {
        return Ok(vec![
            json!({"op":"expression","object":sketch.name,"property":path,"expression":value}),
        ]);
    }
    let named = constraint["name"]
        .as_str()
        .filter(|n| !n.is_empty())
        .map(|n| format!("Constraints.{n}"));
    let bound = snapshot["properties"]["ExpressionEngine"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|entry| {
            entry[0].as_str().is_some_and(|p| {
                let p = p.trim_start_matches('.');
                p == path || named.as_deref() == Some(p)
            })
        });
    let mut edits = Vec::new();
    if bound {
        edits.push(
            json!({"op":"expression","object":sketch.name,"property":path,"expression":null}),
        );
    }
    edits.push(json!({"op":"datum","object":sketch.name,"index":index,"value":value}));
    Ok(edits)
}

fn measure(kind: &str, vector: [f64; 2]) -> f64 {
    match kind {
        "DistanceX" => vector[0],
        "DistanceY" => vector[1],
        _ => length(vector),
    }
}

fn equal_compatible(first: &str, second: &str) -> bool {
    [
        &["line"][..],
        &["circle", "arc"][..],
        &["ellipse", "ellipse_arc"][..],
        &["hyperbola_arc"][..],
        &["parabola_arc"][..],
    ]
    .iter()
    .any(|family| family.contains(&first) && family.contains(&second))
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
    fn center_rectangle_uses_center_and_corner_without_faking_center_snap() {
        let edits = draw_snapped_edits(
            "Sketch",
            Tool::CenterRectangle,
            &[[2., 3., 0.], [5., 7., 0.]],
            4,
            true,
            &[None, None],
        )
        .unwrap();
        assert_eq!(edits.len(), 12);
        assert_eq!(edits[0]["start"], json!([-1., -1., 0.]));
        assert_eq!(edits[0]["end"], json!([5., -1., 0.]));
        assert_eq!(edits[0]["construction"], true);
        assert_eq!(edits[4]["args"], json!([4, 2, 5, 1]));
        let snapped = draw_snapped_edits(
            "Sketch",
            Tool::CenterRectangle,
            &[[2., 3., 0.], [5., 7., 0.]],
            4,
            false,
            &[Some(e(-1, 1)), None],
        )
        .unwrap();
        assert_eq!(snapped[12]["kind"], "Symmetric");
        assert_eq!(snapped[12]["args"], json!([4, 1, 6, 1, -1, 1]));
        assert!(
            draw_snapped_edits(
                "Sketch",
                Tool::CenterRectangle,
                &[[2., 3., 0.], [5., 7., 0.]],
                4,
                false,
                &[Some(e(0, 0)), None]
            )
            .is_err()
        );
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

    fn advanced_sketch() -> Value {
        json!({"name":"Sketch","sketch":{"constraint_count":4,"geometry":[
            {"index":0,"kind":"ellipse","handles":[
                {"index":0,"point":3,"position":[5,7,0],"draggable":false}]},
            {"index":1,"kind":"bspline","points":[[1,2,0],[8,4,0]],"handles":[
                {"index":1,"point":1,"position":[1,2,0],"draggable":false},
                {"index":1,"point":2,"position":[8,4,0],"draggable":false}]},
            {"index":2,"kind":"hyperbola_arc","handles":[
                {"index":2,"point":1,"position":[4,3,0]}]},
            {"index":3,"kind":"parabola_arc","handles":[
                {"index":3,"point":2,"position":[6,3,0]}]},
            {"index":4,"kind":"ellipse_arc","handles":[
                {"index":4,"point":1,"position":[4,1,0]}]}
        ],"externals":[
            {"index":0,"geometry_index":-3,"source_index":9,"geometry":{
                "index":-3,"kind":"line","start":[0,10,0],"end":[20,10,0],"handles":[
                    {"index":-3,"point":1,"position":[0,10,0],"draggable":false},
                    {"index":-3,"point":2,"position":[20,10,0],"draggable":false}]}},
            {"index":1,"geometry_index":-4,"source_index":0,"geometry":{
                "index":-4,"kind":"circle","radius":2,"handles":[
                    {"index":-4,"point":3,"position":[12,4,0],"draggable":false}]}},
            {"index":2,"geometry_index":-5,"source_index":0,"geometry":{
                "index":-5,"kind":"ellipse_arc","handles":[
                    {"index":-5,"point":1,"position":[0,1,0],"draggable":false}]}},
            {"index":3,"geometry_index":-6,"source_index":4,"geometry":{
                "index":-6,"kind":"point","position":[25,8,0],"handles":[
                    {"index":-6,"point":1,"position":[25,8,0],"draggable":false}]}}
        ]}})
    }

    #[test]
    fn external_solver_ids_and_fixed_handles_remain_constraint_references() {
        let snapshot = advanced_sketch();
        let distance =
            constrain(&snapshot, "Distance", &[e(1, 1), e(-3, 0)], Some("4 mm")).unwrap();
        assert_eq!(distance[0]["args"], json!([1, 1, -3, 8.]));
        let coincidence = constrain(&snapshot, "Coincident", &[e(1, 2), e(-3, 2)], None).unwrap();
        assert_eq!(coincidence[0]["args"], json!([1, 2, -3, 2]));
        let vertex = constrain(&snapshot, "Coincident", &[e(1, 1), e(-6, 1)], None).unwrap();
        assert_eq!(vertex[0]["args"], json!([1, 1, -6, 1]));
        let vertex_dimension = constrain(&snapshot, "DistanceX", &[e(-6, 1)], None).unwrap();
        assert_eq!(vertex_dimension[0]["args"], json!([-6, 1, 25.]));
        let tangent = constrain(&snapshot, "Tangent", &[e(-3, 0), e(4, 1)], None).unwrap();
        assert_eq!(tangent[0]["args"], json!([4, 1, -3]));
        let perpendicular =
            constrain(&snapshot, "Perpendicular", &[e(1, 1), e(-3, 0)], None).unwrap();
        assert_eq!(perpendicular[0]["args"], json!([1, 1, -3]));
        let radius = constrain(&snapshot, "Radius", &[e(-4, 0)], None).unwrap();
        assert_eq!(radius[0]["args"], json!([-4, 2.]));
        assert!(constrain(&snapshot, "Block", &[e(-3, 0)], None).is_err());
        assert!(constrain(&snapshot, "Block", &[e(-4, 3)], None).is_err());
        assert!(constrain(&snapshot, "Radius", &[e(-4, 0)], Some("4 mm")).is_err());
        let mut mismatched = snapshot;
        mismatched["sketch"]["externals"][0]["index"] = json!(8);
        assert!(constrain(&mismatched, "Distance", &[e(-3, 0)], None).is_err());
    }

    #[test]
    fn advanced_curves_use_only_exposed_native_handles_and_compatible_equal_families() {
        let mut snapshot = advanced_sketch();
        let ellipse_center = constrain(&snapshot, "DistanceX", &[e(0, 3)], Some("8 mm")).unwrap();
        assert_eq!(ellipse_center[0]["args"], json!([0, 3, 5.]));
        for index in 1..=4 {
            let point = if index == 3 { 2 } else { 1 };
            assert!(
                constrain(
                    &snapshot,
                    "PointOnObject",
                    &[e(index, point), e(-3, 0)],
                    None
                )
                .is_ok()
            );
            assert!(constrain(&snapshot, "Block", &[e(index, 0)], None).is_ok());
        }
        let equal = constrain(&snapshot, "Equal", &[e(0, 0), e(-5, 0)], None).unwrap();
        assert_eq!(equal[0]["args"], json!([0, -5]));
        snapshot["sketch"]["geometry"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"index":5,"kind":"parabola_arc","handles":[]}),
                json!({"index":6,"kind":"bspline","handles":[]}),
                json!({"index":7,"kind":"hyperbola_arc","handles":[]}),
            ]);
        assert!(constrain(&snapshot, "Equal", &[e(3, 0), e(5, 0)], None).is_ok());
        assert!(constrain(&snapshot, "Equal", &[e(1, 0), e(6, 0)], None).is_err());
        assert!(constrain(&snapshot, "Equal", &[e(2, 0), e(7, 0)], None).is_ok());
        assert!(constrain(&snapshot, "Equal", &[e(2, 0), e(5, 0)], None).is_err());
        assert!(constrain(&snapshot, "Equal", &[e(0, 0), e(-4, 0)], None).is_err());
        assert!(constrain(&snapshot, "Equal", &[e(1, 0), e(1, 1)], None).is_err());
        snapshot["sketch"]["geometry"][1]["handles"] = json!([]);
        assert!(constrain(&snapshot, "Coincident", &[e(1, 1), e(-3, 1)], None).is_err());
        assert_eq!(Tool::PeriodicBSpline.required_points(), 3);
    }
    #[test]
    fn snapped_arcs_follow_native_ccw_point_order_for_both_sweep_sizes() {
        for (middle, end, ids) in [
            ([3., 3., 0.], [0., 5., 0.], [1, 2]),
            ([3., -3., 0.], [0., -5., 0.], [2, 1]),
            ([0., 5., 0.], [0., -5., 0.], [1, 2]),
            ([0., -5., 0.], [0., 5., 0.], [2, 1]),
        ] {
            let edits = draw_snapped_edits(
                "Sketch",
                Tool::Arc,
                &[[5., 0., 0.], middle, end],
                2,
                false,
                &[Some(e(0, 1)), Some(e(7, 1)), Some(e(1, 1))],
            )
            .unwrap();
            assert_eq!(
                edits.len(),
                3,
                "Middle click places the curve but adds no snap constraint"
            );
            assert_eq!(edits[0]["mid"], json!(middle));
            assert_eq!(edits[1]["args"], json!([2, ids[0], 0, 1]));
            assert_eq!(edits[2]["args"], json!([2, ids[1], 1, 1]));
        }
    }
    #[test]
    fn snapped_circle_and_rectangle_preserve_existing_center_rim_and_corner_forms() {
        let circle = draw_snapped_edits(
            "Sketch",
            Tool::Circle,
            &[[0., 0., 0.], [2., 0., 0.]],
            3,
            false,
            &[Some(e(0, 1)), Some(e(1, 1))],
        )
        .unwrap();
        assert_eq!(circle[1]["args"], json!([3, 3, 0, 1]));
        assert_eq!(circle[2]["args"], json!([1, 1, 3]));
        let rectangle = draw_snapped_edits(
            "Sketch",
            Tool::Rectangle,
            &[[5., 6., 0.], [-2., -3., 0.]],
            4,
            false,
            &[Some(e(0, 1)), Some(e(1, 1))],
        )
        .unwrap();
        assert_eq!(rectangle[12]["args"], json!([6, 1, 0, 1]));
        assert_eq!(rectangle[13]["args"], json!([4, 1, 1, 1]));
    }
    #[test]
    fn polyline_snaps_refer_to_each_actual_segment_and_invalid_snap_counts_fail() {
        let points = [[0., 0., 0.], [4., 0., 0.], [4., 3., 0.]];
        let edits = draw_snapped_edits(
            "Sketch",
            Tool::Polyline,
            &points,
            3,
            false,
            &[Some(e(-1, 1)), None, Some(e(0, 1))],
        )
        .unwrap();
        assert_eq!(edits[3]["args"], json!([3, 1, -1, 1]));
        assert_eq!(edits[4]["args"], json!([4, 2, 0, 1]));
        assert!(draw_snapped_edits("Sketch", Tool::Arc, &points, 0, false, &[None, None]).is_err());
        assert!(
            draw_snapped_edits(
                "Sketch",
                Tool::Line,
                &points[..2],
                0,
                false,
                &[Some(e(0, 7)), None]
            )
            .is_err()
        );
    }
    #[test]
    fn refraction_keeps_endpoint_order_with_boundary_in_any_selection_position() {
        let snapshot = json!({"name":"Sketch","sketch":{"constraint_count":0,"geometry":[
            {"index":0,"kind":"line","start":[-5,-5,0],"end":[0,0,0]},
            {"index":1,"kind":"line","start":[0,0,0],"end":[5,5,0]},
            {"index":2,"kind":"ellipse","center":[0,5,0]}]}});
        for selection in [
            vec![e(2, 0), e(0, 2), e(1, 1)],
            vec![e(0, 2), e(2, 0), e(1, 1)],
            vec![e(0, 2), e(1, 1), e(2, 0)],
        ] {
            let edits = constrain(&snapshot, "SnellsLaw", &selection, Some("1.5")).unwrap();
            assert_eq!(edits[0]["args"], json!([0, 2, 1, 1, 2, 1.0]));
            assert_eq!(edits[0]["value"], "1.5");
        }
        let swapped =
            constrain(&snapshot, "SnellsLaw", &[e(1, 1), e(0, 2), e(2, 0)], None).unwrap();
        assert_eq!(swapped[0]["args"], json!([1, 1, 0, 2, 2, 1.0]));
        for selection in [
            vec![e(0, 0), e(1, 0), e(2, 0)],
            vec![e(0, 2), e(0, 1), e(2, 0)],
            vec![e(0, 2), e(1, 1), e(0, 0)],
            vec![e(0, 2), e(1, 1)],
        ] {
            assert!(constrain(&snapshot, "SnellsLaw", &selection, None).is_err());
        }
    }

    #[test]
    fn existing_ratio_datum_clears_native_named_expression_atomically() {
        let snapshot = json!({"name":"Sketch","properties":{"ExpressionEngine":[[".Constraints.IndexRatio","3/2"]]},
            "sketch":{"constraint_count":1,"geometry":[],"constraints":[{"index":0,"type":"SnellsLaw","name":"IndexRatio","driving":true}]}});
        let edits = datum_edits(&snapshot, 0, "1.25", false).unwrap();
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0]["expression"], Value::Null);
        assert_eq!(edits[1]["value"], "1.25");
        let edits = datum_edits(&snapshot, 0, "5 / 4", true).unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["expression"], "5 / 4");
        assert!(datum_edits(&snapshot, 1, "1", false).is_err());
    }
    #[test]
    fn via_point_preserves_curve_order_with_point_at_any_selection_position() {
        let snapshot = json!({"name":"Sketch","sketch":{"constraint_count":0,"geometry":[
            {"index":0,"kind":"circle","center":[0,5,0],"radius":5},
            {"index":1,"kind":"line","start":[-5,0,0],"end":[5,0,0]},
            {"index":2,"kind":"point","position":[0,0,0]}]}});
        for kind in ["Tangent", "Perpendicular"] {
            for selection in [
                vec![e(2, 1), e(0, 0), e(1, 0)],
                vec![e(0, 0), e(2, 1), e(1, 0)],
                vec![e(0, 0), e(1, 0), e(2, 1)],
            ] {
                let edits = constrain(&snapshot, kind, &selection, None).unwrap();
                assert_eq!(edits.len(), 1);
                assert_eq!(edits[0]["op"], "via_point");
                assert_eq!(edits[0]["args"], json!([0, 1, 2, 1]));
                assert_eq!(edits[0]["kind"], format!("{kind}ViaPoint"));
                assert!(edits[0].get("value").is_none());
            }
            let swapped = constrain(&snapshot, kind, &[e(1, 0), e(0, 0), e(2, 1)], None).unwrap();
            assert_eq!(swapped[0]["args"], json!([1, 0, 2, 1]));
            assert!(constrain(&snapshot, kind, &[e(0, 0), e(1, 0), e(-1, 1)], None).is_ok());
            assert!(constrain(&snapshot, kind, &[e(0, 0), e(1, 0), e(0, 3)], None).is_ok());
            assert!(constrain(&snapshot, kind, &[e(0, 0), e(1, 0), e(2, 1)], Some("2")).is_err());
            assert_eq!(
                constrain(&snapshot, kind, &[e(0, 0), e(1, 0)], None).unwrap()[0]["op"],
                "constraint"
            );
        }
    }

    #[test]
    fn via_point_rejects_fixed_curves_weight_circles_and_unexposed_points() {
        let mut snapshot = json!({"name":"Sketch","sketch":{"constraint_count":2,"geometry":[
            {"index":0,"kind":"circle","center":[0,5,0],"radius":5},
            {"index":1,"kind":"line","start":[-5,0,0],"end":[5,0,0]},
            {"index":2,"kind":"point","position":[0,0,0]}],
            "constraints":[{"type":"Block","first":0,"first_pos":0},{"type":"Block","first":1,"first_pos":0}]}});
        for kind in ["Tangent", "Perpendicular"] {
            assert!(
                constrain(&snapshot, kind, &[e(0, 0), e(1, 0), e(2, 1)], None)
                    .unwrap_err()
                    .contains("fixed")
            );
        }
        snapshot["sketch"]["constraints"] = json!([]);
        snapshot["sketch"]["spline_controls"] = json!([{"control_index":0}]);
        assert!(
            constrain(&snapshot, "Tangent", &[e(0, 0), e(1, 0), e(2, 1)], None)
                .unwrap_err()
                .contains("weight")
        );
        snapshot["sketch"]["spline_controls"] = json!([]);
        assert!(constrain(&snapshot, "Tangent", &[e(0, 0), e(1, 0), e(1, 3)], None).is_err());
        assert!(constrain(&snapshot, "Tangent", &[e(0, 0), e(2, 1), e(1, 1)], None).is_err());
        assert!(constrain(&snapshot, "Angle", &[e(0, 0), e(1, 0), e(2, 1)], None).is_ok());
    }

    #[test]
    fn angle_via_point_keeps_native_measurement_and_selection_order() {
        let mut snapshot = advanced_sketch();
        snapshot["sketch"]["axes"] = json!([
            {"index":-1,"kind":"line","start":[0,0,0],"end":[1,0,0],"handles":[]},
            {"index":-2,"kind":"line","start":[0,0,0],"end":[0,1,0],"handles":[]}
        ]);
        for selection in [
            vec![e(-1, 1), e(-2, 0), e(-1, 0)],
            vec![e(-2, 0), e(-1, 1), e(-1, 0)],
            vec![e(-2, 0), e(-1, 0), e(-1, 1)],
        ] {
            let edit = &constrain(&snapshot, "Angle", &selection, None).unwrap()[0];
            assert_eq!(edit["args"], json!([-2, -1, -1, 1, 0.0]));
            assert_eq!(edit["driving"], false);
        }
        let selection = [e(-1, 0), e(0, 0), e(-1, 1)];
        let edits =
            angle_via_point_edits(&snapshot, &selection, Some("-450 deg"), None, None).unwrap();
        assert_eq!(edits[0]["value"], "-450 deg");
        assert_eq!(edits[0]["driving"], true);
        let edits = angle_via_point_edits(
            &snapshot,
            &selection,
            None,
            Some("30 deg + 15 deg"),
            Some(false),
        )
        .unwrap();
        assert_eq!(edits[0]["expression"], "30 deg + 15 deg");
        assert_eq!(edits[0]["driving"], false);
        assert!(angle_via_point_edits(&snapshot, &selection, Some("1"), Some("2"), None).is_err());
        assert!(
            constrain(
                &snapshot,
                "AngleViaPoint",
                &[e(-1, 0), e(0, 0), e(-2, 1)],
                None
            )
            .is_err()
        );
    }
}
