//! Native settings and quantity text. Parsed and stored physical values stay native.
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

/// Read-only decimal sketch feedback using the current native presentation scale.
/// This text is never submitted as a dimension, edit seed, or geometry value.
pub fn preview_length(state: &Value, millimeters: f64) -> Option<String> {
    let format = &state["units"]["preview_length"];
    let scale = format["scale"].as_f64().filter(|v| v.is_finite() && *v > 0.)?;
    let unit = format["unit"].as_str().filter(|v| {
        !v.is_empty() && v.len() <= 32 && v.trim() == *v && !v.chars().any(char::is_control)
    })?;
    let decimals = format["decimals"].as_u64().filter(|v| *v <= 6)? as usize;
    let value = millimeters * scale;
    (millimeters.is_finite() && millimeters >= 0. && value.is_finite())
        .then(|| format!("{value:.decimals$} {unit}"))
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    #[test]
    fn feedback_uses_native_scale_without_changing_the_quantity_or_state() {
        let mut state = json!({"units":{"preview_length":{"scale":0.5,"unit":"native-unit","decimals":3}}});
        let before = state.clone();
        assert_eq!(preview_length(&state, 5.054).as_deref(), Some("2.527 native-unit"));
        assert_eq!(state, before);
        state["units"]["preview_length"] = json!({"scale":1.,"unit":"mm","decimals":3});
        assert_eq!(preview_length(&state, 5.054).as_deref(), Some("5.054 mm"));
    }

    #[test]
    fn missing_invalid_or_overflowing_native_feedback_is_not_guessed() {
        assert!(preview_length(&Value::Null, 25.4).is_none());
        for value in [json!(0), json!(-1), json!("0.5")] {
            let state = json!({"units":{"preview_length":{"scale":value,"unit":"in","decimals":3}}});
            assert!(preview_length(&state, 25.4).is_none());
        }
        let state = json!({"units":{"preview_length":{"scale":f64::MAX,"unit":"in","decimals":3}}});
        assert!(preview_length(&state, 2.).is_none());
        assert!(preview_length(&state, f64::NAN).is_none());
        assert!(preview_length(&state, -1.).is_none());
    }
}

/// Dimension presentation only. Missing compact metadata preserves native legacy captions.
pub fn dimension_display(quantity: &Value) -> Option<&str> {
    quantity["edit_display"].as_str().or_else(|| quantity["display"].as_str())
}
