use super::*;
use eframe::egui::{Pos2, Rect, Vec2};
use serde_json::json;

fn fixture() -> Viewport {
    let mut v = Viewport {
        rect: Rect::from_min_size(Pos2::new(20., 30.), Vec2::new(800., 500.)),
        ..Default::default()
    };
    let scene = |name: &str, x: f32| {
        (
            json!({"object":name,"revision":1,"world_from_shape":{"position":[x,20,30]},
            "positions":[[0,0,0],[20,0,0],[20,10,0],[0,10,0]],
            "triangles":[[0,1,2],[0,2,3]],
            "faces":[{"subelement":"Face7","first_triangle":0,"triangle_count":2}]}),
            json!({"world_from_shape":{"position":[x,20,30]},
            "edges":[{"subelement":"Edge4","points":[[0,0,0],[20,0,0]]}],
            "vertices":[{"subelement":"Vertex3","point":[20,10,0]}]}),
        )
    };
    v.set_scenes(vec![scene("Near", 100.), scene("Far", 1000.)]);
    v
}

fn select(v: &mut Viewport, object: &str, subelement: &str) {
    v.selected = Some(Selection {
        object: object.into(),
        subelement: subelement.into(),
    });
    v.selections = vec![v.selected.clone().unwrap()];
}

#[test]
fn fit_selection_frames_native_face_in_each_view_without_other_objects() {
    for view in [
        StandardView::Top,
        StandardView::Front,
        StandardView::Left,
        StandardView::Isometric,
    ] {
        let mut v = fixture();
        v.set_view(view);
        select(&mut v, "Near", "Face7");
        let selected = v.selections.clone();
        let rotation = v.rotation;
        assert!(v.fit_selection(None));
        assert_eq!(v.center, Vec3::new(110., 25., 30.));
        for t in v.triangles.iter().filter(|t| t.object.as_ref() == "Near") {
            for point in t.points {
                assert!(v.rect.shrink(20.).contains(v.project(point).0));
            }
        }
        assert_eq!(v.rotation, rotation);
        assert_eq!(v.selections, selected);
    }
}

#[test]
fn fit_selection_uses_exact_native_edge_vertex_or_whole_object() {
    let mut v = fixture();
    v.top();
    select(&mut v, "Near", "Edge4");
    assert!(v.fit_selection(None));
    assert_eq!(v.center, Vec3::new(110., 20., 30.));
    select(&mut v, "Near", "Vertex3");
    assert!(v.fit_selection(None));
    assert_eq!(v.center, Vec3::new(120., 30., 30.));
    assert_eq!(v.scale, 8.);
    v.selections.clear();
    v.selected = None;
    assert!(v.fit_selection(Some("Near")));
    assert_eq!(v.center, Vec3::new(110., 25., 30.));
}

#[test]
fn fit_selection_unions_current_picks_and_never_substitutes_stale_identity() {
    let mut v = fixture();
    select(&mut v, "Near", "Face7");
    v.selections.push(Selection {
        object: "Far".into(),
        subelement: "Edge4".into(),
    });
    assert!(v.fit_selection(None));
    assert_eq!(v.center, Vec3::new(560., 25., 30.));
    select(&mut v, "Missing", "Face7");
    let camera = (v.center, v.scale, v.rotation);
    assert!(!v.can_fit_selection(Some("Near")));
    assert!(!v.fit_selection(Some("Near")));
    assert_eq!((v.center, v.scale, v.rotation), camera);
    select(&mut v, "Near", "Face999");
    assert!(!v.fit_selection(None));
    v.selections.clear();
    v.selected = None;
    assert!(!v.fit_selection(None));
}

#[test]
fn fit_selection_uses_current_world_frame_and_bounded_native_reference_display() {
    let mut v = fixture();
    v.set_scenes(vec![]);
    v.set_references(&json!({"revision":2,"objects":[
        {"name":"Plane","properties":{"Visibility":true},"reference_display":{
            "kind":"plane","placement":{"position":[50,60,70],"rotation":[0,0,0,1]},
            "size":[20,40],"subelement":"Face1"}},
        {"name":"Hidden","properties":{"Visibility":false},"reference_display":{
            "kind":"point","placement":{"position":[500,600,700]}}}]}));
    select(&mut v, "Plane", "Face1");
    assert!(v.fit_selection(None));
    assert_eq!(v.center, Vec3::new(50., 60., 70.));
    select(&mut v, "Hidden", "");
    assert!(!v.fit_selection(None));
    select(&mut v, "Plane", "Face2");
    assert!(!v.fit_selection(None));
}

#[test]
fn fit_selection_rejects_invalid_viewport_without_camera_or_selection_changes() {
    let mut v = fixture();
    select(&mut v, "Near", "Face7");
    v.rect = Rect::NOTHING;
    let before = (v.center, v.scale, v.selected.clone(), v.selections.clone());
    assert!(!v.fit_selection(None));
    assert_eq!(
        (v.center, v.scale, v.selected.clone(), v.selections.clone()),
        before
    );
}
