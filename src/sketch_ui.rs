use crate::{
    dimensions::{self, Selection, Sub},
    model::{Plane, Sketch, SketchConstraint, fmt},
    plugins::Numbers,
    render::{Camera, Projector, point_segment_distance},
    sketch::ResolvedSketch,
};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use std::collections::BTreeMap;

const BLUE: Color32 = Color32::from_rgb(100, 170, 250);
const WHITE: Color32 = Color32::from_rgb(221, 230, 240);
const ORANGE: Color32 = Color32::from_rgb(255, 198, 102);
const RED: Color32 = Color32::from_rgb(247, 119, 132);
const MUTED: Color32 = Color32::from_rgb(144, 159, 180);
#[derive(Clone, Copy, PartialEq, Default)]
pub enum Tool {
    #[default]
    Select,
    Line,
    Rectangle,
    Circle,
    Dimension,
}
impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Line => "Line",
            Self::Rectangle => "Rectangle",
            Self::Circle => "Circle",
            Self::Dimension => "Dimension (D)",
        }
    }
}
#[derive(Clone, Default)]
struct Snap {
    p: [f64; 2],
    target: Option<Selection>,
    x_axis: bool,
    y_axis: bool,
}
struct DimensionEdit {
    constraint: SketchConstraint,
    index: Option<usize>,
    text: String,
    focus: bool,
    error: Option<String>,
}
enum Drag {
    Geometry {
        entity: u64,
        sub: Sub,
        start: [f64; 2],
        original: Sketch,
        checkpoint: bool,
    },
    Label {
        index: usize,
        checkpoint: bool,
    },
}
#[derive(Default)]
pub struct Edit {
    pub changed: bool,
    pub checkpoint: bool,
    pub live: bool,
}
impl Edit {
    fn change(&mut self, checkpoint: bool) {
        self.changed = true;
        self.checkpoint |= checkpoint;
    }
}
pub struct SketchUi {
    pub tool: Tool,
    pub inference: bool,
    pub selected: Vec<Selection>,
    pub show_constraints: bool,
    first: Option<Snap>,
    picks: Vec<Selection>,
    dimension: Option<DimensionEdit>,
    selected_dimension: Option<usize>,
    drag: Option<Drag>,
    last_good: Option<ResolvedSketch>,
    labels: Vec<(usize, Rect)>,
    pub hint: String,
}
impl Default for SketchUi {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            inference: true,
            selected: vec![],
            show_constraints: true,
            first: None,
            picks: vec![],
            dimension: None,
            selected_dimension: None,
            drag: None,
            last_good: None,
            labels: vec![],
            hint: String::new(),
        }
    }
}
impl SketchUi {
    pub fn cancel_gesture(&mut self) {
        self.hint.clear();
        self.first = None;
        self.picks.clear();
        self.dimension = None;
        self.drag = None;
        self.tool = Tool::Select;
    }
    pub fn set_tool(&mut self, tool: Tool) {
        self.cancel_gesture();
        self.tool = tool;
        self.selected_dimension = None;
    }
    pub fn toolbar(&mut self, ui: &mut egui::Ui) {
        for tool in [
            Tool::Select,
            Tool::Line,
            Tool::Rectangle,
            Tool::Circle,
            Tool::Dimension,
        ] {
            if ui
                .selectable_label(self.tool == tool, tool.name())
                .clicked()
            {
                self.set_tool(tool);
            }
        }
        ui.separator();
        ui.checkbox(&mut self.inference, "Inferences")
            .on_hover_text(
                "Snap to the origin, endpoints, centers and horizontal/vertical alignments.",
            );
        ui.checkbox(&mut self.show_constraints, "Constraints");
    }
    pub fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.dimension.take().is_none() {
            self.cancel_gesture();
        }
        for (key, tool) in [
            (egui::Key::D, Tool::Dimension),
            (egui::Key::L, Tool::Line),
            (egui::Key::R, Tool::Rectangle),
            (egui::Key::C, Tool::Circle),
            (egui::Key::V, Tool::Select),
        ] {
            if ctx.input(|i| i.key_pressed(key)) {
                self.set_tool(tool);
            }
        }
    }
    pub fn inspector(
        &mut self,
        ui: &mut egui::Ui,
        sketch: &mut Sketch,
        resolved: Option<&ResolvedSketch>,
        error: Option<&str>,
    ) -> bool {
        if let Some(s) = resolved {
            self.last_good = Some(s.clone());
        }
        let mut changed = false;
        ui.separator();
        if let Some(e) = error {
            ui.colored_label(RED, e);
        } else if let Some(s) = &self.last_good {
            if s.dof == 0 && !s.entities.is_empty() {
                ui.colored_label(WHITE, "Fully constrained");
            } else if !s.entities.is_empty() {
                ui.colored_label(BLUE, format!("{} degrees of freedom", s.dof));
            }
        }
        ui.add_space(8.);
        ui.horizontal_wrapped(|ui| {
            for name in [
                "Coincident",
                "Horizontal",
                "Vertical",
                "Parallel",
                "Perpendicular",
                "Equal",
                "Concentric",
                "Tangent",
                "Fix",
            ] {
                if ui
                    .add_enabled(!self.selected.is_empty(), egui::Button::new(name).small())
                    .clicked()
                {
                    match self.relationship(name, sketch) {
                        Ok(c) => {
                            self.hint.clear();
                            seed_from_resolved(sketch, self.last_good.as_ref());
                            if !sketch.constraints.iter().any(|x| x == &c) {
                                sketch.constraints.push(c);
                                changed = true;
                            }
                        }
                        Err(e) => self.hint = e,
                    }
                }
            }
        });
        if !self.selected.is_empty() {
            let ids: Vec<_> = self.selected.iter().map(|s| s.entity).collect();
            let mut construction = sketch
                .entities
                .iter()
                .filter(|e| ids.contains(&e.id))
                .all(|e| e.construction);
            if ui.checkbox(&mut construction, "Construction").changed() {
                for e in &mut sketch.entities {
                    if ids.contains(&e.id) {
                        e.construction = construction;
                    }
                }
                changed = true;
            }
        }
        ui.separator();
        let mut remove = None;
        let mut edit = None;
        for (index, c) in sketch.constraints.iter_mut().enumerate() {
            let related = self.selected.is_empty()
                || self
                    .selected
                    .iter()
                    .any(|s| s.entity == c.a || s.entity == c.b);
            if !related {
                continue;
            }
            ui.push_id(index, |ui| {
                ui.horizontal(|ui| {
                    let dimension = dimensions::is_dimension(&c.kind);
                    let text = if dimension {
                        format!(
                            "{}  {}{}",
                            c.kind,
                            c.value,
                            if c.reference { " (ref)" } else { "" }
                        )
                    } else {
                        c.kind.clone()
                    };
                    let r = ui.selectable_label(self.selected_dimension == Some(index), text);
                    if r.clicked() {
                        self.selected_dimension = Some(index);
                    }
                    if dimension && r.double_clicked() {
                        edit = Some(index);
                    }
                    if ui.small_button("×").clicked() {
                        remove = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove {
            seed_from_resolved(sketch, self.last_good.as_ref());
            sketch.constraints.remove(index);
            self.selected_dimension = None;
            changed = true;
        }
        if let Some(index) = edit {
            self.edit_dimension(sketch, index);
        }
        if !self.hint.is_empty() {
            ui.label(egui::RichText::new(&self.hint).small().color(ORANGE));
        }
        changed
    }
    fn relationship(&self, name: &str, sketch: &Sketch) -> Result<SketchConstraint, String> {
        let a = *self.selected.first().ok_or("Select sketch geometry")?;
        let b = self.selected.get(1).copied();
        let mut c = SketchConstraint {
            kind: name.into(),
            a: a.entity,
            b: b.map_or(0, |s| s.entity),
            points: [a.index(), b.map_or(0, Selection::index)],
            ..Default::default()
        };
        match name {
            "Fix" => {
                if a.entity == 0 {
                    return Err("The origin is already fixed".into());
                }
            }
            "Horizontal" | "Vertical" => {
                if matches!(a.sub, Sub::Point(_)) {
                    let b = b.ok_or("Select two points")?;
                    if !matches!(b.sub, Sub::Point(_)) {
                        return Err("Select two points, or one line".into());
                    }
                    c.kind = format!("{name}Points");
                } else if let Some(sk) = &self.last_good {
                    dimensions::line(sk, a.entity, a.index())?;
                }
            }
            "Concentric" => {
                let b = b.ok_or("Select two circles")?;
                if ![a.entity, b.entity].into_iter().all(|id| {
                    sketch
                        .entities
                        .iter()
                        .any(|e| e.id == id && e.kind == "circle")
                }) {
                    return Err("Concentric requires two circles".into());
                }
            }
            "Equal" | "Parallel" | "Perpendicular" => {
                let b = b.ok_or("Select two edges")?;
                if !matches!(a.sub, Sub::Edge(_)) || !matches!(b.sub, Sub::Edge(_)) {
                    return Err("Select edges for this relationship".into());
                }
                if (name != "Equal"
                    || ![a.entity, b.entity].into_iter().all(|id| {
                        sketch
                            .entities
                            .iter()
                            .any(|e| e.id == id && e.kind == "circle")
                    }))
                    && let Some(sk) = &self.last_good
                {
                    dimensions::line(sk, a.entity, a.index())?;
                    dimensions::line(sk, b.entity, b.index())?;
                }
            }
            "Coincident" => {
                let b = b.ok_or("Select two points, or a point and a curve")?;
                match (a.sub, b.sub) {
                    (Sub::Point(_), Sub::Point(_)) => (),
                    (Sub::Edge(_), Sub::Point(_)) => {
                        c.a = b.entity;
                        c.b = a.entity;
                        c.points.swap(0, 1);
                        c.kind = if sketch
                            .entities
                            .iter()
                            .any(|e| e.id == a.entity && e.kind == "circle")
                        {
                            "PointOnCircle"
                        } else {
                            "PointOnLine"
                        }
                        .into();
                    }
                    (Sub::Point(_), Sub::Edge(_)) => {
                        c.kind = if sketch
                            .entities
                            .iter()
                            .any(|e| e.id == b.entity && e.kind == "circle")
                        {
                            "PointOnCircle"
                        } else {
                            "PointOnLine"
                        }
                        .into();
                    }
                    _ => return Err("Coincident needs points, or a point and a curve".into()),
                }
            }
            "Tangent" => {
                let b = b.ok_or("Select a line and a circle")?;
                let s = self
                    .last_good
                    .as_ref()
                    .ok_or("Wait for the sketch to solve")?;
                let (line, circle) = if sketch
                    .entities
                    .iter()
                    .any(|e| e.id == a.entity && e.kind == "circle")
                {
                    (b, a)
                } else {
                    (a, b)
                };
                c.negative = dimensions::signed_distance(
                    dimensions::point(s, circle.entity, 0)?,
                    dimensions::line(s, line.entity, line.index())?,
                ) < 0.;
            }
            _ => {
                if b.is_none() {
                    return Err("Select two suitable entities with Ctrl-click".into());
                }
            }
        }
        Ok(c)
    }
    fn edit_dimension(&mut self, sketch: &Sketch, index: usize) {
        if let Some(c) = sketch.constraints.get(index) {
            self.dimension = Some(DimensionEdit {
                constraint: c.clone(),
                index: Some(index),
                text: c.value.clone(),
                focus: true,
                error: None,
            });
            self.tool = Tool::Dimension;
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn canvas(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        response: &egui::Response,
        camera: &Camera,
        sketch: &mut Sketch,
        resolved: Option<&ResolvedSketch>,
        plane: Plane,
        vars: &BTreeMap<String, String>,
        conflict: bool,
    ) -> Edit {
        if let Some(s) = resolved {
            self.last_good = Some(s.clone());
        }
        let sk = self.last_good.clone().unwrap_or(ResolvedSketch {
            entities: vec![],
            plane,
            dof: 0,
        });
        let plane = &sk.plane;
        let projector = camera.projector(rect);
        let pointer = response.hover_pos();
        let raw = pointer.and_then(|p| camera.plane_point(p, rect, plane));
        let hover = pointer.is_some_and(|p| p.y > rect.top() + 44.);
        let hit = pointer.and_then(|p| pick(&sk, &projector, rect, p));
        let snap = raw.map(|p| {
            snap(
                &sk,
                &projector,
                rect,
                p,
                self.inference,
                self.first.as_ref(),
            )
        });
        let mut edit = Edit::default();
        self.labels.clear();
        paint_geometry(ui, &sk, &projector, rect, &self.selected, hit, conflict);
        for (index, c) in sketch.constraints.iter().enumerate() {
            if dimensions::is_dimension(&c.kind) {
                if let Some(label) = paint_dimension(
                    ui,
                    &sk,
                    c,
                    &projector,
                    rect,
                    None,
                    self.selected_dimension == Some(index),
                ) {
                    self.labels.push((index, label));
                }
            } else if self.show_constraints {
                paint_constraint(ui, &sk, c, &projector, rect);
            }
        }
        let label_hit = pointer.and_then(|p| {
            self.labels
                .iter()
                .rev()
                .find(|(_, r)| r.expand(4.).contains(p))
                .map(|(i, _)| *i)
        });
        if self.tool == Tool::Dimension
            && self.dimension.is_none()
            && let Some(p) = raw
            && let Ok(c) = dimensions::infer(&sk, &self.picks, p)
        {
            paint_dimension(ui, &sk, &c, &projector, rect, Some(ORANGE), false);
        }
        if let (Some(first), Some(end)) = (&self.first, &snap) {
            let pts = match self.tool {
                Tool::Rectangle => vec![
                    first.p,
                    [end.p[0], first.p[1]],
                    end.p,
                    [first.p[0], end.p[1]],
                    first.p,
                ],
                Tool::Circle => {
                    let r = (first.p[0] - end.p[0]).hypot(first.p[1] - end.p[1]);
                    (0..=96)
                        .map(|i| {
                            let t = i as f64 * std::f64::consts::TAU / 96.;
                            [first.p[0] + r * t.cos(), first.p[1] + r * t.sin()]
                        })
                        .collect()
                }
                _ => vec![first.p, end.p],
            };
            ui.painter().add(egui::Shape::line(
                pts.iter()
                    .map(|p| projector.project(plane.at(*p), rect))
                    .collect(),
                Stroke::new(1.5f32, ORANGE),
            ));
        }
        if hover && let Some(snap) = &snap {
            let p = projector.project(plane.at(snap.p), rect);
            if self.tool != Tool::Select && self.tool != Tool::Dimension {
                ui.painter().circle_stroke(
                    p,
                    4.,
                    Stroke::new(1.0f32, if snap.target.is_some() { ORANGE } else { BLUE }),
                );
                if snap.target.is_some() {}
            }
        }
        if response.double_clicked()
            && hover
            && let Some(index) = label_hit
        {
            self.edit_dimension(sketch, index);
        } else if response.clicked() && hover && self.dimension.is_none() {
            if let Some(index) = label_hit {
                self.selected_dimension = Some(index);
                self.selected.clear();
            } else if self.tool == Tool::Dimension {
                if let Some(h) = hit.filter(|h| !self.picks.contains(h)) {
                    if self.picks.len() < 2 {
                        self.picks.push(h);
                        self.selected = self.picks.clone();
                    }
                } else if let Some(p) = raw {
                    match dimensions::infer(&sk, &self.picks, p) {
                        Ok(c) => {
                            let existing = sketch.constraints.iter().position(|x| {
                                !x.reference
                                    && x.kind == c.kind
                                    && x.a == c.a
                                    && x.b == c.b
                                    && x.points == c.points
                            });
                            if let Some(i) = existing {
                                self.edit_dimension(sketch, i);
                            } else {
                                self.dimension = Some(DimensionEdit {
                                    text: c.value.clone(),
                                    constraint: c,
                                    index: None,
                                    focus: true,
                                    error: None,
                                });
                            }
                        }
                        Err(e) => self.hint = e,
                    }
                }
            } else if self.tool == Tool::Select {
                self.selected_dimension = None;
                if !ui.input(|i| i.modifiers.ctrl) {
                    self.selected.clear();
                }
                if let Some(h) = hit {
                    if self.selected.contains(&h) {
                        self.selected.retain(|s| s != &h);
                    } else {
                        self.selected.push(h);
                    }
                }
            } else if let Some(snap) = snap.clone() {
                if let Some(first) = self.first.take() {
                    if (first.p[0] - snap.p[0]).hypot(first.p[1] - snap.p[1]) > 1e-7 {
                        seed_from_resolved(sketch, Some(&sk));
                        match self.tool {
                            Tool::Rectangle => sketch.add_rect(first.p, snap.p),
                            Tool::Circle => sketch.add_circle(
                                first.p,
                                (first.p[0] - snap.p[0]).hypot(first.p[1] - snap.p[1]),
                            ),
                            Tool::Line => sketch.add_line(first.p, snap.p),
                            _ => (),
                        };
                        let id = sketch.next_id;
                        let e = sketch.entities.last().unwrap();
                        let first_index = if e.kind == "rectangle" {
                            let ps = rect_points(&e.values);
                            ps.iter()
                                .position(|p| (p[0] - first.p[0]).hypot(p[1] - first.p[1]) < 1e-5)
                                .unwrap_or(0)
                        } else {
                            0
                        };
                        link(sketch, id, first_index, &first);
                        if self.tool == Tool::Line {
                            link(sketch, id, 1, &snap);
                            if (first.p[0] - snap.p[0]).abs() < 1e-7 {
                                add_relation(
                                    sketch,
                                    SketchConstraint {
                                        kind: "Vertical".into(),
                                        a: id,
                                        ..Default::default()
                                    },
                                );
                            } else if (first.p[1] - snap.p[1]).abs() < 1e-7 {
                                add_relation(
                                    sketch,
                                    SketchConstraint {
                                        kind: "Horizontal".into(),
                                        a: id,
                                        ..Default::default()
                                    },
                                );
                            }
                            self.first = Some(Snap {
                                p: snap.p,
                                target: Some(Selection::point(id, 1)),
                                ..Default::default()
                            });
                        }
                        self.selected = vec![Selection::edge(id, 0)];
                        edit.change(true);
                    }
                } else {
                    self.first = Some(snap);
                }
            }
        }
        if self.tool == Tool::Select && self.dimension.is_none() {
            if response.drag_started_by(egui::PointerButton::Primary) && hover {
                let press = ui
                    .input(|i| i.pointer.press_origin())
                    .unwrap_or_else(|| pointer.unwrap());
                let pressed_label = self
                    .labels
                    .iter()
                    .rev()
                    .find(|(_, r)| r.expand(4.).contains(press))
                    .map(|(i, _)| *i);
                let pressed_hit = pick(&sk, &projector, rect, press);
                let pressed_point = camera.plane_point(press, rect, plane);
                if let Some(index) = pressed_label {
                    self.drag = Some(Drag::Label {
                        index,
                        checkpoint: true,
                    });
                } else if let (Some(h), Some(p)) = (pressed_hit, pressed_point)
                    && h.entity > 0
                    && !sketch
                        .constraints
                        .iter()
                        .any(|c| c.kind == "Fix" && c.a == h.entity)
                {
                    seed_from_resolved(sketch, Some(&sk));
                    self.selected = vec![h];
                    self.drag = Some(Drag::Geometry {
                        entity: h.entity,
                        sub: h.sub,
                        start: p,
                        original: sketch.clone(),
                        checkpoint: true,
                    });
                }
            }
            if response.dragged_by(egui::PointerButton::Primary)
                && let (Some(p), Some(drag)) = (raw, &mut self.drag)
            {
                match drag {
                    Drag::Label { index, checkpoint } => {
                        if let Some(c) = sketch.constraints.get_mut(*index) {
                            dimensions::set_label_position(&sk, c, p);
                            edit.change(*checkpoint);
                            *checkpoint = false;
                        }
                    }
                    Drag::Geometry {
                        entity,
                        sub,
                        start,
                        original,
                        checkpoint,
                    } => {
                        let delta = [p[0] - start[0], p[1] - start[1]];
                        if let Some(e) = sketch.entities.iter_mut().find(|e| e.id == *entity) {
                            let original =
                                original.entities.iter().find(|e| e.id == *entity).unwrap();
                            drag_entity(e, original, *sub, delta);
                            if e.kind == "circle" && matches!(sub, Sub::Edge(_)) {
                                let cx = original.values[0].parse::<f64>().unwrap_or(0.);
                                let cy = original.values[1].parse::<f64>().unwrap_or(0.);
                                e.values[2] = fmt((p[0] - cx).hypot(p[1] - cy).max(1e-4));
                            }
                            edit.change(*checkpoint);
                            edit.live = true;
                            *checkpoint = false;
                        }
                    }
                }
            }
            if response.drag_stopped() {
                self.drag = None;
            }
        }
        if hover
            && !ui.ctx().wants_keyboard_input()
            && ui.input(|i| i.key_pressed(egui::Key::Delete))
        {
            if let Some(i) = self.selected_dimension.take() {
                if i < sketch.constraints.len() {
                    seed_from_resolved(sketch, Some(&sk));
                    sketch.constraints.remove(i);
                    edit.change(true);
                }
            } else {
                let ids: Vec<_> = self
                    .selected
                    .iter()
                    .filter(|s| s.entity > 0)
                    .map(|s| s.entity)
                    .collect();
                if !ids.is_empty() {
                    sketch.entities.retain(|e| !ids.contains(&e.id));
                    sketch
                        .constraints
                        .retain(|c| !ids.contains(&c.a) && !ids.contains(&c.b));
                    self.selected.clear();
                    edit.change(true);
                }
            }
        }
        if self.dimension_dialog(ui, &sk, sketch, &projector, rect, vars) {
            edit.change(true);
        }
        let helper = if self.dimension.is_some() {
            ""
        } else {
            match self.tool {
                Tool::Select => "",
                Tool::Dimension => {
                    if self.picks.is_empty() {
                        "Select geometry"
                    } else {
                        "Place dimension"
                    }
                }
                Tool::Line => {
                    if self.first.is_some() {
                        "Next point"
                    } else {
                        "Start point"
                    }
                }
                Tool::Rectangle => {
                    if self.first.is_some() {
                        "Opposite corner"
                    } else {
                        "First corner"
                    }
                }
                Tool::Circle => {
                    if self.first.is_some() {
                        "Radius"
                    } else {
                        "Center"
                    }
                }
            }
        };
        ui.painter().text(
            rect.left_bottom() + Vec2::new(16., -17.),
            egui::Align2::LEFT_BOTTOM,
            helper,
            egui::FontId::proportional(12.),
            MUTED,
        );
        edit
    }
    fn dimension_dialog(
        &mut self,
        ui: &mut egui::Ui,
        sk: &ResolvedSketch,
        sketch: &mut Sketch,
        projector: &Projector,
        rect: Rect,
        vars: &BTreeMap<String, String>,
    ) -> bool {
        let Some(mut draft) = self.dimension.take() else {
            return false;
        };
        let p = dimensions::label_position(sk, &draft.constraint);
        let screen = projector.project(sk.plane.at(p), rect);
        let mut accept = false;
        let mut cancel = false;
        egui::Area::new(egui::Id::new("dimension_input"))
            .fixed_pos(Pos2::new(
                screen.x.clamp(rect.left() + 8., rect.right() - 205.),
                screen.y.clamp(rect.top() + 45., rect.bottom() - 110.),
            ))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(&draft.constraint.kind)
                            .small()
                            .color(MUTED),
                    );
                    let output = egui::TextEdit::singleline(&mut draft.text)
                        .desired_width(145.)
                        .id_salt("value")
                        .show(ui);
                    if draft.focus {
                        output.response.request_focus();
                        let mut state = output.state;
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::two(
                                egui::text::CCursor::new(0),
                                egui::text::CCursor::new(draft.text.chars().count()),
                            )));
                        state.store(ui.ctx(), output.response.id);
                        draft.focus = false;
                    }
                    ui.checkbox(&mut draft.constraint.reference, "Reference dimension");
                    ui.horizontal(|ui| {
                        if ui.small_button("OK").clicked() {
                            accept = true;
                        }
                        if ui.small_button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                    if let Some(e) = &draft.error {
                        ui.colored_label(RED, e);
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        accept = true;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    }
                });
            });
        if accept {
            match Numbers::new(vars).and_then(|n| n.eval(&draft.text)) {
                Ok(value) => {
                    if matches!(
                        draft.constraint.kind.as_str(),
                        "Radius" | "Diameter" | "Length" | "Width" | "Height" | "Distance"
                    ) && value <= 1e-7
                    {
                        draft.error = Some("Dimension must be positive".into());
                    } else {
                        seed_from_resolved(sketch, Some(sk));
                        draft.constraint.value = draft.text;
                        if let Some(index) = draft.index {
                            if let Some(c) = sketch.constraints.get_mut(index) {
                                *c = draft.constraint;
                            }
                        } else {
                            sketch.constraints.push(draft.constraint);
                        }
                        self.picks.clear();
                        self.selected.clear();
                        return true;
                    }
                }
                Err(e) => draft.error = Some(e),
            }
        }
        if !cancel {
            self.dimension = Some(draft);
        } else {
            self.picks.clear();
        }
        false
    }
}

fn add_relation(sk: &mut Sketch, c: SketchConstraint) {
    if !sk
        .constraints
        .iter()
        .any(|x| x.kind == c.kind && x.a == c.a && x.b == c.b && x.points == c.points)
    {
        sk.constraints.push(c);
    }
}
fn link(sk: &mut Sketch, id: u64, index: usize, snap: &Snap) {
    if let Some(target) = snap.target {
        add_relation(
            sk,
            SketchConstraint {
                kind: "Coincident".into(),
                a: id,
                b: target.entity,
                points: [index, target.index()],
                ..Default::default()
            },
        );
    } else {
        if snap.x_axis {
            add_relation(
                sk,
                SketchConstraint {
                    kind: "VerticalPoints".into(),
                    a: id,
                    points: [index, 0],
                    ..Default::default()
                },
            );
        }
        if snap.y_axis {
            add_relation(
                sk,
                SketchConstraint {
                    kind: "HorizontalPoints".into(),
                    a: id,
                    points: [index, 0],
                    ..Default::default()
                },
            );
        }
    }
}
pub fn seed_from_resolved(sketch: &mut Sketch, resolved: Option<&ResolvedSketch>) {
    if let Some(sk) = resolved {
        for e in &mut sketch.entities {
            if let Some(s) = sk.entities.iter().find(|s| s.id == e.id) {
                for (value, &v) in e.values.iter_mut().zip(&s.values) {
                    if value.parse::<f64>().is_ok() {
                        *value = fmt(v);
                    }
                }
            }
        }
    }
}
fn rect_points(values: &[String]) -> Vec<[f64; 2]> {
    let v: Vec<f64> = values.iter().map(|s| s.parse().unwrap_or(0.)).collect();
    vec![
        [v[0], v[1]],
        [v[0] + v[2], v[1]],
        [v[0] + v[2], v[1] + v[3]],
        [v[0], v[1] + v[3]],
    ]
}
fn drag_entity(
    e: &mut crate::model::Entity,
    original: &crate::model::Entity,
    sub: Sub,
    delta: [f64; 2],
) {
    e.values = original.values.clone();
    let mut change = |index: usize, d: f64| {
        if let Ok(v) = original.values[index].parse::<f64>() {
            e.values[index] = fmt(v + d);
        }
    };
    match (e.kind.as_str(), sub) {
        ("line", Sub::Point(n)) => {
            change(n * 2, delta[0]);
            change(n * 2 + 1, delta[1]);
        }
        ("line", Sub::Edge(_)) => {
            change(0, delta[0]);
            change(1, delta[1]);
            change(2, delta[0]);
            change(3, delta[1]);
        }
        ("circle", Sub::Edge(_)) => {
            change(2, delta[0]);
        }
        ("rectangle", Sub::Point(n)) => {
            if n == 0 || n == 3 {
                change(0, delta[0]);
                change(2, -delta[0]);
            } else {
                change(2, delta[0]);
            }
            if n < 2 {
                change(1, delta[1]);
                change(3, -delta[1]);
            } else {
                change(3, delta[1]);
            }
        }
        _ => {
            change(0, delta[0]);
            change(1, delta[1]);
        }
    }
    if e.kind == "rectangle" {
        for index in [2, 3] {
            if let Ok(value) = e.values[index].parse::<f64>() {
                e.values[index] = fmt(value.max(1e-4));
            }
        }
    }
}

fn vertices(sk: &ResolvedSketch) -> Vec<(Selection, [f64; 2])> {
    let mut result = vec![(Selection::point(0, 0), [0., 0.])];
    for e in &sk.entities {
        let count = match e.kind.as_str() {
            "rectangle" => 4,
            "line" => 2,
            _ => 1,
        };
        for n in 0..count {
            if let Ok(p) = dimensions::point(sk, e.id, n) {
                result.push((Selection::point(e.id, n), p));
            }
        }
    }
    result
}
fn pick(sk: &ResolvedSketch, projector: &Projector, rect: Rect, pos: Pos2) -> Option<Selection> {
    let mut nearest = None;
    let mut distance = 7.;
    for (s, p) in vertices(sk) {
        let d = projector.project(sk.plane.at(p), rect).distance(pos);
        if d < distance {
            distance = d;
            nearest = Some(s);
        }
    }
    if nearest.is_some() {
        return nearest;
    }
    distance = 6.;
    for e in &sk.entities {
        let points = e.points();
        for (i, p) in points.windows(2).enumerate() {
            let a = projector.project(sk.plane.at(p[0]), rect);
            let b = projector.project(sk.plane.at(p[1]), rect);
            let d = point_segment_distance(pos, a, b);
            if d < distance {
                distance = d;
                nearest = Some(Selection::edge(
                    e.id,
                    if e.kind == "rectangle" { i } else { 0 },
                ));
            }
        }
    }
    nearest
}
fn snap(
    sk: &ResolvedSketch,
    projector: &Projector,
    rect: Rect,
    p: [f64; 2],
    enabled: bool,
    first: Option<&Snap>,
) -> Snap {
    let mut snap = Snap {
        p,
        ..Default::default()
    };
    if !enabled {
        return snap;
    }
    let cursor = projector.project(sk.plane.at(p), rect);
    let mut closest = 8.;
    for (s, q) in vertices(sk) {
        let d = projector.project(sk.plane.at(q), rect).distance(cursor);
        if d < closest {
            closest = d;
            snap.p = q;
            snap.target = Some(s);
        }
    }
    if snap.target.is_none() {
        let x = projector
            .project(sk.plane.at([0., p[1]]), rect)
            .distance(cursor);
        let y = projector
            .project(sk.plane.at([p[0], 0.]), rect)
            .distance(cursor);
        if x < 6. {
            snap.p[0] = 0.;
            snap.x_axis = true;
        }
        if y < 6. {
            snap.p[1] = 0.;
            snap.y_axis = true;
        }
        if let Some(first) = first {
            if projector
                .project(sk.plane.at([first.p[0], p[1]]), rect)
                .distance(cursor)
                < 6.
            {
                snap.p[0] = first.p[0];
            }
            if projector
                .project(sk.plane.at([p[0], first.p[1]]), rect)
                .distance(cursor)
                < 6.
            {
                snap.p[1] = first.p[1];
            }
        }
    }
    snap
}
pub fn paint_readonly(ui: &egui::Ui, sk: &ResolvedSketch, projector: &Projector, rect: Rect) {
    paint_geometry(ui, sk, projector, rect, &[], None, false);
}
fn paint_geometry(
    ui: &egui::Ui,
    sk: &ResolvedSketch,
    projector: &Projector,
    rect: Rect,
    selected: &[Selection],
    hover: Option<Selection>,
    conflict: bool,
) {
    let painter = ui.painter().with_clip_rect(rect);
    for e in &sk.entities {
        let base = if conflict {
            RED
        } else if e.fixed {
            WHITE
        } else {
            BLUE
        };
        let points = e.points();
        for (i, p) in points.windows(2).enumerate() {
            let selection = Selection::edge(e.id, if e.kind == "rectangle" { i } else { 0 });
            let color = if selected.contains(&selection) || hover == Some(selection) {
                ORANGE
            } else {
                base
            };
            let segment = [
                projector.project(sk.plane.at(p[0]), rect),
                projector.project(sk.plane.at(p[1]), rect),
            ];
            if e.construction {
                let d = segment[1] - segment[0];
                let len = d.length();
                if len > 0.1 {
                    let unit = d / len;
                    for n in 0..(len / 9.).ceil().min(200.) as usize {
                        let a = segment[0] + unit * n as f32 * 9.;
                        let b = segment[0] + unit * (n as f32 * 9. + 4.).min(len);
                        painter.line_segment([a, b], Stroke::new(1.0f32, color));
                    }
                }
            } else {
                painter.line_segment(
                    segment,
                    Stroke::new(if color == ORANGE { 2.2f32 } else { 1.5f32 }, color),
                );
            }
        }
    }
    for (s, p) in vertices(sk) {
        let pos = projector.project(sk.plane.at(p), rect);
        let color = if selected.contains(&s) || hover == Some(s) {
            ORANGE
        } else if s.entity == 0 {
            WHITE
        } else if conflict {
            RED
        } else {
            sk.entities
                .iter()
                .find(|e| e.id == s.entity)
                .map_or(BLUE, |e| if e.fixed { WHITE } else { BLUE })
        };
        if s.entity == 0 {
            painter.line_segment(
                [pos - Vec2::new(6., 0.), pos + Vec2::new(6., 0.)],
                Stroke::new(1.0f32, color),
            );
            painter.line_segment(
                [pos - Vec2::new(0., 6.), pos + Vec2::new(0., 6.)],
                Stroke::new(1.0f32, color),
            );
            painter.circle_stroke(pos, 4., Stroke::new(1.0f32, color));
        } else {
            painter.circle_filled(pos, if selected.contains(&s) { 4. } else { 2.5 }, color);
        }
    }
}
fn paint_constraint(
    ui: &egui::Ui,
    sk: &ResolvedSketch,
    c: &SketchConstraint,
    projector: &Projector,
    rect: Rect,
) {
    let p = dimensions::point(sk, c.a, c.points[0].min(1)).unwrap_or([0., 0.]);
    let pos = projector.project(sk.plane.at(p), rect);
    let (symbol, offset) = match c.kind.as_str() {
        "Horizontal" | "HorizontalPoints" => ("H", Vec2::new(12., 16.)),
        "Vertical" | "VerticalPoints" => ("V", Vec2::new(27., 16.)),
        "Coincident" | "Origin" => ("□", Vec2::new(10., -12.)),
        "Equal" => ("=", Vec2::new(15., -15.)),
        "Parallel" => ("//", Vec2::new(25., -15.)),
        "Perpendicular" => ("⊥", Vec2::new(25., -15.)),
        "Tangent" => ("T", Vec2::new(12., -15.)),
        "Concentric" => ("◎", Vec2::new(10., -15.)),
        "Fix" => ("F", Vec2::new(12., -15.)),
        _ => return,
    };
    ui.painter().with_clip_rect(rect).text(
        pos + offset,
        egui::Align2::CENTER_CENTER,
        symbol,
        egui::FontId::proportional(11.),
        MUTED,
    );
}
fn arrow(painter: &egui::Painter, tip: Pos2, toward: Pos2, color: Color32) {
    let d = (toward - tip).normalized();
    let n = Vec2::new(-d.y, d.x);
    if d.is_finite() {
        painter.line_segment([tip, tip + d * 7. + n * 2.8], Stroke::new(1.0f32, color));
        painter.line_segment([tip, tip + d * 7. - n * 2.8], Stroke::new(1.0f32, color));
    }
}
pub fn paint_dimension(
    ui: &egui::Ui,
    sk: &ResolvedSketch,
    c: &SketchConstraint,
    projector: &Projector,
    rect: Rect,
    override_color: Option<Color32>,
    selected: bool,
) -> Option<Rect> {
    let value = dimensions::measure(sk, c).ok()?;
    let a = dimensions::point(sk, c.a, c.points[0]).unwrap_or([0., 0.]);
    let position = dimensions::label_position(sk, c);
    let label = projector.project(sk.plane.at(position), rect);
    let color = override_color.unwrap_or(if selected {
        ORANGE
    } else if c.reference {
        MUTED
    } else {
        WHITE
    });
    let painter = ui.painter().with_clip_rect(rect);
    let project = |p| projector.project(sk.plane.at(p), rect);
    if c.kind == "Radius" || c.kind == "Diameter" {
        let center = dimensions::point(sk, c.a, 0).ok()?;
        let radius = sk.entities.iter().find(|e| e.id == c.a)?.values[2];
        let dx = position[0] - center[0];
        let dy = position[1] - center[1];
        let length = dx.hypot(dy).max(1e-7);
        let rim = project([
            center[0] + dx / length * radius,
            center[1] + dy / length * radius,
        ]);
        if c.kind == "Diameter" {
            let opposite = project([
                center[0] - dx / length * radius,
                center[1] - dy / length * radius,
            ]);
            painter.line_segment([opposite, rim], Stroke::new(1.0f32, color));
            arrow(&painter, opposite, rim, color);
            arrow(&painter, rim, opposite, color);
        }
        painter.line_segment([rim, label], Stroke::new(1.0f32, color));
        if c.kind == "Radius" {
            arrow(&painter, rim, label, color);
        }
    } else if c.kind == "Angle" {
        let first = dimensions::line(sk, c.a, c.points[0]).ok()?;
        let second = dimensions::line(sk, c.b, c.points[1]).ok()?;
        painter.line_segment([project(first[0]), label], Stroke::new(0.7f32, color));
        painter.line_segment([project(second[0]), label], Stroke::new(0.7f32, color));
    } else {
        let (a, b, axis) = match c.kind.as_str() {
            "Width" | "Height" => {
                let l = dimensions::line(sk, c.a, c.points[0]).ok()?;
                (l[0], l[1], if c.kind == "Width" { 1 } else { 2 })
            }
            "Length" => {
                let l = dimensions::line(sk, c.a, c.points[0]).ok()?;
                (l[0], l[1], 0)
            }
            "X" | "Y" | "EndX" | "EndY" => {
                let p = dimensions::point(
                    sk,
                    c.a,
                    if c.kind.starts_with("End") {
                        1
                    } else {
                        c.points[0]
                    },
                )
                .ok()?;
                ([0., 0.], p, if c.kind.ends_with('X') { 1 } else { 2 })
            }
            "LineDistance" => {
                let l = dimensions::line(sk, c.a, c.points[0]).ok()?;
                let p = dimensions::line(sk, c.b, c.points[1]).ok()?[0];
                let d = [l[1][0] - l[0][0], l[1][1] - l[0][1]];
                let t = ((p[0] - l[0][0]) * d[0] + (p[1] - l[0][1]) * d[1])
                    / (d[0] * d[0] + d[1] * d[1]).max(1e-9);
                (p, [l[0][0] + t * d[0], l[0][1] + t * d[1]], 0)
            }
            "PointLineDistance" => {
                let l = dimensions::line(sk, c.b, c.points[1]).ok()?;
                let p = a;
                let d = [l[1][0] - l[0][0], l[1][1] - l[0][1]];
                let t = ((p[0] - l[0][0]) * d[0] + (p[1] - l[0][1]) * d[1])
                    / (d[0] * d[0] + d[1] * d[1]).max(1e-9);
                (p, [l[0][0] + t * d[0], l[0][1] + t * d[1]], 0)
            }
            _ => (
                a,
                dimensions::point(sk, c.b, c.points[1]).ok()?,
                if c.kind == "DistanceX" {
                    1
                } else if c.kind == "DistanceY" {
                    2
                } else {
                    0
                },
            ),
        };
        let (x, y) = if axis == 1 {
            ([a[0], position[1]], [b[0], position[1]])
        } else if axis == 2 {
            ([position[0], a[1]], [position[0], b[1]])
        } else {
            let d = [b[0] - a[0], b[1] - a[1]];
            let len = d[0].hypot(d[1]).max(1e-9);
            let normal = [-d[1] / len, d[0] / len];
            let offset = (position[0] - (a[0] + b[0]) / 2.) * normal[0]
                + (position[1] - (a[1] + b[1]) / 2.) * normal[1];
            (
                [a[0] + normal[0] * offset, a[1] + normal[1] * offset],
                [b[0] + normal[0] * offset, b[1] + normal[1] * offset],
            )
        };
        let x = project(x);
        let y = project(y);
        painter.line_segment([project(a), x], Stroke::new(0.7f32, color));
        painter.line_segment([project(b), y], Stroke::new(0.7f32, color));
        painter.line_segment([x, y], Stroke::new(1.0f32, color));
        arrow(&painter, x, y, color);
        arrow(&painter, y, x, color);
    }
    let prefix = match c.kind.as_str() {
        "Diameter" => "Ø",
        "Radius" => "R",
        _ => "",
    };
    let text = format!(
        "{prefix}{}{}",
        fmt(if value.abs() >= 0.001 {
            (value.abs() * 1000.).round() / 1000.
        } else {
            value.abs()
        }),
        if c.kind == "Angle" { "°" } else { "" }
    );
    let text = if c.reference {
        format!("({text})")
    } else {
        text
    };
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(14.), color);
    let bounds = Rect::from_center_size(label, galley.size() + Vec2::new(10., 6.));
    painter.rect_filled(bounds, 3., Color32::from_rgb(17, 23, 32));
    painter.galley(bounds.center() - galley.size() / 2., galley, color);
    Some(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Harness {
        ctx: egui::Context,
        editor: SketchUi,
        sketch: Sketch,
        camera: Camera,
        time: f64,
    }
    impl Harness {
        fn new() -> Self {
            let mut camera = Camera::default();
            camera.align(&Plane::named("Top", 0.));
            Self {
                ctx: egui::Context::default(),
                editor: SketchUi::default(),
                sketch: Sketch::default(),
                camera,
                time: 0.,
            }
        }
        fn frame(&mut self, events: Vec<egui::Event>) {
            self.time += 0.05;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.))),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let resolved = crate::sketch::resolve(
                &self.sketch,
                Plane::named("Top", 0.),
                &Numbers::new(&BTreeMap::new()).unwrap(),
            )
            .unwrap();
            let _ = self.ctx.run(input, |ctx| {
                self.editor.shortcuts(ctx);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        let rect = ui.available_rect_before_wrap();
                        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                        self.editor.canvas(
                            ui,
                            rect,
                            &response,
                            &self.camera,
                            &mut self.sketch,
                            Some(&resolved),
                            Plane::named("Top", 0.),
                            &BTreeMap::new(),
                            false,
                        );
                    });
            });
        }
        fn click(&mut self, x: f32, y: f32) {
            let pos = Pos2::new(x, y);
            self.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            self.frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        fn key(&mut self, key: egui::Key) {
            self.frame(vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            self.frame(vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    }
    #[test]
    fn draw_press_d_place_type_and_enter_creates_a_driving_dimension() {
        let mut h = Harness::new();
        h.editor.set_tool(Tool::Rectangle);
        h.frame(vec![]);
        h.click(500., 350.);
        h.click(700., 220.);
        assert_eq!(h.sketch.entities.len(), 1);
        h.key(egui::Key::D);
        h.click(600., 350.);
        h.click(600., 420.);
        assert!(h.editor.dimension.is_some());
        h.frame(vec![egui::Event::Text("40".into())]);
        h.key(egui::Key::Enter);
        assert!(h.editor.dimension.is_none());
        let constraint = h
            .sketch
            .constraints
            .iter()
            .find(|c| c.kind == "Width")
            .unwrap();
        assert_eq!(constraint.value, "40");
        assert!(!constraint.reference);
        let solved = crate::sketch::resolve(
            &h.sketch,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        assert!((solved.entities[0].values[2] - 40.).abs() < 1e-7);
        assert_eq!(solved.dof, 1);
    }
    #[test]
    fn cancelling_an_uncommitted_dimension_leaves_geometry_and_constraints_unchanged() {
        let mut h = Harness::new();
        h.editor.set_tool(Tool::Circle);
        h.frame(vec![]);
        h.click(500., 350.);
        h.click(560., 350.);
        let before = h.sketch.clone();
        h.key(egui::Key::D);
        h.click(560., 350.);
        h.click(640., 410.);
        assert!(h.editor.dimension.is_some());
        h.key(egui::Key::Escape);
        assert!(h.editor.dimension.is_none());
        assert_eq!(h.sketch, before);
    }
}
