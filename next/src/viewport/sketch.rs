//! Native sketch handles, selection and annotations. Coordinates always come from the solved snapshot.
use super::{
    Color32, Pos2, Rect, Shape, SketchElement, Stroke, Vec2, Vec3, Viewport, egui,
    segment_distance, vector,
};
use serde_json::Value;

pub(super) struct DimensionHit {
    index: usize,
    label: Rect,
    segments: Vec<[Pos2; 2]>,
}

impl Viewport {
    pub fn clear_sketch_selection(&mut self) {
        self.sketch_selection.clear();
        self.clear_selection();
    }

    /// A solved point in native sketch coordinates. Whole lines use their midpoint;
    /// whole arcs use their midpoint and whole circles use the positive-X radius point.
    /// Other whole curves use a native display sample; point handles use exact solved values.
    pub fn geometry_point(&self, element: SketchElement) -> Option<[f64; 3]> {
        self.sketch.as_ref()?;
        if element
            == (SketchElement {
                index: -1,
                point: 1,
            })
        {
            return Some([0.; 3]);
        }
        if element.point != 0 {
            return super::sketch_curves::native_handle(
                self.curves.iter().map(|curve| &curve.native),
                element,
            )
            .map(|handle| handle.position);
        }
        let geometry = self.native_geometry(element.index)?;
        let point =
            |value: &Value| Some([value[0].as_f64()?, value[1].as_f64()?, value[2].as_f64()?]);
        match geometry["kind"].as_str()? {
            "point" => point(&geometry["position"]),
            "line" => {
                let a = point(&geometry["start"])?;
                let b = point(&geometry["end"])?;
                Some(std::array::from_fn(|i| (a[i] + b[i]) * 0.5))
            }
            "arc" => point(&geometry["mid"]),
            "circle" => {
                let mut center = point(&geometry["center"])?;
                center[0] += geometry["radius"].as_f64()?;
                Some(center)
            }
            _ => self
                .curves
                .iter()
                .find(|curve| curve.native.index == element.index)
                .and_then(|curve| curve.native.points.get(curve.native.points.len() / 2))
                .copied(),
        }
    }

    fn native_geometry(&self, index: i32) -> Option<&Value> {
        let sketch = &self.sketch.as_ref()?["sketch"];
        sketch["geometry"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(
                sketch["externals"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|wrapper| &wrapper["geometry"]),
            )
            .find(|geo| geo["index"].as_i64() == Some(i64::from(index)))
    }

    /// Only native handles explicitly verified for movement are drag targets.
    pub fn sketch_draggable(&self, element: SketchElement) -> bool {
        if element.index < 0 {
            return false;
        }
        if element.point == 0 {
            return self.curves.iter().any(|curve| {
                curve.native.index == element.index
                    && matches!(curve.native.kind.as_str(), "line" | "circle" | "arc")
            });
        }
        self.curves
            .iter()
            .flat_map(|curve| &curve.native.handles)
            .any(|handle| handle.id == element && handle.draggable)
    }

    /// Native point identifiers have priority over whole edges within a seven-point handle radius.
    pub fn sketch_hit(&self, pos: Pos2) -> Option<SketchElement> {
        self.sketch.as_ref()?;
        let mut best: Option<(f32, SketchElement)> = None;
        for curve in &self.curves {
            for (id, world) in &curve.handles {
                let distance = self.project(*world).0.distance(pos);
                if distance <= 7. && best.as_ref().is_none_or(|(d, _)| distance < *d) {
                    best = Some((distance, *id));
                }
            }
        }
        let origin_distance = self.project(self.sketch_origin).0.distance(pos);
        // The origin's outer ring remains selectable when an endpoint or center coincides with it.
        let origin_ring = (4.5..=8.).contains(&origin_distance)
            && best.as_ref().is_some_and(|(_, element)| {
                self.geometry_point(*element)
                    .is_some_and(|point| point.iter().all(|value| value.abs() < 1e-9))
            });
        if origin_ring
            || (origin_distance <= 7. && best.as_ref().is_none_or(|(d, _)| origin_distance < *d))
        {
            best = Some((
                origin_distance,
                SketchElement {
                    index: -1,
                    point: 1,
                },
            ));
        }
        if let Some((_, element)) = best {
            return Some(element);
        }
        self.curves
            .iter()
            .filter_map(|curve| {
                let distance = curve
                    .points
                    .windows(2)
                    .map(|line| {
                        segment_distance(pos, self.project(line[0]).0, self.project(line[1]).0).0
                    })
                    .min_by(f32::total_cmp)?;
                (distance <= 6.).then_some((
                    distance,
                    SketchElement {
                        index: curve.native.index,
                        point: 0,
                    },
                ))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, element)| element)
    }

    /// Select displayed geometry using the same tessellated paths as the canvas.
    /// A crossing box accepts intersecting segments; a window requires full containment.
    /// The sketch origin remains an explicit point selection, outside geometry boxes.
    pub fn sketch_in_rect(&self, rect: Rect, crossing: bool) -> Vec<SketchElement> {
        if self.sketch.is_none() {
            return vec![];
        }
        self.curves
            .iter()
            .filter_map(|curve| {
                if curve.points.is_empty() {
                    return None;
                }
                let points: Vec<Pos2> = curve
                    .points
                    .iter()
                    .map(|point| self.project(*point).0)
                    .collect();
                let selected = if crossing {
                    points.iter().any(|point| rect.contains(*point))
                        || points
                            .windows(2)
                            .any(|line| segment_intersects_rect(line[0], line[1], rect))
                } else {
                    points.iter().all(|point| rect.contains(*point))
                };
                selected.then_some(SketchElement {
                    index: curve.native.index,
                    point: if points.len() == 1 { 1 } else { 0 },
                })
            })
            .collect()
    }

    pub fn dimension_hit(&self, pos: Pos2) -> Option<usize> {
        self.dimension_hits
            .iter()
            .find(|hit| hit.label.contains(pos))
            .map(|hit| hit.index)
            .or_else(|| {
                self.dimension_hits
                    .iter()
                    .filter_map(|hit| {
                        let distance = hit
                            .segments
                            .iter()
                            .map(|[a, b]| segment_distance(pos, *a, *b).0)
                            .min_by(f32::total_cmp)?;
                        (distance <= 5.).then_some((distance, hit.index))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, index)| index)
            })
    }

    pub(super) fn select_sketch_element(&mut self, hit: Option<SketchElement>, toggle: bool) {
        let Some(hit) = hit else {
            self.clear_sketch_selection();
            return;
        };
        if toggle {
            if let Some(index) = self
                .sketch_selection
                .iter()
                .position(|element| *element == hit)
            {
                self.sketch_selection.remove(index);
            } else {
                self.sketch_selection.push(hit);
            }
        } else {
            self.sketch_selection = vec![hit];
        }
    }

    pub(super) fn draw_sketch(&self, painter: &egui::Painter) {
        let Some(sketch) = &self.sketch else {
            return;
        };
        let constrained = sketch["sketch"]["degrees_of_freedom"].as_i64() == Some(0);
        let selected_color = Color32::from_rgb(236, 204, 96);
        for curve in &self.curves {
            let selected = self
                .sketch_selection
                .iter()
                .any(|element| element.index == curve.native.index && element.point == 0);
            let color = if selected {
                selected_color
            } else if curve.native.external {
                if curve.native.defining {
                    Color32::from_rgb(202, 140, 194)
                } else {
                    Color32::from_rgb(161, 122, 192)
                }
            } else if curve.native.construction {
                Color32::from_rgb(150, 127, 188)
            } else if constrained {
                Color32::from_rgb(83, 198, 185)
            } else {
                Color32::from_rgb(222, 228, 231)
            };
            let points: Vec<Pos2> = curve.points.iter().map(|p| self.project(*p).0).collect();
            let stroke = Stroke::new(if selected { 2.5_f32 } else { 1.8_f32 }, color);
            if curve.native.construction && !(curve.native.external && curve.native.defining) {
                painter.extend(Shape::dashed_line(&points, stroke, 7., 4.));
            } else {
                painter.add(Shape::line(points, stroke));
            }
            for (id, world) in &curve.handles {
                let position = self.project(*world).0;
                let handle_selected = selected || self.sketch_selection.contains(id);
                let handle_color = if handle_selected {
                    selected_color
                } else {
                    color
                };
                if id.point == 3 {
                    painter.circle_stroke(position, 3.5, Stroke::new(1.2_f32, handle_color));
                    painter.line_segment(
                        [position - Vec2::new(5., 0.), position + Vec2::new(5., 0.)],
                        Stroke::new(1_f32, handle_color),
                    );
                    painter.line_segment(
                        [position - Vec2::new(0., 5.), position + Vec2::new(0., 5.)],
                        Stroke::new(1_f32, handle_color),
                    );
                } else {
                    painter.circle_filled(
                        position,
                        if handle_selected { 4. } else { 3. },
                        handle_color,
                    );
                }
            }
        }
        let origin = self.project(self.sketch_origin).0;
        let origin_color = if self.sketch_selection.contains(&SketchElement {
            index: -1,
            point: 1,
        }) {
            selected_color
        } else {
            Color32::from_rgb(175, 119, 142)
        };
        painter.circle_stroke(origin, 5., Stroke::new(1_f32, origin_color));
        painter.line_segment(
            [origin - Vec2::new(8., 0.), origin + Vec2::new(8., 0.)],
            Stroke::new(1_f32, origin_color),
        );
        painter.line_segment(
            [origin - Vec2::new(0., 8.), origin + Vec2::new(0., 8.)],
            Stroke::new(1_f32, origin_color),
        );
        self.draw_constraint_glyphs(painter, sketch);
    }

    fn draw_constraint_glyphs(&self, painter: &egui::Painter, sketch: &Value) {
        for constraint in sketch["sketch"]["constraints"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if constraint["active"].as_bool() == Some(false) {
                continue;
            }
            let glyph = match constraint["type"].as_str().unwrap_or("") {
                "Horizontal" => "H",
                "Vertical" => "V",
                "Parallel" => "ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã¢â‚¬Â¹ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¥",
                "Perpendicular" => "ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¥",
                "Equal" => "=",
                "Tangent" => "T",
                "Coincident" => "ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â·",
                "Block" => {
                    "ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã…â€œÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â "
                }
                _ => continue,
            };
            let index = constraint["first"].as_i64().unwrap_or(-2000) as i32;
            let point = constraint["first_pos"].as_i64().unwrap_or(0) as i32;
            let Some(position) = self.native_sketch_position(index, point).or_else(|| {
                self.curves
                    .iter()
                    .find(|curve| curve.native.index == index)
                    .and_then(|curve| {
                        curve
                            .points
                            .first()
                            .zip(curve.points.last())
                            .map(|(a, b)| (*a + *b) * 0.5)
                    })
            }) else {
                continue;
            };
            let position = self.project(position).0 + Vec2::new(9., -9.);
            painter.text(
                position,
                egui::Align2::LEFT_BOTTOM,
                glyph,
                egui::FontId::monospace(11.),
                Color32::from_rgb(148, 163, 171),
            );
        }
    }

    fn native_sketch_position(&self, index: i32, point: i32) -> Option<Vec3> {
        if index == -1 && point == 1 {
            return Some(self.sketch_origin);
        }
        let local = self.geometry_point(SketchElement { index, point })?;
        Some(
            self.sketch_origin
                + self.sketch_rotation
                    * Vec3::new(local[0] as f32, local[1] as f32, local[2] as f32),
        )
    }

    pub(super) fn draw_dimensions(&mut self, painter: &egui::Painter) {
        let Some(sketch) = &self.sketch else {
            self.dimension_hits.clear();
            return;
        };
        let points: Vec<Pos2> = self
            .curves
            .iter()
            .flat_map(|curve| curve.points.iter())
            .map(|p| self.project(*p).0)
            .collect();
        let sketch_center = if points.is_empty() {
            self.rect.center()
        } else {
            Rect::from_points(&points).center()
        };
        let color = Color32::from_rgb(184, 159, 211);
        let stroke = Stroke::new(1_f32, color);
        let mut hits = vec![];
        let mut occupied = vec![];
        for (fallback_index, constraint) in sketch["sketch"]["constraints"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            if constraint["active"].as_bool() == Some(false) {
                continue;
            }
            let index = constraint["index"]
                .as_u64()
                .unwrap_or(fallback_index as u64) as usize;
            let kind = constraint["type"].as_str().unwrap_or("");
            let first = constraint["first"].as_i64().unwrap_or(-2000) as i32;
            let first_point = constraint["first_pos"].as_i64().unwrap_or(0) as i32;
            let second = constraint["second"].as_i64().unwrap_or(-2000) as i32;
            let second_point = constraint["second_pos"].as_i64().unwrap_or(0) as i32;
            let value = constraint["value"].as_f64().unwrap_or(0.);
            let mut segments = vec![];
            let mut label = None;
            if matches!(kind, "Distance" | "DistanceX" | "DistanceY") {
                let endpoints = if first_point == 0 {
                    self.curves
                        .iter()
                        .find(|curve| curve.native.index == first && curve.points.len() == 2)
                        .map(|curve| (curve.points[0], curve.points[1]))
                } else {
                    self.native_sketch_position(first, first_point).map(|a| {
                        let b = if kind == "Distance"
                            && second != -1
                            && second != -2
                            && second_point == 0
                        {
                            self.curves
                                .iter()
                                .find(|curve| {
                                    curve.native.index == second && curve.points.len() == 2
                                })
                                .and_then(|curve| {
                                    let direction = curve.points[1] - curve.points[0];
                                    (direction.length_squared() > 0.).then(|| {
                                        curve.points[0]
                                            + direction
                                                * ((a - curve.points[0]).dot(direction)
                                                    / direction.length_squared())
                                    })
                                })
                                .unwrap_or(self.sketch_origin)
                        } else {
                            self.native_sketch_position(second, second_point)
                                .unwrap_or(self.sketch_origin)
                        };
                        (b, a)
                    })
                };
                if let Some((world_a, world_b)) = endpoints {
                    let (a, b) = (self.project(world_a).0, self.project(world_b).0);
                    let mut direction = (b - a).normalized();
                    let (dimension_a, dimension_b, normal) =
                        if kind == "DistanceX" || kind == "DistanceY" {
                            // Project native sketch axes, so attached or orbited sketches keep their datum orientation.
                            let axis = self.sketch_rotation
                                * if kind == "DistanceX" {
                                    Vec3::X
                                } else {
                                    Vec3::Y
                                };
                            let origin = self.project(self.sketch_origin).0;
                            direction =
                                (self.project(self.sketch_origin + axis).0 - origin).normalized();
                            let mut normal = Vec2::new(-direction.y, direction.x);
                            if normal.dot(a.lerp(b, 0.5) - sketch_center) < 0. {
                                normal = -normal;
                            }
                            let level = a.to_vec2().dot(normal).max(b.to_vec2().dot(normal))
                                + 34.
                                + (index % 3) as f32 * 18.;
                            (
                                a + normal * (level - a.to_vec2().dot(normal)),
                                b + normal * (level - b.to_vec2().dot(normal)),
                                normal,
                            )
                        } else {
                            let mut normal = Vec2::new(-direction.y, direction.x);
                            if normal.dot(a.lerp(b, 0.5) - sketch_center) < 0. {
                                normal = -normal;
                            }
                            (a + normal * 26., b + normal * 26., normal)
                        };
                    if dimension_a.distance(dimension_b) > 1. {
                        segments.extend([
                            [a, dimension_a + normal * 5.],
                            [b, dimension_b + normal * 5.],
                            [dimension_a, dimension_b],
                        ]);
                        let tick = (direction + normal).normalized() * 4.;
                        for endpoint in [dimension_a, dimension_b] {
                            segments.push([endpoint - tick, endpoint + tick]);
                        }
                        label = Some((
                            dimension_a.lerp(dimension_b, 0.5),
                            normal,
                            format!("{value:.2} mm"),
                        ));
                    }
                }
            } else if matches!(kind, "Radius" | "Diameter") {
                if let Some(geo) = self.native_geometry(first) {
                    let center = vector(&geo["center"]);
                    let radius = geo["radius"].as_f64().unwrap_or(0.) as f32;
                    if radius > 0. {
                        let radial = Vec3::new(1., 1., 0.).normalize() * radius;
                        let project = |p| {
                            self.project(self.sketch_origin + self.sketch_rotation * p)
                                .0
                        };
                        let a = project(if kind == "Diameter" {
                            center - radial
                        } else {
                            center
                        });
                        let b = project(center + radial);
                        let direction = (b - a).normalized();
                        let leader = b + direction * 24.;
                        segments.push([a, leader]);
                        let tick =
                            (direction + Vec2::new(-direction.y, direction.x)).normalized() * 4.;
                        segments.push([b - tick, b + tick]);
                        if kind == "Diameter" {
                            segments.push([a - tick, a + tick]);
                        }
                        label = Some((
                            leader + direction * 28.,
                            direction,
                            format!(
                                "{}{value:.2} mm",
                                if kind == "Diameter" {
                                    "ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â¹Ãƒâ€¦Ã¢â‚¬Å“"
                                } else {
                                    "R"
                                }
                            ),
                        ));
                    }
                }
            } else if kind == "Angle" {
                let line = |index: i32| {
                    self.curves
                        .iter()
                        .find(|curve| curve.native.index == index && curve.points.len() == 2)
                        .map(|curve| (curve.points[0], curve.points[1]))
                };
                if let Some((a, b)) = line(first) {
                    let inverse = self.sketch_rotation.conjugate();
                    let (mut vertex, start_angle) = if let Some((c, d)) = line(second) {
                        let (a, b, c, d) = (
                            inverse * (a - self.sketch_origin),
                            inverse * (b - self.sketch_origin),
                            inverse * (c - self.sketch_origin),
                            inverse * (d - self.sketch_origin),
                        );
                        let u = b - a;
                        let v = d - c;
                        let denominator = u.x * v.y - u.y * v.x;
                        let vertex = if denominator.abs() > 1e-6 {
                            a + u * ((c.x - a.x) * v.y - (c.y - a.y) * v.x) / denominator
                        } else {
                            a
                        };
                        (vertex, u.y.atan2(u.x))
                    } else {
                        (inverse * (a - self.sketch_origin), 0.)
                    };
                    vertex.z = 0.;
                    let radius = 34. / self.scale;
                    let sweep = value as f32;
                    if sweep.is_finite() && sweep.abs() <= std::f32::consts::TAU + 0.001 {
                        let project = |angle: f32| {
                            self.project(
                                self.sketch_origin
                                    + self.sketch_rotation
                                        * (vertex
                                            + Vec3::new(angle.cos(), angle.sin(), 0.) * radius),
                            )
                            .0
                        };
                        for step in 0..24 {
                            segments.push([
                                project(start_angle + sweep * step as f32 / 24.),
                                project(start_angle + sweep * (step + 1) as f32 / 24.),
                            ]);
                        }
                        let middle = start_angle + sweep * 0.5;
                        let position = self
                            .project(
                                self.sketch_origin
                                    + self.sketch_rotation
                                        * (vertex
                                            + Vec3::new(middle.cos(), middle.sin(), 0.)
                                                * radius
                                                * 1.7),
                            )
                            .0;
                        let center = self
                            .project(self.sketch_origin + self.sketch_rotation * vertex)
                            .0;
                        label = Some((
                            position,
                            (position - center).normalized(),
                            format!(
                                "{:.1}ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â°",
                                value.to_degrees()
                            ),
                        ));
                    }
                }
            }
            if let Some((position, normal, text)) = label {
                for segment in &segments {
                    painter.line_segment(*segment, stroke);
                }
                let label = dimension_label(painter, position, normal, text, color, &mut occupied);
                if label.center().distance(position) > 2. {
                    segments.push([position, label.center()]);
                }
                hits.push(DimensionHit {
                    index,
                    label,
                    segments,
                });
            }
        }
        self.dimension_hits = hits;
    }
}

// Liang-Barsky clipping tests the segment itself, including parallel and boundary cases.
fn segment_intersects_rect(a: Pos2, b: Pos2, rect: Rect) -> bool {
    let delta = b - a;
    let mut enter = 0_f32;
    let mut exit = 1_f32;
    for (direction, distance) in [
        (-delta.x, a.x - rect.min.x),
        (delta.x, rect.max.x - a.x),
        (-delta.y, a.y - rect.min.y),
        (delta.y, rect.max.y - a.y),
    ] {
        if direction.abs() < f32::EPSILON {
            if distance < 0. {
                return false;
            }
        } else {
            let t = distance / direction;
            if direction < 0. {
                enter = enter.max(t);
            } else {
                exit = exit.min(t);
            }
            if enter > exit {
                return false;
            }
        }
    }
    true
}

fn dimension_label(
    painter: &egui::Painter,
    position: Pos2,
    normal: Vec2,
    text: String,
    color: Color32,
    occupied: &mut Vec<Rect>,
) -> Rect {
    let galley = painter.layout_no_wrap(text, egui::FontId::monospace(12.), color);
    let size = galley.size() + Vec2::new(8., 4.);
    let mut rectangle = Rect::from_center_size(position, size);
    for offset in 1..=8 {
        if !occupied
            .iter()
            .any(|rect| rect.expand(3.).intersects(rectangle))
        {
            break;
        }
        rectangle = Rect::from_center_size(position + normal * (offset as f32 * 20.), size);
    }
    if rectangle.center().distance(position) > 2. {
        painter.line_segment([position, rectangle.center()], Stroke::new(1_f32, color));
    }
    painter.rect_filled(rectangle, 2., Color32::from_rgb(8, 10, 12));
    painter.galley(rectangle.center() - galley.size() * 0.5, galley, color);
    occupied.push(rectangle);
    rectangle
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn snapshot() -> Value {
        json!({"name":"Sketch","sketch":{"degrees_of_freedom":4,"geometry":[
            {"index":0,"kind":"line","start":[10,10,0],"end":[30,10,0]},
            {"index":1,"kind":"circle","center":[20,25,0],"radius":5}],"constraints":[]}})
    }
    fn viewport() -> Viewport {
        let mut viewport = Viewport::default();
        viewport.set_sketch(Some(snapshot()));
        viewport.align_sketch();
        viewport
    }
    #[test]
    fn native_endpoint_center_origin_and_whole_edge_are_distinct() {
        let viewport = viewport();
        for (coordinate, element) in [
            ([10., 10., 0.], SketchElement { index: 0, point: 1 }),
            ([30., 10., 0.], SketchElement { index: 0, point: 2 }),
            ([20., 10., 0.], SketchElement { index: 0, point: 0 }),
            ([20., 25., 0.], SketchElement { index: 1, point: 3 }),
            ([25., 25., 0.], SketchElement { index: 1, point: 0 }),
            (
                [0., 0., 0.],
                SketchElement {
                    index: -1,
                    point: 1,
                },
            ),
        ] {
            assert_eq!(
                viewport.sketch_hit(viewport.sketch_screen_point(coordinate)),
                Some(element)
            );
        }
        // Endpoint handles win even when a click also falls inside the edge tolerance.
        assert_eq!(
            viewport.sketch_hit(viewport.sketch_screen_point([10., 10., 0.]) + Vec2::new(3., 0.)),
            Some(SketchElement { index: 0, point: 1 })
        );
        assert_eq!(
            viewport.sketch_hit(viewport.sketch_screen_point([-15., -15., 0.])),
            None
        );
    }
    #[test]
    fn origin_ring_remains_selectable_under_a_coincident_endpoint() {
        let mut viewport = viewport();
        let mut sketch = snapshot();
        sketch["sketch"]["geometry"][0]["start"] = json!([0, 0, 0]);
        viewport.set_sketch(Some(sketch));
        let origin = viewport.sketch_screen_point([0.; 3]);
        assert_eq!(
            viewport.sketch_hit(origin),
            Some(SketchElement { index: 0, point: 1 })
        );
        assert_eq!(
            viewport.sketch_hit(origin + Vec2::new(0., 6.)),
            Some(SketchElement {
                index: -1,
                point: 1
            })
        );
    }
    #[test]
    fn native_snap_points_stay_local_to_attached_sketch() {
        let mut viewport = viewport();
        let mut sketch = snapshot();
        sketch["global_placement"] = json!({"position":[100,200,300],"rotation":[0,0,1,0]});
        viewport.set_sketch(Some(sketch));
        assert_eq!(
            viewport.geometry_point(SketchElement {
                index: -1,
                point: 1
            }),
            Some([0.; 3])
        );
        assert_eq!(
            viewport.geometry_point(SketchElement { index: 0, point: 1 }),
            Some([10., 10., 0.])
        );
        assert_eq!(
            viewport.geometry_point(SketchElement { index: 0, point: 0 }),
            Some([20., 10., 0.])
        );
        assert_eq!(
            viewport.geometry_point(SketchElement { index: 1, point: 3 }),
            Some([20., 25., 0.])
        );
        assert_eq!(
            viewport.geometry_point(SketchElement { index: 1, point: 0 }),
            Some([25., 25., 0.])
        );
    }
    #[test]
    fn multiselection_toggles_and_same_sketch_recompute_preserves_native_handles() {
        let mut viewport = viewport();
        let endpoint = SketchElement { index: 0, point: 2 };
        let center = SketchElement { index: 1, point: 3 };
        viewport.select_sketch_element(Some(endpoint), false);
        viewport.select_sketch_element(Some(center), true);
        assert_eq!(viewport.sketch_selection, vec![endpoint, center]);
        let mut updated = snapshot();
        updated["sketch"]["geometry"][0]["end"] = json!([35, 10, 0]);
        viewport.set_sketch(Some(updated));
        assert_eq!(viewport.sketch_selection, vec![endpoint, center]);
        viewport.select_sketch_element(Some(endpoint), true);
        assert_eq!(viewport.sketch_selection, vec![center]);
        let mut deleted = snapshot();
        deleted["sketch"]["geometry"].as_array_mut().unwrap().pop();
        viewport.set_sketch(Some(deleted));
        assert!(viewport.sketch_selection.is_empty());
        viewport.select_sketch_element(Some(endpoint), false);
        let mut renamed = snapshot();
        renamed["name"] = json!("OtherSketch");
        viewport.set_sketch(Some(renamed));
        assert!(viewport.sketch_selection.is_empty());
    }
    #[test]
    fn crossing_box_intersects_segments_with_both_endpoints_outside() {
        let viewport = viewport();
        let center = viewport.sketch_screen_point([20., 10., 0.]);
        let rectangle = Rect::from_center_size(center, Vec2::splat(8.));
        assert_eq!(
            viewport.sketch_in_rect(rectangle, true),
            vec![SketchElement { index: 0, point: 0 }]
        );
        assert!(viewport.sketch_in_rect(rectangle, false).is_empty());
        let full = Rect::from_two_pos(
            viewport.sketch_screen_point([9., 9., 0.]),
            viewport.sketch_screen_point([31., 11., 0.]),
        );
        assert_eq!(
            viewport.sketch_in_rect(full, false),
            vec![SketchElement { index: 0, point: 0 }]
        );
        let miss = Rect::from_center_size(center + Vec2::new(0., 10.), Vec2::splat(4.));
        assert!(viewport.sketch_in_rect(miss, true).is_empty());
    }
    #[test]
    fn crossing_curve_and_native_point_boxes_match_display_paths() {
        let mut viewport = viewport();
        let circle = &viewport.curves[1];
        let a = viewport.project(circle.points[8]).0;
        let b = viewport.project(circle.points[9]).0;
        let rectangle = Rect::from_center_size(a.lerp(b, 0.5), Vec2::splat(0.05));
        assert_eq!(
            viewport.sketch_in_rect(rectangle, true),
            vec![SketchElement { index: 1, point: 0 }]
        );
        assert!(viewport.sketch_in_rect(rectangle, false).is_empty());
        let mut sketch = snapshot();
        sketch["sketch"]["geometry"]
            .as_array_mut()
            .unwrap()
            .push(json!({"index":2,"kind":"point","position":[-10,-10,0]}));
        viewport.set_sketch(Some(sketch));
        let rectangle = Rect::from_center_size(
            viewport.sketch_screen_point([-10., -10., 0.]),
            Vec2::splat(4.),
        );
        for crossing in [false, true] {
            assert_eq!(
                viewport.sketch_in_rect(rectangle, crossing),
                vec![SketchElement { index: 2, point: 1 }]
            );
        }
    }
    #[test]
    fn rectangle_clipping_rejects_diagonal_bbox_false_positives() {
        let rectangle = Rect::from_min_max(Pos2::new(0., 0.), Pos2::new(2., 2.));
        assert!(!segment_intersects_rect(
            Pos2::new(-1., 1.),
            Pos2::new(1., 3.1),
            rectangle
        ));
        assert!(segment_intersects_rect(
            Pos2::new(-1., 1.),
            Pos2::new(3., 1.),
            rectangle
        ));
        assert!(segment_intersects_rect(
            Pos2::new(-1., 0.),
            Pos2::new(3., 0.),
            rectangle
        ));
        assert!(!segment_intersects_rect(
            Pos2::new(-1., -0.1),
            Pos2::new(3., -0.1),
            rectangle
        ));
    }
    #[test]
    fn point_to_line_distance_uses_perpendicular_foot_from_native_references() {
        let mut viewport = viewport();
        let mut sketch = snapshot();
        sketch["sketch"]["geometry"]
            .as_array_mut()
            .unwrap()
            .push(json!({"index":2,"kind":"point","position":[20,20,0]}));
        sketch["sketch"]["constraints"] = json!([{ "index":9,"type":"Distance","first":2,"first_pos":1,"second":0,"second_pos":0,"value":10 }]);
        viewport.set_sketch(Some(sketch));
        let context = egui::Context::default();
        let _ = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| viewport.draw_dimensions(ui.painter()));
        });
        let hit = &viewport.dimension_hits[0];
        assert_eq!(hit.index, 9);
        assert!(hit.segments[0][0].distance(viewport.sketch_screen_point([20., 10., 0.])) < 1e-5);
        assert!(hit.segments[1][0].distance(viewport.sketch_screen_point([20., 20., 0.])) < 1e-5);
    }
    #[test]
    fn drawn_dimension_label_and_line_return_native_constraint_index() {
        let mut viewport = viewport();
        let mut sketch = snapshot();
        sketch["sketch"]["constraints"] = json!([
            {"index":7,"type":"Distance","first":0,"first_pos":0,"value":20},
            {"index":8,"type":"DistanceX","first":0,"first_pos":2,"second":-1,"second_pos":1,"value":30}
        ]);
        viewport.set_sketch(Some(sketch));
        let context = egui::Context::default();
        let _ = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| viewport.draw_dimensions(ui.painter()));
        });
        assert_eq!(viewport.dimension_hits.len(), 2);
        for hit in &viewport.dimension_hits {
            assert_eq!(viewport.dimension_hit(hit.label.center()), Some(hit.index));
            let [a, b] = hit.segments[0];
            assert_eq!(viewport.dimension_hit(a.lerp(b, 0.5)), Some(hit.index));
        }
        assert!(
            !viewport.dimension_hits[0]
                .label
                .intersects(viewport.dimension_hits[1].label)
        );
    }
    fn external_line(token: &str) -> Value {
        json!({"index":0,"geometry_index":-3,"source_index":4,
            "source":{"link":"Source","subelements":["Edge7"]},"persistent_reference":token,
            "defining":true,"intersection":false,"projection":true,"missing":false,"frozen":false,
            "geometry":{"index":-3,"kind":"line","construction":true,
                "start":[10,-10,0],"end":[30,-10,0],"points":[[10,-10,0],[30,-10,0]],
                "handles":[{"index":-3,"point":1,"position":[10,-10,0],"draggable":false},
                    {"index":-3,"point":2,"position":[30,-10,0],"draggable":false}]}})
    }
    #[test]
    fn native_external_solver_ids_are_pickable_without_aliasing_internal_geometry_or_origin() {
        let mut viewport = viewport();
        let mut sketch = snapshot();
        sketch["sketch"]["externals"] = json!([external_line("Source.;g1;SKT")]);
        viewport.set_sketch(Some(sketch.clone()));
        for (coordinate, element) in [
            (
                [10., -10., 0.],
                SketchElement {
                    index: -3,
                    point: 1,
                },
            ),
            (
                [20., -10., 0.],
                SketchElement {
                    index: -3,
                    point: 0,
                },
            ),
            (
                [30., -10., 0.],
                SketchElement {
                    index: -3,
                    point: 2,
                },
            ),
        ] {
            assert_eq!(
                viewport.sketch_hit(viewport.sketch_screen_point(coordinate)),
                Some(element)
            );
            assert_eq!(viewport.geometry_point(element), Some(coordinate));
            assert!(!viewport.sketch_draggable(element));
        }
        assert_eq!(
            viewport.geometry_point(SketchElement { index: 0, point: 1 }),
            Some([10., 10., 0.])
        );
        assert_eq!(
            viewport.geometry_point(SketchElement {
                index: -1,
                point: 1
            }),
            Some([0.; 3])
        );
        assert_eq!(
            viewport.geometry_point(SketchElement {
                index: -2,
                point: 1
            }),
            None
        );
        let rectangle = Rect::from_two_pos(
            viewport.sketch_screen_point([9., -11., 0.]),
            viewport.sketch_screen_point([31., -9., 0.]),
        );
        assert_eq!(
            viewport.sketch_in_rect(rectangle, false),
            vec![SketchElement {
                index: -3,
                point: 0
            }]
        );
        let selected = SketchElement {
            index: -3,
            point: 2,
        };
        viewport.select_sketch_element(Some(selected), false);
        sketch["revision"] = json!(2);
        viewport.set_sketch(Some(sketch.clone()));
        assert_eq!(viewport.sketch_selection, vec![selected]);
        // A topology/source change reusing a flat external index must not keep a stale reference selected.
        sketch["sketch"]["externals"][0]["persistent_reference"] =
            json!("Source.;different-native-reference;SKT");
        viewport.set_sketch(Some(sketch));
        assert!(viewport.sketch_selection.is_empty());
    }
    #[test]
    fn advanced_native_samples_handles_and_control_geometry_keep_native_identity() {
        let mut viewport = Viewport::default();
        viewport.set_sketch(Some(json!({"name":"Advanced","global_placement":{"position":[100,200,300],"rotation":[0,0,1,0]},
            "sketch":{"geometry":[
                {"index":0,"kind":"ellipse","points":[[13,10,0],[10,12,0],[7,10,0],[10,8,0],[13,10,0]],
                    "handles":[{"index":0,"point":3,"position":[10,10,0],"draggable":true}]},
                {"index":1,"kind":"bspline","points":[[10,-10,0],[20,-8,0],[30,-10,0]],
                    "handles":[{"index":1,"point":1,"position":[10,-10,0],"draggable":true},
                        {"index":1,"point":2,"position":[30,-10,0],"draggable":true}]},
                {"index":2,"kind":"circle","construction":true,"points":[[20.25,-3,0],[20,-2.75,0],[19.75,-3,0],[20,-3.25,0],[20.25,-3,0]],
                    "handles":[{"index":2,"point":3,"position":[20,-3,0],"draggable":true}]}]}})));
        viewport.align_sketch();
        assert_eq!(viewport.curves[1].native.points.len(), 3);
        for (coordinate, element) in [
            ([10., 10., 0.], SketchElement { index: 0, point: 3 }),
            ([10., 12., 0.], SketchElement { index: 0, point: 0 }),
            ([20., -8., 0.], SketchElement { index: 1, point: 0 }),
            ([20., -3., 0.], SketchElement { index: 2, point: 3 }),
        ] {
            assert_eq!(
                viewport.sketch_hit(viewport.sketch_screen_point(coordinate)),
                Some(element)
            );
        }
        assert_eq!(
            viewport.geometry_point(SketchElement { index: 1, point: 3 }),
            None
        );
        assert_eq!(
            viewport.geometry_point(SketchElement { index: 0, point: 1 }),
            None
        );
        assert!(viewport.sketch_draggable(SketchElement { index: 0, point: 3 }));
        assert!(viewport.sketch_draggable(SketchElement { index: 1, point: 1 }));
        assert!(viewport.sketch_draggable(SketchElement { index: 2, point: 3 }));
        assert!(!viewport.sketch_draggable(SketchElement { index: 0, point: 0 }));
        assert!(!viewport.sketch_draggable(SketchElement { index: 1, point: 0 }));
    }
    #[test]
    fn dimension_to_external_line_uses_native_perpendicular_foot() {
        let mut viewport = viewport();
        let mut sketch = snapshot();
        sketch["sketch"]["externals"] = json!([external_line("Source.;g1;SKT")]);
        sketch["sketch"]["constraints"] = json!([{"index":11,"type":"Distance","first":0,"first_pos":1,"second":-3,"second_pos":0,"value":20}]);
        viewport.set_sketch(Some(sketch));
        let context = egui::Context::default();
        let _ = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| viewport.draw_dimensions(ui.painter()));
        });
        let hit = &viewport.dimension_hits[0];
        assert_eq!(hit.index, 11);
        assert!(hit.segments[0][0].distance(viewport.sketch_screen_point([10., -10., 0.])) < 1e-5);
        assert!(hit.segments[1][0].distance(viewport.sketch_screen_point([10., 10., 0.])) < 1e-5);
    }
}
