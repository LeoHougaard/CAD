//! Worker-local revision state only. Native documents and mutation timing stay native.
use serde_json::{Value, json};
use std::collections::HashMap;

pub(crate) const CHANNEL: &str = "cad-next-revisions";

#[derive(Default)]
pub(crate) struct Revisions {
    clock: u64,
    documents: HashMap<String, u64>,
}

type Failure = (&'static str, &'static str);

impl Revisions {
    pub(crate) fn respond(&mut self, frame: &Value) -> Value {
        match self.apply(frame) {
            Ok(result) => json!({"ok":true,"result":result}),
            Err((code, message)) => json!({"ok":false,"error":{"code":code,"message":message}}),
        }
    }

    fn apply(&mut self, frame: &Value) -> Result<Value, Failure> {
        let action = frame["action"]
            .as_str()
            .ok_or(("revision_protocol", "Missing revision action"))?;
        let document = frame["document"].as_str();
        if action == "lookup" {
            return Ok(document
                .and_then(|name| self.documents.get(name))
                .map_or(Value::Null, |v| json!(v)));
        }
        if action == "check" {
            let current = document
                .and_then(|name| self.documents.get(name))
                .ok_or(("not_found", "Document is not open in this worker"))?;
            let expected = &frame["expected_revision"];
            if expected.is_null() {
                return Err((
                    "missing_revision",
                    "Document mutations require expected_revision",
                ));
            }
            // bools and floating-point JSON numbers are not integer revision tokens.
            if expected.as_u64() != Some(*current) {
                return Err(("stale_revision", "Document revision changed"));
            }
            return Ok(json!(current));
        }
        let document = document.filter(|name| !name.is_empty()).ok_or((
            "revision_protocol",
            "Revision document must be a nonempty string",
        ))?;
        match action {
            // Unpublished new/open snapshots use zero and acquire no registry entry.
            "peek" => Ok(json!(self.documents.get(document).copied().unwrap_or(0))),
            "current" => self
                .documents
                .get(document)
                .map(|v| json!(v))
                .ok_or(("revision_state", "Published document has no revision")),
            // Native acceptance or rollback decides when to call this. First publication
            // registers a document; later publications advance the same global clock.
            "bump" => {
                let next = self
                    .clock
                    .checked_add(1)
                    .ok_or(("revision_state", "Worker revision clock exhausted"))?;
                self.documents.insert(document.into(), next);
                self.clock = next;
                Ok(json!(next))
            }
            "close" => {
                self.documents
                    .remove(document)
                    .ok_or(("revision_state", "Closed document has no revision"))?;
                Ok(Value::Null)
            }
            _ => Err(("revision_protocol", "Unknown revision action")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(store: &mut Revisions, action: &str, doc: &str, expected: Value) -> Value {
        store.respond(&json!({"action":action,"document":doc,"expected_revision":expected}))
    }

    #[test]
    fn publication_close_and_name_reuse_never_reuse_a_revision() {
        let mut store = Revisions::default();
        assert_eq!(call(&mut store, "peek", "A", Value::Null)["result"], 0);
        assert_eq!(
            call(&mut store, "lookup", "A", Value::Null)["result"],
            Value::Null
        );
        for (doc, revision) in [("A", 1), ("B", 2), ("A", 3)] {
            assert_eq!(
                call(&mut store, "bump", doc, Value::Null)["result"],
                revision
            );
        }
        assert_eq!(call(&mut store, "current", "B", Value::Null)["result"], 2);
        assert_eq!(call(&mut store, "close", "A", Value::Null)["ok"], true);
        assert_eq!(
            call(&mut store, "lookup", "A", Value::Null)["result"],
            Value::Null
        );
        assert_eq!(call(&mut store, "bump", "A", Value::Null)["result"], 4);
        assert_eq!(
            call(&mut store, "check", "A", json!(3))["error"]["code"],
            "stale_revision"
        );
        call(&mut store, "close", "A", Value::Null);
        call(&mut store, "close", "B", Value::Null);
        assert_eq!(call(&mut store, "bump", "A", Value::Null)["result"], 5);
    }

    #[test]
    fn checks_match_integer_token_semantics_without_changing_state() {
        let mut store = Revisions::default();
        assert_eq!(
            call(&mut store, "check", "A", Value::Null)["error"]["code"],
            "not_found"
        );
        call(&mut store, "bump", "A", Value::Null);
        assert_eq!(
            call(&mut store, "check", "A", Value::Null)["error"]["code"],
            "missing_revision"
        );
        for token in [
            json!(true),
            json!(false),
            json!(1.0),
            json!(-1),
            json!(0),
            json!(2),
            json!("1"),
            json!([]),
            json!({}),
        ] {
            assert_eq!(
                call(&mut store, "check", "A", token)["error"]["code"],
                "stale_revision"
            );
        }
        assert_eq!(call(&mut store, "check", "A", json!(1))["result"], 1);
        assert_eq!(call(&mut store, "current", "A", Value::Null)["result"], 1);
        assert_eq!(call(&mut store, "bump", "B", Value::Null)["result"], 2);
    }

    #[test]
    fn exhausted_clock_does_not_register_or_change_a_document() {
        let mut store = Revisions {
            clock: u64::MAX,
            documents: HashMap::from([("A".into(), 17)]),
        };
        for doc in ["A", "B"] {
            assert_eq!(
                call(&mut store, "bump", doc, Value::Null)["error"]["code"],
                "revision_state"
            );
        }
        assert_eq!(call(&mut store, "current", "A", Value::Null)["result"], 17);
        assert_eq!(
            call(&mut store, "lookup", "B", Value::Null)["result"],
            Value::Null
        );
    }
}
