//! Optional, read-only help. Native document and interaction ownership stay intact.
use eframe::egui;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Route {
    id: String,
    path: String,
}
#[derive(Deserialize)]
struct Step {
    instruction: String,
}
#[derive(Deserialize)]
struct Family {
    id: String,
    title: String,
    access: String,
    route_ids: Vec<String>,
    prerequisites: Vec<String>,
    selection: Vec<String>,
    inputs: Vec<String>,
    steps: Vec<Step>,
    missing_operation: String,
}
#[derive(Deserialize)]
struct Metadata {
    source_commit: String,
    audit_date: String,
}
#[derive(Deserialize)]
struct Guide {
    metadata: Metadata,
    routes: Vec<Route>,
    families: Vec<Family>,
}

fn data() -> &'static Guide {
    static GUIDE: OnceLock<Guide> = OnceLock::new();
    GUIDE.get_or_init(|| {
        serde_json::from_str(include_str!("../../coverage/workflows.json"))
            .expect("validated built-in workflow ledger")
    })
}
fn id() -> egui::Id {
    egui::Id::new("workflow_guide")
}
pub(super) fn active(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(id()).unwrap_or(false))
}
pub(super) fn open(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(id(), true));
}
pub(super) fn close(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(id(), false));
}
fn matches(family: &Family, query: &str) -> bool {
    query.is_empty()
        || family.title.to_lowercase().contains(query)
        || family.id.to_lowercase().contains(query)
        || family
            .inputs
            .iter()
            .any(|s| s.to_lowercase().contains(query))
}

fn instructions(ui: &mut egui::Ui, ctx: &egui::Context) {
    let pages = [
        (
            "Assembly",
            include_str!("../../ASSEMBLY.md"),
            Some("UI workflow"),
        ),
        (
            "Measure",
            include_str!("../../MEASUREMENTS.md"),
            Some("Use the editor"),
        ),
        (
            "Sketch attachment",
            include_str!("../../SKETCH-ATTACHMENT.md"),
            None,
        ),
        ("Select other", include_str!("../../SELECT-OTHER.md"), None),
        (
            "Fit selection",
            include_str!("../../FIT-SELECTION.md"),
            None,
        ),
    ];
    let mut page = ctx
        .data(|d| d.get_temp::<usize>(id().with("page")).unwrap_or(0))
        .min(pages.len() - 1);
    egui::ComboBox::from_id_salt("current_workflow")
        .selected_text(pages[page].0)
        .show_ui(ui, |ui| {
            for (index, (title, _, _)) in pages.iter().enumerate() {
                ui.selectable_value(&mut page, index, *title);
            }
        });
    let (_, document, section) = pages[page];
    let text = if let Some(section) = section {
        let marker = format!("## {section}");
        document
            .split_once(&marker)
            .expect("documented workflow")
            .1
            .split("\n## ")
            .next()
            .unwrap()
    } else {
        document
    };
    egui::ScrollArea::vertical()
        .id_salt(("current_instructions", page))
        .max_height((ctx.content_rect().height() - 220.).clamp(120., 650.))
        .show(ui, |ui| {
            for paragraph in text.replace("\r\n", "\n").split("\n\n") {
                let paragraph = paragraph
                    .trim()
                    .replace('\n', " ")
                    .replace("**", "")
                    .replace('`', "");
                if let Some(title) = paragraph.strip_prefix("# ") {
                    ui.strong(title);
                } else {
                    ui.label(paragraph);
                }
                ui.add_space(4.);
            }
        });
    ctx.data_mut(|d| d.insert_temp(id().with("page"), page));
}

pub(super) fn show(ctx: &egui::Context) {
    if !active(ctx) {
        return;
    }
    let mut query = ctx.data(|d| d.get_temp::<String>(id().with("query")).unwrap_or_default());
    let mut selected = ctx.data(|d| {
        d.get_temp::<String>(id().with("selected"))
            .unwrap_or_else(|| "document".into())
    });
    let mut done = false;
    let mut audit = ctx.data(|d| d.get_temp::<bool>(id().with("audit")).unwrap_or(false));
    egui::Modal::new(id()).show(ctx, |ui| {
        ui.set_width(680_f32.min((ctx.content_rect().width() - 60.).max(260.)));
        ui.heading("Workflow guide");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut audit, false, "Current instructions");
            ui.selectable_value(&mut audit, true, "Parity checklist");
        });
        if audit {
            let guide = data();
            ui.weak(format!(
                "Audit {} at {}. Full workflow acceptance remains open.",
                guide.metadata.audit_date,
                &guide.metadata.source_commit[..7]
            ));
            ui.horizontal(|ui| {
                ui.label("Find a workflow");
                ui.text_edit_singleline(&mut query);
            });
            let lower = query.to_lowercase();
            let choices: Vec<_> = guide
                .families
                .iter()
                .filter(|f| matches(f, &lower))
                .collect();
            if !choices.iter().any(|f| f.id == selected)
                && let Some(first) = choices.first()
            {
                selected = first.id.clone();
            }
            let family = choices.iter().copied().find(|f| f.id == selected);
            egui::ComboBox::from_id_salt("workflow_family")
                .selected_text(
                    family
                        .map(|f| f.title.as_str())
                        .unwrap_or("No matching workflow"),
                )
                .show_ui(ui, |ui| {
                    for choice in &choices {
                        ui.selectable_value(&mut selected, choice.id.clone(), &choice.title);
                    }
                });
            egui::ScrollArea::vertical()
                .id_salt("workflow_steps")
                .max_height((ctx.content_rect().height() - 250.).clamp(120., 650.))
                .show(ui, |ui| {
                    if let Some(family) = choices.iter().copied().find(|f| f.id == selected) {
                        if family.access == "missing" {
                            ui.colored_label(
                                egui::Color32::from_rgb(225, 142, 138),
                                "This workflow is not implemented.",
                            );
                        }
                        for (title, lines) in [
                            ("Before starting", &family.prerequisites),
                            ("Select", &family.selection),
                            ("Inputs", &family.inputs),
                        ] {
                            ui.strong(title);
                            for line in lines {
                                ui.label(line);
                            }
                        }
                        ui.strong("Entry points");
                        for route in &family.route_ids {
                            if let Some(route) = guide.routes.iter().find(|r| r.id == *route) {
                                ui.label(&route.path);
                            }
                        }
                        ui.strong("Steps");
                        for (i, step) in family.steps.iter().enumerate() {
                            ui.label(format!("{}. {}", i + 1, step.instruction));
                        }
                        ui.strong("Remaining functionality");
                        ui.label(&family.missing_operation);
                    } else {
                        ui.label("No matching workflow");
                    }
                });
        } else {
            instructions(ui, ctx);
        }
        done = ui.button("Close guide").clicked();
    });
    ctx.data_mut(|d| {
        d.insert_temp(id().with("query"), query);
        d.insert_temp(id().with("selected"), selected);
        d.insert_temp(id().with("audit"), audit);
    });
    if done {
        close(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtin_families_keep_routes_and_lifecycle_steps() {
        let guide = data();
        assert_eq!(guide.families.len(), 41);
        assert_eq!(guide.routes.len(), 144);
        for family in &guide.families {
            assert!(!family.steps.is_empty() && !family.missing_operation.is_empty());
            assert!(
                family
                    .route_ids
                    .iter()
                    .all(|id| guide.routes.iter().any(|r| r.id == *id))
            );
        }
    }
    #[test]
    fn guide_state_and_search_are_local_to_the_ui_context() {
        let a = egui::Context::default();
        let b = egui::Context::default();
        open(&a);
        assert!(active(&a) && !active(&b));
        close(&a);
        assert!(!active(&a));
        assert!(data().families.iter().any(|f| matches(f, "sketch")));
        assert!(
            !data()
                .families
                .iter()
                .any(|f| matches(f, "no-such-cad-workflow-123"))
        );
    }
}
