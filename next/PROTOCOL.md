# Local modeling boundary, version 1

Hole task metadata is an additive version-1 snapshot extension. For
`PartDesign::Hole`, `property_metadata.ThreadType` includes
`hole_task_version:1`, `hole_task_enumerations` and `hole_task_cut_angles`.
The first map is keyed by native ThreadType and supplies exact native
ThreadSize, ThreadClass, ThreadFit and HoleCutType enum lists. The second maps
native ThreadType and HoleCutType to custom countersink-angle input availability.
The worker collects these JSON facts during its existing startup schema pass,
before accepting requests. It retains no schema document. No task request creates
a native document, feature or transaction to discover choices.

Rust applies this metadata only to a Hole schema with the matching task version.
Its bounded task presentation keeps conditional input fields reachable after a
failed preview and during deferred Redo editing. It does not change native write
admission. Computed Shape and ThreadDiameter, standard-derived Diameter and
preset dimensions retain the ordinary native protection. All model values,
presets, quantities and expressions remain native. Hole controller expressions
are ordered with literal controllers before dependent values; the client never
evaluates an expression. Editor candidates omit stale literal overrides for
explicitly inactive Hole task inputs, such as Depth after selecting ThroughAll.
An enabled Hole input widget records bounded literal provenance in the private
draft parameter `hole_task_literals: {version:1, fields:{property:true}}`.
Only the named conditional TaskHole input whitelist participates. This lets a
controller round trip omit a formerly enabled literal even when that input was
inactive in the initial native snapshot. Dormant values and unit-text buffers
remain in the draft for switching back; they are excluded only from the current
candidate's literal writes. This marker is not native document data or a worker
protocol field and grants no write permission. Unknown marker versions have no
effect. Shape, ThreadDiameter and unlisted fields cannot acquire provenance.
Expressions and unlisted computed properties still pass through native write
admission unchanged. After a Hole controller expression is bound or cleared,
the adapter asks that same native Hole to recompute inside the existing
transaction. This evaluates native expressions and updates native enum domains
and editor locks before dependent setters. Interim invalid feature results are
not published; the final whole-document recompute, validation and ordinary
rollback still decide acceptance. No additional document or transaction is
created. The authoritative snapshot, revision, native document,
preview lease and failure rollback contract are unchanged. Unknown task versions
use the original native form metadata.

Guarded Open also preserves the serialized Hole inputs that pinned FreeCAD 1.1.4
can normalize incorrectly while restoring: ThreadSize and custom cut values.
The adapter reads their native XML scalar/enum representations without changing
the archive, then reapplies changed user inputs through native setters. Quantity
floats retain native serialization's internal units; this path does not parse
user unit strings. Property types, native enum membership, finite values and
native readonly state are checked. Shape, computed ThreadDiameter, expression
bindings, object links and property status flags are never assigned by this
repair. After all Hole inputs have been restored, every restored Hole is
explicitly touched through native invalidation, including those whose saved
inputs already match. The existing whole-document native recompute then orders
dependencies and regenerates all affected geometry before validation and
publication. Thus a stale cached BRep is not accepted as evidence of valid Hole
inputs: an invalid depth fails Open, and a valid changed depth regenerates its
Hole and downstream features. This intentionally tightens Hole Open admission;
it does not alter serialization or readonly policy. Exceptions remain inside the existing
failed-Open cleanup guard, which closes newly created documents and preserves
the previous document. No extra document, transaction, revision or history entry
is created by the repair.

When a missing native preset entry forces HoleCutCustomValues true and readonly,
the form retains that computed protection. A complete candidate's identical
literal true is accepted as an exact no-op; false and expression changes still
use normal native readonly admission. No property status is changed.

Editing ModelThread follows pinned TaskHoleParameters::holeTypeChanged by
refreshing the current Threaded value through its native setter first. This
updates a stale restored editor flag through native callbacks. Native readonly
admission then runs normally; no readonly/status flag is relaxed directly.

The Rust host starts a configured executable and argument list with the worker
script as an argument. Keep that process alive between requests. One UTF-8 JSON
object per line goes in and one response comes out. Diagnostics use stderr.
The native request stream explicitly uses UTF-8 independently of the operating
system locale, including portable Windows Python and Unicode file paths.

The native worker can exchange private `cad-next-sketch-journal`,
`cad-next-revisions` and `cad-next-leases` callback frames with `WorkerSession`
while a public request is pending. Rust owns accepted sketch actions, opaque native
checkpoints and their history cursor, the worker-local
revision registry, missing/stale revision checks and worker-wide gesture ownership.
Rust also owns preview/sketch tokens, active-sketch edit scope, staged preview metadata, native-redo
admission and poison state. Python supplies native transaction/redo facts and
retains the active preview's native checkpoint without an authorization token store. Python chooses when native
acceptance or recovery requires publication. The native worker owns document mutations. These
frames never reach public NDJSON clients. Version, outer request ID and a
session-wide monotonic sequence correlate each callback. Callback replies and
ordinary request writes share the original request deadline, including blocked
pipe writes. Transport failure closes the session because the mutation outcome
may be unknown. On Unix the configured launcher and descendants run in a private
process group so timeout termination also reaches the native worker.
Closing a document removes its registry entry without resetting the global
clock; reusing its name cannot reuse an old revision. Clock exhaustion terminates
the session at publication. It does not reserve capacity before native edits.

Requests contain `version: 1`, a caller-chosen integer `id`, `op`, and operation
parameters. Responses repeat version and id, contain `ok`, and contain either
`result` or `error: {code, message}`. Unknown versions and operations fail.

The initial operations are `capabilities`, `new`, `open`, `inspect`, `transaction`,
`preview`, `preview_commit`, `preview_cancel`, `undo`, `redo`, `save`, `mesh`,
`topology`, `measure`, `quantities`, `document_settings`, `sketch_begin`, `sketch_finish`, `sketch_cancel` and `close`.
`new` takes a document `name`; `open`
and `save` take a native file `path`; other document operations take `document`.
`mesh` and `topology` also take the result object's `object` name and optional
positive `deflection`, default 0.1 model units.

The shared Rust sketch-creation compiler accepts optional `reversed`, `offset`
with native placement `position` and quaternion `rotation`, a native length
`offset_expression`, and `detach`. Defaults preserve the original creation
command. These options follow support assignment within the same guarded
transaction. `offset_expression` targets `AttachmentOffset.Base.z` through the
native expression engine. `detach_sketch` is a document edit for a native
SketchObject: recompute and evaluate its native support, retain the resulting
Placement, clear AttachmentSupport and set MapMode to Deactivated. It does not
discard sketch geometry, constraints or dormant native offset data. The normal
transaction revision/lease, rollback and validation rules apply. It is not a
sketch-session edit or a separate placement solver.

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

`measure` is a guarded read with required `expected_revision` and one or two native
`selections`. Each selection names an `object` and optional native `subelement`
path, or an explicit native `sketch_geometry:{index,point}`. It returns applicable
native scalar properties, world-space centers, native closest distance/witnesses,
and straight-line/planar-normal angles where defined. Results use the document's
native UnitSystem without changing global unit preferences. It neither recomputes
nor changes native ownership, dirty state, revision or history, including on
failure. During sketch editing, only the active sketch is current; cached solid
context is rejected as `stale_context`. See [MEASUREMENTS.md](MEASUREMENTS.md) for
the complete request, result, identity lifetime and UI workflow.

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

`mirror_geometry` is a sketch edit with native source `indices`,
`reference:{index,point}`, and optional boolean `delete_original` and
`create_symmetry_constraints`. The options are mutually exclusive. It shares
sketch tokens, preview baselines, action replay, document history and native
validation. See [Mirror](SKETCH-MIRROR.md) for selection semantics, the pinned
Windows adapter and native constraint/expression behavior.

`capabilities.sketch_tasks.mirror` and each sketch object's
`sketch.task_capabilities.mirror` report `default_copy` and
`create_symmetry_constraints`, each with `available` and `reason`. The task
disables constrained mode and displays its reason when unavailable. The worker
preflights that mode before opening a transaction or changing a revision,
returning `sketch_mode_unavailable`. Default Python copy remains available
without an adapter. Availability is checked once per worker, including platform,
artifact/runtime hashes and successful synchronous adapter loading.

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
Rust-owned command journal groups accepted sketch actions. Session Undo/Redo
abort the current native transaction and restore the selected accepted action's
opaque `SketchObject.dumpContent()` checkpoint through the same native
`Persistence::restoreFromStream` engine, using the pinned adapter described in
[native checkpoint restore](CHECKPOINT-RESTORE.md). The leaking Python
`restoreContent` buffer binding is bypassed. Restoration remains
against the same object and document. The private journal `checkpoint` callback
returns only that cursor's payload, including a staged move or acceptance. Native
checkpoint bytes are base64 encoded for transport; Rust does not interpret them.
Missing checkpoint metadata is a protocol failure, never a silent fallback to
re-running geometry creation. New candidate edits run after native restoration;
failed replacements restore the last accepted preview checkpoint itself. Thus
accepted geometry does not acquire new IDs merely because a later task is canceled.
The complete native object serialization carries geometry extensions, constraints,
expressions, link properties and native Shape/ElementMap data together. These operations
do not alter a frontend geometry/history graph. This also avoids FreeCAD's
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

Snapshots are prepared before accepting a sketch action or finishing the native
transaction. Failed preview Commit preparation retains the existing candidate,
action history and revision. Failed Begin preparation releases its unpublished
session and retains the revision. If Cancel preparation fails after abort/replay,
the worker restores the owned action prefix and gesture and advances revision.
The client must inspect before retrying with a fresh revision. Failed restoration
marks the document unavailable; Close remains available for recovery.

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

## Native feature schemas and property forms

`feature_schema` takes a native `type` and returns `{type, properties,
property_types, property_metadata, expressions}`. The worker creates its schema
cache before accepting requests and closes every temporary document. Reads do
not create objects, acquire a preview lease or affect document history.

Object snapshots include the same property metadata. Each property reports its
native group, documentation, editor mode/status, read-only/hidden flags and,
for enumerations, current choices. Live metadata is authoritative for conditional
fields. For example, choosing a Hole thread type unlocks its thread size and
threading options. Schema defaults are initial suggestions, not live document
state. The Rust editor writes explicit changes only and sets controlling options
before their dependent dimensions and choices. On successful preview the form
refreshes native normalized values and removes overrides for fields that became
read-only. New solid feature commands publish/show their native Body Tip after
all links are assigned. Existing-feature tasks carry the baseline `body`, `tip`
and `tip_visibility` so changing linked inputs preserves downstream display.

Computed Shape/history, Sketch geometry/constraint lists, expression engines and
underscore-prefixed native bookkeeping cannot be assigned through generic `set`.
Dedicated sketch/expression operations retain their existing contracts.
`binder_support` takes `object` and `value`, a list of native sublinks, and sets
Support only on ShapeBinder/SubShapeBinder. This task-specific input is marked
`task_editable` even where FreeCAD marks the generic property read-only.

Pad/Pocket snapshots also mark native task inputs `task_editable` where their
previous recompute left conditional read-only flags. This is limited to exact
Pad/Pocket types and Type2, Length/Length2, TaperAngle/TaperAngle2,
UpToFace/UpToFace2, UpToShape/UpToShape2, Offset/Offset2, Direction, ReferenceAxis
and AlongSketchNormal. Native read-only metadata remains visible. Computed data
and other types retain the generic protection. Controllers are assigned before
targets and dimensions, then native recomputation validates the complete task.

`cad-next-packages --extrusion-schema` derives the same task presentation as the
editor from `{schema, parameters}` without document writes. It retains inactive
properties and expressions. A controller expression makes dependent inputs
available conservatively after an incomplete preview fails; Rust does not
evaluate its text. Exact expression keys and clears retain native semantics.

Native Visibility is an Output/NoModify display property. It does not create its
own native undo entry and native transaction rollback does not provide the same
history behavior as modeling properties. The editor uses explicit tree/View
Show/Hide actions and excludes Visibility from parameter previews. Callers must
preserve these native display semantics rather than assume every property is a
modeling-history entry.

`remove_object` removes a native leaf. Dependent objects and nonempty Bodies are
rejected before deletion. Removing the current solid tip restores its native
BaseFeature as Tip. This operation does not infer new dependencies or silently
cascade through linked models.

Snapshots include native `effective_visibility`, accounting for geometric
container ancestors. Reference objects also carry `reference_display` with their
native global placement, type, finite presentation size, visibility and actual
subelement where one exists. Infinite datum Shapes are never tessellated to make
viewport proxies. The proxy remains display data; attachments use native object
and subelement references. Topology replies include native Vertex names/points.


## Advanced native sketches and ordered transforms

`geometry` additionally accepts `ellipse` (`center`, `major`, `minor` absolute
points), `ellipse_arc` and `hyperbola_arc` (those points plus
`start_parameter`/`end_parameter`), `parabola_arc` (`center`, positive `focal`,
optional `angle` radians, start/end native parameters), and `bspline` (`poles`,
optional native `degree`, `periodic`, `weights`, `knots`, `multiplicities`).
FreeCAD constructs and solves them. These are native curve definitions, not a
Rust kernel. Constraint constructors are checked against the pinned native
`constraint_signatures` capability before entering the native binding; unknown
names or argument forms cannot be passed through unchecked.

`expose_internal` takes sketch `object` and geometry `index`. Native Sketcher
creates editable construction geometry and its InternalAlignment constraints.
Ellipse axes are native diameter lines; spline poles are native construction
circle centers. Never invent extra point identifiers on a curve.

`external_geometry` takes sketch `object`, `source` object and native `subelement`,
with optional booleans `defining` and `intersection`. `external_state` changes
those flags and `delete_external` removes a projection; both take the flattened
external `index`. Native persistent references own mapping and parent updates.
Sketch snapshots add `externals` wrappers with `index`, signed `geometry_index`
(-3 minus index), source/native persistent-reference metadata, flags and solved
`geometry`. Geometry entries include bounded native display samples and explicit
`handles: [{index, point, position, draggable}]`. Read permission and dragging
permission differ: projected handles can constrain internal geometry but cannot
be dragged. Re-read solved IDs after an edit. All-external dimensions are added
as native reference measurements by appending `constraint_state` with
`driving:false` in the same transaction.

Pinned FreeCAD 1.1.4 Python `Part.Point` wrappers discard geometry extensions.
For projected points, the worker recovers the native reference and flags from
`dumpPropertyContent("ExternalGeo")`, validating the native property count and
types in the same order used by native property serialization. It restores
detached Python points to read the native extension. It does not restore or
rewrite the document, infer source identity from coordinates, or equate source
link indices with cache indices. Other geometry uses its existing extension.
One property recovery pass serves all affected points in a snapshot.

If native metadata cannot be read or validated, the point and its signed native
ID remain visible. Its unresolved reference, source and flag fields are `null`
with `metadata_error`; source-dependent deletion/intersection edits and
Independent Copy fail explicitly. Recovered missing-source caches retain their
native reference and `missing` flag, while an unavailable source link remains
`null`. Independent Copy uses these same native flags, materializes defining
points through native `addGeometry`, and removes copied external identity only
when the Python geometry actually exposes it.

Internal `Geometry` points have the same pinned wrapper limitation. Before
Independent Copy tests or removes retained external identity, it restores their
full native property records into detached points using the same validated
property order. Native cloning/assignment then preserves Sketch geometry IDs,
construction modes and the other serialized metadata while removing only the
external extension. Missing native Sketch metadata fails explicitly; the worker
does not invent a replacement point ID or repair the source document.

The Rust property compiler accepts an ordered `transformations` array for
MultiTransform. Each child has `type`, stable `name`, `create`, explicit
`properties` and `expressions`. `removed_transformations` lists retired children.
The shared compiler creates child tasks, assigns the parent's native ordered
Transformations link, then unlinks/deletes retired children and restores the
captured Body Tip/visibility. Child Originals/BaseFeature links belong to the
parent operation. Preview, Commit, Cancel and Undo retain one native transaction
owner; child steps do not form an independent Rust history.

## Native Body history

`body_tip` and `body_reorder` are dedicated guarded transaction edits. They are
not accepted in previews or sketch sessions. A preview containing either edit
is rejected before replacing an existing candidate. The editor's reorder form
compiles locally until Apply; opening or canceling it preserves native redo.

| `op` | Fields and native behavior |
| --- | --- |
| `body_tip` | `object` names a native Body; required `tip` names one of its native PartDesign feature members, or is null to roll back to the start. Successors remain in native history. Native recomputation rejects ineligible or empty feature Tips. |
| `body_reorder` | `object` names a movable Body member; required `target` names another member of the same Body or is null; required boolean `after` selects insertion side. Native null-target semantics are `after:true` for the beginning and `after:false` for the end. Optional `tip` explicitly chooses the final Tip, including null. Omitting it retains the native remove/insert result. |

Reordering calls native Body removal and insertion, including BaseFeature
rewiring; assigning a reordered Group array is not equivalent. Native dependency
checks, recomputation, transaction rollback and persistent references remain
authoritative. Failed dependencies or invalid final shapes restore the previous
history. Setting a Tip also applies native sibling feature visibility behavior;
sketch and datum visibility is retained. A null Body Tip can validly have an
empty Body Shape while retaining its complete Group.

Shape summaries expose raw native statistics. Volume is meaningful for solids;
unbounded reference planes can return unstable kernel pseudo-volumes. Compare
reference geometry through its native properties, placement, topology and
appropriate measures rather than treating a plane as a solid.

Object snapshots include native-derived
`history_flags: {can_tip, can_reorder, transform_step}`. These describe current
native eligibility and membership, not another feature graph. The Rust tree
follows native Body Group order. Owned references outside that list are retained
after its ordered members.

`cad-next-packages --native-history FILE` reads one JSON object with the complete
native `state` snapshot and a `request` containing one of the edits above. It
returns an edits array. Use the same captured state for an open task and submit
its result with a revision guard. Compilation changes no model; native validation
still decides whether the candidate history is valid. Suppression remains a
separate incomplete workflow, including native placement persistence limits.

## Native sketch task edits

These edits target one native SketchObject through the existing guarded
transaction or sketch-session boundary. They do not open their own transactions.
All curve indices refer to internal geometry and must come from the accepted
native snapshot. Topology-changing operations can renumber geometry and
constraints; read the returned snapshot before another action.

| `op` | Fields and native behavior |
| --- | --- |
| `fillet_geometry` | `object`, positive `radius` in mm or a compatible unit string; either `index` plus endpoint `point` 1/2, or two distinct `indices` and two sketch-local `positions`. Optional booleans `trim:true`, `preserve_corner:false`, `chamfer:false`. Radius seeds the generated geometry; a separate native Radius constraint drives it afterward. Native chamfer leaves the fillet arc as construction geometry. |
| `split_geometry` | `object`, `index`, sketch-local `position`. Native Sketcher splits the picked curve. |
| `extend_geometry` | `object`, `index`, endpoint `point` 1/2, `increment`. Native support is line segments and circular arcs. Numeric increments use mm for lines and radians for arcs; explicit compatible unit strings are accepted. |
| `carbon_copy` | `object`, another native sketch `source` in the same document, optional `construction:true`. Native Sketcher enforces scope/alignment/dependencies and creates expression links to copied driving dimensions. |
| `spline_degree` | `object`, spline `index`, nonzero integer `change`; resulting native degree must be 1..25. Native degree reduction approximates/replaces the spline and removes old constraints. |
| `spline_knot` | `object`, spline `index`; either native `parameter` with optional positive `multiplicity:1`, or one-based `knot` with nonzero integer multiplicity `change`. Native Sketcher validates the curve-specific limits. |
| `spline_pole` | `object`, spline `index`, zero-based `pole`, sketch-local `position`. Exposes the native pole control and moves its circle center through Sketcher. |
| `spline_weight` | `object`, spline `index`, zero-based `pole`, positive dimensionless `value`, optional `release_constraints:[]`. Releases only explicitly listed Equal constraints involving that native pole circle, then adds or updates its native Weight constraint. |

Sketch snapshots include `spline_controls` entries with `spline_index`, zero-based
`pole_index` and actual exposed `control_index`. Reading these entries does not
expose geometry. Task-generated control/constraint IDs belong to the current
candidate, not the captured baseline. Preview replacement must compile against
the same baseline and original selections; never carry candidate IDs into the
next replacement. The adapter registers the native Geometry property before
arc extension so native abort/Undo also restores that in-place handle change.
Persistent native geometry IDs and reference mapping remain authoritative.
Knot multiplicity reduction/removal can approximate the curve and remove or
retarget native control constraints. Exact reversal in a fixture that removes a
just-inserted knot does not establish general shape preservation.

`cad-next-sketch` accepts a shared UI/agent NDJSON compiler request:

```json
{"op":"edit","snapshot":{"name":"Sketch","sketch":{"geometry":[{"index":0,"kind":"line"}]}},"kind":"extend","selection":[{"index":0,"point":2}],"parameters":{"increment":"2 mm"}}
```

Use a complete current native sketch object as `snapshot` in actual calls.
Kinds are `fillet`, `chamfer`, `split`, `extend`, `carbon_copy`, `spline_degree`,
`spline_knot`, `spline_pole` and `spline_weight`. Responses use the existing
`{ok,edits}` or `invalid_sketch_command` format. Compilation changes no document;
submit returned edits with `expected_revision` and the active session token.
The editor task owns its preview token. Apply accepts one sketch action; Cancel
restores the task baseline. When redo prevents a native preview, native execution
is deferred to an ordinary Apply transaction and Cancel preserves redo. All edits
still need native validation, and Finish still validates downstream features.

## Geometry exchange

`import_exchange` is an ordinary guarded transaction edit with a `path` ending in
`.step`, `.stp`, `.iges` or `.igs`. Native STEP import preserves App::Part
hierarchy; native IGES import can produce faces/shells without solids. The edit
returns added object names. Failed or empty imports roll back with the batch.

`export_exchange` takes `document`, `expected_revision`, `path`, an explicit
nonempty `objects` name list and optional `overwrite:false`. Active preview or
sketch ownership blocks exchange. It returns unchanged document/revision plus
`exported_objects`, format/path and `placement:"selected_root_local"`. Native
export retains selected roots' local placement; select the enclosing Part to
retain its hierarchy and placement. A same-directory temporary output is
atomically published. Existing output requires explicit replacement; an output
created by another writer also blocks publication. Export does not mutate the
native document or mark it dirty. Original FCStd and imported files remain
unchanged unless that exact destination is explicitly selected for replacement.


## Native sketch creation supports

`cad-next-packages --native-sketch-create JSON` (or `-` for stdin) compiles
`{state,request}` using the editor's same Rust builder. Request fields are
`name`, `body`, `create_body` and `support`. An origin support is
`{"kind":"origin","role":"XY_Plane"}`; XZ_Plane and YZ_Plane are also valid.
Reference support supplies `kind:"reference"`, `object`, one `subelements` entry
and `mode:"FlatFace"` for a native plane/face or `mode:"ObjectXY"` for a whole
Sketch frame. Whole native supports use the empty subname. Supports must belong
to the target Body. A Body face reference resolves to its current native Tip to
avoid a dependency on the whole Body being extended. Native FlatFace attachment
owns planarity rejection and the complete transaction rolls back on failure.

The optional `origin_plane` field on `add_object` is limited to Body-owned
Sketcher::SketchObject creation. After native Body creation it resolves the
actual `Body.Origin.OriginFeatures` plane by Role and assigns AttachmentSupport
plus FlatFace. This allows a new Body, its origin and Sketch to share one native
transaction without guessing generated names. Submit compiled edits with the
current expected_revision. Compilation and chooser cancellation do not mutate
the document. Attached MapReversed and AttachmentOffset remain ordinary native
properties, with native expression units and frame semantics.


## Native profile reference choices

`cad-next-packages --native-reference JSON` (or `-` for stdin) compiles
`{state,request}` against the same captured native state as the editor. Request
fields are `source`, target `body`, optional copy `name`, and `mode`:
`independent`, `dependent` or `cross_reference`. Independent/dependent modes
return a `reference_copy` edit; a cross-reference needs no copy and the feature
links its original source. Compilation changes no native document.

`reference_copy` fields are `source`, `name`, `body` and `mode`, limited to
`independent` or `dependent`. It is an ordinary guarded edit, sharing the caller's
transaction or preview lease. Independent requires a native Sketch and copies
eligible native properties through native serialization, retaining internal
constraints, construction/control metadata, expressions and their comments.
It removes external geometry and its persistent cache from the copy only,
leaves attachment deactivated, and verifies the solved profile is unchanged.
Explicit expression links to other document objects are retained. Unsupported
properties or a changed solved profile reject the complete batch atomically.
Defining/intersection curves are captured before target recomputation and
materialized as native internal copy geometry. External identity extensions are
removed from both those curves and any already-internal copied curves; ordinary
sketch geometry IDs remain native. Circular defining/intersection fixtures and
a legacy materialized curve cover this policy. Broader curve and constraint
combinations still require acceptance; a changed solved profile rejects the batch.
Dependent creates a native ShapeBinder with whole-object Support and native
TraceSupport defaults. Copy placement follows pinned native local-frame behavior;
there is no client world-placement compensation.

The native property compiler accepts optional
`reference_choice:{state,request}` alongside feature parameters. It verifies the
source and target match Profile/body, prefixes copy edits, substitutes only the
compiled candidate's Profile and preserves the original task source. All preview
replacements compile against that captured baseline, never a temporary candidate
copy. Existing direct foreign links remain unchanged unless explicitly edited.
The compiler retains the normal final Body Tip and visibility behavior.

Pinned native Undo/Redo can restore an object's expression values without their
live dependency edges. After ordinary document Undo/Redo, the adapter rebinds
expressions only on newly restored object names through native setters, including
native comments. Existing objects are not rewritten and no extra transaction is
created. The compensating history path uses the same repair before recomputation.
This fixes the verified restoration cases; it does not replace native history or
claim a general Rust expression engine.


## Native arc handles and dimension commands

Snapshot geometry `handles` use native `Sketch.getPoint(index, point)` identities.
Raw curve `start`/`end` and sampled points retain the native curve's tracing order.
These orders differ for clockwise arcs: native point 2 is the first three-point
input click, and point 1 is the final click. Use handles for solver constraints
and input picking, and the curve interval/axis for its display trace.

The Rust sketch compiler's Draw request accepts optional `snaps`, one null or
`{index,point}` per input click. The editor and compiler share the same snapped
edit builder. Point targets add Coincident; whole-curve targets add PointOnObject.
The middle click constructs a three-point arc without a new snap constraint.
Native Sketcher remains responsible for solving the compiled edits.

Explicit Distance on one circular arc uses native arc length; Angle uses its
span in radians at the constructor boundary. Explicit Distance also supports
point-to-circle, circle-to-line and circle-pair boundary clearances. Native
signed overlap/containment conventions apply. Arc/circle references use native
solved snapshots and the same Rust witness calculation as viewport annotations.
Smart Dimension retains its existing radius and center-distance choices. Native
units, expressions, reference/driving state and history retain their existing
protocol behavior.


## Refraction constraint workflow

`refraction` is an atomic sketch edit with `object`, native SnellsLaw `args`
`[ray1, endpoint1, ray2, endpoint2, boundary, seed]`, and optional `value` as a
number or native quantity string. `value` overrides the seed.
The shared Rust sketch compiler emits this edit for kind `SnellsLaw`. It keeps
ray endpoint selection order as n1 then n2, independent of the boundary's position
in the selection. The bounded creation scope is two distinct straight ray
endpoints and a different internal or projected native boundary curve, with at
least one internal entity and a finite ratio n2/n1 accepted by native Sketcher.
Axes and curved rays are outside this editor creation scope.

The worker uses native `getPoint` and `isPointOnCurve`, adding missing Coincident
and PointOnObject constraints in the pinned GUI task's order, solving after each
addition. Geometrically satisfied relationships are skipped even when no explicit
relationship constraint exists. It then adds native SnellsLaw. Existing user
constraints are never removed to force a solve. The ordinary atomic transaction,
preview and sketch-session boundaries own rollback. The edit result reports the
native SnellsLaw `index` and any added `preparation` indices. Refresh the snapshot
to obtain authoritative constraint numbering after preparation or deletion.

Boundary dispatch includes lines, circles, circular arcs, ellipses, elliptical
arcs, hyperbola arcs, parabola arcs and B-splines. B-spline weight circles are
rejected explicitly. Native solver failures name the failed constraint and
boundary type and restore the accepted state. Six curved boundary classes have
successful native workflow evidence. The tested B-spline fixture is rejected by
both pinned native Sketcher and this workflow; broad spline success is unclaimed.

Existing SnellsLaw datums use native `setDatum` without calling pinned FreeCAD
1.1.4's unsupported SnellsLaw `getDatum`. Native Sketcher owns signed values,
reference measurements, inactive constraints and solver failures. No global
positive-ratio policy is applied when loading, recomputing or editing documents.
Expressions pass directly to native `setExpression`; native recompute and the
ordinary validity checks decide whether an edit can commit.

The ratio display and default input have no unit suffix. Quantity inputs preserve
pinned native conversion behavior: creation uses `Quantity.Value`, as the GUI
creation command does; `setDatum` receives the quantity itself, including native
angle-to-radian conversion. Native expression conversion can accept unit-bearing
results as scalar constraint values. No additional refraction-only unit policy
is imposed. Invalid native edits retain the ordinary atomic rollback behavior.
Driving/reference changes remain native `setDriving` operations. The editor and `cad-next-sketch` share
`datum_edits`; the CLI accepts `{"op":"datum","snapshot":...,"index":0,
"value":"1.5","expression":false}`. A numeric replacement clears a previously
bound indexed or named native expression in the same atomic action.

At a shared vertex, click selects the native handle hit first. Repeated Shift
clicks visit the other co-located handles, then revisit each handle to remove it.
For A, B, C the sequence is `[A]`, `[A,B]`, `[A,B,C]`, `[B,C]`, `[C]`, `[]`.
The next Shift click starts adding again. Surviving handles keep their selection
order, including ray n1/n2 order and negative projected geometry IDs. The origin
ring and isolated handles retain their normal toggle behavior. A changed native
snapshot, cleared selection or direct element selection resets the remembered handle;
an identical snapshot refresh preserves the cycle.


## Tangency and perpendicularity through an exposed point

The shared Rust `constrain` builder accepts three selected elements for existing
kind `Tangent` or `Perpendicular`: two distinct whole selectable native curves and
one exposed native point. The point can appear anywhere in selection order;
relative curve order is preserved. No datum or expression is accepted. Existing
two-element creation keeps its existing path. Angle through a point uses the
separate measured workflow below.

The builder emits one atomic `via_point` sketch edit with `object`, native `kind`
`TangentViaPoint` or `PerpendicularViaPoint`, and four signed integer `args`
`[curve1, curve2, point_geometry, point_position]`. Native constructor validation
runs before mutation. A point must be a handle exposed by the same solved
snapshot, including the origin, endpoints, centers, standalone/internal points or
projected-curve handles. Whole curves use the editor's existing line, circular,
conic and spline families. The worker rejects two external/blocked curves and
B-spline weight-circle curves, matching pinned native GUI checks.

Preparation follows CommandConstraints.cpp: test native point-on-curve for curve
1, curve 2, then curve 1 again using the latest solved point. Add missing native
PointOnObject constraints for non-B-splines; skip these additions for B-splines
because the native via-point constraint includes them. Add the native constraint,
then remove matching existing PointOnObject constraints only on the selected
B-spline curves. Match the pinned helper's point-geometry-ID comparison exactly;
it does not filter FirstPos. Delete descending through native `delConstraint` so
FreeCAD owns expression-path remapping. Do not replace the constraint array or
delete unrelated constraints. Check final solver diagnostics after substitution.
A transient diagnostic before native cleanup does not by itself abort the task.

The edit result returns the final native `index`, final `preparation` indices and
`removed_point_on_object` as the removed pre-cleanup indices. Native stored type
is `Tangent`/`Perpendicular` with the selected point in `Third`/`ThirdPos`; native
constraint values/branch choices are retained. Refresh the complete snapshot
rather than retaining old constraint indices. Ordinary transactions, preview
replacement/Cancel, sketch actions and consumed-sketch Finish/Cancel own the
atomic lifecycle. There is no frontend solver or second constraint history.

## Angle through two curves and a point

The shared Rust builder accepts kind `Angle` with three selected elements, or
explicit kind `AngleViaPoint`. Select two whole native curves and one exposed
point in any order; relative curve order is preserved until the native command
selects its angle branch. Native sketch axes are whole curves with signed IDs
`-1` and `-2`. They appear in the additive snapshot `sketch.axes` array with
native line coordinates, empty `handles` and `editable:false`. The origin keeps
its existing point identity `[-1,1]`; axis unit endpoints are not handles.

The builder emits one atomic `via_point` edit with kind `AngleViaPoint`,
`args:[curve1,curve2,point_geometry,point_position,0.0]` and a boolean `driving`.
The last argument is a guarded placeholder. After native PointOnObject
preparation, the worker calls native `calculateAngleViaPoint`, swaps curves for
a negative measurement below native angular precision, and uses the command's
eight-significant-digit constructor seed. Native C++ owns spline parameters,
the angle branch, solving and expression remapping. Preparation and selected
spline PointOnObject cleanup follow the pinned Angle command. B-spline weight
circles are rejected. Two fixed curves produce a reference angle rather than
the Tangent/Perpendicular fixed-curve rejection.

The optional edit `value` is forwarded to native `setDatum`; optional
`expression` binds the final native index after spline cleanup. The shared CLI
constraint request accepts those options as strings, plus optional boolean
`driving`. Use either value or expression. `driving:false` requests a reference;
the native command still creates reference mode for two fixed curves when
`driving:true` is requested. Native reference policy decides whether a subsequent
datum/expression is valid. There is no extra positive/range/unit restriction on
via-point angles. Existing datum requests use the same native policy.

Stored native type is `Angle`, with its point in `third`/`third_pos`. Each such
constraint snapshot includes `angle_via_point:{center,start_angle}`, measured
through the native API for annotations. The center is in the sketch frame and
start_angle is radians relative to native X. If native witness measurement is
unavailable, `display_error` is explicit and the valid document still opens.
This display information never defines constraint geometry or the solver seed.

The editor starts an owned preview to seed its existing Dimension window. Apply
replaces the preview with the selected value, expression or reference option,
then commits only on native success. Cancel releases the preview. Failed Apply
keeps the dialog recoverable. With document or sketch-action Redo, opening the
dialog performs no mutation; native preparation/measurement waits for Apply.
Ordinary edits, sketch actions and consumed-profile Finish/Cancel use the
existing journal, revision and lease boundary.

## Private gesture owner boundary

Shared-write rejection order is worker-wide sketch lease, worker-wide preview
lease, document lookup, poison, expected revision, local preview lease and local
sketch owner. Preview token validation and ownership follow those guards.
A nested sketch preview therefore reports sketch_active for a foreign document,
preview_active for a local ordinary write, and sketch_owner before preview_owner
for a local preview with the wrong session token. A poisoned owner's Close still
requires the current revision and then bypasses local gesture guards.

Rust stages candidate preview metadata before snapshot preparation. Rejected
preparation restores the exact prior metadata or releases an unpublished first
candidate. Sketch Begin publishes its owner only with a prepared snapshot and
releases the unpublished owner/journal on preparation failure. Native replay,
validation, transaction commit/abort and recomputation stay in Python/FreeCAD.
The existing publication and recovery rules above continue to apply. The private
lease channel uses the same outer request ID, sequence and original deadline as
the journal and revision channels. Internal lease-state errors terminate the
session; ordinary ownership errors leave it usable. Close removes gesture and
poison state, and reopening/reusing a name starts with no previous lease.

Preview label validation precedes native redo admission. Rust requests the
native RedoCount fact only for a first feature preview or unopened sketch
transaction. Existing candidates and open sketch transactions skip that getter
for admission. The demand and fact callbacks share the original request deadline.

## Document settings and quantity text

See [Native document units](DOCUMENT-UNITS.md) for the additive revision-checked
`document_settings` and read-only `quantities` operations, metadata, text wrappers
and default-unit rules. UnitSystem is explicitly outside native transaction
Undo/Redo. It uses guarded assignment, preparation and explicit failure
restoration. Numeric protocol values and native expressions retain their existing
meanings. Sketch attachment offsets additionally accept native `axis_angle` with
unitless `axis` and a native angle input, alongside the existing quaternion.
