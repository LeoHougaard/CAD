//! Black-box viewport interaction tests, also compiled against the old viewport.
use super::*;
use serde_json::json;

pub(super) fn scene(object: &str, height: f32) -> (Value, Value) {
    (
        json!({"object":object,"revision":1,
        "world_from_shape":{"position":[0,0,height]},
        "positions":[[0,0,0],[10,0,0],[10,10,0],[0,10,0]],
        "triangles":[[0,1,2],[0,2,3]],
        "faces":[{"subelement":"Face7","first_triangle":0,"triangle_count":2}]}),
        json!({"world_from_shape":{"position":[0,0,height]},
        "faces":[{"subelement":"Face7"}],
        "edges":[{"subelement":"Edge4","points":[[0,2,0],[8,2,0]]}],
        "vertices":[{"subelement":"Vertex3","point":[2,2,0]}]}),
    )
}

pub(super) fn viewport() -> Viewport {
    let mut v = Viewport::default();
    v.top();
    v.scale = 10.;
    let (mesh, _) = scene("Front", 10.);
    v.set_scenes(vec![
        scene("Rear", 0.),
        (mesh, json!({"faces":[{"subelement":"Face7"}]})),
    ]);
    v
}

fn frame(
    v: &mut Viewport,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    enabled: bool,
) -> egui::FullOutput {
    let modifiers = events
        .iter()
        .find_map(|event| match event {
            egui::Event::PointerButton { modifiers, .. } | egui::Event::Key { modifiers, .. } => {
                Some(*modifiers)
            }
            _ => None,
        })
        .unwrap_or_default();
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
            events,
            modifiers,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    v.ui(ui);
                });
            });
        },
    )
}

fn pointer(
    v: &mut Viewport,
    ctx: &egui::Context,
    pos: Pos2,
    button: egui::PointerButton,
    shift: bool,
) {
    for pressed in [true, false] {
        frame(
            v,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: egui::Modifiers {
                        shift,
                        ..Default::default()
                    },
                },
            ],
            true,
        );
    }
}

fn open(v: &mut Viewport, ctx: &egui::Context, point: Vec3) {
    frame(v, ctx, vec![], true);
    let position = v.project(point).0;
    pointer(v, ctx, position, egui::PointerButton::Secondary, false);
    for _ in 0..3 {
        frame(v, ctx, vec![], true);
    }
}

fn text_rect(output: &egui::FullOutput, text: &str) -> Rect {
    let hits: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            Shape::Text(t) if t.galley.text() == text => {
                Some(t.galley.rect.translate(t.pos.to_vec2()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(hits.len(), 1, "Expected one visible control {text}");
    hits[0]
}

#[test]
fn chooser_selects_hidden_native_face_and_shift_toggles_it() {
    let mut v = viewport();
    let ctx = egui::Context::default();
    let point = Vec3::new(5., 5., 0.);
    let front = Selection {
        object: "Front".into(),
        subelement: "Face7".into(),
    };
    let rear = Selection {
        object: "Rear".into(),
        subelement: "Face7".into(),
    };
    frame(&mut v, &ctx, vec![], true);
    let position = v.project(point).0;
    pointer(&mut v, &ctx, position, egui::PointerButton::Primary, false);
    assert_eq!(v.selected, Some(front.clone()));
    open(&mut v, &ctx, point);
    assert_eq!(v.selected, Some(front.clone()));
    let output = frame(&mut v, &ctx, vec![], true);
    text_rect(&output, "Select other");
    let target = text_rect(&output, "Rear / Face7").center();
    pointer(&mut v, &ctx, target, egui::PointerButton::Primary, false);
    assert_eq!(v.selected, Some(rear.clone()));
    assert_eq!(v.selections, vec![rear.clone()]);
    assert!(v.topology_references("Rear").contains(&"Face7".to_owned()));
    open(&mut v, &ctx, point);
    let output = frame(&mut v, &ctx, vec![], true);
    let target = text_rect(&output, "Rear / Face7").center();
    pointer(&mut v, &ctx, target, egui::PointerButton::Primary, true);
    assert!(v.selected.is_none());
    assert!(v.selections.is_empty());
}

#[test]
fn chooser_escape_and_disabled_ui_preserve_prior_selection() {
    let mut v = viewport();
    let ctx = egui::Context::default();
    let selected = Selection {
        object: "Front".into(),
        subelement: "Face7".into(),
    };
    v.select_native_element(Some(selected.clone()), false);
    open(&mut v, &ctx, Vec3::new(5., 5., 0.));
    let output = frame(&mut v, &ctx, vec![], true);
    text_rect(&output, "Rear / Face7");
    frame(
        &mut v,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        true,
    );
    frame(&mut v, &ctx, vec![], true);
    assert_eq!(v.selected, Some(selected.clone()));
    assert_eq!(v.selections, vec![selected.clone()]);
    open(&mut v, &ctx, Vec3::new(5., 5., 0.));
    let output = frame(&mut v, &ctx, vec![], false);
    assert!(
        !output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape, Shape::Text(t) if t.galley.text()=="Rear / Face7"))
    );
    assert_eq!(v.selected, Some(selected));
}

#[test]
fn chooser_vertices_and_edges_keep_native_names() {
    let mut v = viewport();
    let ctx = egui::Context::default();
    for (mode, name) in [
        (SelectionMode::Edges, "Edge4"),
        (SelectionMode::Vertices, "Vertex3"),
    ] {
        v.selection_mode = mode;
        open(&mut v, &ctx, Vec3::new(2., 2., 0.));
        let output = frame(&mut v, &ctx, vec![], true);
        let target = text_rect(&output, &format!("Rear / {name}")).center();
        pointer(&mut v, &ctx, target, egui::PointerButton::Primary, false);
        assert_eq!(
            v.selected,
            Some(Selection {
                object: "Rear".into(),
                subelement: name.into()
            })
        );
    }
}
