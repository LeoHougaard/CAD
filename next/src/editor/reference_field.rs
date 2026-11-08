//! Shared graphics-area selection field. Native identities stay in its value;
//! the normal feature dialog never exposes the native object namespace.
use eframe::egui;
use serde_json::Value;

pub(super) struct FieldResponse {
    pub activated: bool,
    pub changed: bool,
    pub response: egui::Response,
}

struct Chip {
    row: Option<usize>,
    element: Option<usize>,
    text: String,
}

fn chips(value: &Value, objects: &[Value]) -> Vec<Chip> {
    let rows: Vec<(Option<usize>, &Value)> = if let Some(rows) = value.as_array() {
        rows.iter().enumerate().map(|(i, row)| (Some(i), row)).collect()
    } else {
        vec![(None, value)]
    };
    let mut result = Vec::new();
    for (row, reference) in rows {
        let Some(name) = reference["link"].as_str().filter(|n| !n.is_empty()) else { continue };
        let label = objects.iter().find(|o| o["name"] == name)
            .and_then(|o| o["label"].as_str()).unwrap_or(name);
        let elements: Vec<_> = reference["subelements"].as_array().into_iter().flatten()
            .enumerate().filter_map(|(i, e)| e.as_str().filter(|s| !s.is_empty()).map(|s| (i, s))).collect();
        if elements.is_empty() {
            result.push(Chip { row, element: None, text: label.to_owned() });
        } else {
            for (i, element) in elements {
                let kind = if element.starts_with("Edge") { "Edge" }
                    else if element.starts_with("Face") { "Face" }
                    else if element.starts_with("Vertex") { "Vertex" }
                    else { element };
                result.push(Chip { row, element: Some(i), text: format!("{kind} of {label}") });
            }
        }
    }
    result
}

fn remove(value: &mut Value, chip: &Chip) {
    let reference = if let Some(row) = chip.row { &mut value[row] } else { &mut *value };
    if let Some(index) = chip.element {
        if let Some(elements) = reference["subelements"].as_array_mut() {
            elements.remove(index);
            if !elements.is_empty() { return }
        }
    }
    if let Some(row) = chip.row {
        value.as_array_mut().unwrap().remove(row);
    } else {
        *value = Value::Null;
    }
}

pub(super) fn show(ui: &mut egui::Ui, value: &mut Value, objects: &[Value], active: bool) -> FieldResponse {
    let items = chips(value, objects);
    let mut removed = None;
    let stroke = if active { ui.visuals().selection.stroke } else { ui.visuals().widgets.inactive.bg_stroke };
    let frame = egui::Frame::new().fill(ui.visuals().extreme_bg_color).stroke(stroke)
        .corner_radius(3.).inner_margin(5.).show(ui, |ui| {
            ui.scope_builder(egui::UiBuilder::new().id_salt("selection_field").sense(egui::Sense::click()), |ui| {
            ui.set_min_width(245.);
            ui.set_min_height(22.);
            ui.horizontal_wrapped(|ui| {
                if items.is_empty() { ui.add(egui::Label::new(egui::RichText::new("Select entities").weak()).selectable(false)); }
                for (index, item) in items.iter().enumerate() {
                    ui.push_id(index, |ui| {
                        egui::Frame::new().fill(ui.visuals().faint_bg_color).corner_radius(2.)
                            .inner_margin(3.).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(egui::Label::new(&item.text).selectable(false));
                                    let button = ui.small_button("×").on_hover_text("Remove selection");
                                    #[cfg(test)]
                                    ui.ctx().data_mut(|d| d.insert_temp(ui.id().with("remove"), button.rect));
                                    if button.clicked() { removed = Some(index); }
                                });
                            });
                    });
                }
            });
            }).response
        });
    let response = frame.inner;
    if let Some(index) = removed { remove(value, &items[index]); }
    if response.clicked() {response.request_focus();}
    FieldResponse { activated: response.clicked() && removed.is_none(), changed: removed.is_some(), response }
}
