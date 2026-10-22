//! Local NDJSON sketch command compiler. It does not open or mutate documents.

use cad_next::sketch_commands::{SketchElement, Tool, constrain, draw_edits, draw_snapped_edits};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Draw {
        object: String,
        tool: Tool,
        points: Vec<[f64; 3]>,
        #[serde(default)]
        index: usize,
        #[serde(default)]
        construction: bool,
        #[serde(default)]
        snaps: Vec<Option<SketchElement>>,
    },
    Edit {
        snapshot: Value,
        kind: String,
        selection: Vec<SketchElement>,
        parameters: Value,
    },
    Datum {
        snapshot: Value,
        index: usize,
        value: String,
        #[serde(default)]
        expression: bool,
    },
    Constraint {
        snapshot: Value,
        kind: String,
        selection: Vec<SketchElement>,
        #[serde(default)]
        value: Option<String>,
    },
}

fn compile(line: &str) -> Result<Vec<Value>, String> {
    match serde_json::from_str::<Request>(line).map_err(|error| error.to_string())? {
        Request::Draw {
            object,
            tool,
            points,
            index,
            construction,
            snaps,
        } => {
            if snaps.is_empty() {
                draw_edits(&object, tool, &points, index, construction)
            } else {
                draw_snapped_edits(&object, tool, &points, index, construction, &snaps)
            }
        }
        Request::Edit {
            snapshot,
            kind,
            selection,
            parameters,
        } => cad_next::sketch_operation_commands::build(&snapshot, &kind, &selection, &parameters),
        Request::Datum {
            snapshot,
            index,
            value,
            expression,
        } => cad_next::sketch_commands::datum_edits(&snapshot, index, &value, expression),
        Request::Constraint {
            snapshot,
            kind,
            selection,
            value,
        } => constrain(&snapshot, &kind, &selection, value.as_deref()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(argument) = std::env::args().nth(1) {
        if argument == "--help" || argument == "-h" {
            println!(
                "cad-next-sketch\nCompile one local JSON request per stdin line into native sketch edits.\nDraw: {{\"op\":\"draw\",\"object\":\"Sketch\",\"tool\":\"line\",\"points\":[[0,0,0],[10,0,0]],\"index\":0,\"construction\":false}}\nConstraint: {{\"op\":\"constraint\",\"snapshot\":<native sketch object>,\"kind\":\"Distance\",\"selection\":[{{\"index\":0,\"point\":0}}],\"value\":\"10 mm\"}}\nEdit: {{\"op\":\"edit\",\"snapshot\":<native sketch object>,\"kind\":\"extend\",\"selection\":[{{\"index\":0,\"point\":2}}],\"parameters\":{{\"increment\":\"5 mm\"}}}}\nDraw may include snaps:[null,{{\"index\":0,\"point\":1}},null] paired with the input points.\nOutput: {{\"ok\":true,\"edits\":[...]}} or {{\"ok\":false,\"error\":{{\"code\":\"invalid_sketch_command\",\"message\":\"...\"}}}}\nDatum: {{\"op\":\"datum\",\"snapshot\":<native sketch object>,\"index\":0,\"value\":\"1.5\",\"expression\":false}}. Refraction creation uses kind SnellsLaw and two selected line endpoints in n1, n2 order plus one whole boundary.\nNo document is opened or changed. Submit returned edits with a native revision guard."
            );
            return Ok(());
        }
        return Err("Expected NDJSON on stdin; run --help for examples".into());
    }
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let response = match compile(&line?) {
            Ok(edits) => json!({"ok":true,"edits":edits}),
            Err(message) => {
                json!({"ok":false,"error":{"code":"invalid_sketch_command","message":message}})
            }
        };
        serde_json::to_writer(&mut output, &response)?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}
