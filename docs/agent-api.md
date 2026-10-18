# Agent API

`cad-agent` runs locally without a window, service, account, or network connection. It reads one JSON request from stdin, waits for EOF, writes one JSON response to stdout, and exits. Exit status is zero for a successful request and nonzero for a rejected request. Both requests and responses have `version: 1`. Input is limited to 32 MiB and an apply request can contain at most 4000 edits.

Build the headless CLI with `cargo build --release --no-default-features --features step --bin cad-agent`. Run it with `cad-agent < request.json` in a shell that supports input redirection. Python and other agent tools can send UTF-8 JSON through a subprocess's stdin. Close stdin after sending the request.

Every request has `version`, `path`, and `action`. An optional `plugins` directory loads the same local packages and disabled-package list as the desktop app. Without it, the CLI uses its bundled packages. Feature package hashes and versions remain pinned.

## Inspect

```json
{"version":1,"path":"part.cadpart","action":{"op":"inspect"}}
```

The response contains `result.document`, `result.revision`, `result.plugins`, and `result.model`. Plugin schemas list field keys, kinds, defaults, and choices. The model lists solved sketch entities, degrees of freedom, feature statuses, part volumes in mm³, display-mesh bounds in mm, face keys and planes, and edge keys. Inspect can return an invalid model so an agent can diagnose it. Check `result.validation_error` and `result.model.error` before relying on geometry.

Each resolved sketch also lists `regions`, with stable `key` identities, display-only `outer` and `holes` contours, and `whole_sketch` indicating the legacy whole-sketch selection. Set Extrude's `regions` parameter to an array of these keys to choose enclosed cells. `null` means whole sketch; `[]` is an explicit empty selection and is rejected. Region changes are validated atomically like other parameter edits. See [sketch regions](sketch-regions.md) for supported intersections and identity changes.

Revisions are `fnv1a64:` followed by a stable hash of every file byte, including whitespace. They detect changes and are not security signatures. A preview's returned revision identifies the bytes it would save.

## Edit and regenerate

```json
{
  "version": 1,
  "path": "part.cadpart",
  "action": {
    "op": "apply",
    "expected_revision": "fnv1a64:replace_with_revision_from_inspect",
    "output": "part.cadpart",
    "edits": [
      {"op":"set_param","feature":"f2","key":"depth","before":"10","value":"12"},
      {"op":"set_variable","name":"width","before":"40","value":"50"}
    ]
  }
}
```

Omit `output` for a validated preview that returns the edited document and regenerated model without saving. Set a new output path for a separate design. An existing output is rejected unless it is the source document, and replacing the source requires `expected_revision`.

Supported edits are:

| Operation | Target and value |
| --- | --- |
| `set_param` | `feature` ID, declared parameter `key`, JSON `value` |
| `set_variable` | Variable `name`, expression string `value`; `before: null` guards creation of a new variable |
| `set_dimension` | Sketch `feature` ID, constraint array `index`, expression string `value`; driving dimensions only |
| `set_entity_values` | Sketch `feature` ID, `entity` ID, expression-string array `values`; preserves entity identity and type |
| `add_feature` | Installed feature `plugin` ID, optional `name`, and `params` object; assigns a new ID and pins its package |

Parameters use their plugin's declared types. Numeric values can be numbers or expression strings. Dimensions, variables, and sketch entity values use expression strings. `set_param` cannot replace sketch geometry or feature metadata. Add a sketch through `add_feature` with a complete serialized `Sketch` in `params.geometry`. It contains `entities`, `constraints`, and `next_id`. Entity and constraint structures match `result.document` from inspect. `add_feature` also accepts its normal plane, offset, and support parameters.

Sketch entities may contain optional `curve_identity` metadata. Preserve it when editing existing entities: it keeps references to the same logical curve stable when a rectangle is decomposed into connected lines for per-edge construction. New entities normally omit it; arbitrary or duplicate aliases are rejected. It does not identify a replacement curve after an actual trim or split.

New Part Studio documents use `format: 2` for the shared [numeric expression language](numeric-expressions.md). Older format 1 documents retain their original radians semantics. Use the document's format when interpreting expressions. Construction-plane `plane_ref` fields use typed origin, datum or face references; see [construction planes](construction-planes.md).

Fields with kind `bodies` take distinct persistent part IDs, not `@last`.
Extrude and Revolve use an explicit `targets` scope for Add, Remove and Intersect;
an empty scope is an error. `profile_face` is null or `{"part":"f2:0","face":"f2:end"}`.
Revolve's `axis_ref` is null for its legacy sketch axis, or an origin, sketch entity
or solid edge reference. Inspect the installed field schema and see
[geometry controls](geometry-controls.md) for the exact shapes and compatibility.
The agent validator checks these structures before regeneration and keeps the
source file unchanged on failure.

Fields with kind `sketches`, such as a loft's `profiles`, take an ordered array of up to 32 distinct earlier, unsuppressed sketch IDs. Change the array with `set_param`, including its previous array as the `before` guard. The feature then validates its required profile count and supported geometry.

For example, change an existing width dimension without changing its references:

```json
{
  "op": "set_dimension",
  "feature": "f1",
  "index": 1,
  "before": "40",
  "value": "55",
  "binding": {"kind":"Width","a":1,"b":0,"points":[0,0],"reference":false,"negative":false}
}
```

Copy `binding` from that constraint in the inspected document. Its `value`, label position, and other display fields do not affect binding matching. A stale dimension edit needs this binding because indices can change when a person adds or removes constraints. Stale `set_entity_values` edits need `kind`, such as `"rectangle"`, to guard the entity type.

All edits form one transaction. The CLI validates package pins, parameter types, expressions, and sketches, regenerates the active part studio history, and saves atomically only after success. Failure preserves the previous document. Direct edits to suppressed or rolled-back features return `inactive_feature`. Adding features requires the history at End, with `rollback: null`. Variable edits regenerate the current active history; later unsuppressing features or moving past rollback can still expose geometry errors. Create and inspect also report the current active history, without guaranteeing every inactive future history.

Edits cannot change existing feature IDs, package IDs, versions, hashes, entity IDs, or constraint kinds and references. This first API does not delete or reorder features, remove variables, or insert and remove constraints on existing sketches.

## Multiple agents

Agents can work on separate output files or the same design. For shared edits, each agent should inspect the document, keep its revision, and include the inspected `before` value for every edit. Source writes use an operating-system lock on an adjacent `.part.cadpart.lock` file. The desktop app uses the same lock. The lock file stays present; the operating system releases its lock when the process exits or crashes. The CLI waits up to two seconds for another writer, then returns `document_busy`.

If the revision changed, an edit can rebase when its current target still equals `before` or already equals the proposed value. Disjoint edits merge. An already-applied edit is idempotent. A changed target returns `edit_conflict` with target, current, expected, and proposed values. A missing `before` on a stale request also conflicts. A stale `add_feature` needs `before` equal to the inspected number of features. Concurrent appends conflict rather than guessing order.

Always inspect the returned document and model after a rebase. If two edits touch different fields but produce incompatible geometry, regeneration fails and the file stays unchanged. Locks coordinate this app and CLI. Other software that ignores them can still race a write; the CLI also checks the source bytes immediately before saving.

Native Pixel Linux verification covered two CLI processes editing one document from the same starting revision. Their disjoint sketch/depth edits merged into a fully constrained 40 x 40 x 5mm part, with one request rebased. The GUI reloaded that result, rejected a stale save after an agent changed the file, and retained the unsaved human edit when "Keep editing" was chosen. Native Save as preserved the human's 6mm-depth fork while the shared file retained the agent's 7mm depth. See the [verification report](../output/collaboration-pixel-verification.json), [stale-save screenshot](../output/pixel-stale-save-detail.png), and [reload guard](../output/pixel-reload-guard-detail.png). This verifies the recorded workflow; the GUI rejects stale saves rather than merging unsaved edits automatically.

## Create and export

Create an empty design, then add features with guarded apply requests:

```json
{
  "version": 1,
  "path": "new-part.cadpart",
  "action": {
    "op": "create",
    "document": {"format":2,"name":"New part","variables":{},"features":[],"rollback":null,"next_id":1}
  }
}
```

`create` also accepts a complete design document. It validates and regenerates it, and requires a new destination file.

```json
{"version":1,"path":"part.cadpart","action":{"op":"export","format":"step","output":"part.step","expected_revision":"fnv1a64:replace_with_revision_from_inspect"}}
```

Export supports `step` and `stl`, requires valid regenerated geometry, and writes a new file. `expected_revision` is optional for export; if supplied it must match exactly. STEP requires a build with the `step` feature.

## Responses

```json
{"version":1,"ok":true,"result":{"revision":"fnv1a64:...","rebased":false,"saved_path":"...","document":{},"model":{}}}
```

Each action returns only its applicable result fields. Failures use the same envelope and exit nonzero:

```json
{"version":1,"ok":false,"error":{"code":"edit_conflict","message":"...","details":{"conflicts":[{"target":"features/f2/params/depth","current":"15","expected":"10","proposed":"12"}]}}}
```

Other error codes include `invalid_request`, `validation_failed`, `regeneration_failed`, `inactive_feature`, `stale_revision`, `output_exists`, `document_busy`, `io_error`, and `internal_error`. A regeneration failure includes feature statuses when the engine produced them.
