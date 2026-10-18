use crate::{
    dimensions::{self, Selection, Sub},
    model::{Plane, Sketch, SketchConstraint, fmt},
    plugins::Numbers,
    render::{Camera, Projector, point_segment_distance},
    shortcuts::{self, Command, Context, SketchTool},
    sketch::ResolvedSketch,
};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use std::collections::BTreeMap;

#[cfg(test)]
#[path = "sketch_feedback_tests.rs"]
mod feedback_tests;

const BLUE: Color32 = Color32::from_rgb(86, 112, 139);
const WHITE: Color32 = Color32::from_rgb(221, 230, 240);
const HOVER: Color32 = Color32::from_rgb(170, 145, 196);
const ORANGE: Color32 = HOVER;
const RED: Color32 = Color32::from_rgb(247, 119, 132);
const MUTED: Color32 = Color32::from_rgb(144, 159, 180);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Select,
    Line,
    Rectangle,
    CenterRectangle,
    Circle,
    Arc,
    ThreePointArc,
    TangentArc,
    Trim,
    Dimension,
    Relationship(&'static str),
}
impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Line => "Line (L)",
            Self::Rectangle => "Rectangle (G)",
            Self::CenterRectangle => "Center rectangle (R)",
            Self::Circle => "Circle (C)",
            Self::Arc => "Center arc",
            Self::ThreePointArc => "3 point arc (A)",
            Self::TangentArc => "Tangent arc",
            Self::Trim => "Trim (M)",
            Self::Dimension => "Dimension (D)",
            Self::Relationship(name) => name,
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
#[derive(Clone)]
struct TangentStart {
    snap: Snap,
    source: Selection,
    direction: [f64; 2],
    edge: usize,
    circle: bool,
}
struct DimensionEdit {
    constraint: SketchConstraint,
    index: Option<usize>,
    text: String,
    focus: bool,
    error: Option<String>,
}
enum Drag {
    Box {
        start: Pos2,
        current: Pos2,
        original: Vec<Selection>,
        subtract: bool,
    },
    Geometry {
        selected: Vec<Selection>,
        start: [f64; 2],
        original: ResolvedSketch,
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
    second: Option<Snap>,
    clockwise_arc: bool,
    tangent: Option<TangentStart>,
    line_moved_away: bool,
    picks: Vec<Selection>,
    dimension: Option<DimensionEdit>,
    selected_dimension: Option<usize>,
    drag: Option<Drag>,
    last_good: Option<ResolvedSketch>,
    labels: Vec<(usize, Rect)>,
    pending_relationship: Option<&'static str>,
    pending_construction: bool,
    construction: bool,
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
            second: None,
            clockwise_arc: false,
            tangent: None,
            line_moved_away: false,
            picks: vec![],
            dimension: None,
            selected_dimension: None,
            drag: None,
            last_good: None,
            labels: vec![],
            pending_relationship: None,
            pending_construction: false,
            construction: false,
            hint: String::new(),
        }
    }
}
impl SketchUi {
    pub fn cancel_gesture(&mut self) {
        self.hint.clear();
        self.first = None;
        self.second = None;
        self.tangent = None;
        self.line_moved_away = false;
        self.picks.clear();
        self.dimension = None;
        self.drag = None;
        self.pending_relationship = None;
        self.pending_construction = false;
        self.tool = Tool::Select;
    }
    pub fn set_tool(&mut self, tool: Tool) {
        self.cancel_gesture();
        self.tool = tool;
        self.selected_dimension = None;
        if tool == Tool::Dimension {
            if self.selected.len() > 2 {
                self.hint = "Select one curve or two entities".into();
                self.selected.clear();
            }
            self.picks = self.selected.clone();
            self.selected = self.picks.clone();
        } else if !matches!(tool, Tool::Select | Tool::Relationship(_)) {
            self.selected.clear();
        }
    }
    fn toggle_tool(&mut self, tool: Tool) {
        self.set_tool(if self.tool == tool {
            Tool::Select
        } else {
            tool
        });
    }
    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.picks.clear();
        self.selected_dimension = None;
    }
    pub fn toolbar(&mut self, ui: &mut egui::Ui) {
        for tool in [
            Tool::Select,
            Tool::Line,
            Tool::Rectangle,
            Tool::CenterRectangle,
            Tool::Circle,
            Tool::ThreePointArc,
            Tool::Arc,
            Tool::TangentArc,
            Tool::Trim,
            Tool::Dimension,
        ] {
            if ui
                .selectable_label(self.tool == tool, tool.name())
                .clicked()
            {
                self.toggle_tool(tool);
            }
        }
        if self.tool == Tool::Arc {
            ui.checkbox(&mut self.clockwise_arc, "Clockwise");
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
        let commands: Vec<_> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|event| {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } = event
                    {
                        shortcuts::command_for(*key, *modifiers, Context::Sketch)
                    } else {
                        None
                    }
                })
                .collect()
        });
        for command in commands {
            match command {
                Command::Cancel => {
                    if self.dimension.take().is_none() {
                        self.cancel_gesture();
                    }
                }
                Command::ClearSelection => self.clear_selection(),
                Command::SketchTool(tool) => self.toggle_tool(match tool {
                    SketchTool::Line => Tool::Line,
                    SketchTool::CornerRectangle => Tool::Rectangle,
                    SketchTool::CenterRectangle => Tool::CenterRectangle,
                    SketchTool::Circle => Tool::Circle,
                    SketchTool::ThreePointArc => Tool::ThreePointArc,
                    SketchTool::Trim => Tool::Trim,
                    SketchTool::Dimension => Tool::Dimension,
                }),
                Command::Relationship(name) => {
                    if self.tool == Tool::Relationship(name) {
                        self.cancel_gesture();
                    } else {
                        self.set_tool(Tool::Relationship(name));
                        self.pending_relationship = (!self.selected.is_empty()).then_some(name);
                    }
                }
                Command::ToggleTangentArc => match self.tool {
                    Tool::Line => {
                        self.tool = Tool::TangentArc;
                        self.tangent = None;
                    }
                    Tool::TangentArc => {
                        self.tool = Tool::Line;
                        self.tangent = None;
                        self.line_moved_away = false;
                    }
                    _ => (),
                },
                Command::Construction => {
                    if self.selected.is_empty() {
                        self.construction = !self.construction;
                    } else {
                        self.pending_construction = true;
                    }
                }
                _ => (),
            }
        }
    }
    fn edit_construction(
        &mut self,
        sketch: &mut Sketch,
        state: Option<bool>,
        vars: &BTreeMap<String, String>,
        format: u32,
    ) -> bool {
        let Some(resolved) = &self.last_good else {
            return false;
        };
        // All numeric seeds come from the evaluated sketch. Expressions remain
        // persisted constraints during structural replacement.
        match Numbers::for_format(vars, format).and_then(|numbers| {
            crate::sketch::set_construction(sketch, resolved, &self.selected, state, &numbers)
        }) {
            Ok((candidate, solved, selected)) => {
                let changed = *sketch != candidate;
                *sketch = candidate;
                self.last_good = Some(solved);
                self.selected = selected;
                self.hint.clear();
                changed
            }
            Err(error) => {
                self.hint = error;
                false
            }
        }
    }
    fn update_resolved(&mut self, resolved: &ResolvedSketch) {
        let valid = |s: &Selection| {
            if s.entity == 0 {
                return s.sub == Sub::Point(0);
            }
            resolved
                .entities
                .iter()
                .find(|e| e.id == s.entity)
                .is_some_and(|e| match s.sub {
                    Sub::Point(index) => index < e.control_points().len(),
                    Sub::Edge(index) => index < if e.kind == "rectangle" { 4 } else { 1 },
                })
        };
        self.selected.retain(valid);
        self.picks.retain(valid);
        self.last_good = Some(resolved.clone());
    }
    pub fn inspector(
        &mut self,
        ui: &mut egui::Ui,
        sketch: &mut Sketch,
        resolved: Option<&ResolvedSketch>,
        error: Option<&str>,
        vars: &BTreeMap<String, String>,
        format: u32,
    ) -> bool {
        if let Some(s) = resolved {
            self.update_resolved(s);
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
                    match self.relationships(name, sketch) {
                        Ok(constraints) => {
                            self.hint.clear();
                            seed_from_resolved(sketch, self.last_good.as_ref());
                            for c in constraints {
                                if !sketch.constraints.iter().any(|x| x == &c) {
                                    sketch.constraints.push(c);
                                    changed = true;
                                }
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
                changed |= self.edit_construction(sketch, Some(construction), vars, format);
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
    fn relationships(&self, name: &str, sketch: &Sketch) -> Result<Vec<SketchConstraint>, String> {
        if self.selected.len() > 2
            && matches!(name, "Horizontal" | "Vertical")
            && self.selected.iter().any(|s| matches!(s.sub, Sub::Point(_)))
        {
            return Err("Select two points, or select lines".into());
        }
        if self.selected.len() > 2
            && !matches!(
                name,
                "Fix" | "Horizontal" | "Vertical" | "Equal" | "Parallel" | "Concentric"
            )
        {
            return Err("Select exactly two suitable entities".into());
        }
        let mut groups = Vec::new();
        if name == "Fix"
            || matches!(name, "Horizontal" | "Vertical")
                && self.selected.iter().all(|s| matches!(s.sub, Sub::Edge(_)))
        {
            groups.extend(self.selected.iter().map(|s| vec![*s]));
        } else if matches!(name, "Equal" | "Parallel" | "Concentric") && self.selected.len() > 2 {
            groups.extend(
                self.selected[1..]
                    .iter()
                    .map(|s| vec![self.selected[0], *s]),
            );
        } else {
            groups.push(self.selected.clone());
        }
        groups
            .into_iter()
            .map(|group| self.relationship(name, sketch, &group))
            .collect()
    }
    fn relationship(
        &self,
        name: &str,
        sketch: &Sketch,
        selected: &[Selection],
    ) -> Result<SketchConstraint, String> {
        let a = *selected.first().ok_or("Select sketch geometry")?;
        let b = selected.get(1).copied();
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
                        .any(|e| e.id == id && matches!(e.kind.as_str(), "circle" | "arc"))
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
                            .any(|e| e.id == id && matches!(e.kind.as_str(), "circle" | "arc"))
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
                        c.kind = if sketch.entities.iter().any(|e| {
                            e.id == a.entity && matches!(e.kind.as_str(), "circle" | "arc")
                        }) {
                            "PointOnCircle"
                        } else {
                            "PointOnLine"
                        }
                        .into();
                    }
                    (Sub::Point(_), Sub::Edge(_)) => {
                        c.kind = if sketch.entities.iter().any(|e| {
                            e.id == b.entity && matches!(e.kind.as_str(), "circle" | "arc")
                        }) {
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
                    .any(|e| e.id == a.entity && matches!(e.kind.as_str(), "circle" | "arc"))
                {
                    (b, a)
                } else {
                    (a, b)
                };
                if let Some(arc) = s
                    .entities
                    .iter()
                    .find(|e| [a.entity, b.entity].contains(&e.id) && e.kind == "arc")
                    && let Some(circle) = s
                        .entities
                        .iter()
                        .find(|e| [a.entity, b.entity].contains(&e.id) && e.kind == "circle")
                {
                    let points = arc.control_points();
                    let center = circle.control_points()[0];
                    let index = [1, 2]
                        .into_iter()
                        .find(|&i| {
                            ((points[i][0] - center[0]).hypot(points[i][1] - center[1])
                                - circle.values[2])
                                .abs()
                                < 1e-5
                        })
                        .ok_or("Arc tangency requires an endpoint on the circle")?;
                    c.points = if a.entity == arc.id {
                        [index, 0]
                    } else {
                        [0, index]
                    };
                    return Ok(c);
                }
                if sketch
                    .entities
                    .iter()
                    .any(|e| [a.entity, b.entity].contains(&e.id) && e.kind == "arc")
                {
                    let endpoint_indices = |selection: Selection| {
                        let id = selection.entity;
                        if sketch
                            .entities
                            .iter()
                            .any(|e| e.id == id && e.kind == "arc")
                        {
                            vec![1, 2]
                        } else if sketch
                            .entities
                            .iter()
                            .any(|e| e.id == id && e.kind == "rectangle")
                        {
                            vec![selection.index() % 4, (selection.index() + 1) % 4]
                        } else {
                            vec![0, 1]
                        }
                    };
                    for i in endpoint_indices(a) {
                        for j in endpoint_indices(b) {
                            let p = dimensions::point(s, a.entity, i)?;
                            let q = dimensions::point(s, b.entity, j)?;
                            if (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-5 {
                                c.points = [i, j];
                                if sketch
                                    .entities
                                    .iter()
                                    .any(|e| e.id == line.entity && e.kind == "rectangle")
                                {
                                    c.value = format!("edge:{}", line.index());
                                }
                                return Ok(c);
                            }
                        }
                    }
                    return Err("Arc tangency requires a shared endpoint".into());
                }
                c.negative = dimensions::signed_distance(
                    dimensions::point(s, circle.entity, 0)?,
                    dimensions::line(s, line.entity, line.index())?,
                ) < 0.;
            }
            _ => {
                if b.is_none() {
                    return Err("Select two suitable entities".into());
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
        format: u32,
        conflict: bool,
    ) -> Edit {
        if let Some(s) = resolved {
            self.update_resolved(s);
        }
        let mut sk = self.last_good.clone().unwrap_or(ResolvedSketch {
            entities: vec![],
            plane,
            dof: 0,
        });
        // A just-created chain endpoint can precede its asynchronous Scene.
        // Resolve only when a tangent gesture needs that new source immediately.
        if matches!(self.tool, Tool::Line | Tool::TangentArc)
            && self
                .first
                .as_ref()
                .and_then(|p| p.target)
                .is_some_and(|p| !sk.entities.iter().any(|e| e.id == p.entity))
            && let Ok(solved) = Numbers::for_format(vars, format)
                .and_then(|n| crate::sketch::resolve(sketch, sk.plane.clone(), &n))
        {
            self.last_good = Some(solved.clone());
            sk = solved;
        }
        let plane = &sk.plane;
        let projector = camera.projector(rect);
        let pointer = response.hover_pos();
        let raw = pointer.and_then(|p| camera.plane_point(p, rect, plane));
        let hover = pointer.is_some_and(|p| p.y > rect.top() + 44.);
        let hit = pointer.and_then(|p| {
            if self.tool == Tool::Trim {
                pick_curve(&sk, &projector, rect, p)
            } else {
                pick(&sk, &projector, rect, p)
            }
        });
        let mut snap = raw.map(|p| {
            snap(
                &sk,
                &projector,
                rect,
                p,
                self.inference && !ui.input(|i| i.modifiers.shift),
                self.first.as_ref(),
            )
        });
        if self.tool == Tool::Line
            && self.inference
            && !ui.input(|i| i.modifiers.shift)
            && let (Some(first), Some(pointer)) = (&self.first, pointer)
        {
            let distance = projector.project(plane.at(first.p), rect).distance(pointer);
            if distance > 14. {
                self.line_moved_away = true;
            }
            if self.line_moved_away
                && distance < 7.
                && !ui.input(|i| i.pointer.any_down())
                && let Ok(start) = tangent_start(&sk, first, first.target)
            {
                self.tool = Tool::TangentArc;
                self.tangent = Some(start);
                self.line_moved_away = false;
            }
        }
        if self.tool == Tool::TangentArc
            && self.tangent.is_none()
            && let Some(first) = &self.first
        {
            match tangent_start(&sk, first, first.target) {
                Ok(start) => self.tangent = Some(start),
                Err(error) => self.hint = error,
            }
        }
        // Line continuation snaps to an arc's tangent before adding a persistent
        // relation. Existing endpoint snaps keep priority over angular inference.
        if self.tool == Tool::Line
            && self.inference
            && !ui.input(|i| i.modifiers.shift)
            && let (Some(first), Some(end)) = (&self.first, &mut snap)
            && end.target.is_none()
            && let Ok(start) = tangent_start(&sk, first, first.target)
            && sk
                .entities
                .iter()
                .any(|e| e.id == start.source.entity && e.kind == "arc")
        {
            let delta = [end.p[0] - first.p[0], end.p[1] - first.p[1]];
            let d = delta[0] * start.direction[0] + delta[1] * start.direction[1];
            let p = [
                first.p[0] + d * start.direction[0],
                first.p[1] + d * start.direction[1],
            ];
            if d > 1e-7
                && projector
                    .project(plane.at(p), rect)
                    .distance(projector.project(plane.at(end.p), rect))
                    < 6.
            {
                *end = Snap {
                    p,
                    ..Default::default()
                };
            }
        }
        if self.tool == Tool::TangentArc
            && self.tangent.is_none()
            && response.drag_started_by(egui::PointerButton::Primary)
            && let Some(press) = ui.input(|i| i.pointer.press_origin())
            && let Some(p) = camera.plane_point(press, rect, plane)
        {
            match tangent_start(
                &sk,
                &Snap {
                    p,
                    ..Default::default()
                },
                pick(&sk, &projector, rect, press),
            ) {
                Ok(start) => {
                    self.first = Some(start.snap.clone());
                    self.tangent = Some(start);
                    self.hint.clear();
                }
                Err(error) => self.hint = error,
            }
        }
        let mut edit = Edit::default();
        if let Some(name) = self.pending_relationship.take() {
            match self.relationships(name, sketch) {
                Ok(constraints) => {
                    seed_from_resolved(sketch, Some(&sk));
                    let before = sketch.constraints.len();
                    for c in constraints {
                        add_relation(sketch, c);
                    }
                    if sketch.constraints.len() != before {
                        edit.change(true);
                    }
                    self.hint.clear();
                    self.clear_selection();
                }
                Err(error) => self.hint = error,
            }
        }
        if std::mem::take(&mut self.pending_construction)
            && self.edit_construction(sketch, None, vars, format)
        {
            edit.change(true);
        }
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
                Tool::CenterRectangle => vec![
                    [2. * first.p[0] - end.p[0], 2. * first.p[1] - end.p[1]],
                    [end.p[0], 2. * first.p[1] - end.p[1]],
                    end.p,
                    [2. * first.p[0] - end.p[0], end.p[1]],
                    [2. * first.p[0] - end.p[0], 2. * first.p[1] - end.p[1]],
                ],
                Tool::Rectangle => vec![
                    first.p,
                    [end.p[0], first.p[1]],
                    end.p,
                    [first.p[0], end.p[1]],
                    first.p,
                ],
                Tool::TangentArc => self
                    .tangent
                    .as_ref()
                    .and_then(|start| tangent_arc_values(start, end.p).ok())
                    .map(|values| {
                        crate::sketch::ResolvedEntity {
                            id: 0,
                            kind: "arc".into(),
                            curve_identity: None,
                            values,
                            construction: false,
                            fixed: false,
                        }
                        .points()
                    })
                    .unwrap_or_else(|| vec![first.p, end.p]),
                Tool::Arc | Tool::ThreePointArc => self
                    .second
                    .as_ref()
                    .and_then(|second| {
                        arc_draft(self.tool, first, second, end, self.clockwise_arc).ok()
                    })
                    .map(|values| {
                        crate::sketch::ResolvedEntity {
                            id: 0,
                            kind: "arc".into(),
                            curve_identity: None,
                            values,
                            construction: false,
                            fixed: false,
                        }
                        .points()
                    })
                    .unwrap_or_else(|| vec![first.p, end.p]),
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
            if !matches!(
                self.tool,
                Tool::Select | Tool::Trim | Tool::Dimension | Tool::Relationship(_)
            ) {
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
        } else if (response.clicked()
            || (self.tool == Tool::TangentArc
                && response.drag_stopped_by(egui::PointerButton::Primary)))
            && hover
            && self.dimension.is_none()
        {
            if let Some(index) = label_hit {
                self.selected_dimension = Some(index);
                self.selected.clear();
            } else if self.tool == Tool::Trim {
                if let (Some(hit), Some(point)) = (hit, raw) {
                    match Numbers::for_format(vars, format).and_then(|numbers| {
                        crate::sketch::apply_trim(
                            sketch,
                            &sk,
                            hit.entity,
                            hit.index(),
                            point,
                            &numbers,
                        )
                    }) {
                        Ok(result) => {
                            *sketch = result.sketch;
                            self.last_good = Some(result.resolved);
                            self.clear_selection();
                            self.hint = if result.removed_constraints == 0 {
                                String::new()
                            } else {
                                format!("Trim removed {} constraints", result.removed_constraints)
                            };
                            edit.change(true);
                            edit.live = true;
                        }
                        Err(error) => self.hint = error,
                    }
                }
            } else if self.tool == Tool::Dimension {
                if let Some(h) = hit {
                    if self.picks.contains(&h) {
                        self.picks.retain(|s| s != &h);
                        self.selected = self.picks.clone();
                    } else if self.picks.len() < 2 {
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
                if let Some(h) = hit {
                    toggle_selection(&mut self.selected, h);
                } else {
                    self.clear_selection();
                }
            } else if let Tool::Relationship(name) = self.tool {
                if let Some(h) = hit {
                    toggle_selection(&mut self.selected, h);
                    self.pending_relationship = Some(name);
                } else {
                    self.clear_selection();
                }
            } else if self.tool == Tool::TangentArc {
                if let Some(end) = snap.clone() {
                    if let Some(start) = self.tangent.clone() {
                        let result = Numbers::for_format(vars, format).and_then(|n| {
                            let values = tangent_arc_values(&start, end.p)?;
                            let mut candidate = sketch.clone();
                            seed_from_resolved(&mut candidate, Some(&sk));
                            candidate.add_arc(
                                [values[0], values[1]],
                                [values[2], values[3]],
                                [values[4], values[5]],
                                values[6] < 0.,
                            );
                            let id = candidate.next_id;
                            candidate.entities.last_mut().unwrap().construction = self.construction;
                            if start.circle {
                                add_relation(
                                    &mut candidate,
                                    SketchConstraint {
                                        kind: "PointOnCircle".into(),
                                        a: id,
                                        b: start.source.entity,
                                        points: [1, 0],
                                        ..Default::default()
                                    },
                                );
                            } else {
                                link(&mut candidate, id, 1, &start.snap);
                            }
                            link(&mut candidate, id, 2, &end);
                            add_relation(
                                &mut candidate,
                                SketchConstraint {
                                    kind: "Tangent".into(),
                                    a: id,
                                    b: start.source.entity,
                                    points: [1, start.source.index()],
                                    value: format!("edge:{}", start.edge),
                                    ..Default::default()
                                },
                            );
                            let solved = crate::sketch::resolve(&candidate, plane.clone(), &n)?;
                            seed_from_resolved(&mut candidate, Some(&solved));
                            Ok((candidate, solved, id))
                        });
                        match result {
                            Ok((candidate, solved, id)) => {
                                let end = solved
                                    .entities
                                    .iter()
                                    .find(|e| e.id == id)
                                    .unwrap()
                                    .control_points()[2];
                                *sketch = candidate;
                                self.last_good = Some(solved);
                                self.first = Some(Snap {
                                    p: end,
                                    target: Some(Selection::point(id, 2)),
                                    ..Default::default()
                                });
                                self.tool = Tool::Line;
                                self.tangent = None;
                                self.line_moved_away = false;
                                self.selected.clear();
                                self.hint.clear();
                                edit.change(true);
                            }
                            Err(error) => self.hint = error,
                        }
                    } else {
                        match tangent_start(&sk, &end, hit) {
                            Ok(start) => {
                                self.first = Some(start.snap.clone());
                                self.tangent = Some(start);
                                self.hint.clear();
                            }
                            Err(error) => self.hint = error,
                        }
                    }
                }
            } else if matches!(self.tool, Tool::Arc | Tool::ThreePointArc) {
                if let Some(snap) = snap.clone() {
                    if let (Some(first), Some(second)) = (&self.first, &self.second) {
                        match arc_draft(self.tool, first, second, &snap, self.clockwise_arc) {
                            Ok(values) => {
                                seed_from_resolved(sketch, Some(&sk));
                                sketch.add_arc(
                                    [values[0], values[1]],
                                    [values[2], values[3]],
                                    [values[4], values[5]],
                                    values[6] < 0.,
                                );
                                let id = sketch.next_id;
                                sketch.entities.last_mut().unwrap().construction =
                                    self.construction;
                                if self.tool == Tool::Arc {
                                    link(sketch, id, 0, first);
                                    link(sketch, id, 1, second);
                                    link(sketch, id, 2, &snap);
                                } else {
                                    link(sketch, id, 1, first);
                                    link(sketch, id, 2, second);
                                    if let Some(point) = snap.target {
                                        add_relation(
                                            sketch,
                                            SketchConstraint {
                                                kind: "PointOnCircle".into(),
                                                a: point.entity,
                                                b: id,
                                                points: [point.index(), 0],
                                                ..Default::default()
                                            },
                                        );
                                    }
                                }
                                self.first = None;
                                self.second = None;
                                self.selected.clear();
                                self.hint.clear();
                                edit.change(true);
                            }
                            Err(error) => self.hint = error,
                        }
                    } else if self.first.is_some() {
                        self.second = Some(snap);
                    } else {
                        self.first = Some(snap);
                    }
                }
            } else if let Some(snap) = snap.clone() {
                if let Some(first) = self.first.take() {
                    if (first.p[0] - snap.p[0]).hypot(first.p[1] - snap.p[1]) > 1e-7 {
                        seed_from_resolved(sketch, Some(&sk));
                        match self.tool {
                            Tool::Rectangle => sketch.add_rect(first.p, snap.p),
                            Tool::CenterRectangle => sketch.add_rect(
                                [2. * first.p[0] - snap.p[0], 2. * first.p[1] - snap.p[1]],
                                snap.p,
                            ),
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
                        if self.tool != Tool::CenterRectangle {
                            link(sketch, id, first_index, &first);
                        }
                        if let Some(entity) = sketch.entities.last_mut() {
                            entity.construction = self.construction;
                        }
                        if self.tool == Tool::CenterRectangle {
                            add_center_rectangle_relations(sketch, id, &first);
                        }
                        if self.tool == Tool::Line {
                            link(sketch, id, 1, &snap);
                            if self.inference
                                && !ui.input(|i| i.modifiers.shift)
                                && let Ok(start) = tangent_start(&sk, &first, first.target)
                                && sk
                                    .entities
                                    .iter()
                                    .any(|e| e.id == start.source.entity && e.kind == "arc")
                            {
                                let d = [snap.p[0] - first.p[0], snap.p[1] - first.p[1]];
                                if (d[0] * start.direction[1] - d[1] * start.direction[0]).abs()
                                    < 1e-7 * d[0].hypot(d[1]).max(1.)
                                {
                                    add_relation(
                                        sketch,
                                        SketchConstraint {
                                            kind: "Tangent".into(),
                                            a: id,
                                            b: start.source.entity,
                                            points: [0, start.source.index()],
                                            ..Default::default()
                                        },
                                    );
                                }
                            }
                            self.line_moved_away = false;
                            if self.inference
                                && !ui.input(|i| i.modifiers.shift)
                                && (first.p[0] - snap.p[0]).abs() < 1e-7
                            {
                                add_relation(
                                    sketch,
                                    SketchConstraint {
                                        kind: "Vertical".into(),
                                        a: id,
                                        ..Default::default()
                                    },
                                );
                            } else if self.inference
                                && !ui.input(|i| i.modifiers.shift)
                                && (first.p[1] - snap.p[1]).abs() < 1e-7
                            {
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
                        self.selected.clear();
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
                    if !self.selected.contains(&h) {
                        self.selected.push(h);
                    }
                    self.drag = Some(Drag::Geometry {
                        selected: self.selected.clone(),
                        start: p,
                        original: sk.clone(),
                        checkpoint: true,
                    });
                } else if pressed_hit.is_none() {
                    self.drag = Some(Drag::Box {
                        start: press,
                        current: press,
                        original: self.selected.clone(),
                        subtract: ui.input(|i| i.modifiers.ctrl),
                    });
                }
            }
            if response.dragged_by(egui::PointerButton::Primary)
                && let (Some(p), Some(drag)) = (raw, &mut self.drag)
            {
                match drag {
                    Drag::Box {
                        start,
                        current,
                        original,
                        subtract,
                    } => {
                        *current = pointer.unwrap_or(*current);
                        let hits = box_hits(&sk, &projector, rect, *start, *current);
                        self.selected = original.clone();
                        for hit in hits {
                            if *subtract {
                                self.selected.retain(|s| *s != hit);
                            } else if !self.selected.contains(&hit) {
                                self.selected.push(hit);
                            }
                        }
                    }
                    Drag::Label { index, checkpoint } => {
                        if let Some(c) = sketch.constraints.get_mut(*index) {
                            dimensions::set_label_position(&sk, c, p);
                            edit.change(*checkpoint);
                            *checkpoint = false;
                        }
                    }
                    Drag::Geometry {
                        selected,
                        start,
                        original,
                        checkpoint,
                    } => {
                        let delta = [p[0] - start[0], p[1] - start[1]];
                        let targets = drag_targets(original, selected, p, delta);
                        if let Ok(numbers) = Numbers::for_format(vars, format) {
                            let mut candidate = sketch.clone();
                            for target in &targets {
                                if target.kind == "CurvePoint"
                                    && let Some(entity) = candidate
                                        .entities
                                        .iter_mut()
                                        .find(|e| e.id == target.a && e.kind == "circle")
                                    && entity.values[2].parse::<f64>().is_ok()
                                {
                                    entity.values[2] = target.value.clone();
                                }
                            }
                            match crate::sketch::resolve_with_targets(
                                &candidate,
                                sk.plane.clone(),
                                &numbers,
                                &targets,
                            ) {
                                Ok(solved) => {
                                    let before = sketch.clone();
                                    seed_from_resolved(sketch, Some(&solved));
                                    if *sketch != before {
                                        self.last_good = Some(solved);
                                        edit.change(*checkpoint);
                                        edit.live = true;
                                        *checkpoint = false;
                                    }
                                }
                                Err(error) => self.hint = error,
                            }
                        }
                    }
                }
            }
            if let Some(Drag::Box { start, current, .. }) = &self.drag {
                let crossing = current.x < start.x;
                let color = if crossing {
                    ui.visuals().widgets.hovered.bg_stroke.color
                } else {
                    ui.visuals().selection.stroke.color
                };
                let bounds = Rect::from_two_pos(*start, *current);
                ui.painter()
                    .rect_filled(bounds, 0., color.gamma_multiply(0.12));
                stroke_curve(
                    ui.painter(),
                    &[
                        bounds.left_top(),
                        bounds.right_top(),
                        bounds.right_bottom(),
                        bounds.left_bottom(),
                        bounds.left_top(),
                    ],
                    crossing,
                    Stroke::new(1.2_f32, color),
                );
            }
            if response.drag_stopped() {
                self.drag = None;
            }
        }
        if hover
            && !ui.ctx().wants_keyboard_input()
            && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
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
        if self.dimension_dialog(ui, &sk, sketch, &projector, rect, || {
            Numbers::for_format(vars, format)
        }) {
            edit.change(true);
        }
        let helper = if !self.hint.is_empty() {
            self.hint.as_str()
        } else if self.dimension.is_some() {
            ""
        } else {
            match self.tool {
                Tool::Select => "",
                Tool::Trim => "Choose a segment to trim",
                Tool::Relationship(name) => name,
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
                Tool::Rectangle | Tool::CenterRectangle => {
                    if self.first.is_some() {
                        "Opposite corner"
                    } else {
                        "First corner"
                    }
                }
                Tool::TangentArc => {
                    if self.tangent.is_some() {
                        "Arc endpoint"
                    } else {
                        "Choose a line or arc endpoint"
                    }
                }
                Tool::Arc | Tool::ThreePointArc => {
                    if self.second.is_some() {
                        if self.tool == Tool::Arc {
                            "End point"
                        } else {
                            "Point on arc"
                        }
                    } else if self.first.is_some() {
                        if self.tool == Tool::Arc {
                            "Start point"
                        } else {
                            "End point"
                        }
                    } else if self.tool == Tool::Arc {
                        "Center"
                    } else {
                        "Start point"
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
        if hover
            && !self.selected.is_empty()
            && self.dimension.is_none()
            && let Some(pointer) = pointer
        {
            let points = self
                .selected
                .iter()
                .filter(|s| matches!(s.sub, Sub::Point(_)))
                .count();
            let curves = self.selected.len() - points;
            let count =
                |n, singular, plural| format!("{n} {}", if n == 1 { singular } else { plural });
            let mut parts = Vec::new();
            if curves > 0 {
                parts.push(count(curves, "curve", "curves"));
            }
            if points > 0 {
                parts.push(count(points, "point", "points"));
            }
            let color = ui.visuals().selection.stroke.color;
            let label = ui.painter().layout_no_wrap(
                parts.join(", "),
                egui::FontId::proportional(12.),
                color,
            );
            let size = label.size() + Vec2::new(8., 4.);
            let position = Pos2::new(
                (pointer.x + 15.).min(rect.right() - size.x),
                (pointer.y + 18.).min(rect.bottom() - size.y),
            );
            let badge = Rect::from_min_size(position, size);
            ui.painter()
                .rect_filled(badge, 2., Color32::from_rgb(7, 7, 10));
            ui.painter()
                .galley(badge.min + Vec2::new(4., 2.), label, color);
        }
        edit
    }
    fn dimension_dialog(
        &mut self,
        ui: &mut egui::Ui,
        sk: &ResolvedSketch,
        sketch: &mut Sketch,
        projector: &Projector,
        rect: Rect,
        numbers: impl FnOnce() -> crate::model::Result<Numbers>,
    ) -> bool {
        let Some(mut draft) = self.dimension.take() else {
            return false;
        };
        let p = dimensions::label_position(sk, &draft.constraint);
        let screen = projector.project(sk.plane.at(p), rect);
        let mut accept = false;
        let mut cancel = false;
        let mut input_id = None;
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
                    input_id = Some(output.response.id);
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
            match numbers().and_then(|n| n.eval(&draft.text).map(|value| (n, value))) {
                Ok((numbers, value)) => {
                    if matches!(
                        draft.constraint.kind.as_str(),
                        "Radius" | "Diameter" | "Length" | "Width" | "Height" | "Distance"
                    ) && value <= 1e-7
                    {
                        draft.error = Some("Dimension must be positive".into());
                    } else {
                        let mut candidate = sketch.clone();
                        seed_from_resolved(&mut candidate, Some(sk));
                        draft.constraint.value = draft.text.clone();
                        if let Some(index) = draft.index {
                            if let Some(c) = candidate.constraints.get_mut(index) {
                                *c = draft.constraint.clone();
                            }
                        } else {
                            candidate.constraints.push(draft.constraint.clone());
                        }
                        match crate::sketch::resolve(&candidate, sk.plane.clone(), &numbers) {
                            Ok(solved) => {
                                seed_from_resolved(&mut candidate, Some(&solved));
                                *sketch = candidate;
                                self.last_good = Some(solved);
                                self.picks.clear();
                                self.selected.clear();
                                if let Some(id) = input_id {
                                    ui.memory_mut(|m| m.surrender_focus(id));
                                }
                                return true;
                            }
                            Err(error) => draft.error = Some(error),
                        }
                    }
                }
                Err(e) => draft.error = Some(e),
            }
        }
        if !cancel {
            self.dimension = Some(draft);
        } else {
            self.picks.clear();
            if let Some(id) = input_id {
                ui.memory_mut(|m| m.surrender_focus(id));
            }
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

fn toggle_selection(selected: &mut Vec<Selection>, hit: Selection) {
    if selected.contains(&hit) {
        selected.retain(|s| *s != hit);
    } else {
        selected.push(hit);
    }
}

fn add_center_rectangle_relations(sketch: &mut Sketch, rectangle: u64, center: &Snap) {
    let corners = rect_points(
        &sketch
            .entities
            .iter()
            .find(|e| e.id == rectangle)
            .unwrap()
            .values,
    );
    let mut diagonals = Vec::new();
    for corner in [0, 2] {
        sketch.add_line(center.p, corners[corner]);
        let diagonal = sketch.next_id;
        sketch.entities.last_mut().unwrap().construction = true;
        add_relation(
            sketch,
            SketchConstraint {
                kind: "Coincident".into(),
                a: diagonal,
                b: rectangle,
                points: [1, corner],
                ..Default::default()
            },
        );
        diagonals.push(diagonal);
    }
    link(sketch, diagonals[0], 0, center);
    for (kind, points) in [
        ("Coincident", [0, 0]),
        ("Parallel", [0, 0]),
        ("Equal", [0, 0]),
    ] {
        add_relation(
            sketch,
            SketchConstraint {
                kind: kind.into(),
                a: diagonals[0],
                b: diagonals[1],
                points,
                ..Default::default()
            },
        );
    }
}

fn box_hits(
    sk: &ResolvedSketch,
    projector: &Projector,
    viewport: Rect,
    start: Pos2,
    end: Pos2,
) -> Vec<Selection> {
    let bounds = Rect::from_two_pos(start, end);
    let enclosed = end.x >= start.x;
    let mut result = Vec::new();
    for entity in &sk.entities {
        let points = entity.points();
        if entity.kind == "rectangle" {
            for (index, pair) in points.windows(2).enumerate() {
                let a = projector.project(sk.plane.at(pair[0]), viewport);
                let b = projector.project(sk.plane.at(pair[1]), viewport);
                if if enclosed {
                    bounds.contains(a) && bounds.contains(b)
                } else {
                    segment_intersects_rect(a, b, bounds)
                } {
                    result.push(Selection::edge(entity.id, index));
                }
            }
        } else {
            let projected: Vec<_> = points
                .iter()
                .map(|p| projector.project(sk.plane.at(*p), viewport))
                .collect();
            let selected = if enclosed {
                projected.iter().all(|p| bounds.contains(*p))
            } else {
                projected
                    .windows(2)
                    .any(|p| segment_intersects_rect(p[0], p[1], bounds))
            };
            if selected && !projected.is_empty() {
                result.push(Selection::edge(entity.id, 0));
            }
        }
    }
    result
}

fn segment_intersects_rect(a: Pos2, b: Pos2, bounds: Rect) -> bool {
    let mut enter = 0.0_f32;
    let mut leave = 1.0_f32;
    for (origin, delta, low, high) in [
        (a.x, b.x - a.x, bounds.left(), bounds.right()),
        (a.y, b.y - a.y, bounds.top(), bounds.bottom()),
    ] {
        if delta.abs() < f32::EPSILON {
            if origin < low || origin > high {
                return false;
            }
        } else {
            let first = (low - origin) / delta;
            let last = (high - origin) / delta;
            enter = enter.max(first.min(last));
            leave = leave.min(first.max(last));
            if enter > leave {
                return false;
            }
        }
    }
    true
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
pub use crate::sketch::seed_from_resolved;
fn rect_points(values: &[String]) -> Vec<[f64; 2]> {
    let v: Vec<f64> = values.iter().map(|s| s.parse().unwrap_or(0.)).collect();
    vec![
        [v[0], v[1]],
        [v[0] + v[2], v[1]],
        [v[0] + v[2], v[1] + v[3]],
        [v[0], v[1] + v[3]],
    ]
}
fn tangent_start(
    sk: &ResolvedSketch,
    snap: &Snap,
    hit: Option<Selection>,
) -> Result<TangentStart, String> {
    let mut selected = hit
        .or(snap.target)
        .ok_or("Choose a line or arc endpoint, or a circle rim")?;
    // The origin wins ordinary snap ties, but cannot supply a tangent.
    if selected.entity == 0 {
        selected = sk
            .entities
            .iter()
            .flat_map(|entity| {
                entity
                    .control_points()
                    .into_iter()
                    .enumerate()
                    .filter_map(move |(index, p)| {
                        let endpoint = match entity.kind.as_str() {
                            "line" | "rectangle" => true,
                            "arc" => index == 1 || index == 2,
                            _ => false,
                        };
                        (endpoint && p[0].hypot(p[1]) < 1e-7)
                            .then_some(Selection::point(entity.id, index))
                    })
            })
            .next()
            .ok_or("Choose a line or arc endpoint, or a circle rim")?;
    }
    let entity = sk
        .entities
        .iter()
        .find(|e| e.id == selected.entity)
        .ok_or("Tangent source is unavailable")?;
    let points = entity.control_points();
    let mut edge = 0;
    let (index, p, direction, circle) = match entity.kind.as_str() {
        "line" | "rectangle" => {
            edge = if entity.kind == "rectangle" {
                match selected.sub {
                    Sub::Edge(e) => e % 4,
                    Sub::Point(p) => (p + 3) % 4,
                }
            } else {
                0
            };
            let a = if entity.kind == "rectangle" { edge } else { 0 };
            let b = if entity.kind == "rectangle" {
                (edge + 1) % 4
            } else {
                1
            };
            let index = match selected.sub {
                Sub::Point(i) if i == a || i == b => i,
                _ => {
                    if (snap.p[0] - points[a][0]).hypot(snap.p[1] - points[a][1])
                        < (snap.p[0] - points[b][0]).hypot(snap.p[1] - points[b][1])
                    {
                        a
                    } else {
                        b
                    }
                }
            };
            let other = if index == a { b } else { a };
            (
                index,
                points[index],
                [
                    points[index][0] - points[other][0],
                    points[index][1] - points[other][1],
                ],
                false,
            )
        }
        "arc" => {
            let index = match selected.sub {
                Sub::Point(i) if i == 1 || i == 2 => i,
                Sub::Edge(_) => {
                    if (snap.p[0] - points[1][0]).hypot(snap.p[1] - points[1][1])
                        < (snap.p[0] - points[2][0]).hypot(snap.p[1] - points[2][1])
                    {
                        1
                    } else {
                        2
                    }
                }
                _ => return Err("Choose an arc endpoint".into()),
            };
            let r = [
                points[index][0] - points[0][0],
                points[index][1] - points[0][1],
            ];
            let sign = entity.values[6] * if index == 2 { 1. } else { -1. };
            (index, points[index], [-r[1] * sign, r[0] * sign], false)
        }
        "circle" if matches!(selected.sub, Sub::Edge(_)) => {
            let radial = [snap.p[0] - points[0][0], snap.p[1] - points[0][1]];
            let length = radial[0].hypot(radial[1]);
            if length < 1e-7 {
                return Err("Choose the circle rim".into());
            }
            let p = [
                points[0][0] + radial[0] * entity.values[2] / length,
                points[0][1] + radial[1] * entity.values[2] / length,
            ];
            (0, p, [-radial[1], radial[0]], true)
        }
        _ => return Err("Choose a line or arc endpoint, or a circle rim".into()),
    };
    let length = direction[0].hypot(direction[1]);
    if length < 1e-7 {
        return Err("Tangent source is degenerate".into());
    }
    let source = if circle {
        Selection::edge(entity.id, 0)
    } else {
        Selection::point(entity.id, index)
    };
    Ok(TangentStart {
        snap: Snap {
            p,
            target: (!circle).then_some(source),
            ..Default::default()
        },
        source,
        direction: direction.map(|v| v / length),
        edge,
        circle,
    })
}
fn tangent_arc_values(start: &TangentStart, end: [f64; 2]) -> Result<Vec<f64>, String> {
    let p = start.snap.p;
    let delta = [end[0] - p[0], end[1] - p[1]];
    let length = delta[0].hypot(delta[1]);
    if length < 1e-7 {
        return Err("Choose a distinct arc endpoint".into());
    }
    let mut t = start.direction;
    if start.circle && t[0] * delta[0] + t[1] * delta[1] < 0. {
        t = t.map(|v| -v);
    }
    let normal = [-t[1], t[0]];
    let projection = delta[0] * normal[0] + delta[1] * normal[1];
    if projection.abs() < 1e-9 * length.max(1.) {
        return Err("Move away from the tangent line".into());
    }
    let signed_radius = length * length / (2. * projection);
    let center = [
        p[0] + signed_radius * normal[0],
        p[1] + signed_radius * normal[1],
    ];
    let values = vec![
        center[0],
        center[1],
        p[0],
        p[1],
        end[0],
        end[1],
        signed_radius.signum(),
    ];
    if values.iter().any(|v| !v.is_finite() || v.abs() > 1e8) {
        return Err("Arc exceeds the modeling range".into());
    }
    crate::sketch::arc_geometry(&values)?;
    Ok(values)
}
fn arc_draft(
    tool: Tool,
    first: &Snap,
    second: &Snap,
    last: &Snap,
    clockwise: bool,
) -> Result<Vec<f64>, String> {
    let (center, start, end, clockwise) = if tool == Tool::Arc {
        let radius = (second.p[0] - first.p[0]).hypot(second.p[1] - first.p[1]);
        let length = (last.p[0] - first.p[0]).hypot(last.p[1] - first.p[1]);
        if radius < 1e-7 || length < 1e-7 {
            return Err("Choose distinct arc points".into());
        }
        (
            first.p,
            second.p,
            [
                first.p[0] + (last.p[0] - first.p[0]) * radius / length,
                first.p[1] + (last.p[1] - first.p[1]) * radius / length,
            ],
            clockwise,
        )
    } else {
        let u = [second.p[0] - first.p[0], second.p[1] - first.p[1]];
        let v = [last.p[0] - first.p[0], last.p[1] - first.p[1]];
        let cross = u[0] * v[1] - u[1] * v[0];
        if cross.abs() < 1e-8 * (u[0].hypot(u[1]) * v[0].hypot(v[1])).max(1.) {
            return Err("Three arc points must not be collinear".into());
        }
        let uu = u[0] * u[0] + u[1] * u[1];
        let vv = v[0] * v[0] + v[1] * v[1];
        let center = [
            first.p[0] + (uu * v[1] - vv * u[1]) / (2. * cross),
            first.p[1] + (u[0] * vv - v[0] * uu) / (2. * cross),
        ];
        (center, first.p, second.p, cross > 0.)
    };
    let values = vec![
        center[0],
        center[1],
        start[0],
        start[1],
        end[0],
        end[1],
        if clockwise { -1. } else { 1. },
    ];
    crate::sketch::arc_geometry(&values)?;
    Ok(values)
}

fn drag_targets(
    original: &ResolvedSketch,
    selected: &[Selection],
    pointer: [f64; 2],
    delta: [f64; 2],
) -> Vec<SketchConstraint> {
    let mut targets = BTreeMap::new();
    let whole_selection = selected.len() > 1;
    for &selection in selected {
        let Some(entity) = original.entities.iter().find(|e| e.id == selection.entity) else {
            continue;
        };
        let values = &entity.values;
        if entity.kind == "circle" && matches!(selection.sub, Sub::Edge(_)) && !whole_selection {
            let radius = (pointer[0] - values[0])
                .hypot(pointer[1] - values[1])
                .max(1e-4);
            return vec![SketchConstraint {
                kind: "CurvePoint".into(),
                a: selection.entity,
                value: fmt(radius),
                position: Some(pointer),
                ..Default::default()
            }];
        }
        let points = entity.control_points();
        for (index, point) in points.iter().enumerate() {
            if matches!(selection.sub,Sub::Point(n) if n!=index) {
                continue;
            }
            targets.insert((selection.entity, index, "X"), point[0] + delta[0]);
            targets.insert((selection.entity, index, "Y"), point[1] + delta[1]);
        }
    }
    targets
        .into_iter()
        .map(|((entity, index, kind), value)| SketchConstraint {
            kind: kind.into(),
            a: entity,
            points: [index, 0],
            value: fmt(value),
            ..Default::default()
        })
        .collect()
}

fn vertices(sk: &ResolvedSketch) -> Vec<(Selection, [f64; 2])> {
    let mut result = vec![(Selection::point(0, 0), [0., 0.])];
    for e in &sk.entities {
        result.extend(
            e.control_points()
                .into_iter()
                .enumerate()
                .map(|(n, p)| (Selection::point(e.id, n), p)),
        );
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
    pick_curve(sk, projector, rect, pos)
}
fn pick_curve(
    sk: &ResolvedSketch,
    projector: &Projector,
    rect: Rect,
    pos: Pos2,
) -> Option<Selection> {
    let mut distance = 6.;
    let mut nearest = None;
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
    paint_curves(ui, sk, projector, rect, &[], None, false);
}
fn paint_curves(
    ui: &egui::Ui,
    sk: &ResolvedSketch,
    projector: &Projector,
    rect: Rect,
    selected: &[Selection],
    hover: Option<Selection>,
    conflict: bool,
) {
    let painter = ui.painter().with_clip_rect(rect);
    let mut curves = Vec::new();
    for entity in &sk.entities {
        let points: Vec<_> = entity
            .points()
            .into_iter()
            .map(|p| projector.project(sk.plane.at(p), rect))
            .collect();
        let pieces: Vec<_> = if entity.kind == "rectangle" {
            points
                .windows(2)
                .enumerate()
                .map(|(edge, p)| (edge, p.to_vec()))
                .collect()
        } else {
            vec![(0, points)]
        };
        for (edge, points) in pieces {
            let selection = Selection::edge(entity.id, edge);
            let layer = if selected.contains(&selection) {
                2
            } else if hover == Some(selection) {
                1
            } else {
                0
            };
            let color = match layer {
                2 => ui.visuals().selection.stroke.color,
                1 => ui.visuals().widgets.hovered.bg_stroke.color,
                _ if conflict => RED,
                _ if entity.fixed => WHITE,
                _ => BLUE,
            };
            curves.push((layer, points, entity.construction, color));
        }
    }
    // Interaction highlights must survive coincident geometry drawn later.
    for layer in 0..=2 {
        for (_, points, construction, color) in curves.iter().filter(|c| c.0 == layer) {
            let width: f32 = match layer {
                2 => 3.2,
                1 => 2.5,
                _ => 1.5,
            };
            if layer > 0 {
                stroke_curve(
                    &painter,
                    points,
                    *construction,
                    Stroke::new(width + 3., Color32::from_rgb(7, 7, 10)),
                );
            }
            stroke_curve(&painter, points, *construction, Stroke::new(width, *color));
        }
    }
}
fn stroke_curve(painter: &egui::Painter, points: &[Pos2], construction: bool, stroke: Stroke) {
    if !construction {
        painter.add(egui::Shape::line(points.to_vec(), stroke));
        return;
    }
    let clip = painter.clip_rect().expand(stroke.width);
    let mut phase = 0.;
    for p in points.windows(2) {
        let start = [f64::from(p[0].x), f64::from(p[0].y)];
        let delta = [f64::from(p[1].x) - start[0], f64::from(p[1].y) - start[1]];
        let length = delta[0].hypot(delta[1]);
        if length < 1e-8 {
            continue;
        }
        if let Some((low, high)) = crate::render::clip_segment(p[0], p[1], clip) {
            let (low, high) = (low * length, high * length);
            let point = |distance: f64| {
                Pos2::new(
                    (start[0] + delta[0] * distance / length) as f32,
                    (start[1] + delta[1] * distance / length) as f32,
                )
            };
            // Emit only visible dashes, retaining phase along the original path.
            // Work and memory must not grow with offscreen distance or zoom.
            for dash in ((phase + low) / 10.).floor() as i64..=((phase + high) / 10.).floor() as i64
            {
                let a = (dash as f64 * 10. - phase).max(low);
                let b = (dash as f64 * 10. + 6. - phase).min(high);
                if a < b {
                    painter.line_segment([point(a), point(b)], stroke);
                }
            }
        }
        phase = (phase + length) % 10.;
    }
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
    paint_curves(ui, sk, projector, rect, selected, hover, conflict);
    let painter = ui.painter().with_clip_rect(rect);
    let points = vertices(sk);
    for layer in 0..=2 {
        for (selection, point) in &points {
            let point_layer = if selected.contains(selection) {
                2
            } else if hover == Some(*selection) {
                1
            } else {
                0
            };
            if layer != point_layer {
                continue;
            }
            let pos = projector.project(sk.plane.at(*point), rect);
            let color = match layer {
                2 => ui.visuals().selection.stroke.color,
                1 => ui.visuals().widgets.hovered.bg_stroke.color,
                _ if selection.entity == 0 => WHITE,
                _ if conflict => RED,
                _ if sk
                    .entities
                    .iter()
                    .any(|e| e.id == selection.entity && e.fixed) =>
                {
                    WHITE
                }
                _ => BLUE,
            };
            if selection.entity == 0 {
                painter.line_segment(
                    [pos - Vec2::new(6., 0.), pos + Vec2::new(6., 0.)],
                    Stroke::new(1.0_f32, color),
                );
                painter.line_segment(
                    [pos - Vec2::new(0., 6.), pos + Vec2::new(0., 6.)],
                    Stroke::new(1.0_f32, color),
                );
            }
            if layer > 0 {
                painter.circle_filled(pos, 6., Color32::from_rgb(7, 7, 10));
                painter.circle_stroke(
                    pos,
                    if layer == 2 { 4.5 } else { 4. },
                    Stroke::new(2_f32, color),
                );
            } else if selection.entity == 0 {
                painter.circle_stroke(pos, 4., Stroke::new(1_f32, color));
            } else {
                painter.circle_filled(pos, 2.2, color);
            }
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
        ui.visuals().selection.stroke.color
    } else if c.reference {
        MUTED
    } else {
        WHITE
    });
    let painter = ui.painter().with_clip_rect(rect);
    let project = |p| projector.project(sk.plane.at(p), rect);
    if c.kind == "Radius" || c.kind == "Diameter" {
        let center = dimensions::point(sk, c.a, 0).ok()?;
        let entity = sk.entities.iter().find(|e| e.id == c.a)?;
        let radius = entity.arc().map_or(entity.values[2], |arc| arc.radius);
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
            "X" | "Y" | "StartX" | "StartY" | "EndX" | "EndY" => {
                let p = dimensions::point(
                    sk,
                    c.a,
                    if c.kind.starts_with("End")
                        && sk.entities.iter().any(|e| e.id == c.a && e.kind == "arc")
                    {
                        2
                    } else if c.kind.starts_with("End") || c.kind.starts_with("Start") {
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
    painter.rect_filled(bounds, 3., Color32::from_rgb(12, 12, 15));
    if selected {
        painter.rect_stroke(bounds, 3., Stroke::new(1.2_f32, color), egui::StrokeKind::Outside);
    }
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
        modifiers: egui::Modifiers,
        checkpoints: usize,
        format: u32,
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
                modifiers: egui::Modifiers::NONE,
                checkpoints: 0,
                format: 2,
            }
        }
        fn frame(&mut self, events: Vec<egui::Event>) {
            self.time += 0.05;
            for event in &events {
                if let egui::Event::Key { modifiers, .. }
                | egui::Event::PointerButton { modifiers, .. } = event
                {
                    self.modifiers = *modifiers;
                }
            }
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.))),
                time: Some(self.time),
                events,
                modifiers: self.modifiers,
                ..Default::default()
            };
            let resolved = crate::sketch::resolve(
                &self.sketch,
                Plane::named("Top", 0.),
                &Numbers::for_format(&BTreeMap::new(), self.format).unwrap(),
            )
            .unwrap();
            let _ = self.ctx.run(input, |ctx| {
                self.editor.shortcuts(ctx);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        let rect = ui.available_rect_before_wrap();
                        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                        let edit = self.editor.canvas(
                            ui,
                            rect,
                            &response,
                            &self.camera,
                            &mut self.sketch,
                            Some(&resolved),
                            Plane::named("Top", 0.),
                            &BTreeMap::new(),
                            self.format,
                            false,
                        );
                        self.checkpoints += usize::from(edit.checkpoint);
                    });
            });
        }
        fn screen(&self, point: [f64; 2]) -> Pos2 {
            let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.));
            self.camera
                .projector(rect)
                .project([point[0], point[1], 0.], rect)
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
            self.key_with_modifiers(key, egui::Modifiers::NONE);
        }
        fn drag_box(&mut self, start: Pos2, end: Pos2, modifiers: egui::Modifiers) {
            self.frame(vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                },
            ]);
            self.frame(vec![egui::Event::PointerMoved(end)]);
            self.frame(vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }]);
        }
        fn key_with_modifiers(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
            self.frame(vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers,
            }]);
            self.frame(vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: false,
                repeat: false,
                modifiers,
            }]);
        }
    }
    #[test]
    fn tangent_arc_shift_a_chain_is_exact_parametric_and_reopens() {
        let mut h = Harness::new();
        h.key(egui::Key::L);
        for p in [[-30., -10.], [0., -10.]] {
            let p = h.screen(p);
            h.click(p.x, p.y);
        }
        let before = h.sketch.clone();
        let checkpoints = h.checkpoints;
        h.key_with_modifiers(egui::Key::A, egui::Modifiers::SHIFT);
        assert_eq!(h.editor.tool, Tool::TangentArc);
        let p = h.screen([10., 0.]);
        h.click(p.x, p.y);
        assert_eq!(h.editor.tool, Tool::Line);
        assert_eq!(h.checkpoints, checkpoints + 1);
        assert_eq!(h.sketch.entities[1].kind, "arc");
        assert!(
            h.sketch
                .constraints
                .iter()
                .any(|c| c.kind == "Coincident" && c.a == 2 && c.points[0] == 1)
        );
        assert!(
            h.sketch
                .constraints
                .iter()
                .any(|c| c.kind == "Tangent" && c.a == 2 && c.b == 1)
        );
        for p in [[10., 20.], [-30., 20.], [-30., -10.]] {
            let p = h.screen(p);
            h.click(p.x, p.y);
        }
        h.key(egui::Key::Escape);
        assert!(
            h.sketch
                .constraints
                .iter()
                .any(|c| c.kind == "Tangent" && c.a == 3 && c.b == 2)
        );
        let numbers = Numbers::new(&BTreeMap::new()).unwrap();
        let resolved =
            crate::sketch::resolve(&h.sketch, Plane::named("Top", 0.), &numbers).unwrap();
        let arc = resolved.entities[1].arc().unwrap();
        assert!((arc.radius - 10.).abs() < 1e-4);
        let registry = crate::plugins::Registry::builtin().unwrap();
        let mut doc = crate::model::Document::default();
        let mut sketch = registry.create("sketch", &mut doc).unwrap();
        sketch.set_sketch(&h.sketch);
        doc.features.push(sketch);
        let mut extrude = registry.create("extrude", &mut doc).unwrap();
        extrude.params["sketch"] = serde_json::json!("f1");
        extrude.params["depth"] = serde_json::json!("5");
        extrude.params["regions"] =
            serde_json::json!([crate::sketch::regions(&resolved).unwrap()[0].key]);
        doc.features.push(extrude);
        let mut runtime = crate::engine::Runtime::new(registry);
        let output = runtime.evaluate(&doc).unwrap();
        assert!(output.scene.error.is_none(), "{:?}", output.scene.error);
        assert!(
            (output.scene.meshes[0].volume - (1100. + 25. * std::f64::consts::PI) * 5.).abs()
                < 0.02
        );
        let support = output.scene.meshes[0]
            .faces
            .iter()
            .find(|f| f.plane.as_ref().is_some_and(|p| p.normal[1] < -0.99))
            .unwrap()
            .key
            .clone();
        let rim = output.scene.meshes[0]
            .edges
            .iter()
            .find(|e| e.key.contains("a2["))
            .unwrap()
            .key
            .clone();
        let mut attached = runtime.registry.create("sketch", &mut doc).unwrap();
        attached.params["support"] = serde_json::json!(support);
        let mut attached_profile = Sketch::default();
        attached_profile.add_circle([1., 1.], 1.);
        attached.set_sketch(&attached_profile);
        doc.features.push(attached);
        let path = std::path::PathBuf::from("target/test-output")
            .join(format!("tangent-arc-{}.cadpart", std::process::id()));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        doc.save(&path).unwrap();
        let reopened = crate::model::Document::load(&path).unwrap();
        let scene = runtime.evaluate(&reopened).unwrap().scene;
        assert!(scene.error.is_none());
        let complete = h.sketch.clone();
        h.sketch = before;
        h.editor.cancel_gesture();
        h.frame(vec![]);
        assert_eq!(h.sketch.entities.len(), 1);
        h.sketch = complete;
        h.frame(vec![]);
        assert_eq!(h.sketch.entities[1].kind, "arc");
        h.editor.selected = vec![Selection::edge(2, 0)];
        h.key(egui::Key::D);
        let label = h.screen([25., -20.]);
        h.click(label.x, label.y);
        assert_eq!(
            h.editor.dimension.as_ref().unwrap().constraint.kind,
            "Radius"
        );
        h.frame(vec![egui::Event::Text("15".into())]);
        h.key(egui::Key::Enter);
        assert!(h.editor.dimension.is_none());
        let changed = crate::sketch::resolve(&h.sketch, Plane::named("Top", 0.), &numbers).unwrap();
        let arc = changed.entities[1].arc().unwrap();
        assert!((arc.radius - 15.).abs() < 1e-5);
        doc.features[0].set_sketch(&h.sketch);
        doc.save(&path).unwrap();
        let changed = runtime
            .evaluate(&crate::model::Document::load(&path).unwrap())
            .unwrap()
            .scene;
        assert!(changed.error.is_none(), "{:?}", changed.error);
        for radius in [6., 12.5, 20., 15.] {
            h.sketch
                .constraints
                .iter_mut()
                .find(|c| c.kind == "Radius")
                .unwrap()
                .value = radius.to_string();
            let solved =
                crate::sketch::resolve(&h.sketch, Plane::named("Top", 0.), &numbers).unwrap();
            assert!((solved.entities[1].arc().unwrap().radius - radius).abs() < 1e-5);
            seed_from_resolved(&mut h.sketch, Some(&solved));
            doc.features[0].set_sketch(&h.sketch);
            doc.save(&path).unwrap();
            let scene = runtime
                .evaluate(&crate::model::Document::load(&path).unwrap())
                .unwrap()
                .scene;
            assert!(scene.error.is_none(), "radius {radius}: {:?}", scene.error);
            assert!(
                scene.meshes[0]
                    .edges
                    .iter()
                    .any(|e| crate::model::reference_matches(&e.key, &rim))
            );
            let face = scene.meshes[0]
                .faces
                .iter()
                .find(|f| crate::model::reference_matches(&f.key, &support))
                .unwrap()
                .plane
                .as_ref()
                .unwrap();
            let attached = &scene.sketches["f3"].plane;
            assert!(face.normal[1] < -0.99);
            for (actual, expected) in attached
                .origin
                .iter()
                .chain(&attached.u)
                .chain(&attached.v)
                .chain(&attached.normal)
                .zip(
                    face.origin
                        .iter()
                        .chain(&face.u)
                        .chain(&face.v)
                        .chain(&face.normal),
                )
            {
                assert!(
                    (actual - expected).abs() < 1e-8,
                    "radius {radius}: {actual} != {expected}"
                );
            }
        }
    }
    #[test]
    fn tangent_arc_auto_transition_and_cancel_do_not_create_geometry() {
        let mut h = Harness::new();
        h.key(egui::Key::L);
        for p in [[-20., 0.], [20., 0.]] {
            let p = h.screen(p);
            h.click(p.x, p.y);
        }
        let away = h.screen([30., 10.]);
        h.frame(vec![egui::Event::PointerMoved(away)]);
        let endpoint = h.screen([20., 0.]);
        h.frame(vec![egui::Event::PointerMoved(endpoint)]);
        assert_eq!(h.editor.tool, Tool::TangentArc);
        let before = h.sketch.clone();
        h.key(egui::Key::Escape);
        assert_eq!(h.sketch, before);
        assert_eq!(h.editor.tool, Tool::Select);
        h.editor.set_tool(Tool::TangentArc);
        let start = h.screen([-20., 0.]);
        h.click(start.x, start.y);
        let straight = h.screen([-40., 0.]);
        h.click(straight.x, straight.y);
        assert_eq!(h.sketch, before);
        assert!(h.editor.hint.contains("tangent line"));
        h.key(egui::Key::Escape);
        assert_eq!(h.sketch, before);
    }
    #[test]
    fn tangent_arc_joins_existing_arc_and_circle_with_persistent_relations() {
        for circle in [false, true] {
            let mut h = Harness::new();
            if circle {
                h.sketch.add_circle([0., 0.], 10.);
            } else {
                h.sketch.add_arc([0., 0.], [10., 0.], [0., 10.], false);
            }
            h.sketch.constraints.push(SketchConstraint {
                kind: "Fix".into(),
                a: 1,
                ..Default::default()
            });
            h.frame(vec![]);
            h.editor.set_tool(Tool::TangentArc);
            let p = h.screen([10., 0.]);
            h.click(p.x, p.y);
            let p = h.screen(if circle { [20., 10.] } else { [20., -10.] });
            h.click(p.x, p.y);
            assert_eq!(h.sketch.entities.len(), 2, "{}", h.editor.hint);
            h.editor.selected=vec![Selection::edge(2,0),Selection::edge(1,0)];
            let relationship=h.editor.relationship("Tangent",&h.sketch,&h.editor.selected).unwrap();
            assert_eq!(relationship.points[0],1);
            assert!(
                h.sketch
                    .constraints
                    .iter()
                    .any(|c| c.kind == "Tangent" && c.a == 2 && c.b == 1)
            );
            assert!(h.sketch.constraints.iter().any(|c| c.kind
                == if circle {
                    "PointOnCircle"
                } else {
                    "Coincident"
                }));
            h.key(egui::Key::Escape);
            h.sketch.constraints.push(SketchConstraint {
                kind: "Radius".into(),
                a: 2,
                value: "15".into(),
                ..Default::default()
            });
            let solved = crate::sketch::resolve(
                &h.sketch,
                Plane::named("Top", 0.),
                &Numbers::new(&BTreeMap::new()).unwrap(),
            )
            .unwrap();
            let a = solved.entities[1].arc().unwrap();
            assert!((a.radius - 15.).abs() < 1e-5);
            let p = solved.entities[1].control_points()[1];
            let r = [p[0] - a.center[0], p[1] - a.center[1]];
            assert!(
                (p[0] * r[1] - p[1] * r[0]).abs() < 1e-5,
                "Tangent radii must stay collinear"
            );
        }
    }
    #[test]
    fn tangent_arc_click_drag_has_one_checkpoint_and_supports_construction() {
        let mut h = Harness::new();
        h.sketch.add_line([-20., 0.], [0., 0.]);
        h.frame(vec![]);
        h.editor.set_tool(Tool::TangentArc);
        h.key(egui::Key::Q);
        let start = h.screen([0., 0.]);
        let end = h.screen([10., 10.]);
        let checkpoints = h.checkpoints;
        h.drag_box(start, end, egui::Modifiers::NONE);
        assert_eq!(h.sketch.entities.len(), 2, "{}", h.editor.hint);
        assert!(h.sketch.entities[1].construction);
        assert_eq!(h.checkpoints, checkpoints + 1);
        assert_eq!(h.editor.tool, Tool::Line);
    }
    #[test]
    fn tangent_arc_profile_has_exact_step_geometry() {
        let mut sketch = Sketch::default();
        sketch.add_line([-30., -10.], [0., -10.]);
        let numbers = Numbers::new(&BTreeMap::new()).unwrap();
        let resolved = crate::sketch::resolve(&sketch, Plane::named("Top", 0.), &numbers).unwrap();
        let source = tangent_start(
            &resolved,
            &Snap {
                p: [0., -10.],
                target: Some(Selection::point(1, 1)),
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let values = tangent_arc_values(&source, [10., 0.]).unwrap();
        sketch.add_arc(
            [values[0], values[1]],
            [values[2], values[3]],
            [values[4], values[5]],
            values[6] < 0.,
        );
        for (a, b) in [
            ([10., 0.], [10., 20.]),
            ([10., 20.], [-30., 20.]),
            ([-30., 20.], [-30., -10.]),
        ] {
            sketch.add_line(a, b);
        }
        let resolved = crate::sketch::resolve(&sketch, Plane::named("Top", 0.), &numbers).unwrap();
        let registry = crate::plugins::Registry::builtin().unwrap();
        let mut doc = crate::model::Document::default();
        let mut feature = registry.create("sketch", &mut doc).unwrap();
        feature.set_sketch(&sketch);
        doc.features.push(feature);
        let mut feature = registry.create("extrude", &mut doc).unwrap();
        feature.params["sketch"] = serde_json::json!("f1");
        feature.params["depth"] = serde_json::json!("5");
        feature.params["regions"] =
            serde_json::json!([crate::sketch::regions(&resolved).unwrap()[0].key]);
        doc.features.push(feature);
        let mut runtime = crate::engine::Runtime::new(registry);
        let output = runtime.evaluate(&doc).unwrap();
        assert!(output.scene.error.is_none(), "{:?}", output.scene.error);
        let volume = (1100. + 25. * std::f64::consts::PI) * 5.;
        assert!((output.scene.meshes[0].volume - volume).abs() < 1e-6);
        #[cfg(feature = "step")]
        {
            let folder = std::path::Path::new("target/test-output/geometry-controls");
            std::fs::create_dir_all(folder).unwrap();
            doc.save(&folder.join("tangent-arc-profile.cadpart"))
                .unwrap();
            output
                .geometry
                .export("step", &folder.join("tangent-arc-profile.step"))
                .unwrap();
            std::fs::write(folder.join("tangent-arc-profile.json"),serde_json::to_vec_pretty(&serde_json::json!({
                "case":"tangent-arc-profile","step_exported":true,"expected_solids":[{"volume_mm3":volume,"bounds_mm":[[-30.,-10.,0.],[10.,20.,5.]]}]
            })).unwrap()).unwrap();
        }
    }
    #[test]
    fn tangent_arc_math_keeps_outward_direction_for_major_and_clockwise_arcs() {
        let start = TangentStart {
            snap: Snap {
                p: [0., 0.],
                ..Default::default()
            },
            source: Selection::point(1, 1),
            direction: [1., 0.],
            edge: 0,
            circle: false,
        };
        for (end, sign, major) in [
            ([10., 10.], 1., false),
            ([10., -10.], -1., false),
            ([-10., 10.], 1., true),
            ([-10., -10.], -1., true),
        ] {
            let v = tangent_arc_values(&start, end).unwrap();
            let a = crate::sketch::arc_geometry(&v).unwrap();
            assert_eq!(a.sweep.signum(), sign);
            assert_eq!(a.sweep.abs() > std::f64::consts::PI, major);
            let tangent = [-(v[3] - v[1]) * sign, (v[2] - v[0]) * sign];
            assert!(tangent[0] > 0.);
            assert!(tangent[1].abs() < 1e-9);
        }
        assert!(tangent_arc_values(&start, [10., 0.]).is_err());
        assert!(tangent_arc_values(&start, [0., 0.]).is_err());
    }
    #[test]
    fn dimension_expressions_use_document_format_and_preserve_invalid_drafts() {
        for (format, expression) in [(2, "sqrt(81) + 2 x sin(30)"), (1, "10 * sin(PI / 2)")] {
            let mut h = Harness::new();
            h.format = format;
            h.sketch.add_circle([0., 0.], 20.);
            h.frame(vec![]);
            h.editor.selected = vec![Selection::edge(1, 0)];
            h.key(egui::Key::D);
            let label = h.screen([30., 25.]);
            h.click(label.x, label.y);
            h.frame(vec![egui::Event::Text(expression.into())]);
            h.key(egui::Key::Enter);
            assert!(h.editor.dimension.is_none());
            assert_eq!(h.sketch.constraints[0].value, expression);
            let solved = crate::sketch::resolve(
                &h.sketch,
                Plane::named("Top", 0.),
                &Numbers::for_format(&BTreeMap::new(), format).unwrap(),
            )
            .unwrap();
            let radius = if h.sketch.constraints[0].kind == "Diameter" {
                5.
            } else {
                10.
            };
            assert!(
                (solved.entities[0].values[2] - radius).abs() < 1e-6,
                "{format}/{expression}: {:?}",
                solved.entities[0].values
            );
            let before = h.sketch.clone();
            h.editor.edit_dimension(&h.sketch, 0);
            h.frame(vec![]);
            h.frame(vec![egui::Event::Text("sqrt(-1)".into())]);
            h.key(egui::Key::Enter);
            assert_eq!(h.sketch, before);
            assert!(h.editor.dimension.as_ref().unwrap().error.is_some());
            assert_eq!(h.editor.dimension.as_ref().unwrap().text, "sqrt(-1)");
        }
    }
    #[test]
    fn trim_key_edits_the_exact_curve_as_one_undo_checkpoint_and_reopens() {
        let mut h = Harness::new();
        h.sketch.add_circle([0., 0.], 20.);
        h.sketch.add_line([-25., 0.], [25., 0.]);
        h.frame(vec![]);
        let before = h.sketch.clone();
        h.key(egui::Key::M);
        assert_eq!(h.editor.tool, Tool::Trim);
        let p = h.screen([0., 20.]);
        h.click(p.x, p.y);
        assert_eq!(h.sketch.entities[0].kind, "arc");
        assert_eq!(h.checkpoints, 1);
        let reopened: Sketch =
            serde_json::from_str(&serde_json::to_string(&h.sketch).unwrap()).unwrap();
        let resolved = crate::sketch::resolve(
            &reopened,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        assert_eq!(crate::sketch::regions(&resolved).unwrap().len(), 1);
        // The application stores this exact pre-edit sketch for its undo checkpoint.
        h.sketch = before.clone();
        h.frame(vec![]);
        assert_eq!(h.sketch, before);
    }

    #[test]
    fn q_changes_only_selected_rectangle_sides_and_restores_mixed_flags() {
        let mut h = Harness::new();
        h.sketch.add_rect([-20., -10.], [20., 10.]);
        h.sketch.add_circle([60., 0.], 5.);
        h.sketch.entities[1].construction = true;
        h.editor.selected = vec![Selection::edge(1, 2), Selection::edge(2, 0)];
        h.frame(vec![]);
        let before = h.sketch.clone();
        let checkpoints = h.checkpoints;
        h.key(egui::Key::Q);
        assert_eq!(h.sketch.entities.len(), 5);
        assert_eq!(h.checkpoints, checkpoints + 1);
        for (edge, line) in h.sketch.entities[..4].iter().enumerate() {
            assert_eq!(line.construction, edge == 2);
        }
        assert!(!h.sketch.entities[4].construction);
        let after = h.sketch.clone();
        h.key(egui::Key::Q);
        assert!(h.sketch.entities[..4].iter().all(|e| !e.construction));
        assert!(h.sketch.entities[4].construction);
        // Editor undo/redo restores complete snapshots; stale selected child IDs
        // are pruned when undo restores the original structural rectangle.
        h.sketch = before;
        h.editor.cancel_gesture();
        h.frame(vec![]);
        assert!(
            h.editor
                .selected
                .iter()
                .all(|s| h.sketch.entities.iter().any(|e| e.id == s.entity))
        );
        h.sketch = after.clone();
        h.frame(vec![]);
        assert_eq!(
            serde_json::from_str::<Sketch>(&serde_json::to_string(&h.sketch).unwrap()).unwrap(),
            after
        );
    }
    #[test]
    fn center_rectangle_side_conversion_preserves_center_and_width_dimension() {
        let mut h = Harness::new();
        h.key(egui::Key::R);
        let a = h.screen([0., 0.]);
        h.click(a.x, a.y);
        let b = h.screen([20., 10.]);
        h.click(b.x, b.y);
        h.key(egui::Key::Escape);
        h.sketch.constraints.push(SketchConstraint {
            kind: "Width".into(),
            a: 1,
            value: "40".into(),
            ..Default::default()
        });
        h.frame(vec![]);
        h.editor.selected = vec![Selection::edge(1, 1)];
        h.key(egui::Key::Q);
        assert_eq!(h.sketch.entities.len(), 6);
        assert!(h.sketch.entities[1].construction);
        let width = h
            .sketch
            .constraints
            .iter_mut()
            .find(|c| c.kind == "Length" && c.value == "40")
            .unwrap();
        width.value = "70".into();
        let solved = crate::sketch::resolve(
            &h.sketch,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        let endpoints: Vec<_> = solved.entities[..4]
            .iter()
            .flat_map(|e| e.control_points())
            .collect();
        let lo = endpoints.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let hi = endpoints
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(
            (lo + hi).abs() < 1e-6,
            "Center rectangle must remain centered: {lo}..{hi}"
        );
        assert!((hi - lo - 70.).abs() < 1e-6);
    }
    #[test]
    fn q_creation_mode_and_dimension_cancel_release_keyboard_focus() {
        let mut h = Harness::new();
        h.key(egui::Key::L);
        h.key(egui::Key::Q);
        let a = h.screen([-20., 0.]);
        h.click(a.x, a.y);
        let b = h.screen([20., 0.]);
        h.click(b.x, b.y);
        assert!(h.sketch.entities[0].construction);
        h.key(egui::Key::Escape);
        h.editor.selected = vec![Selection::edge(1, 0)];
        h.key(egui::Key::D);
        let label = h.screen([0., -15.]);
        h.click(label.x, label.y);
        assert!(h.editor.dimension.is_some());
        assert!(h.ctx.wants_keyboard_input());
        h.key(egui::Key::Escape);
        assert!(h.editor.dimension.is_none());
        assert!(!h.ctx.wants_keyboard_input());
        h.editor.selected = vec![Selection::edge(1, 0)];
        h.key(egui::Key::Q);
        assert!(!h.sketch.entities[0].construction);
    }
    #[test]
    fn center_arc_draw_drag_radius_dimension_and_construction_are_native() {
        let mut h = Harness::new();
        h.editor.set_tool(Tool::Arc);
        h.frame(vec![]);
        for p in [[0., 0.], [20., 0.], [0., 20.]] {
            let p = h.screen(p);
            h.click(p.x, p.y);
        }
        assert_eq!(h.sketch.entities.len(), 1);
        assert_eq!(h.sketch.entities[0].kind, "arc");
        let resolved = crate::sketch::resolve(
            &h.sketch,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        let arc = resolved.entities[0].arc().unwrap();
        assert!((arc.sweep - std::f64::consts::FRAC_PI_2).abs() < 1e-5);
        h.editor.set_tool(Tool::Select);
        h.drag_box(
            h.screen([0., 20.]),
            h.screen([0., 30.]),
            egui::Modifiers::NONE,
        );
        let resolved = crate::sketch::resolve(
            &h.sketch,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        assert!((resolved.entities[0].arc().unwrap().radius - 30.).abs() < 1e-4);
        h.editor.selected = vec![Selection::edge(1, 0)];
        h.key(egui::Key::D);
        let p = h.screen([35., 30.]);
        h.click(p.x, p.y);
        assert_eq!(
            h.editor.dimension.as_ref().unwrap().constraint.kind,
            "Radius"
        );
        h.frame(vec![egui::Event::Text("40".into())]);
        h.key(egui::Key::Enter);
        assert!(h.editor.dimension.is_none());
        let resolved = crate::sketch::resolve(
            &h.sketch,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        assert!((resolved.entities[0].arc().unwrap().radius - 40.).abs() < 1e-4);
        h.editor.selected = vec![Selection::edge(1, 0)];
        h.key(egui::Key::Q);
        assert!(h.sketch.entities[0].construction);
    }

    #[test]
    fn onshape_three_point_arc_key_draws_through_third_point_and_cancels_cleanly() {
        let mut h = Harness::new();
        h.key(egui::Key::A);
        assert_eq!(h.editor.tool, Tool::ThreePointArc);
        for p in [[-20., 0.], [20., 0.], [0., 20.]] {
            let p = h.screen(p);
            h.click(p.x, p.y);
        }
        assert_eq!(h.sketch.entities.len(), 1);
        let resolved = crate::sketch::resolve(
            &h.sketch,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        let arc = resolved.entities[0].arc().unwrap();
        assert!((arc.radius - 20.).abs() < 1e-4);
        assert!((arc.sweep + std::f64::consts::PI).abs() < 1e-5);
        let middle = arc.start + arc.sweep / 2.;
        assert!((arc.center[0] + arc.radius * middle.cos()).abs() < 1e-4);
        assert!((arc.center[1] + arc.radius * middle.sin() - 20.).abs() < 1e-4);
        let before = h.sketch.clone();
        let p = h.screen([30., 30.]);
        h.click(p.x, p.y);
        h.key(egui::Key::Escape);
        assert_eq!(h.sketch, before);
        assert!(h.editor.first.is_none());
        assert!(h.editor.second.is_none());
    }

    #[test]
    fn dragging_circle_rim_resizes_free_radius_or_moves_a_dimensioned_circle() {
        for dimensioned in [false, true] {
            let mut h = Harness::new();
            h.sketch.add_circle([20., 10.], 10.);
            if dimensioned {
                h.sketch.constraints.push(SketchConstraint {
                    kind: "Radius".into(),
                    a: 1,
                    value: "10".into(),
                    ..Default::default()
                });
            }
            h.frame(vec![]);
            h.drag_box(
                h.screen([30., 10.]),
                h.screen([40., 10.]),
                egui::Modifiers::NONE,
            );
            let resolved = crate::sketch::resolve(
                &h.sketch,
                Plane::named("Top", 0.),
                &Numbers::new(&BTreeMap::new()).unwrap(),
            )
            .unwrap();
            let v = &resolved.entities[0].values;
            assert!(
                (v[0] - if dimensioned { 30. } else { 20. }).abs() < 1e-4,
                "{v:?}"
            );
            assert!((v[1] - 10.).abs() < 1e-4, "{v:?}");
            assert!(
                (v[2] - if dimensioned { 10. } else { 20. }).abs() < 1e-4,
                "{v:?}"
            );
            assert_eq!(h.sketch.constraints.len(), usize::from(dimensioned));
        }
    }

    #[test]
    fn conflicting_dimension_is_rejected_without_changing_the_visible_sketch() {
        let mut h = Harness::new();
        h.sketch.add_circle([20., 10.], 10.);
        h.sketch.constraints.push(SketchConstraint {
            kind: "Radius".into(),
            a: 1,
            value: "10".into(),
            ..Default::default()
        });
        h.frame(vec![]);
        h.editor.selected = vec![Selection::edge(1, 0)];
        h.key(egui::Key::D);
        let p = h.screen([40., 30.]);
        h.click(p.x, p.y);
        let before = h.sketch.clone();
        h.frame(vec![egui::Event::Text("40".into())]);
        h.key(egui::Key::Enter);
        assert_eq!(h.sketch, before);
        assert!(h.editor.dimension.as_ref().unwrap().error.is_some());
    }

    #[test]
    fn dragging_an_expression_driven_rectangle_uses_its_resolved_control_points() {
        let mut h = Harness::new();
        h.sketch.add_rect([0., 0.], [40., 20.]);
        h.sketch.entities[0].values[2] = "20+20".into();
        h.frame(vec![]);
        h.drag_box(
            h.screen([20., 0.]),
            h.screen([25., 10.]),
            egui::Modifiers::NONE,
        );
        let solved = crate::sketch::resolve(
            &h.sketch,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        for (&value, expected) in solved.entities[0].values.iter().zip([5., 10., 40., 20.]) {
            assert!(
                (value - expected).abs() < 1e-5,
                "{:?}",
                solved.entities[0].values
            );
        }
        assert_eq!(h.sketch.entities[0].values[2], "20+20");
    }

    #[test]
    fn native_qa_rectangle_drag_moves_every_connected_seed_and_survives_save() {
        let mut h = Harness::new();
        h.sketch =
            serde_json::from_str(include_str!("../tests/fixtures/constrained-chain.json")).unwrap();
        let before = h.sketch.clone();
        h.frame(vec![]);
        h.drag_box(
            h.screen([0., 10.]),
            h.screen([12., 2.]),
            egui::Modifiers::NONE,
        );
        let saved = serde_json::to_string(&h.sketch).unwrap();
        let reopened: Sketch = serde_json::from_str(&saved).unwrap();
        let resolved = crate::sketch::resolve(
            &reopened,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        for (original, entity) in before.entities.iter().zip(&resolved.entities) {
            if entity.kind == "circle" {
                assert_eq!(entity.values, vec![50., 20., 6.]);
                assert_eq!(reopened.entities[4], before.entities[4]);
            } else {
                for (i, &value) in entity.values.iter().enumerate() {
                    assert!(
                        (value
                            - original.values[i].parse::<f64>().unwrap()
                            - if i % 2 == 0 { 12. } else { -8. })
                        .abs()
                            < 1e-5,
                        "{:?}",
                        entity.values
                    );
                }
                for (i, value) in reopened
                    .entities
                    .iter()
                    .find(|e| e.id == entity.id)
                    .unwrap()
                    .values
                    .iter()
                    .enumerate()
                {
                    assert!((value.parse::<f64>().unwrap() - entity.values[i]).abs() < 1e-6);
                }
            }
        }
        assert_eq!(reopened.constraints, before.constraints);
        assert_eq!(resolved.dof, 5);
    }

    #[test]
    fn dragging_connected_dimensioned_lines_follows_pointer_and_preserves_shape() {
        let mut h = Harness::new();
        h.sketch.add_line([0., 0.], [20., 0.]);
        h.sketch.add_line([20., 0.], [20., 20.]);
        for (id, kind) in [
            (1, "Horizontal"),
            (2, "Vertical"),
            (1, "Length"),
            (2, "Length"),
        ] {
            h.sketch.constraints.push(SketchConstraint {
                kind: kind.into(),
                a: id,
                value: "20".into(),
                ..Default::default()
            });
        }
        h.frame(vec![]);
        h.drag_box(
            h.screen([20., 0.]),
            h.screen([30., 10.]),
            egui::Modifiers::NONE,
        );
        let solved = crate::sketch::resolve(
            &h.sketch,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        let a = &solved.entities[0].values;
        let b = &solved.entities[1].values;
        assert!(
            (a[2] - 30.).abs() < 1e-5 && (a[3] - 10.).abs() < 1e-5,
            "{a:?}"
        );
        assert!(
            (b[0] - 30.).abs() < 1e-5 && (b[1] - 10.).abs() < 1e-5,
            "{b:?}"
        );
        assert!((a[2] - a[0] - 20.).abs() < 1e-5);
        assert!((b[3] - b[1] - 20.).abs() < 1e-5);
        assert_eq!(solved.dof, 2);
        assert_eq!(h.sketch.constraints.len(), 5);
    }

    #[test]
    fn dragging_two_selected_edges_moves_both_entities() {
        let mut h = Harness::new();
        h.sketch.add_line([0., 0.], [20., 0.]);
        h.sketch.add_line([40., 20.], [60., 20.]);
        h.editor.selected = vec![Selection::edge(1, 0), Selection::edge(2, 0)];
        h.frame(vec![]);
        h.drag_box(
            h.screen([10., 0.]),
            h.screen([15., 10.]),
            egui::Modifiers::NONE,
        );
        let solved = crate::sketch::resolve(
            &h.sketch,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        for (entity, expected) in solved
            .entities
            .iter()
            .zip([[5., 10., 25., 10.], [45., 30., 65., 30.]])
        {
            for (&value, target) in entity.values.iter().zip(expected) {
                assert!((value - target).abs() < 1e-5, "{:?}", entity.values);
            }
        }
        assert!(h.sketch.constraints.is_empty());
        assert_eq!(solved.dof, 8);
    }

    #[test]
    fn clicks_toggle_additively_and_space_or_empty_click_clear_selection() {
        let mut h = Harness::new();
        h.sketch.add_rect([0., 0.], [40., 30.]);
        h.frame(vec![]);
        h.click(600., 350.);
        h.click(500., 250.);
        assert_eq!(h.editor.selected.len(), 2);
        h.click(600., 350.);
        assert_eq!(h.editor.selected, vec![Selection::edge(1, 3)]);
        h.key(egui::Key::Space);
        assert!(h.editor.selected.is_empty());
        h.click(600., 350.);
        h.click(100., 600.);
        assert!(h.editor.selected.is_empty());
    }
    #[test]
    fn dimension_uses_two_preselected_edges_and_repeated_key_exits_tool() {
        let mut h = Harness::new();
        h.sketch.add_line([0., 0.], [40., 0.]);
        h.sketch.add_line([0., 20.], [40., 20.]);
        h.frame(vec![]);
        h.click(600., 350.);
        h.click(600., 242.3);
        assert_eq!(h.editor.selected.len(), 2);
        h.key(egui::Key::D);
        assert_eq!(h.editor.picks.len(), 2);
        h.click(800., 300.);
        assert_eq!(
            h.editor.dimension.as_ref().unwrap().constraint.kind,
            "LineDistance"
        );
        h.key(egui::Key::Escape);
        h.key(egui::Key::D);
        assert_eq!(h.editor.tool, Tool::Select);
        h.key(egui::Key::L);
        assert_eq!(h.editor.tool, Tool::Line);
        h.key(egui::Key::L);
        assert_eq!(h.editor.tool, Tool::Select);
    }
    #[test]
    fn vertical_and_perpendicular_keys_apply_constraints_instead_of_starting_drawing() {
        let mut h = Harness::new();
        h.sketch.add_line([0., 0.], [0., 30.]);
        h.sketch.add_line([0., 0.], [40., 0.]);
        h.frame(vec![]);
        h.click(500., 250.);
        h.key(egui::Key::V);
        assert!(h.sketch.constraints.iter().any(|c| c.kind == "Vertical"));
        h.key(egui::Key::Escape);
        h.click(500., 250.);
        h.click(600., 350.);
        h.key_with_modifiers(egui::Key::L, egui::Modifiers::SHIFT);
        assert!(
            h.sketch
                .constraints
                .iter()
                .any(|c| c.kind == "Perpendicular")
        );
        assert_eq!(h.editor.tool, Tool::Relationship("Perpendicular"));
    }
    #[test]
    fn center_rectangle_keeps_its_center_when_width_changes() {
        let mut h = Harness::new();
        h.key(egui::Key::R);
        h.click(500., 350.);
        h.click(600., 275.);
        assert_eq!(h.sketch.entities.len(), 3);
        assert!(h.sketch.entities[1..].iter().all(|e| e.construction));
        h.sketch.constraints.push(SketchConstraint {
            kind: "Width".into(),
            a: 1,
            value: "60".into(),
            ..Default::default()
        });
        let solved = crate::sketch::resolve(
            &h.sketch,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        let rectangle = &solved.entities[0];
        assert!((rectangle.values[0] + rectangle.values[2] / 2.).abs() < 1e-7);
        assert!((rectangle.values[1] + rectangle.values[3] / 2.).abs() < 1e-7);
        let regions = crate::sketch::regions(&solved).unwrap();
        assert_eq!(
            regions.len(),
            1,
            "Construction diagonals must not subdivide the solid profile"
        );
        assert!(regions[0].contains([0., 0.]));
        assert!(regions[0].holes.is_empty());
    }
    #[test]
    fn box_selection_distinguishes_enclosed_and_crossing_geometry() {
        let mut sketch = Sketch::default();
        sketch.add_line([0., 0.], [40., 0.]);
        let resolved = crate::sketch::resolve(
            &sketch,
            Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        let mut camera = Camera::default();
        camera.align(&resolved.plane);
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.));
        let projector = camera.projector(viewport);
        assert!(
            box_hits(
                &resolved,
                &projector,
                viewport,
                Pos2::new(550., 330.),
                Pos2::new(650., 370.)
            )
            .is_empty()
        );
        assert_eq!(
            box_hits(
                &resolved,
                &projector,
                viewport,
                Pos2::new(650., 330.),
                Pos2::new(550., 370.)
            ),
            vec![Selection::edge(1, 0)]
        );
        assert!(!segment_intersects_rect(
            Pos2::new(0., 0.),
            Pos2::new(10., 10.),
            Rect::from_min_max(Pos2::new(1., 8.), Pos2::new(2., 9.))
        ));
    }
    #[test]
    fn box_drag_adds_and_ctrl_crossing_drag_subtracts_without_deleting_geometry() {
        let mut h = Harness::new();
        h.sketch.add_rect([0., 0.], [40., 30.]);
        h.frame(vec![]);
        h.drag_box(
            Pos2::new(450., 175.),
            Pos2::new(750., 375.),
            egui::Modifiers::NONE,
        );
        assert_eq!(h.editor.selected.len(), 4);
        h.drag_box(
            Pos2::new(650., 330.),
            Pos2::new(550., 370.),
            egui::Modifiers::CTRL,
        );
        assert_eq!(h.editor.selected.len(), 3);
        assert!(!h.editor.selected.contains(&Selection::edge(1, 0)));
        assert_eq!(h.sketch.entities.len(), 1);
        assert!(h.sketch.constraints.is_empty());
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
