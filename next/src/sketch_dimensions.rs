//! Circular dimension seeds and witnesses from the native solved snapshot.
//! No document state, geometry kernel or solver is implemented here.
use crate::sketch_commands::SketchElement;
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub enum Witness {
    Linear {
        start: [f64; 3],
        end: [f64; 3],
    },
    Arc {
        center: [f64; 3],
        start: [f64; 3],
        radius: f64,
        sweep: f64,
    },
}

pub struct Measurement {
    pub args: Value,
    pub witness: Witness,
}

fn vector(value: &Value) -> Option<[f64; 3]> {
    let a = value.as_array()?;
    if a.len() != 3 {
        return None;
    }
    let p = [a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?];
    p.iter().all(|n| n.is_finite()).then_some(p)
}

fn geometry(snapshot: &Value, index: i32) -> Option<&Value> {
    if index >= 0 {
        return snapshot["sketch"]["geometry"]
            .as_array()?
            .iter()
            .find(|g| g["index"].as_i64() == Some(index as i64));
    }
    snapshot["sketch"]["externals"]
        .as_array()?
        .iter()
        .find_map(|g| {
            let flat = i32::try_from(g["index"].as_i64()?).ok()?;
            let signed = i32::try_from(g["geometry_index"].as_i64()?).ok()?;
            (flat >= 0
                && (-3_i32).checked_sub(flat) == Some(signed)
                && signed == index
                && g["geometry"]["index"].as_i64() == Some(index as i64))
            .then_some(&g["geometry"])
        })
}

fn point(snapshot: &Value, e: SketchElement) -> Option<[f64; 3]> {
    if e == (SketchElement {
        index: -1,
        point: 1,
    }) {
        return Some([0.; 3]);
    }
    let g = geometry(snapshot, e.index)?;
    if let Some(handles) = g["handles"].as_array() {
        return handles
            .iter()
            .find(|h| {
                h.get("index").unwrap_or(&g["index"]).as_i64() == Some(e.index as i64)
                    && h["point"].as_i64() == Some(e.point as i64)
            })
            .and_then(|h| vector(&h["position"]));
    }
    let field = match (g["kind"].as_str()?, e.point) {
        ("point", 1) => "position",
        ("line" | "arc", 1) => "start",
        ("line" | "arc", 2) => "end",
        ("circle" | "arc", 3) => "center",
        _ => return None,
    };
    vector(&g[field])
}

fn circle(g: &Value) -> Option<([f64; 3], f64)> {
    if !matches!(g["kind"].as_str()?, "circle" | "arc") {
        return None;
    }
    let radius = g["radius"].as_f64()?;
    (radius.is_finite() && radius > 0.).then_some((vector(&g["center"])?, radius))
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3], scale: f64) -> [f64; 3] {
    [
        a[0] + b[0] * scale,
        a[1] + b[1] * scale,
        a[2] + b[2] * scale,
    ]
}
fn length(a: [f64; 3]) -> f64 {
    a[0].hypot(a[1])
}
fn direction(a: [f64; 3]) -> [f64; 3] {
    let l = length(a);
    if l <= 1e-12 {
        [1., 0., 0.]
    } else {
        [a[0] / l, a[1] / l, 0.]
    }
}

/// Returns None for existing non-circular forms, which retain their current semantics.
pub fn measure(
    snapshot: &Value,
    kind: &str,
    selection: &[SketchElement],
) -> Result<Option<Measurement>, String> {
    if !matches!(kind, "Distance" | "Angle") {
        return Ok(None);
    }
    if let [e] = selection
        && e.point == 0
        && let Some(g) = geometry(snapshot, e.index)
        && g["kind"] == "arc"
    {
        let center = vector(&g["center"]).ok_or("Native arc center is unavailable")?;
        let start = vector(&g["start"]).ok_or("Native arc start is unavailable")?;
        let (_, radius) = circle(g).ok_or("Native arc radius is unavailable")?;
        let first = g["first_parameter"]
            .as_f64()
            .ok_or("Native arc interval is unavailable")?;
        let last = g["last_parameter"]
            .as_f64()
            .ok_or("Native arc interval is unavailable")?;
        let sweep = last - first;
        if !sweep.is_finite() || sweep <= 0. || sweep > std::f64::consts::TAU + 1e-8 {
            return Err("Native arc interval is invalid".into());
        }
        let value = if kind == "Angle" {
            sweep
        } else {
            radius * sweep
        };
        if !value.is_finite() {
            return Err("Native arc measure is not finite".into());
        }
        let axis = vector(&g["axis"]).ok_or("Native arc orientation is unavailable")?;
        return Ok(Some(Measurement {
            args: json!([e.index, value]),
            witness: Witness::Arc {
                center,
                start,
                radius,
                sweep: sweep * axis[2].signum(),
            },
        }));
    }
    if kind != "Distance" {
        return Ok(None);
    }
    let &[mut first, mut second] = selection else {
        return Ok(None);
    };
    if first.point == 0 && second.point != 0 {
        std::mem::swap(&mut first, &mut second);
    }
    let a = geometry(snapshot, first.index);
    let b = geometry(snapshot, second.index);
    if first.point != 0
        && second.point == 0
        && let Some((center, radius)) = b.and_then(circle)
    {
        let p = point(snapshot, first).ok_or("Selected native point is unavailable")?;
        let offset = sub(p, center);
        let q = add(center, direction(offset), radius);
        return Ok(Some(Measurement {
            args: json!([
                first.index,
                first.point,
                second.index,
                (length(offset) - radius).abs()
            ]),
            witness: Witness::Linear { start: p, end: q },
        }));
    }
    if first.point != 0 || second.point != 0 {
        return Ok(None);
    }
    let (Some(mut a), Some(mut b)) = (a, b) else {
        return Ok(None);
    };
    if circle(a).is_none() && circle(b).is_some() {
        std::mem::swap(&mut a, &mut b);
        std::mem::swap(&mut first, &mut second);
    }
    let Some((center, radius)) = circle(a) else {
        return Ok(None);
    };
    if let Some((other, other_radius)) = circle(b) {
        let d = length(sub(other, center));
        let (start, end, value) = if d >= radius && d >= other_radius {
            let u = direction(sub(other, center));
            (
                add(center, u, radius),
                add(other, u, -other_radius),
                d - radius - other_radius,
            )
        } else {
            let (big, r_big, small, r_small) = if radius >= other_radius {
                (center, radius, other, other_radius)
            } else {
                (other, other_radius, center, radius)
            };
            let u = direction(sub(small, big));
            (
                add(big, u, r_big),
                add(small, u, r_small),
                r_big - r_small - d,
            )
        };
        return Ok(Some(Measurement {
            args: json!([first.index, second.index, value]),
            witness: Witness::Linear { start, end },
        }));
    }
    if b["kind"] == "line" {
        let start = vector(&b["start"]).ok_or("Native line start is unavailable")?;
        let end = vector(&b["end"]).ok_or("Native line end is unavailable")?;
        let u = sub(end, start);
        let l = length(u);
        if l <= 1e-12 {
            return Err("Selected line has zero length".into());
        }
        let v = sub(center, start);
        let foot = add(start, u, (v[0] * u[0] + v[1] * u[1]) / (l * l));
        let offset = sub(foot, center);
        let d = length(offset);
        // At the line's center crossing there is no unique nearest side.
        let toward = if d <= 1e-12 {
            [-u[1] / l, u[0] / l, 0.]
        } else {
            direction(offset)
        };
        return Ok(Some(Measurement {
            args: json!([first.index, second.index, d - radius]),
            witness: Witness::Linear {
                start: add(center, toward, radius),
                end: foot,
            },
        }));
    }
    Ok(None)
}

/// Decode the same native signature used by creation, including imported constraints.
pub fn annotation(snapshot: &Value, constraint: &Value) -> Option<Witness> {
    let kind = constraint["type"].as_str()?;
    let e = |index: &str, point: &str| SketchElement {
        index: constraint[index].as_i64().unwrap_or(-2000) as i32,
        point: constraint[point].as_i64().unwrap_or(0) as i32,
    };
    let mut selection = vec![e("first", "first_pos")];
    let second = e("second", "second_pos");
    if second.index != -2000 {
        selection.push(second);
    }
    // Via-point Angle is a separate native signature and must never be drawn
    // as an arc's span simply because one of its curves happens to be circular.
    if constraint["third"].as_i64().unwrap_or(-2000) != -2000 {
        return None;
    }
    measure(snapshot, kind, &selection)
        .ok()
        .flatten()
        .map(|m| m.witness)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn e(index: i32, point: i32) -> SketchElement {
        SketchElement { index, point }
    }
    fn fixture() -> Value {
        json!({"sketch":{"geometry":[
            {"index":0,"kind":"circle","center":[0,0,0],"radius":2},
            {"index":1,"kind":"circle","center":[10,0,0],"radius":3},
            {"index":2,"kind":"line","start":[-10,6,0],"end":[10,6,0]},
            {"index":3,"kind":"point","position":[8,0,0]},
            {"index":4,"kind":"arc","center":[0,0,0],"radius":5,"axis":[0,0,1],"start":[5,0,0],"first_parameter":0.0,"last_parameter":4.0}
        ]}})
    }
    #[test]
    fn seeds_and_witnesses_measure_clearance_not_centers() {
        let s = fixture();
        let m = measure(&s, "Distance", &[e(0, 0), e(1, 0)])
            .unwrap()
            .unwrap();
        assert_eq!(m.args, json!([0, 1, 5.]));
        assert_eq!(
            m.witness,
            Witness::Linear {
                start: [2., 0., 0.],
                end: [7., 0., 0.]
            }
        );
        let m = measure(&s, "Distance", &[e(2, 0), e(0, 0)])
            .unwrap()
            .unwrap();
        assert_eq!(m.args, json!([0, 2, 4.]));
        assert_eq!(
            m.witness,
            Witness::Linear {
                start: [0., 2., 0.],
                end: [0., 6., 0.]
            }
        );
        let m = measure(&s, "Distance", &[e(0, 0), e(3, 1)])
            .unwrap()
            .unwrap();
        assert_eq!(m.args, json!([3, 1, 0, 6.]));
        assert_eq!(
            m.witness,
            Witness::Linear {
                start: [8., 0., 0.],
                end: [2., 0., 0.]
            }
        );
    }
    #[test]
    fn arcs_use_native_major_sweep_and_clockwise_orientation() {
        let mut s = fixture();
        assert_eq!(
            measure(&s, "Distance", &[e(4, 0)]).unwrap().unwrap().args,
            json!([4, 20.])
        );
        assert_eq!(
            measure(&s, "Angle", &[e(4, 0)]).unwrap().unwrap().args,
            json!([4, 4.])
        );
        s["sketch"]["geometry"][4]["axis"] = json!([0, 0, -1]);
        assert!(
            matches!(measure(&s,"Angle",&[e(4,0)]).unwrap().unwrap().witness,Witness::Arc{sweep,..} if sweep == -4.)
        );
    }
    #[test]
    fn contained_and_overlapping_circle_signs_match_native() {
        let mut s = fixture();
        s["sketch"]["geometry"][0]["radius"] = json!(10.);
        s["sketch"]["geometry"][1]["center"] = json!([2, 0, 0]);
        let m = measure(&s, "Distance", &[e(0, 0), e(1, 0)])
            .unwrap()
            .unwrap();
        assert_eq!(m.args, json!([0, 1, 5.]));
        assert_eq!(
            m.witness,
            Witness::Linear {
                start: [10., 0., 0.],
                end: [5., 0., 0.]
            }
        );
        s["sketch"]["geometry"][0]["radius"] = json!(2.);
        s["sketch"]["geometry"][1]["center"] = json!([4, 0, 0]);
        assert_eq!(
            measure(&s, "Distance", &[e(0, 0), e(1, 0)])
                .unwrap()
                .unwrap()
                .args,
            json!([0, 1, -1.])
        );
    }
    #[test]
    fn existing_point_center_distance_and_via_point_angles_keep_existing_paths() {
        let s = fixture();
        assert!(
            measure(&s, "Distance", &[e(0, 3), e(1, 3)])
                .unwrap()
                .is_none()
        );
        assert!(annotation(&s,&json!({"type":"Angle","first":4,"first_pos":0,"second":2,"second_pos":0,"third":3,"third_pos":1})).is_none());
    }
}
