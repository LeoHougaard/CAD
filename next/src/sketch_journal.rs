//! Accepted sketch actions and replay order. No native document or geometry lives here.
//!
//! A staged action/history move is visible to snapshot counts and replay planning.
//! Native success commits it; native failure rejects it before restoration replay.
use serde_json::{Value, json};
use std::collections::HashMap;

pub(crate) const CHANNEL: &str = "cad-next-sketch-journal";

#[derive(Default)]
pub(crate) struct Journals(HashMap<String, Journal>);

#[derive(Default)]
struct Journal {
    groups: Vec<Value>,
    cursor: usize,
    pending: Option<Change>,
}

enum Change {
    Accept(Value),
    Move(usize),
}

impl Journal {
    fn cursor(&self) -> usize {
        match &self.pending {
            Some(Change::Accept(_)) => self.cursor + 1,
            Some(Change::Move(cursor)) => *cursor,
            None => self.cursor,
        }
    }

    fn counts(&self) -> Value {
        let cursor = self.cursor();
        let count = if matches!(self.pending, Some(Change::Accept(_))) {
            cursor
        } else {
            self.groups.len()
        };
        json!({"undo_count": cursor, "redo_count": count - cursor})
    }

    fn replay(&self) -> Value {
        let mut groups = self.groups[..if matches!(self.pending, Some(Change::Accept(_))) {
            self.cursor
        } else {
            self.cursor()
        }]
            .to_vec();
        if let Some(Change::Accept(group)) = &self.pending {
            groups.push(group.clone());
        }
        json!(groups)
    }
}

type Failure = (&'static str, &'static str);

impl Journals {
    /// Private version-1 worker callback contract. The host validates correlation.
    pub(crate) fn respond(&mut self, frame: &Value) -> Value {
        match self.apply(frame) {
            Ok(result) => json!({"ok": true, "result": result}),
            Err((code, message)) => {
                json!({"ok": false, "error": {"code": code, "message": message}})
            }
        }
    }

    fn apply(&mut self, frame: &Value) -> Result<Value, Failure> {
        let document = frame["document"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or((
                "journal_protocol",
                "Journal document must be a nonempty string",
            ))?;
        let action = frame["action"]
            .as_str()
            .ok_or(("journal_protocol", "Missing journal action"))?;
        if action == "begin" {
            if self.0.contains_key(document) {
                return Err(("journal_state", "Sketch journal is already active"));
            }
            self.0.insert(document.into(), Journal::default());
            return Ok(Value::Null);
        }
        if action == "end" {
            self.0.remove(document);
            return Ok(Value::Null);
        }
        let journal = self
            .0
            .get_mut(document)
            .ok_or(("journal_state", "Sketch journal is not active"))?;
        match action {
            "counts" => Ok(journal.counts()),
            "replay" => Ok(journal.replay()),
            "stage_accept" | "stage_undo" | "stage_redo" => {
                if journal.pending.is_some() {
                    return Err(("journal_state", "A sketch journal change is already staged"));
                }
                journal.pending = Some(match action {
                    "stage_accept" => {
                        let group = &frame["group"];
                        if !group.is_object() || group["edits"].as_array().is_none_or(Vec::is_empty)
                        {
                            return Err((
                                "journal_protocol",
                                "Accepted action requires nonempty edits",
                            ));
                        }
                        Change::Accept(group.clone())
                    }
                    "stage_undo" if journal.cursor == 0 => {
                        return Err(("nothing_to_undo", "No sketch action to undo"));
                    }
                    "stage_undo" => Change::Move(journal.cursor - 1),
                    "stage_redo" if journal.cursor == journal.groups.len() => {
                        return Err(("nothing_to_redo", "No sketch action to redo"));
                    }
                    _ => Change::Move(journal.cursor + 1),
                });
                Ok(Value::Null)
            }
            "resolve" => {
                let accepted = frame["accepted"]
                    .as_bool()
                    .ok_or(("journal_protocol", "Resolve requires a boolean outcome"))?;
                let pending = journal
                    .pending
                    .take()
                    .ok_or(("journal_state", "No sketch journal change is staged"))?;
                if accepted {
                    match pending {
                        Change::Accept(group) => {
                            journal.groups.truncate(journal.cursor);
                            journal.groups.push(group);
                            journal.cursor = journal.groups.len();
                        }
                        Change::Move(cursor) => journal.cursor = cursor,
                    }
                }
                Ok(Value::Null)
            }
            _ => Err(("journal_protocol", "Unknown sketch journal action")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(journals: &mut Journals, action: &str, extra: Value) -> Value {
        let mut frame = json!({"action": action, "document": "Doc"});
        frame
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        journals.respond(&frame)
    }

    fn accept(journals: &mut Journals, n: i32) {
        assert_eq!(
            call(journals, "stage_accept", json!({"group":{"edits":[n]}}))["ok"],
            true
        );
        assert_eq!(
            call(journals, "resolve", json!({"accepted":true}))["ok"],
            true
        );
    }

    #[test]
    fn branch_and_rejected_branch_preserve_the_right_replay() {
        let mut journals = Journals::default();
        call(&mut journals, "begin", json!({}));
        accept(&mut journals, 1);
        accept(&mut journals, 2);
        call(&mut journals, "stage_undo", json!({}));
        assert_eq!(
            call(&mut journals, "replay", json!({}))["result"],
            json!([{"edits":[1]}])
        );
        call(&mut journals, "resolve", json!({"accepted":true}));
        call(
            &mut journals,
            "stage_accept",
            json!({"group":{"edits":[3]}}),
        );
        assert_eq!(
            call(&mut journals, "counts", json!({}))["result"],
            json!({"undo_count":2,"redo_count":0})
        );
        call(&mut journals, "resolve", json!({"accepted":false}));
        assert_eq!(
            call(&mut journals, "counts", json!({}))["result"],
            json!({"undo_count":1,"redo_count":1})
        );
        accept(&mut journals, 3);
        assert_eq!(
            call(&mut journals, "replay", json!({}))["result"],
            json!([{"edits":[1]},{"edits":[3]}])
        );
        assert_eq!(
            call(&mut journals, "stage_redo", json!({}))["error"]["code"],
            "nothing_to_redo"
        );
    }

    #[test]
    fn rejected_history_move_and_lifecycle() {
        let mut journals = Journals::default();
        call(&mut journals, "begin", json!({}));
        assert_eq!(
            call(&mut journals, "stage_undo", json!({}))["error"]["code"],
            "nothing_to_undo"
        );
        accept(&mut journals, 1);
        call(&mut journals, "stage_undo", json!({}));
        assert_eq!(
            call(&mut journals, "stage_redo", json!({}))["error"]["code"],
            "journal_state"
        );
        call(&mut journals, "resolve", json!({"accepted":false}));
        assert_eq!(
            call(&mut journals, "counts", json!({}))["result"]["undo_count"],
            1
        );
        call(&mut journals, "end", json!({}));
        call(&mut journals, "begin", json!({}));
        assert_eq!(
            call(&mut journals, "replay", json!({}))["result"],
            json!([])
        );
    }
}
