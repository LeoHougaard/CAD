//! Native settings and quantity text. Rust never converts a physical value.
use serde_json::{Value, json};

pub fn settings(state: &Value, unit_system: u64) -> Result<Value, String> {
    let document = state["document"].as_str().ok_or("Open a native document")?;
    let revision = state["revision"]
        .as_u64()
        .ok_or("Refresh the native document")?;
    if state["available"] == false {
        return Err("Close and reopen the unavailable native document".into());
    }
    if !state["preview"].is_null() || !state["sketch_edit"].is_null() {
        return Err("Finish or cancel the active modeling task before changing units".into());
    }
    if !state["units"]["choices"].as_array().is_some_and(|choices| {
        choices
            .iter()
            .any(|c| c["index"].as_u64() == Some(unit_system))
    }) {
        return Err("Choose a native document unit system".into());
    }
    Ok(
        json!({"op":"document_settings","document":document,"expected_revision":revision,"unit_system":unit_system}),
    )
}

pub fn input(text: &str, metadata: &Value) -> Value {
    json!({"quantity_input":text,"default_unit":metadata["default_unit"].as_str().unwrap_or("")})
}

pub fn scalar(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
        || value.as_str().is_some_and(|s| !s.trim().is_empty())
        || value["quantity_input"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty())
}
