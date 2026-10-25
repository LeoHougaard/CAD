//! Read-only requests shared by the editor and callers of WorkerSession.
use crate::measurement_types::Reference;
use serde_json::{Value, json};

pub fn request(state: &Value, selections: &[Reference]) -> Result<Value, String> {
    if !state["document"].is_string() || !state["revision"].is_u64() {
        return Err("Open a native document to measure".into());
    }
    if !(1..=2).contains(&selections.len()) {
        return Err("Select one or two native vertices, edges, faces or objects".into());
    }
    if selections.len() == 2 && selections[0] == selections[1] {
        return Err("Select two distinct native references".into());
    }
    Ok(
        json!({"document":state["document"],"expected_revision":state["revision"],"selections":selections}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measurements_require_a_snapshot_and_one_or_two_distinct_references() {
        let r = Reference {
            object: "Box".into(),
            subelement: "Face1".into(),
            sketch_geometry: None,
        };
        let state = json!({"document":"D","revision":17});
        assert!(request(&Value::Null, std::slice::from_ref(&r)).is_err());
        assert!(request(&state, &[]).is_err());
        assert!(request(&state, &[r.clone(), r.clone()]).is_err());
        let query = request(&state, std::slice::from_ref(&r)).unwrap();
        assert_eq!(query["expected_revision"], 17);
        assert_eq!(query["selections"][0]["subelement"], "Face1");
        assert!(query.get("edits").is_none());
    }
}
