//! Decodes derived sketch display samples and native solver handles.
//!
//! Samples are a display cache. Geometry and solver identities remain native.
use crate::sketch_commands::SketchElement;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativeHandle {
    pub id: SketchElement,
    pub position: [f64; 3],
    /// This must be explicitly granted by the native snapshot. External handles
    /// can be constrained against, but they are never local drag targets.
    pub draggable: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativeCurve {
    pub index: i32,
    pub kind: String,
    pub points: Vec<[f64; 3]>,
    pub handles: Vec<NativeHandle>,
    pub construction: bool,
    pub external: bool,
    pub defining: bool,
    pub intersection: bool,
    /// Preserve native source metadata verbatim. Display code must not rebuild
    /// attachment/projection ownership from a detached mesh or numerical index.
    pub reference: Option<Value>,
}

fn signed_index(value: &Value) -> Option<i32> {
    i32::try_from(value.as_i64()?).ok()
}
fn point(value: &Value) -> Option<[f64; 3]> {
    let values = value.as_array()?;
    if values.len() != 3 {
        return None;
    }
    let point = [
        values[0].as_f64()?,
        values[1].as_f64()?,
        values[2].as_f64()?,
    ];
    point.iter().all(|value| value.is_finite()).then_some(point)
}

impl NativeCurve {
    /// Use only native curve samples and verified native solver IDs. In
    /// particular, never number B-spline poles or external endpoints locally.
    pub fn from_snapshot(entry: &Value) -> Option<Self> {
        if entry.get("points").is_none() {
            return Self::legacy_snapshot(entry);
        }
        let index = signed_index(&entry["index"])?;
        // Native horizontal/vertical axes and the origin have separate viewport
        // handling. Projected geometry begins at solver index -3.
        if matches!(index, -1 | -2) {
            return None;
        }
        let external = entry["external"].as_bool().unwrap_or(index <= -3);
        if external != (index <= -3) {
            return None;
        }
        let samples = entry["points"].as_array()?;
        let points: Vec<_> = samples.iter().map(point).collect::<Option<_>>()?;
        if points.is_empty() {
            return None;
        }
        let mut handles = vec![];
        for handle in entry["handles"].as_array().into_iter().flatten() {
            let point_id = signed_index(&handle["point"])?;
            if point_id <= 0 {
                return None;
            }
            let id = SketchElement {
                index: match handle.get("index") {
                    Some(value) => signed_index(value)?,
                    None => index,
                },
                point: point_id,
            };
            if matches!(id.index, -1 | -2) {
                return None;
            }
            // An external reference cannot grant local drag access through an
            // accidentally positive internal/alignment index in display data.
            let draggable = !external && id.index >= 0 && handle["draggable"] == true;
            let position = point(&handle["position"])?;
            if let Some(previous) = handles
                .iter()
                .find(|previous: &&NativeHandle| previous.id == id)
            {
                if previous.position != position {
                    return None;
                }
                continue;
            }
            handles.push(NativeHandle {
                id,
                position,
                draggable,
            });
        }
        Some(Self {
            index,
            kind: entry["kind"].as_str()?.to_owned(),
            points,
            handles,
            construction: entry["construction"].as_bool().unwrap_or(false),
            external,
            defining: entry["defining"].as_bool().unwrap_or(false),
            intersection: entry["intersection"].as_bool().unwrap_or(false),
            reference: entry
                .get("reference")
                .filter(|value| !value.is_null())
                .cloned(),
        })
    }
    pub fn from_external(wrapper: &Value) -> Option<Self> {
        let index = signed_index(&wrapper["geometry_index"])?;
        let flat_index = signed_index(&wrapper["index"])?;
        if flat_index < 0 || index != (-3_i32).checked_sub(flat_index)? {
            return None;
        }
        let mut curve = Self::from_snapshot(&wrapper["geometry"])?;
        if curve.index != index || !curve.external {
            return None;
        }
        curve.defining = wrapper["defining"].as_bool().unwrap_or(false);
        curve.intersection = wrapper["intersection"].as_bool().unwrap_or(false);
        curve.reference = wrapper
            .get("persistent_reference")
            .filter(|v| !v.is_null())
            .cloned();
        Some(curve)
    }

    // Compatibility with the original simple-geometry snapshot, before native
    // display samples and explicit handles were added to the protocol.
    fn legacy_snapshot(entry: &Value) -> Option<Self> {
        use std::f64::consts::TAU;
        let index = signed_index(&entry["index"])?;
        let mut points = vec![];
        let mut handles = vec![];
        let mut handle = |id, position: [f64; 3]| {
            handles.push(serde_json::json!({
            "index":index,"point":id,"position":position,"draggable":index>=0}))
        };
        match entry["kind"].as_str()? {
            "point" => {
                let p = point(&entry["position"])?;
                points.push(p);
                handle(1, p);
            }
            "line" => {
                let a = point(&entry["start"])?;
                let b = point(&entry["end"])?;
                points.extend([a, b]);
                handle(1, a);
                handle(2, b);
            }
            kind @ ("circle" | "arc") => {
                let center = point(&entry["center"])?;
                let radius = entry["radius"]
                    .as_f64()
                    .filter(|r| r.is_finite() && *r > 0.)?;
                let (start, sweep) = if kind == "circle" {
                    (0., TAU)
                } else {
                    let a = point(&entry["start"])?;
                    let b = point(&entry["end"])?;
                    let m = point(&entry["mid"])?;
                    handle(1, a);
                    handle(2, b);
                    let start = (a[1] - center[1]).atan2(a[0] - center[0]);
                    let end = ((b[1] - center[1]).atan2(b[0] - center[0]) - start).rem_euclid(TAU);
                    let mid = ((m[1] - center[1]).atan2(m[0] - center[0]) - start).rem_euclid(TAU);
                    (start, if mid <= end { end } else { end - TAU })
                };
                handle(3, center);
                let steps = (sweep.abs() * 20.).ceil().max(8.) as usize;
                for i in 0..=steps {
                    let a = start + sweep * i as f64 / steps as f64;
                    points.push([
                        center[0] + a.cos() * radius,
                        center[1] + a.sin() * radius,
                        center[2],
                    ]);
                }
            }
            _ => return None,
        }
        let mut expanded = entry.clone();
        expanded["points"] = serde_json::json!(points);
        expanded["handles"] = serde_json::json!(handles);
        Self::from_snapshot(&expanded)
    }
}

pub(super) fn native_handle<'a>(
    curves: impl IntoIterator<Item = &'a NativeCurve>,
    id: SketchElement,
) -> Option<&'a NativeHandle> {
    curves
        .into_iter()
        .flat_map(|curve| &curve.handles)
        .find(|handle| handle.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn native_alignment_geometry_handles_keep_solver_ids_and_full_precision() {
        let data = json!({"index":9,"kind":"circle","points":[[0,0,0],[1.23456789012345,2,0],[4,0,0]],
            "handles":[{"index":9,"point":3,"position":[1.23456789012345,2,0],"draggable":true}],"construction":true});
        let curve = NativeCurve::from_snapshot(&data).unwrap();
        assert_eq!(curve.index, 9);
        assert!(curve.construction);
        let curves = [curve];
        let handle = native_handle(&curves, SketchElement { index: 9, point: 3 }).unwrap();
        assert_eq!(handle.position[0], 1.23456789012345);
        assert!(handle.draggable);
        assert!(native_handle(&curves, SketchElement { index: 4, point: 3 }).is_none());
    }
    #[test]
    fn negative_external_ids_and_native_reference_tokens_are_preserved() {
        let token = json!({"object":"Pad","subelement":"Edge7","mapped_name":"native-mapped-name","projection":"intersection"});
        let curve = NativeCurve::from_snapshot(
            &json!({"index":-5,"kind":"ellipse_arc","points":[[1,0,0],[0,2,0]],
            "handles":[{"point":1,"position":[1,0,0],"draggable":true}],"external":true,
            "defining":true,"intersection":true,"reference":token}),
        )
        .unwrap();
        assert_eq!(curve.index, -5);
        assert!(curve.external && curve.defining && curve.intersection);
        assert_eq!(
            curve.handles[0].id,
            SketchElement {
                index: -5,
                point: 1
            }
        );
        assert!(!curve.handles[0].draggable);
        assert_eq!(curve.reference, Some(token));
    }
    #[test]
    fn display_points_are_not_converted_to_invented_pole_or_endpoint_handles() {
        for kind in [
            "ellipse",
            "ellipse_arc",
            "bspline",
            "hyperbola_arc",
            "parabola_arc",
        ] {
            let curve = NativeCurve::from_snapshot(
                &json!({"index":0,"kind":kind,"points":[[0,0,0],[1,1,0],[2,0,0]]}),
            )
            .unwrap();
            assert!(curve.handles.is_empty());
            assert_eq!(curve.points.len(), 3);
        }
    }
    #[test]
    fn invalid_ids_and_incomplete_native_samples_are_rejected() {
        for data in [
            json!({"index":-1,"kind":"line","points":[[0,0,0],[1,0,0]]}),
            json!({"index":-2,"kind":"line","points":[[0,0,0],[1,0,0]]}),
            json!({"index":u64::MAX,"kind":"bspline","points":[[0,0,0],[1,0,0]]}),
            json!({"index":0,"external":true,"kind":"line","points":[[0,0,0],[1,0,0]]}),
            json!({"index":0,"kind":"ellipse","points":[[0,0],[1,0,0]]}),
            json!({"index":0,"kind":"bspline","points":[[0,0,0]],"handles":[{"point":0,"position":[0,0,0]}]}),
            json!({"index":0,"kind":"bspline","points":[[0,0,0]],"handles":[{"index":"bad","point":1,"position":[0,0,0]}]}),
            json!({"index":0,"kind":"bspline","points":[[0,0,0]],"handles":[{"index":-1,"point":1,"position":[0,0,0]}]}),
            json!({"index":0,"kind":"bspline","points":[[0,0,0]],"handles":[{"point":3,"position":[0,0,0]},{"point":3,"position":[1,0,0]}]}),
        ] {
            assert!(NativeCurve::from_snapshot(&data).is_none(), "{data}");
        }
    }
    #[test]
    fn flattened_external_cache_ids_are_distinct_from_source_link_indices() {
        let mut wrapper = serde_json::json!({"index":1,"geometry_index":-4,"source_index":25,"source":null,
            "persistent_reference":"Source.;g17;SKT","defining":true,"intersection":null,
            "geometry":{"index":-4,"kind":"line","construction":true,"points":[[0,0,0],[1,0,0]],
                "handles":[{"index":-4,"point":1,"position":[0,0,0],"draggable":false}]}});
        let curve = NativeCurve::from_external(&wrapper).unwrap();
        assert_eq!(curve.index, -4);
        assert!(curve.defining && curve.external);
        assert_eq!(curve.reference, Some(serde_json::json!("Source.;g17;SKT")));
        wrapper["geometry_index"] = serde_json::json!(-28);
        assert!(NativeCurve::from_external(&wrapper).is_none());
        wrapper["geometry_index"] = serde_json::json!(-4);
        wrapper["geometry"]["index"] = serde_json::json!(0);
        assert!(NativeCurve::from_external(&wrapper).is_none());
    }
}
