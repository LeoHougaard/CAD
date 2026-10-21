//! Click interpretation for native conics and splines. The native solver owns
//! the resulting geometry; these values only describe the user's construction.
use crate::sketch_commands::Tool;
use serde_json::{Value, json};
use std::f64::consts::TAU;

const EPS: f64 = 1e-9;

pub fn advanced(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Ellipse
            | Tool::EllipseArc
            | Tool::BSpline
            | Tool::PeriodicBSpline
            | Tool::HyperbolaArc
            | Tool::ParabolaArc
    )
}

pub(crate) fn draw(
    object: &str,
    tool: Tool,
    points: &[[f64; 3]],
    construction: bool,
) -> Result<Vec<Value>, String> {
    let spline = matches!(tool, Tool::BSpline | Tool::PeriodicBSpline);
    let minimum = if tool == Tool::PeriodicBSpline {
        3
    } else {
        tool.required_points()
    };
    if points.len() < minimum || !spline && points.len() != minimum {
        return Err(format!(
            "{tool:?} needs {} points",
            if spline {
                format!("at least {minimum}")
            } else {
                minimum.to_string()
            }
        ));
    }
    let mut edit = json!({"op":"geometry","object":object,"construction":construction});
    if spline {
        if !points.iter().any(|p| distance(points[0], *p) > EPS) {
            return Err("Choose distinct spline poles".into());
        }
        edit["kind"] = json!("bspline");
        edit["poles"] = json!(points);
        edit["periodic"] = json!(tool == Tool::PeriodicBSpline);
    } else if tool == Tool::ParabolaArc {
        let center = points[0];
        let focus = points[1];
        let focal = distance(center, focus);
        if focal <= EPS {
            return Err("The focus must differ from the parabola vertex".into());
        }
        let u = [
            (focus[0] - center[0]) / focal,
            (focus[1] - center[1]) / focal,
        ];
        let v = [-u[1], u[0]];
        let parameter = |p: [f64; 3]| (p[0] - center[0]) * v[0] + (p[1] - center[1]) * v[1];
        let (first, last) = ordered(parameter(points[2]), parameter(points[3]))?;
        edit["kind"] = json!("parabola_arc");
        edit["center"] = json!(center);
        edit["focal"] = json!(focal);
        edit["angle"] = json!(u[1].atan2(u[0]));
        edit["start_parameter"] = json!(first);
        edit["end_parameter"] = json!(last);
    } else {
        let center = points[0];
        let major = points[1];
        let a = distance(center, major);
        if a <= EPS {
            return Err("Choose a nonzero major axis".into());
        }
        let u = [(major[0] - center[0]) / a, (major[1] - center[1]) / a];
        let mut v = [-u[1], u[0]];
        let signed_b = (points[2][0] - center[0]) * v[0] + (points[2][1] - center[1]) * v[1];
        let b = signed_b.abs();
        if b <= EPS {
            return Err("Choose a nonzero minor axis".into());
        }
        if tool != Tool::HyperbolaArc && b > a {
            return Err("The minor radius must not exceed the major radius".into());
        }
        if signed_b < 0. {
            v = [-v[0], -v[1]];
        }
        let minor = [center[0] + b * v[0], center[1] + b * v[1], 0.];
        edit["center"] = json!(center);
        edit["major"] = json!(major);
        edit["minor"] = json!(minor);
        edit["kind"] = json!(match tool {
            Tool::Ellipse => "ellipse",
            Tool::EllipseArc => "ellipse_arc",
            _ => "hyperbola_arc",
        });
        if tool != Tool::Ellipse {
            let parameter = |point: [f64; 3]| {
                let d = [point[0] - center[0], point[1] - center[1]];
                let y = (d[0] * v[0] + d[1] * v[1]) / b;
                if tool == Tool::HyperbolaArc {
                    y.asinh()
                } else {
                    y.atan2((d[0] * u[0] + d[1] * u[1]) / a)
                }
            };
            let first = parameter(points[3]);
            let mut last = parameter(points[4]);
            let (first, last) = if tool == Tool::EllipseArc {
                if (last - first).abs() < EPS {
                    return Err("Choose distinct arc endpoints".into());
                }
                while last <= first {
                    last += TAU;
                }
                (first, last)
            } else {
                ordered(first, last)?
            };
            edit["start_parameter"] = json!(first);
            edit["end_parameter"] = json!(last);
        }
    }
    Ok(vec![edit])
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}
fn ordered(a: f64, b: f64) -> Result<(f64, f64), String> {
    if !a.is_finite() || !b.is_finite() || (a - b).abs() < EPS {
        Err("Choose distinct arc endpoints".into())
    } else {
        Ok((a.min(b), a.max(b)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conic_clicks_project_the_minor_axis_and_keep_native_orientation() {
        let e = draw(
            "Sketch",
            Tool::Ellipse,
            &[[1., 2., 0.], [6., 2., 0.], [3., -1., 0.]],
            false,
        )
        .unwrap();
        assert_eq!(e[0]["major"], json!([6., 2., 0.]));
        assert_eq!(e[0]["minor"], json!([1., -1., 0.]));
        assert!(
            draw(
                "Sketch",
                Tool::Ellipse,
                &[[0.; 3], [1., 0., 0.], [0., 2., 0.]],
                false
            )
            .is_err()
        );
    }
    #[test]
    fn open_conics_order_parameters_and_splines_preserve_all_poles() {
        let p = draw(
            "Sketch",
            Tool::ParabolaArc,
            &[[0.; 3], [2., 0., 0.], [1., 3., 0.], [1., -2., 0.]],
            true,
        )
        .unwrap();
        assert_eq!(p[0]["focal"], 2.);
        assert_eq!(p[0]["start_parameter"], -2.);
        assert_eq!(p[0]["end_parameter"], 3.);
        let poles = [
            [0.; 3],
            [1., 3., 0.],
            [2., 1., 0.],
            [3., 0., 0.],
            [4., 2., 0.],
        ];
        let b = draw("Sketch", Tool::BSpline, &poles, false).unwrap();
        assert_eq!(b[0]["poles"], json!(poles));
        assert_eq!(b[0]["periodic"], false);
    }
}
