# UI functional coverage

The current application is incomplete. A native feature appearing in the catalog,
or a property being editable, establishes access to that interface. It does not
establish a complete creation/editing workflow or parity with FreeCAD's task
dialogs. Rust implements the editor and command construction; the native engine
still owns modeling, constraints, references and history.

| Area | Current access | Remaining functional acceptance |
| --- | --- | --- |
| Document lifecycle | New, FCStd Open, Save/Save as through a filtered modal file browser, unsaved-change protection, native Undo/Redo and guarded writes. | Broader files and failure/recovery, multiple-document interaction and cross-process locking. Pixel GUI verifies filtered picking, folder navigation, replacement confirmation, cancellation and unsaved-change protection. |
| Sketch creation and editing | Native XY/XZ/YZ origin-plane chooser, same-Body tree datum/sketch-frame and face support; Point, Line, Polyline, Rectangle, Circle, Arc, Ellipse, elliptic/hyperbolic/parabolic arcs and open/periodic B-splines; native internal geometry, snapping/inference, construction, boxes, dragging, trim/deletion and projected/defining/intersection references; fillet/chamfer, split/extend, Carbon Copy and spline degree/knot/pole/weight task forms. | Standalone orientation/reverse/offset dialog, broader support/reference policy, remaining native tools, spline variants and broader Binder/projection combinations. Pixel GUI plus native reopen checks cover fillet/chamfer, split/extension, Carbon Copy, spline degree/knot/pole/weight tasks and invalid-weight recovery; broader operation combinations remain. |
| Sketch constraints | Common geometric constraints, length/coordinate/radius/diameter/angle dimensions, circular arc length/span and boundary clearances, native refraction ratios, Tangent/Perpendicular through an exposed point, expressions, names, reference/driving and activation controls. | Remaining native constraint families and combinations, including AngleViaPoint; full selection, conflict and recovery coverage. |
| Sketch sessions | Action Undo/Redo, gesture previews, Finish validation and Cancel to baseline, including repair of consumed profiles. | Broader histories, topology changes and operation combinations; the weight task now has a real GUI Cancel-preserves-Redo check. |
| Feature property forms | Native schema supplies defaults, enums, documentation, status and property types. Forms edit explicit overrides and expressions, including references, lists, vectors and placements; foreign Sketch profiles offer Independent/Dependent copy or Cross-reference in the feature transaction. | Every declared type's required inputs and conditional options; broader foreign-reference types, broader independent defining/intersection materialization combinations and task-specific behavior that generic writable fields cannot provide. |
| Extrusions and revolutions | Pad/Pocket and Revolution/Groove forms expose native profile, extent/direction and available second-side options. | Complete termination modes, offsets, axes, parent changes, invalid selections, reopen and GUI workflows. |
| Lofts, sweeps and helices | Additive/subtractive feature forms expose profiles, sections, paths, axes and native modes. | Section/path ordering, multiple sections, guide/orientation modes and topology-edit recovery through the UI. |
| Holes and dress-ups | Hole, Fillet, Chamfer, Draft and Thickness forms expose native parameters and references. | Hole/thread variants, face/edge selection rules, neutral planes, shell modes and difficult blends. |
| Patterns and transforms | Linear, Polar, Mirrored and Scaled forms; MultiTransform adds, edits, removes and reorders native child steps in the parent preview transaction. Pixel GUI verifies Linear/Mirror steps, cancellation, Undo/Redo and editing after reopen. | Broader direction/axis/plane and real GUI combinations; full transform task-dialog parity. |
| Primitives and Part operations | Part and additive/subtractive primitive catalogs; Part CSG, extrusion, revolution, fillet and chamfer forms. | Each native operation's creation, edit, failure and persistence workflow, including specialized edge data. |
| Datum and reference geometry | Plane, Line, Point, CoordinateSystem, ShapeBinder and SubShapeBinder interfaces. | Support selection, attachment modes/offsets, binder-specific support editing and linked reference updates. |
| Bodies and Booleans | Active/new Bodies, native tip display, grouped history and linked Union/Subtract/Intersect operations. | Broader GUI coverage, multiple tools, nested references and native transformed-Group behavior. |
| Object/history management | Native properties provide labels, visibility and placements; guarded leaf removal protects dependents; Body Tip rollback/resume and Apply-only native feature reordering follow Group order, including datum/binder rows. | Complete suppression, broader history/dependency editing and document organization workflows. |
| Viewport and input | Native face/edge/vertex identities, depth-aware multiple-object selection, camera navigation and model-thread separation. | Interaction checks for every new geometry/reference kind and complete tool/gesture cancellation behavior. |
| Import/export and migration | Native FCStd persistence; guarded native STEP/IGES import and explicit-object export with atomic output publication; shared format-filtered file browser; earlier prototype and original documents retained. | Broader format variants, explicit `.cadpart` migration, additional formats and unsupported-feature reporting. Pixel GUI verifies filtered import and multi-object Export Browse; independent native imports check the output. |
| Assemblies | Body placements and Part containers are available. | Assembly documents, joints/constraints, component references and saved assembly editing. |
| Extensions and agents | Declarative packages, native feature discovery, shared Rust edit compilers and revision guards. | Executable plugins, UI contributions, cross-process collaboration and broader agent workflows. |

Promotion requires matched creation, multistep editing, failures, Undo/Redo,
preview/Cancel, parent/reference changes and save/reopen behavior for the complete
declared scope. Run the same Rust compilers as the UI. Native workflow fixtures
and schema checks complement real GUI interaction; neither substitutes for the
other. Resource checks guard regressions during migration. UI customization and
performance tuning follow full functional Rust completion.

See [VERIFICATION.md](VERIFICATION.md) for recorded evidence and
[COMPONENTS.md](COMPONENTS.md) for implementation ownership and replacement gates.


### Bounded refraction candidate

Dimension > Refraction ratio n2/n1 creates native SnellsLaw from two selected
straight-ray endpoints in n1, n2 order and a whole internal or projected boundary
curve. Shift-click can select both coincident ray endpoints. The native constraint
list and a pickable n2/n1 annotation open existing ratio/expression editing without
length or angle units. Native reference/driving state remains available.

Headless verification covers missing-relation preparation, geometric relationship
skips, native solver results, explicit ratio and expression edits, quantity inputs,
native solver failures, document and sketch-action Undo/Redo, previews and Cancel,
consumed Pad sketch sessions, external-parent rotation, and save/reopen. Viewport
tests verify rotated-attachment label picking and coincident endpoint selection.
This evidence is not real GUI acceptance. Immutable generated GUI inputs and a
read-only native verifier are retained locally. Real GUI creation, picking,
dialog recovery and persistence remain pending.

Creation uses finite ratios accepted by native Sketcher and selectable native
boundary curves, excluding axes, curved rays and B-spline weight circles. Circle,
arc, ellipse, ellipse arc, hyperbola arc and parabola arc workflows pass native
checks. The tested B-spline case fails in direct native FreeCAD too and rolls back
with an explicit error. This does not claim full Sketcher or general spline parity.

The corrected candidate preserves valid signed ratios, negative reference
measurements, inactive negative/zero constraints, native quantity conversion and
unit-bearing native expressions. Coincident handle tests cover full Shift add
and remove cycles with two through five handles, projected and arc endpoints,
centers, transformed attachments, refresh and reset behavior. Real GUI acceptance
remains pending.


### Tangent and perpendicular through an exposed point

The existing Constrain > Tangent and Constrain > Perpendicular commands accept
two whole curves plus one exposed native point. The point may be selected first,
middle or last; curve order stays as selected. The command submits one atomic
native action without a dimension dialog. Existing two-element behavior is
unchanged. Native constraint rows expose the resulting curve/point identities
through the existing list, highlighting, naming, Active and Delete controls.

Native workflow evidence covers line/circular/conic/spline families, missing
PointOnObject preparation, spline substitution and expression-index remapping,
external source updates, native exposed points, ordinary and sketch-action
Undo/Redo, previews and Cancel, consumed Pad sketch Finish/Cancel, failure rollback
and save/reopen. Independent native checks reproduce the pinned GUI procedure
from each baseline and compare geometry, branch, constraint count/identity and
expressions. These checks do not claim real GUI acceptance.

The scope is the editor's existing selectable curve families and exposed point
handles. Axis-curve selection remains outside the existing selection boundary.
Projected-edge endpoints are covered. AngleViaPoint, ratio validation and
coincident-selection behavior are unchanged by this delta. Real GUI acceptance
remains pending.

Standalone native vertex projections now serialize using authoritative native
property metadata, including their signed geometry IDs, persistent references
and flags. Focused native workflows cover mixed face/edge/vertex caches, equal
coordinates from different sources, shared Rust point selection and via-point
commands, transformed source/target updates, missing-source history, conflicting
edits, projection deletion with constraint remapping, and save/reopen. Existing
Independent Copy is checked with projected and defining standalone points,
including source preservation and native defining-point materialization. These
are native/CLI checks; projected-point GUI picking has not been verified.

Focused Independent Copy checks also cover already-materialized internal points
with hidden external identity and mixed materialized curves with ordinary or
construction points. Raw native Geometry XML and native IDs/modes are compared
through preview/Cancel, failed-batch rollback, Undo/Redo and save/reopen. Source
snapshots and native content bytes remain unchanged during the copy operation;
only copied external identity is removed. These remain native/CLI checks.
