# Local modeling boundary, version 1

The Rust host starts a configured executable and argument list with the worker
script as an argument. Keep that process alive between requests. One UTF-8 JSON
object per line goes in and one response comes out. Diagnostics use stderr.

Requests contain `version: 1`, a caller-chosen integer `id`, `op`, and operation
parameters. Responses repeat version and id, contain `ok`, and contain either
`result` or `error: {code, message}`. Unknown versions and operations fail.

The initial operations are `capabilities`, `new`, `open`, `inspect`, `transaction`,
`undo`, `redo`, `save`, `mesh`, `topology`, and `close`. `new` takes a document `name`; `open`
and `save` take a native file `path`; other document operations take `document`.
`mesh` also takes the result object's `object` name and optional `deflection`.

`transaction` takes `label`, `edits` and a required `expected_revision`. It
applies its edits, recomputes and validates solid results before committing.
Any failure aborts all its edits. Revisions must change on successful mutations
and Undo/Redo. A stale revision must fail before mutating the document. This is a
single-worker revision, not a cross-process file concurrency claim.

`undo`, `redo`, `save` and `close` also require `expected_revision`. Missing tokens
return `missing_revision`; stale tokens return `stale_revision` before mutation.
Read requests preserve revisions. Save advances the revision. A transaction's
response is prepared before commit so serialization errors also abort the edit.

Initial edits are native object operations:

- `add_object`: `type`, `name`; optional `body` for a PartDesign Body owner.
- `set`: `object`, `property`, `value`.
- `expression`: `object`, `property`, `expression`.
- `geometry`: `object`, `kind`, and geometric parameters; returns native index.
- `constraint`: `object`, `kind`, `args` using native Sketcher conventions.
- `datum`: `object`, zero-based constraint `index`, `value` as a native numeric
  value or explicit unit string. Explicit units must match the dimension type.
- `move_geometry`: `object`, geometry `index`, native `point` index 0..3,
  `position: [x,y,z]`, optional `relative: false`.
- `construction`: `object`, geometry `index`, optional boolean `construction`.
  Omitting the boolean toggles the current state.
- `delete_geometry` and `delete_constraint`: `object`, zero-based `index`.

Each edit uses `op` as its discriminator. `geometry` also uses `kind`: `line`
with `start`/`end`, `circle` with `center`/`radius`, or `arc` with `start`/`mid`/`end`.
All points are three-number lists in native sketch coordinates. Geometry edits
accept `construction: false` by default. Constraints use native conventions,
including negative indices for external geometry or the sketch origin.

Use ordinary JSON for scalar/list properties. Represent links as
`{"link":"ObjectName"}` and subelement links as
`{"link":"ObjectName","subelements":["Face6"]}`. Native attachment and element
mapping own these references. Vectors are three-number lists when the receiving
native property requires a vector. Support geometry initially includes line
segments, circles and arcs, and construction flags. Do not add a Python eval or
exec operation to the public protocol.

Placements use `{"position":[x,y,z],"rotation":[qx,qy,qz,qw]}`. Expressions use
native paths, including `Constraints[0]` and `Placement.Base.x`. Dimensionless
numeric datums use FreeCAD's native conventions; prefer explicit unit strings.

`inspect` includes document name and revision, object names/types, errors,
native solid measures and validity, and sketch placement/remaining freedom when
available. `capabilities` identifies FreeCAD, OCCT and protocol versions and the
implemented operation set. Unsupported functionality must fail explicitly.

Sketch snapshots include solved `geometry` and native `constraints`, not only
counts. Primitive geometry includes its index, type, construction state and
coordinates. Constraint entries include native type, entity/point references,
value, name, driving/virtual/active flags. A constrained drag can leave geometry
unchanged. The solved snapshot is authoritative, not the requested target.

`mesh` is for display only and returns positions and triangle indices with
per-face identities. It must not replace native faces or flatten model history.

`topology` takes `document` and `object`. Faces include revision-scoped native
subelement names, mapped aliases where available, type, center, bounds, area and
oriented planar normal. Edges include names, type, length, endpoints, bounds and
face adjacency. Curved faces have null planar normals. Degenerate edges are
explicit. Native indices can change after edits. Store references as native link
properties, not detached index strings in a second history graph.

Mesh and topology coordinates are the native Shape coordinates. Their
`world_from_shape` placement converts them into document world space. For a
feature inside a moved Body this includes the Body placement. The Body's own
Shape is already placed, so its additional transform is identity. Applying the
object placement again would double-transform the model.

Implementation may refine parameter details while this first gate is developed;
changes must be reflected here and shared by the worker, host and workflow
checks. No independently invented GUI model is allowed.
