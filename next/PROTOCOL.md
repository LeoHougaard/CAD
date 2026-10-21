# Local modeling boundary, version 1

The Rust host starts a configured executable and argument list with the worker
script as an argument. Keep that process alive between requests. One UTF-8 JSON
object per line goes in and one response comes out. Diagnostics use stderr.

Requests contain `version: 1`, a caller-chosen integer `id`, `op`, and operation
parameters. Responses repeat version and id, contain `ok`, and contain either
`result` or `error: {code, message}`. Unknown versions and operations fail.

The initial operations are `capabilities`, `new`, `open`, `inspect`, `transaction`,
`preview`, `preview_commit`, `preview_cancel`, `undo`, `redo`, `save`, `mesh`,
`topology`, `sketch_begin`, `sketch_finish`, `sketch_cancel` and `close`.
`new` takes a document `name`; `open`
and `save` take a native file `path`; other document operations take `document`.
`mesh` and `topology` also take the result object's `object` name and optional
positive `deflection`, default 0.1 model units.

`transaction` takes `label`, `edits` and a required `expected_revision`. It
applies its edits, recomputes and validates solid results before committing.
Any failure aborts all its model edits. Revisions change on successful mutations,
Undo/Redo and failed transactions that entered the native edit/rollback cycle.
Native rollback can change element aliases and history even when geometry is
restored. After a failed mutation, call `inspect` to refresh the revision and
display/selection before another write. Preflight rejections, such as stale
revision or an empty edits list, leave the revision unchanged.
A stale revision must fail before mutating the document. This is a
single-worker revision, not a cross-process file concurrency claim.

`undo`, `redo`, `save` and `close` also require `expected_revision`. Missing tokens
return `missing_revision`; stale tokens return `stale_revision` before mutation.
Read requests preserve revisions. Save advances the revision. A transaction's
response is prepared before commit so serialization errors also abort the edit.

`preview` takes `document`, the current `expected_revision`, a caller-chosen
nonempty string `token` of at most 256 characters, `label` and nonempty `edits`.
It returns the same `{document, revision, edits, state}` structure as a
transaction. The first accepted preview opens a native transaction. Subsequent
calls with the same token replace the candidate from the original gesture
baseline, not from the previous candidate. Send the complete candidate edits on
every call. Every accepted candidate advances the revision. Invalid replacements
restore the last accepted candidate and retain its ownership. Native replay can
assign new element aliases, so a failed replacement also advances the revision.
After any failed preview, call `inspect` to obtain the current revision and
refresh display/selection before another write. A first failed preview restores
the baseline and acquires no ownership, but also advances its revision if it
entered the native edit/rollback cycle.

`preview_commit` and `preview_cancel` take `document`, the current
`expected_revision` and the same `token`. They return a document snapshot and
advance the revision. Commit ends the native transaction as one undo entry for
the complete gesture. Cancel aborts it and restores the baseline without adding
an undo entry. Calling either without an active preview returns `no_preview`.
Another token returns `preview_owner`. Stale revisions fail before any action.
The token identifies a gesture, rather than authenticating a client.

Snapshots, mesh and topology responses include `preview: {token, label}` while
a valid candidate is active and `preview: null` otherwise. Read results describe
that candidate with its revision. A client must discard display responses from
older revisions. Reads remain available throughout a gesture.

FreeCAD coordinates native transactions across documents. An active preview
therefore holds a worker-wide write lease. Ordinary transactions, Undo/Redo,
Save/Close, opening or creating another document and previews for another
document return `preview_active` until its owner commits or cancels. A poisoned
document can still be closed for recovery. Keep the lease short and cancel it
when abandoning a tool or document.

Native FreeCAD discards redo history when a transaction starts, including one
later canceled. To preserve history, a first preview with a nonempty redo stack
returns `preview_redo_conflict` before any change. Ordinary explicit transactions
can still branch history after Undo. A failed ordinary transaction also loses
the prior native redo stack, while restoring model edits and advancing revision.
Preserving native redo during a canceled
preview needs a later native adapter change; clients must show this limitation
instead of silently losing history.

## Bodies and native Boolean features

`PartDesign::Boolean` is a supported native object type. Create it with
`add_object` and the target `body`. The Body must already have a solid Tip.
FreeCAD assigns the previous native Tip as `BaseFeature` and advances the target
Tip to the Boolean. Set its `Group` to a nonempty list of native Body links and
its `Type` to `Fuse`, `Cut` or `Common`. `Refine`, `FuzzyTolerance` and `Suppressed`
remain native properties available through the ordinary property interface.
These edits compose in the same atomic transaction/preview boundary as other
features. Source Bodies, sketches, constraints and editable history remain in
the native document.

The adapter rejects self references, duplicate tools, empty tool lists,
non-Body tools, dependency cycles and a tool already consumed by another Boolean.
Changing an existing Boolean's operation and tool list uses the same property
edits. Failed candidates restore the previous native feature, Tip, ownership and
placements. A native empty intersection contains no solid and is rejected before
publishing it as a final result. Native compounds remain supported: Body
`AllowCompound` controls FreeCAD's single-solid rule. A disjoint union can produce
multiple valid solids when it is enabled; a dividing cut can do the same. A
disjoint cut can validly leave the target unchanged.
With multiple tools, native Common uses the combined tool regions and may retain
touching result partitions as distinct solids; do not substitute an independent
all-tools intersection algorithm for this native operation.

Every object snapshot includes `body`, its native owner Body name or null. A Body
owns itself. Body snapshots also include
`body_state: {tip, consumed_by, display_object}`. These fields come from native
Body ownership, Tip and the nonsuppressed Boolean dependencies of active Tips.
Consumed tools and empty Bodies remain editable objects. A consumed, hidden or
empty Body has null `display_object`; a displayed Body names its native Tip so
face references attach to the actual feature. Historical intermediate features
must not reappear as separate parts merely because they still have native Shapes.

`Group` preserves the pinned native property semantics. It does not compensate
Body placements when a tool becomes a child of the Boolean. With a moved or
rotated target this can change the tool's global placement and produce geometry
different from the original world-space overlap. The worker matches a separate
direct FreeCAD 1.1.4 reference for these cases, including compound unions and
empty intersections. World-preserving reparenting is a separate behavior change
and is not silently added to generic property writes. The pinned
[GUI Boolean command](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/PartDesign/Gui/Command.cpp)
and [Boolean task](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/PartDesign/Gui/TaskBooleanParameters.cpp)
use native grouping; the [native feature](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/PartDesign/App/FeatureBoolean.cpp)
owns recomputation and topology.

## Sketch editing sessions

`sketch_begin` takes `document`, `expected_revision`, native sketch `object` and
a nonempty session `token` of at most 256 characters. Its response is a snapshot
with `sketch_edit: {token, object}`. Beginning a session advances the revision but
opens no native transaction, so beginning and canceling without editing preserves
native redo. A session holds the same worker-wide write lease as a preview.
New/Open/Save/Close and edits to other objects fail with `sketch_active`.

During the session, `transaction`, `undo`, `redo`, `preview`, `preview_commit` and
`preview_cancel` require `sketch_token` matching its token. A wrong or missing
token returns `sketch_owner`. Preview `token` still identifies its separate drag
gesture. Sketch edits can change geometry, constraints, datums, construction,
constraint states, trimming, native properties and expressions on that sketch.
The native sketch solver and nonrecursive sketch recompute run after each action.
Downstream features are deliberately left pending: an open intermediate profile
is editable even when a Pad consumes it.

The first edit opens one native transaction for the complete session. A small
worker-owned command journal groups accepted sketch actions. Session Undo/Redo
abort and replay the selected action prefix against the same native document;
they do not alter a frontend geometry/history graph. This also avoids FreeCAD's
20-entry native undo limit discarding a long session's cancellation baseline.
Snapshot `undo_count` and `redo_count` describe session actions until it ends.
Drag previews replace a temporary final group; committing adds one action and
canceling restores the action prefix. Failed actions restore the accepted prefix
and advance revision before reporting the error. Inspect before another write.

`sketch_finish` and `sketch_cancel` take `document`, `expected_revision` and the
session `token`, and return snapshots. Finish recomputes and validates the whole
native document before committing the session as one ordinary native undo entry.
An invalid final profile restores the editable action prefix, leaves the session
active and returns `invalid_model`; the user can correct it or cancel. Cancel
aborts the native transaction and recomputes the original document without adding
history, including any active drag preview owned by that session. Finish requires
committing or canceling a pending drag first. A session with no edits creates no
undo entry. The first actual edit
branches native redo just like an ordinary transaction. Preexisting redo blocks
a native drag preview before that first edit; a UI can defer its committed edit
until release while keeping its pointer overlay separate from native geometry.

Snapshots include `sketch_edit: null` outside a session. Mesh/topology during a
session also include its metadata and `context_revision`. For the active sketch,
that is the current revision. For other objects it is the revision before the
session began: their cached solids are visual context, not regenerated results
or selectable final geometry. `revision` still identifies the current document
snapshot so clients can reject out-of-order responses. Whole-document finish
validation is required before publishing regenerated final solid geometry.

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
- `constraint_state`: `object`, constraint `index`, and at least one of
  `driving`, `active`, `virtual` as booleans or `name` as a string. Native Sketcher
  owns which constraint kinds can become reference dimensions. An empty name
  removes the name. State changes are atomic with the rest of the transaction.
- `trim_geometry`: `object`, geometry `index`, `position: [x,y,z]` in sketch
  coordinates. Native Sketcher trims the curve at the picked segment, preserves
  its supported constraints and renumbers geometry as needed. Read the solved
  geometry and constraints afterward rather than retaining old indices.

Each edit uses `op` as its discriminator. `geometry` also uses `kind`: `line`
with `start`/`end`, `circle` with `center`/`radius`, `arc` with `start`/`mid`/`end`,
or `point` with `position`.
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
per-face identities. `normals` has one finite unit vector per position. Normals
come from the oriented native face at each vertex's surface parameters, preserving
smooth curved surfaces and separate normals at face boundaries. A reversed face
therefore has its reversed normal, including the inward wall of a hole. It must
not replace native faces or flatten model history.

`topology` takes `document` and `object`. Faces include revision-scoped native
subelement names, mapped aliases where available, type, center, bounds, area and
oriented planar normal. Edges include names, type, length, endpoints, bounds and
face adjacency. Curved faces have null planar normals. Degenerate edges are
explicit. Each edge also includes display-only `points`: two exact endpoints for
a line, native curve discretization at the requested deflection otherwise, and
the available endpoints for a degenerate edge. Closed curves include their
intermediate points instead of only coincident endpoints. Native indices can
change after edits. Store references as native link
properties, not detached index strings in a second history graph.

Mesh and topology coordinates are the native Shape coordinates. Their
`world_from_shape` placement converts them into document world space. For a
feature inside a moved Body this includes the Body placement. The Body's own
Shape is already placed, so its additional transform is identity. Applying the
object placement again would double-transform the model.
Normals use the same native Shape coordinate frame. Apply only the rotation of
`world_from_shape` to normals, never its translation.

Implementation may refine parameter details while this first gate is developed;
changes must be reflected here and shared by the worker, host and workflow
checks. No independently invented GUI model is allowed.
