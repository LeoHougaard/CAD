//! Choose obscured native elements without changing the native document.
use super::{
    Color32, Pos2, ProjectedTriangle, Quat, Rect, Selection, SelectionMode, Shape, Stroke, Vec3,
    Viewport, egui, segment_distance, triangle_depth,
};
use std::collections::BTreeMap;

pub(super) struct Picker {
    center: Vec3,
    rotation: Quat,
    scale: f32,
    rect: Rect,
    mode: SelectionMode,
    choices: Vec<Selection>,
}

struct Hit {
    selection: Selection,
    depth: f32,
    distance: f32,
    kind: u8,
}

impl Viewport {
    /// Dismiss this viewport's popup before an operation-level Escape handler.
    /// Do not close a different menu or imply cancellation of native work.
    pub fn dismiss_other_menu(&mut self, ctx: &egui::Context) -> bool {
        let Some(response) = self.response.as_ref() else {
            return false;
        };
        let id = egui::Popup::default_response_id(response);
        if !egui::Popup::is_id_open(ctx, id) {
            return false;
        }
        egui::Popup::close_id(ctx, id);
        self.cancel_other_picker();
        true
    }

    pub(super) fn cancel_other_picker(&mut self) {
        self.other_picker = None;
        self.other_hover = None;
    }

    pub(super) fn validate_other_picker(&mut self, enabled: bool) {
        let valid = enabled
            && (self.sketch.is_none() || self.solid_select_in_sketch)
            && self.other_picker.as_ref().is_none_or(|picker| {
                picker.center == self.center
                    && picker.rotation == self.rotation
                    && picker.scale == self.scale
                    && picker.rect == self.rect
                    && picker.mode == self.selection_mode
            });
        if !valid {
            self.cancel_other_picker();
        }
    }

    fn native_hits(&self, position: Pos2, triangles: &[ProjectedTriangle]) -> Vec<Selection> {
        let mut hits = BTreeMap::<(String, String), Hit>::new();
        let mut add = |object: &str, name: &str, depth: f32, distance: f32, kind: u8| {
            if !depth.is_finite() || !distance.is_finite() {
                return;
            }
            let key = (object.to_owned(), name.to_owned());
            if hits.get(&key).is_none_or(|old| {
                distance < old.distance || distance == old.distance && depth > old.depth
            }) {
                hits.insert(
                    key,
                    Hit {
                        selection: Selection {
                            object: object.to_owned(),
                            subelement: name.to_owned(),
                        },
                        depth,
                        distance,
                        kind,
                    },
                );
            }
        };
        if matches!(
            self.selection_mode,
            SelectionMode::All | SelectionMode::Vertices
        ) {
            for vertex in self
                .vertices
                .iter()
                .filter(|vertex| self.selection_mode == SelectionMode::Vertices || vertex.isolated)
            {
                let (point, depth) = self.project(vertex.point);
                let distance = point.distance(position);
                if distance <= 6. {
                    add(&vertex.object, &vertex.name, depth, distance, 0);
                }
            }
        }
        if matches!(
            self.selection_mode,
            SelectionMode::All | SelectionMode::Edges
        ) {
            for edge in &self.edges {
                for line in edge.points.windows(2) {
                    let a = self.project(line[0]);
                    let b = self.project(line[1]);
                    let (distance, t) = segment_distance(position, a.0, b.0);
                    if distance < 6. {
                        add(&edge.object, &edge.name, a.1 + t * (b.1 - a.1), distance, 1);
                    }
                }
            }
        }
        if matches!(
            self.selection_mode,
            SelectionMode::All | SelectionMode::Faces
        ) {
            for triangle in triangles {
                if let Some(depth) = triangle_depth(position, triangle) {
                    let native = &self.triangles[triangle.face];
                    add(&native.object, &native.face, depth, 0., 2);
                }
            }
        }
        let mut hits: Vec<_> = hits.into_values().collect();
        hits.sort_by(|a, b| {
            b.depth
                .total_cmp(&a.depth)
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.distance.total_cmp(&b.distance))
                .then_with(|| a.selection.object.cmp(&b.selection.object))
                .then_with(|| a.selection.subelement.cmp(&b.selection.subelement))
        });
        hits.into_iter().map(|hit| hit.selection).collect()
    }

    pub(super) fn select_other_menu(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        triangles: &[ProjectedTriangle],
    ) -> Option<Selection> {
        let eligible = ui.is_enabled() && (self.sketch.is_none() || self.solid_select_in_sketch);
        if eligible
            && response.secondary_clicked()
            && let Some(position) = response.interact_pointer_pos()
        {
            self.other_picker = Some(Picker {
                center: self.center,
                rotation: self.rotation,
                scale: self.scale,
                rect: self.rect,
                mode: self.selection_mode,
                choices: self.native_hits(position, triangles),
            });
        }
        let mut chosen = None;
        let before = self.other_hover.clone();
        self.other_hover = None;
        let open = response
            .context_menu(|ui| {
                let Some(picker) = self.other_picker.as_ref().filter(|_| eligible) else {
                    ui.close_kind(egui::UiKind::Menu);
                    return;
                };
                ui.label("Select other");
                ui.weak("Native elements, front to back. Shift toggles selection.");
                ui.separator();
                if picker.choices.is_empty() {
                    ui.weak("No matching face, edge or vertex at this point.");
                }
                egui::ScrollArea::vertical()
                    .max_height(250.)
                    .show(ui, |ui| {
                        for selection in &picker.choices {
                            let response = ui
                                .button(format!("{} / {}", selection.object, selection.subelement));
                            if response.hovered() {
                                self.other_hover = Some(selection.clone());
                            }
                            if response.clicked() {
                                chosen = Some(selection.clone());
                                ui.close_kind(egui::UiKind::Menu);
                            }
                        }
                    });
            })
            .is_some();
        if !open {
            self.cancel_other_picker();
        }
        if let Some(selection) = chosen {
            let toggle = ui.input(|input| {
                input.modifiers.ctrl || input.modifiers.shift || input.modifiers.command
            });
            self.select_native_element(Some(selection.clone()), toggle);
            self.cancel_other_picker();
            ui.ctx().request_repaint();
            return self.selected.clone();
        }
        if self.other_hover != before {
            ui.ctx().request_repaint();
        }
        None
    }

    // A translucent overlay previews an obscured element without selecting it.
    // It uses the same native display mesh/topology as ordinary picking.
    pub(super) fn draw_other_hover(&self, painter: &egui::Painter) {
        let Some(selection) = &self.other_hover else {
            return;
        };
        let color = Color32::from_rgb(95, 221, 207);
        for triangle in self.triangles.iter().filter(|triangle| {
            triangle.object.as_ref() == selection.object && triangle.face == selection.subelement
        }) {
            painter.add(Shape::convex_polygon(
                triangle.points.map(|point| self.project(point).0).to_vec(),
                Color32::from_rgba_unmultiplied(95, 221, 207, 75),
                Stroke::NONE,
            ));
        }
        for edge in self.edges.iter().filter(|edge| {
            edge.object.as_ref() == selection.object && edge.name == selection.subelement
        }) {
            for line in edge.points.windows(2) {
                painter.line_segment(
                    [self.project(line[0]).0, self.project(line[1]).0],
                    Stroke::new(2_f32, color),
                );
            }
        }
        for vertex in self.vertices.iter().filter(|vertex| {
            vertex.object.as_ref() == selection.object && vertex.name == selection.subelement
        }) {
            painter.circle_stroke(
                self.project(vertex.point).0,
                5_f32,
                Stroke::new(2_f32, color),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::select_other_ui_tests::{scene, viewport};
    use super::*;
    use serde_json::json;

    #[test]
    fn native_hit_stack_deduplicates_faces_and_keeps_obscured_elements() {
        let mut v = viewport();
        let point = v.project(Vec3::new(2., 2., 0.)).0;
        assert_eq!(v.pick(point, &v.projected()).unwrap().object, "Front");
        let all = v.native_hits(point, &v.projected());
        assert_eq!(
            all,
            vec![
                Selection {
                    object: "Front".into(),
                    subelement: "Face7".into()
                },
                Selection {
                    object: "Rear".into(),
                    subelement: "Edge4".into()
                },
                Selection {
                    object: "Rear".into(),
                    subelement: "Face7".into()
                },
            ]
        );
        v.selection_mode = SelectionMode::Faces;
        assert_eq!(v.native_hits(point, &v.projected()).len(), 2);
        v.selection_mode = SelectionMode::Edges;
        assert_eq!(v.native_hits(point, &v.projected())[0].subelement, "Edge4");
        v.selection_mode = SelectionMode::Vertices;
        assert_eq!(
            v.native_hits(point, &v.projected())[0].subelement,
            "Vertex3"
        );
    }

    fn capture(v: &mut Viewport) {
        v.other_picker = Some(Picker {
            center: v.center,
            rotation: v.rotation,
            scale: v.scale,
            rect: v.rect,
            mode: v.selection_mode,
            choices: vec![],
        });
        v.other_hover = Some(Selection {
            object: "Rear".into(),
            subelement: "Face7".into(),
        });
    }

    #[test]
    fn changed_view_scene_filter_sketch_or_disabled_parent_discards_captured_hits() {
        let mut v = viewport();
        capture(&mut v);
        v.iso();
        v.validate_other_picker(true);
        assert!(v.other_picker.is_none());
        capture(&mut v);
        v.selection_mode = SelectionMode::Vertices;
        v.validate_other_picker(true);
        assert!(v.other_picker.is_none());
        capture(&mut v);
        v.set_scene(scene("Other", 20.).0, json!({}));
        assert!(v.other_picker.is_none() && v.other_hover.is_none());
        capture(&mut v);
        v.set_references(&json!({"objects":[]}));
        assert!(v.other_picker.is_none());
        capture(&mut v);
        v.set_sketch(Some(json!({"name":"Sketch","sketch":{"geometry":[]}})));
        assert!(v.other_picker.is_none());
        capture(&mut v);
        v.validate_other_picker(false);
        assert!(v.other_picker.is_none());
    }
}
