# Local feature packages

Declarative feature packages are implemented. The editor and collaborating agents
use the same Rust registry to turn typed parameters into native document edits.
Packages do not create a second history or store authoritative geometry. FreeCAD
owns the resulting constraints, features, links, transactions and recomputation.
Saved FCStd documents retain native editable history without needing the package
that originally generated their edits.

Executable plugins, dynamically loaded libraries, custom UI panels and arbitrary
menu contributions are not implemented. The editor's Packages menu lists the
loaded declarative packages and opens its parameter form.

## Load a collection

The default registry bundles `sketch.rectangle`, `sketch.circle`, `feature.pad`,
`feature.pocket` and `feature.fillet`. To add a local package, copy the JSON files
from [packages](packages) into a local directory and add your manifest there.
An explicit directory replaces the bundled collection. Include the bundled IDs
to keep the editor's Rectangle, Circle, Extrude, Pocket and Fillet tools working.
The loader reads immediate `.json` files in sorted order, without recursion.

```sh
cad-next-packages --package-dir ./my-packages
cad-next-packages --package-dir ./my-packages --id local.extrude --parameters '{"name":"Pad","body":"Body","profile":"Sketch","length":20}'
cad-next-editor --runtime runtime.json --packages ./my-packages
```

The first command prints descriptors. The second prints an array of native edits;
it does not open a worker or change a document. Windows binaries end in `.exe`.

## Manifest version 1

Each file describes one package. Unknown fields, duplicate IDs and unsupported
versions fail loading. IDs contain dot-separated ASCII identifiers.

```json
{
  "version": 1,
  "id": "local.extrude",
  "label": "Extrude",
  "description": "Create a native pad from an existing sketch.",
  "parameters": {
    "name": {"type": "identifier"},
    "body": {"type": "identifier"},
    "profile": {"type": "identifier"},
    "length": {"type": "number", "exclusive_minimum": 0}
  },
  "edits": [
    {"op": "add_object", "type": "PartDesign::Pad", "name": {"$param": "name"}, "body": {"$param": "body"}},
    {"op": "set", "object": {"$param": "name"}, "property": "Profile", "value": {"link": {"$param": "profile"}}},
    {"op": "set", "object": {"$param": "name"}, "property": "Length", "value": {"$param": "length"}}
  ]
}
```

Supported parameter types are `identifier`, `edge`, `number`, `integer`, `string`
and `boolean`. An identifier matches native ASCII object names. An edge is a
positive native subelement name such as `Edge3`. Numbers must be finite; integers
must fit signed 64-bit values. Numeric types accept inclusive `minimum` and
`exclusive_minimum`. A `default` makes a parameter optional. All other parameters
are required. Unknown, missing or incorrectly typed inputs fail before submission
to the worker. Builtin numeric lengths use native millimeters.

Templates recursively substitute ordinary JSON values. They support three forms:

- `{"$param":"name"}` copies a declared parameter with its JSON type intact.
- `{"$param":"index","offset":1}` adds a checked signed integer offset to an
  integer parameter. Overflow fails.
- `{"$sum":[{"$param":"x"},{"$param":"width"}]}` adds exactly two numeric
  terms. Terms may be numeric literals, numeric parameters or nested sums. A
  non-finite result fails. Rectangle geometry uses sums to seed its exact bounds
  before native constraints solve.

There is no interpolation, executable expression language, import mechanism,
network dependency resolution or code evaluation. Unknown `$` operators fail.
Edit operations must come from the native protocol's supported edit list.
Loading a manifest does not prove its geometry is valid. The native worker still
checks object types, properties, references, constraints and the final model.

## Rust and agent use

`cad_next::packages::Registry` exposes:

```rust
Registry::builtin() -> Result<Registry, String>
Registry::load(path: &Path) -> Result<Registry, String>
registry.entries() -> &[FeaturePackage]
registry.get(id: &str) -> Option<&FeaturePackage>
registry.build(id: &str, parameters: &serde_json::Value) -> Result<Vec<Value>, String>
```

Sketch packages append geometry to an existing sketch. Supply `index` equal to
its current native geometry count, or the count after preceding edits in the
same transaction. Rectangle creates four lines and twelve constraints. Relative
constraint indices 8/9 set x/y and 10/11 set width/height. Circle creates one
circle and three constraints; relative index 2 sets radius. Add the sketch's
prior constraint count when editing these dimensions later. Body and sketch
creation, attachment and package composition remain ordinary native edits.

Agents can concatenate package output with other protocol edits, then submit one
`transaction` with a label and the latest `expected_revision` from the native
snapshot. Package compilation does not reserve names or revisions. A stale
revision rejects the write before mutation. Refresh with `inspect` after any
failed mutation before issuing another write, because native rollback can advance
the revision and change element aliases.

For a gesture, submit the complete compiled edit array through `preview` with a
unique gesture token and current revision. Subsequent candidates use the same
token and replace the original candidate. Commit with `preview_commit` or restore
the baseline with `preview_cancel`, using the latest revision and matching token.
An active preview holds the worker-wide write lease. Keep it short and cancel
when abandoning a tool. A first preview after Undo rejects with
`preview_redo_conflict` while native redo history remains available. Tokens
identify gesture ownership; they do not authenticate clients. See
[PROTOCOL.md](PROTOCOL.md) for failure, revision and history rules.

`tests/package_workflows.py` verifies package-generated native sketch, pad,
face-attached pocket and fillet history, dimension edits, failed edits, undo/redo,
and save/reopen editing. Rust tests cover parameter and manifest rejection plus
loading a local extension.
