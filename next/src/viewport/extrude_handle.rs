//! Screen-space interaction only. The editor owns quantity edits and previews.
use super::*;

#[derive(Clone, Debug)]
pub(crate) struct DepthHandle {
    pub owner: String,
    pub origin: Vec3,
    pub axis: Vec3,
    pub factor: f32,
    pub length: f64,
    pub ready: bool,
}

#[derive(Default)]
pub(super) struct DepthUi {
    pub handle: Option<DepthHandle>,
    drag: Option<Drag>,
    pub value: Option<(String, f64)>,
    pub consumed: bool,
    blocked: bool,
}
struct Drag { owner: String, start: Pos2, vector: Vec2, length: f64 }

impl Viewport {
    pub(crate) fn set_depth_handle(&mut self, handle: Option<DepthHandle>) {
        if self.depth.drag.as_ref().is_some_and(|d|
            handle.as_ref().is_none_or(|h| h.owner != d.owner)) {
            self.depth.drag = None;
            self.depth.blocked = true;
        }
        self.depth.handle = handle;
    }
    pub(crate) fn take_depth_value(&mut self) -> Option<(String, f64)> {
        self.depth.value.take()
    }
    pub(crate) fn depth_owns_pointer(&self) -> bool { self.depth.consumed }
    #[cfg(test)]
    pub(crate) fn depth_endpoint(&self) -> Option<Pos2> {
        self.depth_projection(self.depth.handle.as_ref()?).map(|p| p.2)
    }

    fn depth_projection(&self, h: &DepthHandle) -> Option<(Pos2, Vec2, Pos2)> {
        let origin = self.project(h.origin).0;
        let vector = self.project(h.origin + h.axis * h.factor).0 - origin;
        // Explicit unavailable when the axis points into the screen. Normal
        // navigation remains available; never divide by a tiny projection.
        if !vector.is_finite() || vector.length() < 0.05 || !h.length.is_finite() {
            return None;
        }
        let endpoint = origin + vector * h.length as f32;
        endpoint.is_finite().then_some((origin, vector, endpoint))
    }
    pub(super) fn depth_input(&mut self, ui: &egui::Ui, response: &egui::Response) {
        self.depth.consumed = false;
        if self.depth.blocked {
            self.depth.consumed = true;
            self.depth.blocked = ui.input(|i| i.pointer.primary_down());
            return;
        }
        let Some(h) = self.depth.handle.clone() else { return; };
        let Some((origin, vector, endpoint)) = self.depth_projection(&h) else {
            self.depth.drag = None;
            return;
        };
        let (pos, pressed, down, released, escape) = ui.input(|i| (
            i.pointer.interact_pos(), i.pointer.primary_pressed(),
            i.pointer.primary_down(), i.pointer.primary_released(),
            i.key_pressed(egui::Key::Escape)));
        if escape && self.depth.drag.is_some() {
            self.depth.drag = None;
            self.depth.blocked = down;
            self.depth.consumed = true;
            return;
        }
        if self.depth.drag.is_none() && h.ready && !self.cube_consumed
            && pressed && response.contains_pointer()
            && let Some(pos) = pos
            && self.rect.contains(endpoint)
            && (pos.distance(endpoint) <= POINT_PICK_RADIUS
                || segment_distance(pos, if origin.distance(endpoint) < 24. {
                    endpoint-vector.normalized()*24.
                } else { origin }, endpoint).0 <= EDGE_PICK_RADIUS) {
            self.depth.drag = Some(Drag { owner: h.owner.clone(), start: pos, vector, length: h.length });
        }
        if let Some(drag) = &self.depth.drag {
            self.depth.consumed = true;
            if let Some(pos) = pos {
                let length = drag.length + ((pos-drag.start).dot(drag.vector) / drag.vector.length_sq()) as f64;
                if length.is_finite() && (down || released) && pos != drag.start {
                    self.depth.value = Some((drag.owner.clone(), length));
                }
            }
            if released || !down { self.depth.drag = None; }
        }
    }
    pub(super) fn paint_depth_handle(&self, painter: &egui::Painter) {
        let Some(h) = &self.depth.handle else { return; };
        let Some((origin, vector, endpoint)) = self.depth_projection(h) else { return; };
        if !self.rect.contains(endpoint) { return; }
        let color = Color32::from_rgb(113, 183, 209);
        let direction = vector.normalized() * if h.length < 0. { -1. } else { 1. };
        // Minimum visible shaft at zero or very short depth; endpoint remains
        // the actual projected quantity and the sole point hit target.
        let start = if origin.distance(endpoint) < 24. { endpoint-direction*24. } else { origin };
        painter.arrow(start, endpoint-start, Stroke::new(2_f32, color));
        painter.circle_filled(endpoint, 4., color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_transformed_projection_and_edge_on_unavailable() {
        let v = Viewport::default();
        let mut h = DepthHandle { owner: "owner".into(), origin: Vec3::new(12.,-7.,9.),
            axis: Vec3::X, factor: 0.5, length: -24., ready: true };
        let (o, axis, end) = v.depth_projection(&h).unwrap();
        assert!((end-o-axis*(-24.)).length()<0.001);
        h.axis = v.rotation * Vec3::Z;
        assert!(v.depth_projection(&h).is_none());
        h.length = f64::NAN;
        assert!(v.depth_projection(&h).is_none());
    }
}
