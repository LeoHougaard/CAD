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
        self.sketch_pick_positions.clear();
        self.last_sketch_pick = None;
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
            .chain(sketch["axes"].as_array().into_iter().flatten())
            .chain(
                sketch["externals"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|wrapper| &wrapper["geometry"]),
            )
            .find(|geo| geo["index"].as_i64() == Some(i64::from(index)))
    }

    fn axis_segments(&self) -> Vec<(SketchElement, [Pos2; 2])> {
        self.sketch
            .as_ref()
            .into_iter()
            .flat_map(|s| s["sketch"]["axes"].as_array())
            .flatten()
            .filter_map(|axis| {
                let index = i32::try_from(axis["index"].as_i64()?).ok()?;
                if !matches!(index, -1 | -2) || axis["kind"] != "line" {
                    return None;
                }
                let world = |p: &Value| self.sketch_origin + self.sketch_rotation * vector(p);
                let start = self.project(world(&axis["start"])).0;
                let delta = self.project(world(&axis["end"])).0 - start;
                if delta.length_sq() < 1e-8 {
                    return None;
                }
                let direction = delta.normalized();
                let center = start + direction * (self.rect.center() - start).dot(direction);
                let extent = self.rect.width() + self.rect.height();
                Some((
                    SketchElement { index, point: 0 },
                    [center - direction * extent, center + direction * extent],
                ))
            })
            .collect()
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
            .or_else(|| {
                self.axis_segments()
                    .into_iter()
                    .filter_map(|(element, [a, b])| {
                        let distance = segment_distance(pos, a, b).0;
                        (distance <= 5.).then_some((distance, element))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, element)| element)
            })
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
                // Witness/leader lines may meet a native endpoint or cross a
                // curve. Keep those entities selectable; labels remain direct
                // dimension targets, and unobstructed leader lines still hit.
                if self.sketch_hit(pos).is_some() {
                    return None;
                }
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

    /// Select at the cursor and remember the actual sketch-local pick, not a curve midpoint.
    pub fn select_sketch_at(&mut self, pos: Pos2, toggle: bool) -> Option<SketchElement> {
        let mut hit = self.sketch_hit(pos);
        // Visit each co-located native handle in order, then visit them again
        // to remove them. A,B,C -> B,C -> C -> empty preserves ray order.
        if toggle && let Some(point) = hit.filter(|h| h.point != 0) {
            let anchor = self
                .geometry_point(point)
                .map(|p| self.sketch_screen_point(p));
            if let Some(anchor) = anchor {
                let mut handles = Vec::new();
                for (id, world) in self.curves.iter().flat_map(|c| &c.handles) {
                    if self.project(*world).0.distance(anchor) < 0.25 && !handles.contains(id) {
                        handles.push(*id);
                    }
                }
                // The origin's ring is its own target, outside this cycle.
                if handles.contains(&point) {
                    hit = self
                        .last_sketch_pick
                        .and_then(|last| handles.iter().position(|id| *id == last))
                        .map(|index| handles[(index + 1) % handles.len()])
                        .or_else(|| {
                            handles
                                .iter()
                                .find(|id| !self.sketch_selection.contains(id))
                                .copied()
                        })
                        .or(hit);
                }
            }
        }
        let position = self.sketch_point(pos);
        self.select_sketch_element(hit, toggle);
        self.last_sketch_pick = hit.filter(|h| h.point != 0);
        if let (Some(element), Some(position)) = (hit, position)
            && self.sketch_selection.contains(&element)
        {
            self.sketch_pick_positions.insert(element, position);
        }
        hit
    }

    /// Cursor position from an actual selected pick in the unchanged native sketch snapshot.
    /// Box selections and selections supplied by other controls have no cursor pick.
    pub fn sketch_pick_position(&self, element: SketchElement) -> Option<[f64; 3]> {
        self.sketch_selection
            .contains(&element)
            .then(|| self.sketch_pick_positions.get(&element).copied())
            .flatten()
    }

    pub(super) fn select_sketch_element(&mut self, hit: Option<SketchElement>, toggle: bool) {
        self.last_sketch_pick = None;
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
        self.sketch_pick_positions
            .retain(|element, _| self.sketch_selection.contains(element));
    }

    pub(super) fn draw_sketch(&self, painter: &egui::Painter) {
        let Some(sketch) = &self.sketch else {
            return;
        };
        let constrained = sketch["sketch"]["degrees_of_freedom"].as_i64() == Some(0);
        let selected_color = Color32::from_rgb(236, 204, 96);
        for (element, segment) in self.axis_segments() {
            let color = if self.sketch_selection.contains(&element) {
                selected_color
            } else if element.index == -1 {
                Color32::from_rgb(120, 65, 75)
            } else {
                Color32::from_rgb(65, 110, 90)
            };
            painter.line_segment(segment, Stroke::new(1_f32, color));
        }
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
                "Parallel" => "\u{2225}",
                "Perpendicular" => "\u{22a5}",
                "Equal" => "=",
                "Tangent" => "T",
                "Coincident" => "\u{00b7}",
                "Block" => "\u{22a0}",
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
            let circular = crate::sketch_dimensions::annotation(sketch, constraint);
            if let Some(crate::sketch_dimensions::Witness::Arc {
                center,
                start,
                radius,
                sweep,
            }) = &circular
            {
                let center = Vec3::from_array(center.map(|n| n as f32));
                let start = Vec3::from_array(start.map(|n| n as f32));
                let radial = start - center;
                let start_angle = radial.y.atan2(radial.x);
                let sweep = *sweep as f32;
                let native_radius = *radius as f32;
                let display_radius = native_radius + 20. / self.scale;
                let project = |angle: f32, radius: f32| {
                    self.project(
                        self.sketch_origin
                            + self.sketch_rotation
                                * (center + Vec3::new(angle.cos(), angle.sin(), 0.) * radius),
                    )
                    .0
                };
                for step in 0..48 {
                    segments.push([
                        project(start_angle + sweep * step as f32 / 48., display_radius),
                        project(
                            start_angle + sweep * (step + 1) as f32 / 48.,
                            display_radius,
                        ),
                    ]);
                }
                for angle in [start_angle, start_angle + sweep] {
                    let a = project(angle, native_radius);
                    let b = project(angle, display_radius + 4. / self.scale);
                    segments.push([a, b]);
                    let middle = project(angle, display_radius);
                    // Screen-space ticks follow the projected circular arc.
                    let tangent = (project(angle + 0.01, display_radius)
                        - project(angle - 0.01, display_radius))
                    .normalized()
                        * 4.;
                    segments.push([middle - tangent, middle + tangent]);
                }
                let middle = start_angle + sweep * 0.5;
                let position = project(middle, display_radius + 18. / self.scale);
                let origin = self
                    .project(self.sketch_origin + self.sketch_rotation * center)
                    .0;
                label = Some((
                    position,
                    (position - origin).normalized(),
                    if kind == "Angle" {
                        constraint["quantity"]["display"]
                            .as_str()
                            .unwrap_or("Angle")
                            .to_owned()
                    } else {
                        constraint["quantity"]["display"]
                            .as_str()
                            .unwrap_or("Distance")
                            .to_owned()
                    },
                ));
            } else if matches!(kind, "Distance" | "DistanceX" | "DistanceY") {
                let endpoints = if let Some(crate::sketch_dimensions::Witness::Linear {
                    start,
                    end,
                }) = circular
                {
                    let world = |p: [f64; 3]| {
                        self.sketch_origin
                            + self.sketch_rotation * Vec3::from_array(p.map(|n| n as f32))
                    };
                    Some((world(start), world(end)))
                } else if first_point == 0 {
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
                    let mut direction = if (b - a).length_sq() > 1e-8 {
                        (b - a).normalized()
                    } else {
                        Vec2::new(1., 0.)
                    };
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
                    {
                        // A native zero or subpixel gap still needs a visible,
                        // pickable dimension. Extend only its display leader.
                        let (leader_a, leader_b) = if dimension_a.distance(dimension_b) <= 1. {
                            (dimension_a - direction * 8., dimension_b + direction * 8.)
                        } else {
                            (dimension_a, dimension_b)
                        };
                        segments.extend([
                            [a, dimension_a + normal * 5.],
                            [b, dimension_b + normal * 5.],
                            [leader_a, leader_b],
                        ]);
                        let tick = (direction + normal).normalized() * 4.;
                        for endpoint in [dimension_a, dimension_b] {
                            segments.push([endpoint - tick, endpoint + tick]);
                        }
                        label = Some((
                            dimension_a.lerp(dimension_b, 0.5),
                            normal,
                            constraint["quantity"]["display"]
                                .as_str()
                                .unwrap_or("Distance")
                                .to_owned(),
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
                                "{}{}",
                                if kind == "Diameter" { "\u{2300}" } else { "R" },
                                constraint["quantity"]["display"]
                                    .as_str()
                                    .unwrap_or("Radius")
                            ),
                        ));
                    }
                }
            } else if kind == "SnellsLaw" {
                if let Some(vertex) = self.native_sketch_position(first, first_point) {
                    let vertex = self.project(vertex).0;
                    let position = vertex + Vec2::new(45., -38.);
                    segments.push([vertex, position]);
                    label = Some((
                        position,
                        Vec2::new(1., -1.).normalized(),
                        format!("n2/n1 = {value:.4}"),
                    ));
                }
            } else if kind == "Angle" {
                let line = |index: i32| {
                    self.curves
                        .iter()
                        .find(|curve| curve.native.index == index && curve.points.len() == 2)
                        .map(|curve| (curve.points[0], curve.points[1]))
                        .or_else(|| {
                            self.native_geometry(index)
                                .filter(|g| g["kind"] == "line")
                                .map(|g| {
                                    (
                                        self.sketch_origin
                                            + self.sketch_rotation * vector(&g["start"]),
                                        self.sketch_origin
                                            + self.sketch_rotation * vector(&g["end"]),
                                    )
                                })
                        })
                };
                let via_point = constraint["third"].as_i64().is_some_and(|i| i != -2000);
                let measured = if via_point {
                    let witness = &constraint["angle_via_point"];
                    witness["start_angle"]
                        .as_f64()
                        .filter(|a| a.is_finite())
                        .and_then(|a| {
                            witness["center"]
                                .as_array()
                                .filter(|p| p.len() == 3)
                                .map(|_| (vector(&witness["center"]), a as f32))
                        })
                } else {
                    line(first).map(|(a, b)| {
                        let inverse = self.sketch_rotation.conjugate();
                        if let Some((c, d)) = line(second) {
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
                        }
                    })
                };
                if let Some((mut vertex, start_angle)) = measured {
                    vertex.z = 0.;
                    let radius = 34. / self.scale;
                    let sweep = value as f32;
                    if sweep.is_finite()
                        && (via_point || sweep.abs() <= std::f32::consts::TAU + 0.001)
                    {
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
                        if via_point {
                            let center = self
                                .project(self.sketch_origin + self.sketch_rotation * vertex)
                                .0;
                            segments.push([center, project(start_angle)]);
                            segments.push([center, project(start_angle + sweep)]);
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
                            constraint["quantity"]["display"]
                                .as_str()
                                .unwrap_or("Angle")
                                .to_owned(),
                        ));
                    }
                }
            }
            if let Some((position, normal, mut text)) = label {
                if constraint["driving"] == false {
                    text.push_str(" ref");
                }
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
    fn native_axes_are_whole_curve_fallback_picks_without_endpoint_handles() {
        let mut viewport = viewport();
        let mut sketch = snapshot();
        sketch["global_placement"] = json!({"position":[100,200,300],"rotation":[0,0,1,0]});
        sketch["sketch"]["axes"] = json!([
            {"index":-1,"kind":"line","start":[0,0,0],"end":[1,0,0],"handles":[]},
            {"index":-2,"kind":"line","start":[0,0,0],"end":[0,1,0],"handles":[]}
        ]);
        viewport.set_sketch(Some(sketch.clone()));
        viewport.align_sketch();
        for (point, index) in [([50., 0., 0.], -1), ([0., 50., 0.], -2)] {
            assert_eq!(
                viewport.sketch_hit(viewport.sketch_screen_point(point)),
                Some(SketchElement { index, point: 0 })
            );
        }
        assert_eq!(
            viewport.sketch_hit(viewport.sketch_screen_point([0., 0., 0.])),
            Some(SketchElement {
                index: -1,
                point: 1
            })
        );
        assert!(
            viewport
                .geometry_point(SketchElement {
                    index: -2,
                    point: 1
                })
                .is_none()
        );
        viewport.sketch_selection = vec![SketchElement {
            index: -2,
            point: 0,
        }];
        viewport.set_sketch(Some(sketch));
        assert_eq!(
            viewport.sketch_selection,
            vec![SketchElement {
                index: -2,
                point: 0
            }]
        );
    }

    #[test]
    fn via_point_annotations_use_native_witnesses_and_keep_signed_sweeps_pickable() {
        for value in [-std::f64::consts::FRAC_PI_4, 0., std::f64::consts::PI * 2.5] {
            let mut viewport = viewport();
            let mut sketch = snapshot();
            sketch["global_placement"] = json!({"position":[100,200,300],"rotation":[0,0,1,0]});
            sketch["sketch"]["constraints"] = json!([{"index":12,"type":"Angle","third":-1,"third_pos":1,
                "first":0,"second":1,"value":value,"driving":true,
                "angle_via_point":{"center":[0,0,0],"start_angle":0.25}}]);
            viewport.set_sketch(Some(sketch));
            viewport.align_sketch();
            let context = egui::Context::default();
            let _ = context.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| viewport.draw_dimensions(ui.painter()));
            });
            let hit = &viewport.dimension_hits[0];
            assert_eq!(hit.index, 12);
            assert!(hit.label.is_finite());
            assert_eq!(viewport.dimension_hit(hit.label.center()), Some(12));
            let center = viewport.sketch_screen_point([0., 0., 0.]);
            assert!(hit.segments.iter().any(|s| s[0].distance(center) < 1e-4));
        }
    }
    #[test]
    fn curve_pick_uses_actual_attached_sketch_cursor_and_invalidates_with_native_changes() {
        let mut viewport = viewport();
        let mut sketch = snapshot();
        sketch["global_placement"] = json!({"position":[100,200,300],"rotation":[0,0,1,0]});
        viewport.set_sketch(Some(sketch.clone()));
        viewport.align_sketch();
        let edge = SketchElement { index: 0, point: 0 };
        let pick = [13., 10.25, 0.]; // Within edge tolerance, far from its midpoint and endpoint handles.
        assert_eq!(
            viewport.select_sketch_at(viewport.sketch_screen_point(pick), false),
            Some(edge)
        );
        let captured = viewport.sketch_pick_position(edge).unwrap();
        assert!(
            captured
                .iter()
                .zip(pick)
                .all(|(actual, expected)| (actual - expected).abs() < 1e-4)
        );
        assert_ne!(Some(captured), viewport.geometry_point(edge));
        viewport.set_sketch(Some(sketch.clone()));
        assert_eq!(viewport.sketch_pick_position(edge), Some(captured));
        let circle = SketchElement { index: 1, point: 0 };
        viewport.select_sketch_at(viewport.sketch_screen_point([25., 25., 0.]), true);
        assert!(viewport.sketch_pick_position(circle).is_some());
        assert_eq!(viewport.sketch_pick_position(edge), Some(captured));
        viewport.select_sketch_at(viewport.sketch_screen_point(pick), true);
        assert_eq!(viewport.sketch_pick_position(edge), None);
        viewport.select_sketch_at(viewport.sketch_screen_point(pick), true);
        sketch["sketch"]["geometry"][0]["end"] = json!([35, 10, 0]);
        viewport.set_sketch(Some(sketch));
        assert!(viewport.sketch_selection.contains(&edge));
        assert_eq!(viewport.sketch_pick_position(edge), None);
        assert_eq!(viewport.sketch_pick_position(circle), None);
        viewport.select_sketch_at(viewport.sketch_screen_point(pick), false);
        viewport.clear_sketch_selection();
        assert_eq!(viewport.sketch_pick_position(edge), None);
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
    fn dimension_witness_lines_do_not_steal_native_endpoints_or_curve_picks() {
        let mut viewport = viewport();
        let mut sketch = snapshot();
        sketch["sketch"]["constraints"] = json!([
            {"index":7,"type":"DistanceX","first":0,"first_pos":1,"second":0,"second_pos":2,"value":20}
        ]);
        viewport.set_sketch(Some(sketch));
        let context = egui::Context::default();
        let _ = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| viewport.draw_dimensions(ui.painter()));
        });
        let point = SketchElement { index: 0, point: 2 };
        let pos = viewport.sketch_screen_point(viewport.geometry_point(point).unwrap());
        assert!(
            viewport.dimension_hits[0]
                .segments
                .iter()
                .any(|[a, b]| segment_distance(pos, *a, *b).0 < 1.)
        );
        assert_eq!(viewport.dimension_hit(pos), None);
        assert_eq!(viewport.select_sketch_at(pos, false), Some(point));
        assert_eq!(viewport.sketch_selection, vec![point]);
        let label = viewport.dimension_hits[0].label.center();
        assert_eq!(viewport.dimension_hit(label), Some(7));
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
    #[test]
    fn zero_and_subpixel_clearances_keep_finite_pickable_labels() {
        for gap in [0., 0.01, -0.01] {
            for family in 0..3 {
                let (geometry, first_point) = match family {
                    0 => (
                        json!([
                            {"index":0,"kind":"circle","center":[0,0,0],"radius":2},
                            {"index":1,"kind":"circle","center":[5.+gap,0,0],"radius":3}
                        ]),
                        0,
                    ),
                    1 => (
                        json!([
                            {"index":0,"kind":"circle","center":[0,2.+gap,0],"radius":2},
                            {"index":1,"kind":"line","start":[-10,0,0],"end":[10,0,0]}
                        ]),
                        0,
                    ),
                    _ => (
                        json!([
                            {"index":0,"kind":"point","position":[2.+gap,0,0]},
                            {"index":1,"kind":"circle","center":[0,0,0],"radius":2}
                        ]),
                        1,
                    ),
                };
                for moved in [false, true] {
                    let mut sketch = json!({"name":"Sketch","sketch":{
                        "geometry":geometry,"constraints":[{"index":4,"type":"Distance",
                        "first":0,"first_pos":first_point,"second":1,"second_pos":0,
                        "value":gap,"driving":false}]}});
                    if moved {
                        sketch["global_placement"] = json!({"position":[100,200,300],
                            "rotation":[0,0,1,0]});
                    }
                    let mut viewport = Viewport::default();
                    viewport.set_sketch(Some(sketch));
                    viewport.align_sketch();
                    let context = egui::Context::default();
                    let _ = context.run(egui::RawInput::default(), |ctx| {
                        egui::CentralPanel::default()
                            .show(ctx, |ui| viewport.draw_dimensions(ui.painter()));
                    });
                    assert_eq!(viewport.dimension_hits.len(), 1, "{gap}/{family}/{moved}");
                    let hit = &viewport.dimension_hits[0];
                    assert!(hit.label.is_finite());
                    assert!(hit.segments.iter().flatten().all(|point| point.is_finite()));
                    assert_eq!(viewport.dimension_hit(hit.label.center()), Some(4));
                    assert!(hit.segments[2][0].distance(hit.segments[2][1]) >= 16.);
                }
            }
        }
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
    #[test]
    fn circular_clearance_annotation_uses_boundary_witnesses_and_native_hit_identity() {
        let mut viewport = viewport();
        let sketch = json!({"name":"Sketch","sketch":{"geometry":[
            {"index":0,"kind":"circle","center":[0,0,0],"radius":2},
            {"index":1,"kind":"circle","center":[10,0,0],"radius":3}],
            "constraints":[{"index":12,"type":"Distance","first":0,"first_pos":0,"second":1,"second_pos":0,"value":5}]}});
        viewport.set_sketch(Some(sketch));
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| viewport.draw_dimensions(ui.painter()));
        });
        let hit = &viewport.dimension_hits[0];
        assert_eq!(hit.index, 12);
        assert!(hit.segments[0][0].distance(viewport.sketch_screen_point([2., 0., 0.])) < 1e-5);
        assert!(hit.segments[1][0].distance(viewport.sketch_screen_point([7., 0., 0.])) < 1e-5);
        assert_eq!(viewport.dimension_hit(hit.label.center()), Some(12));
        assert_eq!(
            viewport.dimension_hit(hit.segments[2][0].lerp(hit.segments[2][1], 0.5)),
            Some(12)
        );
    }
    #[test]
    fn major_arc_length_and_angle_have_pickable_annotations_after_attachment_rotation() {
        for kind in ["Distance", "Angle"] {
            let mut viewport = viewport();
            let sketch = json!({"name":"Sketch","global_placement":{"position":[12,4,7],"rotation":[0,0,std::f64::consts::FRAC_1_SQRT_2,std::f64::consts::FRAC_1_SQRT_2]},
                "sketch":{"geometry":[{"index":0,"kind":"arc","center":[0,0,0],"radius":5,"axis":[0,0,1],
                "start":[5,0,0],"mid":[-2.0807341827,4.546487134,0],"end":[-3.268218104,-3.784012476,0],"first_parameter":0.,"last_parameter":4.}],
                "constraints":[{"index":4,"type":kind,"first":0,"first_pos":0,"value":if kind=="Angle" {4.} else {20.},"driving":false}]}});
            viewport.set_sketch(Some(sketch));
            viewport.align_sketch();
            let ctx = egui::Context::default();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| viewport.draw_dimensions(ui.painter()));
            });
            assert_eq!(viewport.dimension_hits.len(), 1);
            let hit = &viewport.dimension_hits[0];
            assert_eq!(viewport.dimension_hit(hit.label.center()), Some(4));
            assert!(hit.segments.len() >= 52);
            assert_eq!(
                viewport.dimension_hit(hit.segments[12][0].lerp(hit.segments[12][1], 0.5)),
                Some(4)
            );
            let a = hit.segments[0][0];
            let b = hit.segments[47][1];
            assert!(a.distance(b) > 20.);
        }
    }
    #[test]
    fn refraction_ratio_label_is_pickable_at_native_endpoint_after_rotation() {
        let mut viewport = viewport();
        let sketch = json!({"name":"Sketch","global_placement":{"position":[12,4,7],"rotation":[0,0,std::f64::consts::FRAC_1_SQRT_2,std::f64::consts::FRAC_1_SQRT_2]},
            "sketch":{"geometry":[{"index":0,"kind":"line","start":[-5,-5,0],"end":[0,0,0]}],
            "constraints":[{"index":7,"type":"SnellsLaw","first":0,"first_pos":2,"value":1.5}]}});
        viewport.set_sketch(Some(sketch));
        viewport.align_sketch();
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| viewport.draw_dimensions(ui.painter()));
        });
        let hit = &viewport.dimension_hits[0];
        assert_eq!(viewport.dimension_hit(hit.label.center()), Some(7));
        assert!(hit.segments[0][0].distance(viewport.sketch_screen_point([0., 0., 0.])) < 1e-5);
    }
    #[test]
    fn shift_click_can_select_both_coincident_ray_endpoints_in_order() {
        let mut viewport = viewport();
        viewport.set_sketch(Some(json!({"name":"Sketch","sketch":{"geometry":[
            {"index":0,"kind":"line","start":[-5,-5,0],"end":[2,3,0]},
            {"index":1,"kind":"line","start":[2,3,0],"end":[5,5,0]}]}})));
        let pos = viewport.sketch_screen_point([2., 3., 0.]);
        assert_eq!(
            viewport.select_sketch_at(pos, false),
            Some(SketchElement { index: 0, point: 2 })
        );
        assert_eq!(
            viewport.select_sketch_at(pos, true),
            Some(SketchElement { index: 1, point: 1 })
        );
        assert_eq!(
            viewport.sketch_selection,
            vec![
                SketchElement { index: 0, point: 2 },
                SketchElement { index: 1, point: 1 }
            ]
        );
        assert_eq!(
            viewport.select_sketch_at(pos, true),
            Some(SketchElement { index: 0, point: 2 })
        );
        assert_eq!(
            viewport.sketch_selection,
            vec![SketchElement { index: 1, point: 1 }]
        );
        assert_eq!(
            viewport.select_sketch_at(pos, true),
            Some(SketchElement { index: 1, point: 1 })
        );
        assert!(viewport.sketch_selection.is_empty());
        viewport.select_sketch_at(pos, true);
        viewport.select_sketch_at(pos, true);
        assert_eq!(
            viewport.sketch_selection,
            vec![
                SketchElement { index: 0, point: 2 },
                SketchElement { index: 1, point: 1 }
            ]
        );
    }

    #[test]
    fn coincident_shift_cycle_visits_arc_center_and_projected_ids_without_reordering() {
        let elements = [
            SketchElement { index: 0, point: 2 },
            SketchElement { index: 1, point: 1 },
            SketchElement { index: 2, point: 1 },
            SketchElement { index: 3, point: 3 },
            SketchElement {
                index: -3,
                point: 1,
            },
        ];
        for count in [3, 4, 5] {
            for rotated in [false, true] {
                let mut sketch = json!({"name":"Sketch","sketch":{
                    "geometry":[
                        {"index":0,"kind":"line","start":[-5,-5,0],"end":[2,3,0]},
                        {"index":1,"kind":"line","start":[2,3,0],"end":[5,5,0]},
                        {"index":2,"kind":"arc","start":[2,3,0],"mid":[3,4,0],"end":[4,3,0],"center":[3,3,0],"radius":1},
                        {"index":3,"kind":"circle","center":[2,3,0],"radius":5}],
                    "externals":[{"index":0,"geometry_index":-3,"persistent_reference":"Master.Edge1",
                        "geometry":{"index":-3,"kind":"line","start":[2,3,0],"end":[6,3,0]}}]}});
                if count < 5 {
                    sketch["sketch"]["externals"] = json!([]);
                }
                if count < 4 {
                    sketch["sketch"]["geometry"].as_array_mut().unwrap().pop();
                }
                if rotated {
                    sketch["global_placement"] = json!({"position":[12,4,7],
                        "rotation":[0,0,std::f64::consts::FRAC_1_SQRT_2,std::f64::consts::FRAC_1_SQRT_2]});
                }
                let mut viewport = Viewport::default();
                viewport.set_sketch(Some(sketch.clone()));
                viewport.align_sketch();
                let pos = viewport.sketch_screen_point([2., 3., 0.]);
                for step in 0..(4 * count + 1) {
                    // Identical refreshes must not restart an in-progress cycle.
                    viewport.set_sketch(Some(sketch.clone()));
                    assert_eq!(
                        viewport.select_sketch_at(pos, step != 0),
                        Some(elements[step % count])
                    );
                    let phase = step % (2 * count);
                    let expected = if phase < count {
                        &elements[..phase + 1]
                    } else {
                        &elements[phase - count + 1..count]
                    };
                    assert_eq!(
                        viewport.sketch_selection, expected,
                        "count {count}, rotated {rotated}, step {step}"
                    );
                    for element in expected {
                        assert!(viewport.sketch_pick_position(*element).is_some());
                    }
                }
                if count == 5 {
                    assert!(!viewport.sketch_draggable(elements[4]));
                }
            }
        }
    }

    #[test]
    fn coincident_cycle_resets_for_clear_other_picks_and_changed_native_snapshot() {
        let mut viewport = viewport();
        let mut sketch = json!({"name":"Sketch","sketch":{"geometry":[
            {"index":0,"kind":"line","start":[-5,-5,0],"end":[2,3,0]},
            {"index":1,"kind":"line","start":[2,3,0],"end":[5,5,0]}]}});
        viewport.set_sketch(Some(sketch.clone()));
        let pos = viewport.sketch_screen_point([2., 3., 0.]);
        let a = SketchElement { index: 0, point: 2 };
        let b = SketchElement { index: 1, point: 1 };
        viewport.select_sketch_at(pos, false);
        viewport.select_sketch_at(pos, true);
        viewport.clear_sketch_selection();
        assert_eq!(viewport.select_sketch_at(pos, true), Some(a));
        viewport.select_sketch_element(Some(b), false);
        assert_eq!(viewport.select_sketch_at(pos, true), Some(a));
        assert_eq!(viewport.sketch_selection, vec![b, a]);
        // A changed native snapshot starts a new cycle, preserving surviving IDs.
        sketch["sketch"]["geometry"][1]["end"] = json!([6, 5, 0]);
        viewport.set_sketch(Some(sketch));
        assert_eq!(viewport.select_sketch_at(pos, true), Some(a));
        assert_eq!(viewport.sketch_selection, vec![b]);
        let other = viewport.sketch_screen_point([-5., -5., 0.]);
        viewport.select_sketch_at(other, false);
        assert_eq!(viewport.select_sketch_at(pos, true), Some(a));
    }

    #[test]
    fn coincident_cycle_keeps_origin_ring_and_isolated_handle_toggle_targets() {
        let mut viewport = viewport();
        viewport.set_sketch(Some(json!({"name":"Sketch","sketch":{"geometry":[
            {"index":0,"kind":"line","start":[0,0,0],"end":[2,3,0]}]}})));
        for pos in [
            viewport.sketch_screen_point([2., 3., 0.]),
            viewport.sketch_screen_point([0.; 3]) + Vec2::new(0., 6.),
        ] {
            let target = viewport.sketch_hit(pos);
            assert!(target.is_some());
            for step in 0..6 {
                assert_eq!(viewport.select_sketch_at(pos, step != 0), target);
                assert_eq!(viewport.sketch_selection.len(), usize::from(step % 2 == 0));
            }
        }
    }
}
