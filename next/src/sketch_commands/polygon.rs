//! One local-plane construction shared by preview and native payload generation.
use super::*;

/// The second click is a vertex (inscribed) or a side midpoint (circumscribed).
/// Returned vertices are counterclockwise; closure uses the first vertex exactly.
pub fn regular_polygon(center: [f64; 3], rim: [f64; 3], sides: u32,
    circumscribed: bool) -> Result<(Vec<[f64; 3]>, f64), String> {
    finite_point(&center)?;
    finite_point(&rim)?;
    if !(3..=64).contains(&sides) { return Err("Polygon sides must be between 3 and 64".into()); }
    let d = delta(center, rim);
    let input_radius = nonzero(length(d), "A polygon needs a positive radius")?;
    let half = std::f64::consts::PI / f64::from(sides);
    let radius = if circumscribed { input_radius / half.cos() } else { input_radius };
    nonzero(radius, "Polygon radius is too large")?;
    let angle = d[1].atan2(d[0]) - if circumscribed { half } else { 0. };
    let vertices: Vec<_> = (0..sides).map(|i| {
        let a = angle + TAU * f64::from(i) / f64::from(sides);
        [center[0] + radius * a.cos(), center[1] + radius * a.sin(), 0.]
    }).collect();
    for i in 0..vertices.len() {
        finite_point(&vertices[i])?;
        nonzero(length(delta(vertices[i], vertices[(i + 1) % vertices.len()])),
            "Polygon vertices must be distinct")?;
    }
    Ok((vertices, radius))
}

pub(super) fn draw(object: &str, points: &[[f64; 3]], count: usize,
    construction: bool, sides: u32, circumscribed: bool) -> Result<Vec<Value>, String> {
    if points.len() != 2 { return Err("A polygon needs center and radius clicks".into()); }
    let (v, radius) = regular_polygon(points[0], points[1], sides, circumscribed)?;
    let circle = i32::try_from(count).map_err(|_| "Native geometry index is too large")?;
    circle.checked_add(sides as i32 + 1).ok_or("Native geometry index is too large")?;
    let mut edits = vec![json!({"op":"geometry","object":object,"kind":"circle",
        "center":points[0],"radius":radius,"construction":true})];
    for i in 0..v.len() {
        edits.push(json!({"op":"geometry","object":object,"kind":"line",
            "start":v[i],"end":v[(i + 1) % v.len()],"construction":construction}));
    }
    for i in 0..sides as i32 {
        let line = circle + 1 + i;
        edits.push(constraint(object,"Coincident",json!([line,2,circle+1+(i+1)%sides as i32,1])));
        edits.push(constraint(object,"PointOnObject",json!([line,1,circle])));
        // n-1 equalities: the final side must not add a redundant equality.
        if i > 0 { edits.push(constraint(object,"Equal",json!([circle+1,line]))); }
    }
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_modes_are_regular_and_share_the_requested_radius_direction() {
        for sides in [3,4,5,6,11,64] {
            for mode in [false,true] {
                let c = [11.,-9.,0.]; let p = [14.,-5.,0.];
                let (v,r) = regular_polygon(c,p,sides,mode).unwrap();
                let edge = length(delta(v[0],v[1]));
                for i in 0..v.len() {
                    assert!((length(delta(c,v[i]))-r).abs()<1e-10);
                    assert!((length(delta(v[i],v[(i+1)%v.len()]))-edge).abs()<1e-10);
                }
                let anchor = if mode { [(v[0][0]+v[1][0])/2.,(v[0][1]+v[1][1])/2.,0.] } else {v[0]};
                assert!(length(delta(anchor,p))<1e-10);
            }
        }
    }
    #[test]
    fn payload_is_closed_editable_and_uses_offset_native_ids() {
        for sides in [3,6,64] {
            for mode in [false,true] { for construction in [false,true] {
                let tool = Tool::RegularPolygon {sides,circumscribed:mode};
                let e = draw_snapped_edits("Sketch",tool,&[[0.;3],[5.,0.,0.]],7,construction,
                    &[Some(SketchElement{index:-1,point:1}),None]).unwrap();
                assert_eq!(e[0]["construction"],true);
                assert_eq!(e.len(),4*sides as usize+1);
                for i in 0..sides as usize {
                    assert_eq!(e[i+1]["construction"],construction);
                    assert_eq!(e[i+1]["end"],e[(i+1)%sides as usize+1]["start"]);
                }
                assert_eq!(e.last().unwrap()["args"],json!([7,3,-1,1]));
                assert!(!e.iter().any(|e| e["kind"]=="Block"));
                assert_eq!(e.iter().filter(|e|e["kind"]=="Equal").count(),sides as usize-1);
            }}
        }
    }
    #[test]
    fn invalid_inputs_never_produce_partial_payloads() {
        for sides in [0,2,65,u32::MAX] { assert!(regular_polygon([0.;3],[1.,0.,0.],sides,false).is_err()); }
        for p in [[0.;3],[f64::NAN,0.,0.],[f64::INFINITY,0.,0.],[1.,0.,1.]] {
            assert!(regular_polygon([0.;3],p,6,false).is_err());
        }
        assert!(draw_edits("Sketch",Tool::RegularPolygon{sides:6,circumscribed:false},
            &[[0.;3],[1.,0.,0.]],i32::MAX as usize,false).is_err());
    }
}

#[cfg(test)]
mod snap_tests {
    use super::*;
    #[test]
    fn radius_snap_means_vertex_only_in_inscribed_mode_and_center_accepts_edge() {
        for mode in [false,true] {
            let tool=Tool::RegularPolygon{sides:7,circumscribed:mode};
            let e=draw_snapped_edits("Sketch",tool,&[[0.;3],[5.,0.,0.]],9,false,
                &[Some(SketchElement{index:1,point:0}),Some(SketchElement{index:2,point:1})]).unwrap();
            let snap_count=if mode {1} else {2};
            assert_eq!(e.len(),28+snap_count);
            assert_eq!(e[28]["args"],json!([9,3,1]));
            if !mode {assert_eq!(e[29]["args"],json!([10,1,2,1]));}
        }
    }
}
