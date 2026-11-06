use super::*;
impl Viewport {
    pub fn set_regions(&mut self, state: &Value, body: Option<&str>, active: bool) {
        self.region_scene.clear();
        if !active { return; }
        for o in state["objects"].as_array().into_iter().flatten() {
            if o["body"].as_str()!=body || o["effective_visibility"] != true { continue; }
            let catalog=&o["planar_regions"];
            if catalog["revision"] != state["revision"] { continue; }
            for r in catalog["regions"].as_array().into_iter().flatten() {
                let Some(key)=r["key"].as_str() else {continue};
                let triangles=r["triangles"].as_array().into_iter().flatten().filter_map(|t| {
                    let rows=t.as_array()?;
                    if rows.len()!=3 {return None;}
                    Some([vector(&rows[0]),vector(&rows[1]),vector(&rows[2])])
                }).collect();
                self.region_scene.push((Selection{object:o["name"].as_str().unwrap_or("").into(),subelement:format!("Region:{key}")},triangles));
            }
        }
    }
    pub fn region_hit(&self, pos: Pos2) -> Option<Selection> {
        let mut hits=Vec::new();
        for (selection,triangles) in &self.region_scene {
            for triangle in triangles {
                let p=triangle.map(|p| self.project(p));
                let tri=ProjectedTriangle {points:p.map(|x|x.0),depths:p.map(|x|x.1),face:0,colors:[Color32::WHITE;3]};
                if let Some(depth)=triangle_depth(pos,&tri) {hits.push((depth,selection));}
            }
        }
        hits.into_iter().max_by(|a,b|a.0.total_cmp(&b.0)).map(|(_,s)|s.clone())
    }
    pub(super) fn draw_regions(&self, painter:&egui::Painter) {
        for (selection,triangles) in &self.region_scene {
            let selected=self.selections.contains(selection);
            let color=if selected {Color32::from_rgba_unmultiplied(70,190,165,100)} else {Color32::from_rgba_unmultiplied(90,130,145,35)};
            for triangle in triangles {
                painter.add(Shape::convex_polygon(triangle.iter().map(|p|self.project(*p).0).collect(),color,Stroke::NONE));
            }
        }
    }
}

impl Viewport {
    pub fn set_loft_origins(&mut self, state: &Value, active: bool) {
        self.loft_origins.clear();
        if !active { return; }
        for o in state["objects"].as_array().into_iter().flatten() {
            if o["type"] != "Sketcher::SketchObject" || o["effective_visibility"] != true
                || !o["loft_source"].is_object() { continue; }
            let (origin, _) = placement(&o["global_placement"]);
            self.loft_origins.push((o["name"].as_str().unwrap_or_default().into(), origin));
        }
    }
    pub(super) fn loft_origin_hit(&self, pos: Pos2) -> Option<Selection> {
        self.loft_origins.iter().filter_map(|(name, point)| {
            let (screen, depth) = self.project(*point);
            let distance = screen.distance(pos);
            (distance <= POINT_PICK_RADIUS).then_some((name, distance, depth))
        }).min_by(|a,b| a.1.total_cmp(&b.1).then_with(|| b.2.total_cmp(&a.2)))
          .map(|(name,_,_)| Selection {object:name.clone(), subelement:"Origin".into()})
    }
    pub(super) fn draw_loft_origins(&self, painter: &egui::Painter) {
        for (name, point) in &self.loft_origins {
            let screen = self.project(*point).0;
            painter.circle_stroke(screen, 5., Stroke::new(1.5_f32, Color32::from_rgb(140,190,180)));
            painter.text(screen + Vec2::new(8.,-8.), egui::Align2::LEFT_BOTTOM,
                format!("{name} origin"), egui::FontId::proportional(11.), Color32::LIGHT_GRAY);
        }
    }
}
