# Editor verification

Verification record, October 2026. The Rust editor now includes interactive native
sketch editing. This record does not establish complete Part Studio, FreeCAD
sketch-tool or Onshape parity, or a replacement release for the earlier Windows
application.

## Accepted-action journal extraction

Rust now owns accepted sketch edit groups, the action cursor, staged acceptance
and history moves, branching, counts and replay planning. Native document
transactions, replay execution and geometry remain native; Python still owns
gesture tokens, leases and native orchestration. This is a bounded
component extraction, not a complete Rust session or modeling port.

The journal checkpoint passed 146 active Rust tests, strict Clippy and all 16
native suites on Windows.
Matched headless evidence covers 368 broad responses, 30 deterministic journal
responses, five recovery states and 59 additional anchored responses per side.
Comparison retains coordinates, constraints, ownership, revisions and history.
Explicit native-repeat alternatives account for underconstrained motion;
origin-anchored task fixtures eliminate that freedom. Eighteen deliberately
corrupted results are rejected, including large coordinate drift and excessive
peak memory. Four memory threshold boundary checks pass.

The callback writer has deadline tests for stalled large replay replies and
ordinary request writes. A 256 KiB stalled callback with a 200 ms deadline exits
in about 262 ms including host startup. The previous blocking writer took about
1.56 seconds. Timeout closes and reaps the configured worker. Unix descendant
termination passes its Linux-specific test in
[Windows/Linux CI](https://github.com/LeoHougaard/CAD/actions/runs/39809621100).
Both platforms pass their full Rust checks and all 16 native suites for the
journal checkpoint.

Matched Windows debug headless resource checks use a native Pad with 12 holes,
16 geometry items, 47 constraints, 64 accepted actions and 32 Undo/Redo pairs.
Combined median working set is 79.71 MiB before versus 80.21 MiB after; peak
working set is 80.21 versus 80.83 MiB. Warm startup is 624.45 versus 633.20 ms.
All declared latency and memory gates pass. CPU use is zero in six idle samples
of at least five seconds. These measurements do not establish GUI responsiveness,
cold-cache startup or general complex-part performance. Pixel desktop acceptance
of this extraction remains open.

## Revision registry extraction and lifecycle recovery

Rust now exclusively owns document revision state and missing/stale token
checks. Python retains gesture leases and the decision when acceptance or
recovery requires a new revision. A separate review found no actionable defects
in the five-file extraction. It verified malformed tokens and identifiers,
integer request IDs, name reuse, callback failures and lifecycle recovery through
286 additional native responses and focused Rust/Python tests.

The merged desktop source passes 151 active Rust tests, strict Clippy and four
Python callback-transport tests. Public native regression scripts exercise 310
guarded responses and eight injected snapshot-failure cases. They cover all
published operations, worker-wide leases, stale writes, failed preparation,
retained preview ownership, action history and successful retry. Rejected sketch
preview Commit preserves the candidate and redo; Finish prepares its snapshot
once before commit. Failed sketch Cancel and preview Cancel restore their owned
state and advance the revision. Failed Begin releases its unpublished session
without advancing revision.

All 18 merged native suites pass on Windows. Fresh matched resource checks
compare the original Python journal/revision baseline with the actual combined
Rust journal, deadline writer and revision host. Combined median working set is
79.65 versus 80.09 MiB; peak working set is 80.14 versus 80.76 MiB. Warm startup
is 622.56 versus 631.07 ms. Every declared latency and memory gate passes, with
zero CPU in six idle samples of at least five seconds. The fixture and limits
are the headless 12-hole Pad workload described above.

Windows and Linux integration pass all 18 suites in
[revision CI](https://github.com/LeoHougaard/CAD/actions/runs/39810919139).
Pixel desktop acceptance of the revision extraction remains pending.

## Refraction and constraints through an exposed point

The integrated Rust editor and CLI share native SnellsLaw creation/datum edits
and Tangent/Perpendicular creation through two curves and an exposed point.
Native Sketcher prepares missing relationships, solves and retains constraint
identities. Native spline substitution removes implied PointOnObject constraints
through the native API, including its expression-index remapping.

Windows integration passes 160 active Rust tests, strict Clippy, the existing
18 native suites, 12 refraction cases, 42 via-point cases and 35 native
compatibility cases. The compatibility test generates generic input documents
and checks their hashes after oracle, edit and reopen checks. It requires no
private models. Signed ratios, negative reference measurements, inactive zero
and negative constraints and native quantity/expression conversions are retained.

Independent combined review found no actionable defects. New probes combine
spline substitution with a named signed refraction expression in the same sketch,
then exercise preview recovery, action/document Undo/Redo, Cancel, Finish and
reopen. Both saved results match direct native geometry and constraint checks.
Eleven malformed or invalid requests preserve the documented rollback/revision
contract. Headless pointer events verify 112 selection clicks, complete coincident
handle cycles, attached coordinates, native negative IDs and ratio-label picking.

These are source, compiled-input and native checks. Real Pixel GUI acceptance
remains pending, as do broader constraint combinations, AngleViaPoint, axes and
the separate projected-vertex serializer correction. Full FreeCAD parity is not
established.

## Real Pixel desktop workflow

The native Linux ARM64 editor ran as a separate application beside the existing
prototype. The documented `pixel-desktop-tool` client supplied real mouse and
keyboard input. Screenshots were inspected during the workflow.

### Sketch editing

The current sketch editor was exercised with real Pixel input:

- Drew Point, Line, Polyline, Rectangle, Circle and three-point Arc, including
  construction geometry. These tools retain native freedom for later dimensions
  and constraints rather than fixing every coordinate during creation.
- Created a 40 mm dimension through the interactive dimension dialog and applied
  a vertical constraint through Constrain.
- Dragged an endpoint, a circle center and a circle radius through native
  Sketcher, using solved results as the displayed geometry.
- Used a selection box to delete geometry and restored it with session Undo.
  Trim and its Undo were also exercised.
- Finished a sketch and created a native 10 mm pad, then created and applied a
  10 mm pocket.
- Confirmed that Return applies a dimension, bare values receive native length
  units, and Delete on a selected constraint removes the constraint rather than
  its highlighted geometry. Cancel restored the starting sketch, and canceling
  a new sketch removed its temporary feature.
- Exercised sketch entry/Cancel with native redo present and committed a feature
  edit after Undo; Undo/Redo remained usable.
- Reopened a consumed profile, deleted a required segment and confirmed that
  Finish rejected the open contour. Undo repaired the sketch and Finish then
  succeeded.

An independent native worker reopened an earlier GUI-created sketch document.
The saved sketch contained four lines, eight constraints and four remaining
degrees of freedom, including its 40 mm dimension. Its 10 mm pad was valid with
volume 13723.55643507148 cubic millimeters. Reopen preserved the editable
geometry and constraints. A separate copy was edited to 45 mm, passed native
Undo/Redo, saved and reopened with volume 15062.713394201584 cubic millimeters.
The source document's hash remained unchanged. The private report is retained
locally; the source model and screenshots are not published by these checks.

The later all-tools GUI file also passed an independent native check: two
sketches, a Pad and a Pocket; its attached sketch contains eight geometries,
eight constraints and 18 degrees of freedom. The normal circle cuts the solid;
the construction arc, point and five lines do not. Support remains
`Extrude4.Face6` at z=10 mm. The valid pocket volume, 13144.699350942195 cubic
millimeters, matches the pad volume minus the analytical circular cut. Its
original file hash remained unchanged.
The interactive checks above do not establish GUI coverage of every constraint
selection form, every expression/state control or difficult trim/drag case.

### Multi-Body and native Boolean workflows

The editor created two independent Bodies, each with an editable rectangular
sketch and Pad. All visible native Body tips rendered together. Union, Subtract
and Intersect previews were inspected; Cancel restored both independent Bodies.
Union was applied, undone and redone, then saved as a separate FCStd file.

A third Body was created and added to the existing Boolean's tool list. Changing
the operation to Subtract, applying it, and using Undo/Redo retained all three
native sketch/Pad histories. Editing a consumed tool's Pad from 15 to 8 mm
recomputed the target. Separate files were saved and the original was reopened
in the real editor for continued editing. An empty Intersect result was rejected
with Apply disabled; Cancel restored the valid baseline. Temporary test features
were removed through native Undo.

Independent new workers reopened the GUI-created files without modifying them.
The two Pad volumes were 10781.25 and 2456.904121032553 cubic millimeters;
their analytical overlap was 1637.9360806883687. Native Union volume was
11600.218040344183, matching the two volumes minus overlap. The two-tool Cut
had valid single-solid volume 9029.21619039918. Changing the first tool's height
to 8 mm produced 9356.803406536852. Both match analytical box intersections.
All three sketch histories retained four geometry entries, eight constraints
and four degrees of freedom. Tool links, native Body ownership and consumed
Body display metadata were correct; original file hashes were unchanged.

These are functional checks of Rust controls and native modeling behavior,
not a Rust kernel replacement or complete FreeCAD feature coverage.

### Earlier feature-chain baseline

These earlier GUI checks remain useful evidence for solid modeling and face
attachment. The former rectangle tool fixed its dimensions at creation; current
drawing leaves freedom for explicit authoring.

- Created a native, fully constrained 40 by 30 mm rectangle with two clicks.
- Previewed and applied a 10 mm extrusion.
- Selected its top face and created a native face-attached circular sketch.
- Created a pocket, then reopened it and changed its depth to 5 mm.
- Reopened the attached sketch and set its radius to 3 mm.
- Reopened the base sketch and changed its width to 50 mm. The pocket stayed
  attached and recomputed from the changed parent.
- Selected an outer edge and previewed a 2 mm fillet. A 100 mm candidate failed;
  Apply stayed disabled and Cancel restored the original unfilleted body.
- Created and applied the fillet again. Ctrl+Z removed the entire feature;
  Ctrl+Shift+Z restored it. Ctrl+Shift+S saved a separate native FCStd file, and
  Ctrl+S saved subsequent edits.
- Reopened native files in the editor. Orbit, pan, Top, Iso and Fit changed the
  view without changing the native document revision.

An independent native worker reopened the GUI-created final FCStd. Both sketches
had zero remaining freedom. Native support was `Extrude2.Face6` at z=10 mm.
The pocket volume matched `50*30*10 - pi*3^2*5`. The final filleted volume matched
that result minus `50*2^2*(1-pi/4)`, at 14815.707963267949 cubic millimeters.
The native solid was valid. Its display mesh had 644 triangles, with one oriented
native normal per display vertex. This checks the saved model independently of
what the editor drew.

The debug editor used 82508 KiB RSS for this part. Its native Python worker used
97580 KiB, plus a 2768 KiB runtime launcher. All three measured zero CPU over
five idle seconds and zero swap. These figures describe one small part. They
are not a moderate-part performance gate or an Onshape speed comparison.

The earlier Windows application and the existing Pixel prototype were left
running. Their executables and original documents were not replaced.

## Automated checks

Windows x64 and Pixel Linux ARM64 have native workflow results recorded below.
CI repeats the Rust checks, including the optional desktop
target, and native workflows on Windows and Linux x64. The history/reference
batch has 119 Rust tests: 117 passed and two ignored. Formatting, strict
all-target Clippy and Windows desktop/all-binary builds pass. Twelve native
suites pass on Windows x64 and Pixel Linux ARM64. On Pixel, eleven passed in
the combined run; history passed its focused rerun after the nonsolid-volume
comparison correction described below.

- `native_workflows.py`: sketches, construction, dragging, arcs, native units
  and expressions, attachment remapping, transformed Bodies, save/reopen,
  fillets, failures and native undo/redo. Display checks cover outward and inward
  oriented normals, sphere poles, placement transforms and curve detail.
  Unicode labels, paths, Undo/Redo and continued edits are also covered.
- `preview_workflows.py`: candidate replacement from baseline, revision and
  owner rejection, worker-wide lease, failed candidate recovery, one-step undo,
  cancel and save/reopen. Native redo and rollback limitations are explicit in
  [PROTOCOL.md](PROTOCOL.md).
- `package_workflows.py`: typed package expansion through the same native
  document, constrained profiles, attached pocket, parent edits, fillet volume,
  failure atomicity, undo/redo and continued editing after reopen.
- `sketch_commands_workflows.py`: all six Rust drawing tools and 37 authored
  constraint cases run through the actual compiler and native solver. Checks
  compare solved freedom, native constraint types and typed dimensional values,
  including point/axis symmetry, tangency and zero-valued dimension seeds.
- `sketch_editor_workflows.py`: reference/driving, active/virtual and named
  constraints; native points and trim Undo/Redo; consumed profiles with open
  intermediate contours; failed Finish recovery; session ownership and drag
  previews; action Undo/Redo; cancellation after more than 30 actions; one native
  Undo entry on Finish; save/reopen and attached-pocket dimension edits.
- `body_workflows.py`: nine native Body/Boolean cases cover Union, Subtract,
  Intersect, multiple tools, transformed Bodies, native ownership/consumption,
  preview replacement/cancel, failures, Undo/Redo, parent edits and reopening.
  Raw Group placement and multiple-tool results are compared against a separate
  pinned FreeCAD reference rather than normalized to a different behavior.
- `history_workflows.py`: native Body Tip/null Tip and reordered Group/BaseFeature
  chains, visibility, moved Bodies, reference members and MultiTransform children;
  failed dependencies, guarded writes, Undo/Redo and save/reopen/edit.
- `sketch_operations_workflows.py`: shared Rust task compiler, native corner
  fillet/chamfer, two-curve fillet, split/extension, Carbon Copy expressions and
  spline degree/knots/poles/weights. Checks include analytical solid volumes,
  typed units, failed-batch rollback, preview replacement/Cancel, action and
  document Undo/Redo, persistent references and continued editing after reopen.
- Rust tests cover transport failure, actor document preservation after failed
  Open, committed edits followed by display failure, depth-aware picking,
  selection invalidation, sketch coordinates and transformed normals. Sketch
  checks include native handle/origin picking, multiselection, exact box/segment
  crossing, dimension label/line hits, attached-coordinate snapping and arc
  preview interpolation. Multi-object checks cover shared depth, distinct native
  identities, hidden-edge occlusion, per-object transforms and selection
  invalidation. Actor checks reject mismatched object/revision display replies
  without losing accepted edits. Lifecycle fixtures do not substitute for native geometry
  checks.

Run the two sketch suites after building the host and compiler:

```sh
cargo build --manifest-path next/Cargo.toml --locked
python next/tests/sketch_commands_workflows.py --host next/target/debug/cad-next --compiler next/target/debug/cad-next-sketch --runtime next/target/runtime.json --output next/target/sketch-command-documents
python next/tests/sketch_editor_workflows.py --host next/target/debug/cad-next --runtime next/target/runtime.json --output next/target/sketch-editor-documents
cargo test --manifest-path next/Cargo.toml --features desktop --locked
```

Use `.exe` for the host and compiler paths on Windows. Runtime setup is described
in [README.md](README.md).

## Remaining gates

The interactive tools do not yet cover all FreeCAD sketch tools. Advanced task workflows, assemblies,
broader history management and executable plugins remain open. Broader
constraint, selection, trim, dragging and attachment cases need GUI checks.
Feature previews defer recomputation until Apply when existing native redo would
otherwise be lost; a live preview is unavailable in that case, and Cancel
preserves redo.

Moderate parts need controlled input latency, regeneration and memory
measurements. The resource figures above remain the earlier small-part baseline;
they were not remeasured as a performance gate for the expanded sketch editor.
The full portable native runtime remains an evaluation dependency; slim packaging
and matched Rust component replacements have not been promoted.

## Native feature/property UI expansion

The Rust catalog exposes 57 native feature types backed by 60 cached native schemas (including Body, Sketch and imported Part::Feature).
This establishes creation/property access, not complete task-dialog parity.
`feature_workflows.py` uses the same Rust property compiler as the editor and
checks catalog/seed property names against native metadata. Windows x64 and
Pixel Linux ARM64 runs cover revolutions, grooves, lofts, sweeps, chamfer, shell,
draft, holes, patterns, mirror, MultiTransform, datums/binders, additive and
subtractive primitives and helices. Thirty-three saved models undergo a real
shape-parameter or parent-geometry change, changed-volume checks, Undo/Redo and
a second save/reopen. The suite also checks conditional threaded-hole fields,
read-only guards, atomic failed geometry, preview cancellation, guarded leaf
deletion, native vertex identities and container visibility.

Real Pixel interaction changed an existing pad to 12 mm, previewed 14 mm then
canceled back to 12 mm, and created a two-distance chamfer on native Edge4 with
Size 2 mm and Size2 0.75 mm. Independent native reopening confirmed the editable
features and references. The pad volume was 12937.5 cubic millimeters; the chamfer
volume was 12909.375, matching the triangular cut along the selected 37.5 mm edge.
The UI exposes conditional fields after native metadata refresh.

Native baseline caveat: the pinned FreeCAD Hole can normalize an independent
ThreadDepth of 5 mm to Depth 8 mm during native Undo/Redo, even with Dimension
thread-depth mode. A direct native-only reproduction matched the adapter. The
adapter preserves this behavior; the stable combined-options fixture uses equal
8 mm depths. Visibility also retains its native Output/NoModify behavior and is
handled by explicit view actions rather than parameter previews.

See UI-COVERAGE.md for remaining workflow gates. Complete sketch/task coverage, broader interchange, assemblies and executable
UI extensions remain unfinished. Advanced curves, external references and
MultiTransform child tasks now have dedicated checks below.

The Pixel UI also edited a native Scaled Factor to 1.8 through the numeric widget
and created a new scaled pattern from an existing protrusion. The saved new
pattern retained its Originals link, Factor 2, native Body Tip/display identity
and volume 896 cubic millimeters. Creation now explicitly publishes and shows
the new solid Tip after native link setters; existing-feature edits preserve
the task's original Tip and visibility.

A small debug Pixel document in a maximized 1080 by 2157 client used 112936 KiB
Rust editor RSS, 103300 KiB native worker RSS and 2752 KiB runtime shim RSS.
All three measured zero CPU during five idle seconds and no swap. The larger
client and different small model make this a new observation, not a direct
performance comparison with the earlier 1000 by 800 Body case. Moderate parts,
assemblies and interactive latency still require their own measurements.


## Advanced sketches, MultiTransform and exchange

`sketch_curves_workflows.py` runs against the actual Rust sketch compiler on
Windows x64 and Pixel Linux ARM64. It covers six advanced drawing tools, five
closed analytical curve profiles and native Pads, periodic and rational spline
parameters, exposed native spline pole controls and ellipse diameter lines,
conic Equal constraints and typed dimensions. Projected references retain their
negative native solver IDs and persistent mapping. Defining external profiles,
intersection flags, parent updates, reference Distance/Radius/Angle dimensions,
action/gesture ownership, invalid constructors, native Undo/Redo and editable
save/reopen all have native checks. The constructor preflight protects a pinned
Windows binding failure for malformed constraint names/signatures; it does not
replace native geometric validation.

Real Pixel interaction created an ellipse, dragged its center, exposed its
native internal axes and dimensioned the major diameter to 10 mm. It also drew
a four-pole spline, exposed native pole circles, marked the spline construction,
finished the sketch and padded the ellipse 10 mm. Independent native reopening
confirmed the editable sketch, constraints and Body Tip, and volume
626.8749173022981 cubic millimeters matched pi times the solved radii and height.
This verifies those operations; it does not establish complete Sketcher parity.

`transform_workflows.py` sends the shared Rust property compiler's ordered child
commands to native MultiTransform. It checks all four child types, repeated and
failed preview replacement, adding/removing/reordering stable native children,
Commit/Cancel, one native Undo/Redo, downstream Tip/visibility preservation and
save/reopen/edit.

Actual Pixel GUI interaction created a MultiTransform with a Linear step, canceled
back to the original 816 cubic millimeters, then applied 832. Native Undo/Redo
restored 816/832. Adding a Mirror about XZ was canceled at 832, then applied at
864. Reordering Mirror before Linear retained 864; removing Mirror produced 832,
and Undo/Redo restored the exact child order and deletion. After real Open, editing
the saved Linear step from two to three occurrences produced 848. Independent
native workers checked eleven saved private copies, including ordered
Transformations, child ownership, Originals, Body Tip and visibility. This covers
those GUI paths; other transform combinations retain their own acceptance gates.

`exchange_workflows.py` checks native STEP hierarchy and placed
root round trips, nine IGES faces as surfaces, subsequent native cuts with
volumes 120 -> 105 -> 90, atomic failed imports, Undo/Redo, revision/lease guards,
original file hashes and deliberate export replacement. These suites complement
real GUI coverage and do not assert full task-dialog or exchange-format parity.

Run the added suites with the same pinned runtime:

```sh
python next/tests/sketch_curves_workflows.py --host next/target/debug/cad-next --compiler next/target/debug/cad-next-sketch --runtime next/target/runtime.json --output next/target/sketch-curve-documents
python next/tests/sketch_operations_workflows.py --host next/target/debug/cad-next --compiler next/target/debug/cad-next-sketch --runtime next/target/runtime.json --output next/target/sketch-operation-documents
python next/tests/transform_workflows.py --host next/target/debug/cad-next --compiler next/target/debug/cad-next-packages --runtime next/target/runtime.json --output next/target/transform-documents
python next/tests/exchange_workflows.py --host next/target/debug/cad-next --runtime next/target/runtime.json --output next/target/exchange-documents
```


Pixel GUI exchange checks imported STEP and IGES, exported selected objects,
saved FCStd, and reopened the STEP-derived document. Independent native imports
confirmed the GUI STEP result at 150 cubic millimeters and the IGES result as
nine faces with zero solids and area 265.398223686155 square millimeters. A
visible native ShapeBinder in the active Body supplied Edge12 to a sketch.
A point snapped to that projected endpoint produced one Coincident constraint
and zero remaining freedom. After a temporary second point and Cancel, the
separately saved/reopened native sketch snapshot exactly matched the accepted
baseline. Visible binders are rendered separately from the solid Tip so their
native faces/edges can be selected. Direct external geometry outside a Body's
native scope is still rejected; the tested path creates a binder explicitly.
Pinned native Sketcher rejects external sources outside its allowed Body scope;
it has no automatic outside-Body choice dialog. PartDesign feature-reference
picking has separate independent/dependent/cross-reference choices. Broader
Binder/projection variants and those PartDesign property-row choices remain
coverage gaps.


## Native sketch tasks and file browser

The native sketch-operation suite passes on Windows x64 and Pixel Linux ARM64
through the production `cad-next-sketch` compiler. Rounded and chamfered
profiles become editable native Pads with analytical volumes; dimensions remain
editable after reopen. Carbon Copy source dimensions drive the copied profile
and its solid. Spline pole and rational weight edits change the resulting solid,
while degree and knot edits retain native history. Arc extension keeps persistent
external references and restores its baseline after failed batches, preview
cancellation and sketch-session cancellation.

The adapter registers native Geometry assignment before extending an arc because
the pinned method otherwise mutates its handle without registering native undo.
It does not replace Sketcher geometry or transaction ownership. Native degree
reduction can replace a spline and delete its constraints. Knot reduction/removal
can approximate geometry and retarget controls; the fixture checks reversal of a
just-inserted knot. Weight edits release
only explicitly chosen Equal constraints on the selected native pole control.
These are functional limits callers must handle, not a claim of full Sketcher
parity.

Actual Pixel GUI task checks were saved to disposable FCStd copies and reopened
by a separate production native worker without edits or saves:

- A 2 mm fillet accepted a native endpoint selection. Cancel restored the baseline;
  Apply, Undo/Redo, Finish and Save as retained a valid Pad of
  495.707963267949 cubic millimeters. A separate Preserve construction corner
  case retained four lines, the arc and one native construction point, with two
  Tangent and two PointOnObject constraints. The radius field seeds the native
  arc; it does not automatically add a Radius constraint.
- A 2 mm sketch chamfer produced a valid Pad of 490 cubic millimeters. Split
  added a fifth profile curve while preserving the 500 cubic millimeter Pad.
- Line and arc extension produced a 30 mm line and a 120 degree arc.
- Moving spline pole 2 to [2, 4, 0] produced a 63 cubic millimeter Pad. Weight 0
  was rejected with Apply disabled. Changing the weight to 2 and explicitly
  releasing Equal 5 recovered a valid preview and produced
  69.1594553908154 cubic millimeters. After Undo, opening and canceling a deferred
  task preserved Redo; Redo, Finish and Save as retained the accepted weight.
- Canceling a weight task after exposing controls, switching poles and toggling
  an Equal release restored the exact native baseline sketch and expressions:
  two geometries, no exposed controls and Pad volume 54 cubic millimeters.
- Degree elevation to 4 and insertion of knot 0.5 preserved the native Pad volume
  of 54 cubic millimeters. Carbon Copy retained four copied curves and native
  dimension expressions after reopen. Source-dimension propagation also has
  native suite coverage; this GUI case checked copying and persistence.

Fresh forms default Preserve construction corner to checked, matching the pinned
native Fillet and Chamfer tools. Applying their 1 mm defaults produced valid
Pads of 498.926990816987 and 497.5 cubic millimeters, respectively, retaining
native construction corners. After document Undo and sketch reentry, the chamfer
task deferred recomputation. Canceling the task and sketch preserved document
Redo, which restored the exact saved fillet sketch and expressions. Repeating
the sequence and applying the chamfer correctly branched history and survived
Finish, Save as and independent native reopen.

These checks cover the listed inputs and controls. Degree reduction, knot
reduction/removal, additional curve/corner selections and broader task
combinations retain their own GUI gates.

Open/Save as and STEP/IGES Import/Export share a filtered modal file browser.
Fourteen real Pixel GUI checks cover native format filtering, search/list picking,
directory Up/Back navigation, new native filenames with retained FCStd extension,
existing-destination confirmation, cancellation, unsaved-change protection and
native reopen. Canceling an existing native overwrite left its SHA256 unchanged;
explicit confirmation saved it without a second application prompt.

A filtered IGES import merged native geometry into the existing document.
Export Browse switched the destination format to STEP while retaining both
selected native objects. Independent native reopening checked the saved one-box
and merged two-object documents; independent STEP import checked one solid and
nine surface faces, fifteen faces total. Browser overwrite confirmation did not
automatically enable the application's explicit export replacement checkbox.
Cancel left the existing output hash unchanged. The library returns a path and
no overwrite-authorization flag, so export keeps this separate explicit choice.
Earlier GUI exchange checks above used the previous path dialog.

After browser interaction, the maximized debug editor with a saved 150 cubic
millimeter box used 96348 KiB RSS, the runtime launcher 2708 KiB and the native worker
99388 KiB. All three measured zero CPU during five idle seconds with the browser
closed and no swap. This is another small-part observation, not a moderate-part
resource gate or a comparison against a different document/client size.


## Native Body history and reference selection

`history_workflows.py` uses the production Rust history compiler for Tip rollback,
null Tip, resume and native feature insertion. It checks the native Group and
BaseFeature chain, visibility, moved Bodies, insertion while rolled back, Undo/Redo,
preview/lease/revision rejection and save/reopen/edit. Reordering a construction
sketch or datum preserves the solid chain. Reordering a MultiTransform child in
Body history preserves the parent's separate Transformations order. Native
attachment dependencies and expression cycles reject atomically.
History shape comparisons retain solid volumes and all shapes' topology, area,
bounds and validity. Native unbounded origin planes can return unstable kernel
pseudo-volumes after Redo on ARM64; volume is compared only for solids. The raw
snapshot is unchanged, and failure diagnostics now identify the full field path.

Part Fillet/Chamfer's Use selected edges validates each selected object's native
identity against the effective Base before accepting EdgeN. A foreign object's
matching edge number cannot silently alter the current Base. Native Body/current
Tip aliases are accepted; 96 matched native comparisons across both operations,
all 24 tested edges and two placement frames retained identical geometry. Keeping
the Body as Base preserves its world placement. Rust checks cover error rendering,
unchanged inputs on rejection and explicit Base overrides.

The worker reads NDJSON requests as UTF-8 independently of Windows' locale. A
production-host native case checks Unicode labels through Undo/Redo and saves,
reopens and continues parametric editing in a path containing accented letters,
non-Latin characters and an emoji.

Native canonical component expression paths, including the leading self-relative
dot in `.AttachmentOffset.Base.z`, support Set/Clear, Undo/Redo and save/reopen.
The underlying property permission check still rejects computed/internal
properties and malformed empty roots. The native Windows workflow verifies
these cases alongside Unicode round trips.

Fourteen real Pixel history cases were saved separately and reopened by an
independent native worker. They cover rollback to Base, null Tip, resume,
Undo/Redo, native reordering with an explicit final Tip, cancellation preserving
Redo, moved Bodies, construction-sketch/datum ordering, and atomic rejection of
a feature move that would break an attached dependent. Group order, BaseFeature
links, visibility, Tip, attachment, placement and solid geometry matched the
native expected states. Null Tip removed the previous solid from the viewport.

Four saved Part Fillet/Chamfer GUI cases passed independent native checks.
Picking `Right.Edge5` while Base was Left displayed an error and disabled Use
selected edges, preserving the existing Base, edge index and radii. Cancel
restored both original boxes. A 1 mm fillet on Left.Edge5 produced volume
997.8539816339744 cubic millimeters; explicitly changing Base to Right accepted
Right.Edge5 and produced the same volume. A 1 mm Right.Edge5 chamfer produced
995 cubic millimeters. Source-box geometry and native edge identities were
preserved. These cases complement the matched Body/Tip alias checks above;
they do not establish every history or edge-selection combination.

With the history task and browser closed, the saved two-box/Part Chamfer GUI
case used 97136 KiB RSS for the debug Rust editor, 2700 KiB for its runtime launcher
and 121108 KiB for the native worker. All measured zero CPU and swap during
five idle seconds. This small-part observation uses a different model from the
earlier browser baseline and does not establish a performance comparison.
The final Pixel native rerun also passed the canonical expression-path cases.


## Native sketch supports and foreign profiles

`sketch_creation_workflows.py` uses the same Rust sketch-creation compiler as the
editor. Windows and Pixel Linux checks pass for all three origin roles despite colliding object
names, atomic new Body creation, Undo/Redo, moved Bodies, same-Body datum planes,
existing sketch frames and native planar-face attachment. Saved solids reopen
and remain editable; linked parent changes update attached sketches and Pads.
Foreign supports, axes, malformed origin payloads and curved faces reject without
silently creating an unbound XY sketch. A failed new-Body batch restores the
previous native document.

`profile_reference_workflows.py` uses the production native property compiler and
preview lease. Fourteen workflows on Windows and Pixel Linux cover Independent/Dependent copies and
Cross-reference, both creating a Pad and editing a consumed profile before a
later feature. Independent fixtures include local and explicit cross-object
expressions, attachment, external geometry, shifted constraint indices, nondefault
native properties, an exposed rational spline and moved Bodies. Checks cover
preview replacement, Cancel, Apply, atomic failure, Undo/Redo, parent/copy edits,
final Tip/visibility and native save/reopen. Separate native inspection verifies
expression comments, constraints, spline metadata and external-cache cleanup.
Source snapshots and original fixture hashes remain unchanged.

Six additional expression-history cases cover a native Sketch, a native Part Box
and an independent copy, each restored by Redo creation or Undo deletion. After
native expression rebinding, parent edits propagate without a file reopen.
Comments and native history counts are preserved, including a second pending
Redo. Seven malformed reference requests, stale revisions, failed copy batches
and preview ownership guards also pass. These are bounded restoration checks;
native history and the expression engine remain native components.

The independent-copy adapter deliberately avoids three pinned task-copy defects:
source external-constraint pruning, copied external-cache resurrection and stale
copied expression dependencies. It uses native serialization/setters and checks
the resulting solved geometry. At this checkpoint, independent defining/intersection
materialization and custom dynamic properties reject atomically. The later
circular-materialization gate below extends the former; broader coverage remains open. Native fixtures complement the separate real GUI gates;
they do not establish complete reference-task or FreeCAD parity.


Real Pixel interaction verifies chooser Cancel preserves an existing Redo branch,
then creates, draws and extrudes native XZ and YZ sketches. Independent native
reopening checks actual origin supports, solved constraints and analytical Pad
volumes, with saved-file hashes unchanged. A sketch on a datum in a moved and
rotated Body follows a parent height edit from 10 to 14 mm; its native global
position is [96, 203, 314] and its solid matches the analytical volume. A GUI
regression found stale support selection after creation. The corrected build
selects the new Sketch across tree/object/viewport selection state; Datum to
Rectangle to Finish to Extrude now seeds the new Profile without manual
reselection and produces a valid preview and committed solid.


The full attachment GUI set contains seven saved outcomes. Existing Sketch
ObjectXY support retains its local 30-degree rotation combined with its Body's
90-degree rotation. A moved Body's Base.Face6 supplies native planar support.
Selecting a cylindrical face reports the native planarity error and retains the
chooser; explicitly choosing XY recovers, and sketch Cancel leaves only the
original cylinder with volume 785.3981633974483 cubic millimeters. All four
immutable fixture hashes remain unchanged.

These GUI checks exposed a browser layout regression with the compact app
spacing: Save As grew 2 logical pixels per frame, moving its action buttons.
The browser now uses at least 8 pixels of horizontal spacing only during its own
render and restores the original application style afterward. The pinned dialog
subtracts that spacing from the filename field before adding an 8-pixel margin.
A headless rendering regression covers Open/Save/Import/Export across repeated
frames and three changing viewport sizes, including stable bounds, viewport
limits and exact app-spacing restoration. The dialog remains resizable.

All fourteen native workflow suites pass on Windows x64 and Pixel Linux ARM64
for this batch. The Pixel build verifies every source-manifest hash before
building. A real Save As interaction confirms stable dialog bounds and action
buttons across three screenshots after entering a full filename; the Save
button succeeds by mouse click.

The desktop editor embeds the Rust WorkerSession directly. Its small Linux
launcher child is part of the native runtime launch chain, not a separate Rust
CLI host. The earlier browser/history memory labels above have been corrected;
the separate headless Rust CLI measurements describe a different process setup.


Twenty-six saved profile-reference GUI cases pass independent native inspection.
Real interaction covers all three modes, repeated preview replacement, candidate
selection rejection, Cancel preserving an existing Redo branch and opening an
existing foreign Profile without silently copying it. Editing a consumed profile
preserves the downstream Finish Tip. Switching back to a local Profile removes
the generated candidate and reference choices, and Cancel restores the baseline.

After creation Undo/Redo, editing an independent copy's width from 10 to 12 mm
updates its local expression to height 6 mm and Pad volume 360 cubic millimeters;
the original remains 10 by 5 mm. An explicitly cross-object Master.Length
expression remains linked: a parent edit produces copied Pad volume 480, parent
Undo restores 400 and Redo returns 480. Dependent profile edits likewise update
the downstream solid after history operations. Native expression comments,
external caches and original fixture hashes remain intact. These checks cover
bounded workflows, not complete FreeCAD UI parity.

For the saved small two-Body case with forms and browser closed, five idle seconds
measured 124136 KiB RSS for the debug Rust editor with embedded WorkerSession,
2968 KiB for the runtime launcher and 115748 KiB for the FreeCAD Python worker.
Executable paths and command lines establish those process roles. All measured
zero CPU and swap. This is a small-part observation, not a moderate-complexity
performance gate or a release-build speed comparison. The owned Pixel test app
was closed cleanly; other running apps and original documents were preserved.


## Circular dimensions, native endpoint identities and independent copies

The shared Rust dimension builder and viewport witnesses add explicit circular
arc length/span, point-to-circle, circle-to-line and circle-pair boundary
clearances. Smart Dimension keeps its prior selection choices. Ten Windows
native workflows cover both arc orientations, signed overlap/containment and
subpixel clearance, including previews, Cancel/Apply, expressions, units,
reference/driving state, Undo/Redo and save/reopen. A separate native zero
reference clearance survives history and reopen without changing the circles.
The pinned native API rejects assigning a zero driving Distance datum; this
adapter preserves that restriction.

Three additional cases cover external arc parent propagation, native clockwise
endpoint dimensions and consumed-profile sketch sessions. Seven snapped-arc
cases exercise both directions and sweep sizes, shifted/rotated coordinates,
Coincident and whole-curve PointOnObject targets, action/document Undo/Redo and
reopen. The old endpoint mapping could produce a native-valid distorted arc:
one minor arc length changed from 7.8539816 to 34.1810334. With the shared Rust
snapping builder its input endpoints and trace remain intact. Snapshot handles
now use native Sketch.getPoint identities, while raw curve samples preserve
tracing order. The editor and agent CLI use the same edit builder.

Independent review matched 52 circular clearance seeds against native reference
measurements with unchanged geometry. It also found zero/subpixel dimensions
lost their label and hit target. The corrected Rust viewport supplies a finite
fallback direction and short display leader. A regression checks native
constraint identity, finite label/leader coordinates and label picking for
point-circle, circle-line and circle-pair gaps at zero and +/-0.01 mm, both
ordinary and attached frames. All 140 active Rust tests and strict Clippy pass.

Five Windows native independent-copy workflows cover a defining circle, a
circle from genuine cylindrical-face intersection, both in moved Bodies, and an
already-internal curve carrying legacy external identity. The copy materializes
defining curves before removing links. It removes ExternalGeometryExtension
identity from every copied internal curve, retains native Sketch geometry IDs,
constraints, local expressions and comments, and leaves the source untouched.
An earlier candidate with an empty OutList still retained external identity and
was rejected by native extension inspection. An empty link list alone is not
sufficient evidence of independence.

Each copy workflow checks preview replacement/Cancel, Apply, atomic failed edits,
Undo/Redo, source-parent isolation, copy edits and reopen. Analytical volumes
are 234.292036732051 initially, 344.292036732051 after Width 12 drives Height 6,
and 297.168146928204 after the copied circle radius becomes 2. Live source
serialization remains unchanged during copying. Native save/restore can
normalize Shape.Map serialization, so restored source bytes are compared with
an independently restored source-only control. Original geometry IDs and
expression comments also receive native inspection. These circular fixtures
do not establish broader defining/intersection curve and constraint coverage.

All sixteen native suites pass on Windows x64 and Pixel Linux ARM64 for the
initial frozen batch. The later change only fixes viewport annotation and adds
zero/subpixel acceptance cases; its updated common-dimension suite passes on
Windows. Real GUI acceptance and the final Pixel check are recorded below.

The actual Pixel ARM64 editor and CLI binaries built from the verified source
manifests. All sixteen native suites passed on the initial freeze; the updated
common-dimension suite passed on the second freeze. Real defining-copy GUI
interaction covered repeated preview replacement, Cancel, Apply, source radius
changes, independent copied width/radius edits, a local height expression,
document Undo/Redo and save/reopen. Five saved checkpoints passed separate
native inspection. Original fixture hashes stayed unchanged.

Remaining GUI gates include circular dimension creation/editing and zero/subpixel
label picking, snapped clockwise arcs, and further intersection/transformed-copy
cases. The Pixel desktop listener became unavailable, then USB disconnected.
No session restart or process termination was attempted. These remaining gates
have not passed.

## Rust sketch-action journal and revision registry

Rust owns accepted sketch-action groups, cursor, branch truncation, staged
acceptance and replay plans. It also owns the worker-local revision map and
monotonic clock, including malformed/missing/stale token checks and close/name
reuse. Python still chooses native publication points and executes native replay,
transactions, recomputation and solving. These are bounded ownership ports,
not a replacement document or geometry kernel.

Matched streams cover native sketch and consumed-profile action history,
preview replacement and failures, multi-document writes, ownership, reopening
and injected recovery failures. Five lifecycle serialization corrections keep
ownership/history intact or advance the revision after native restoration.
The public revision suite covers 310 guarded responses, and the lifecycle suite
covers eight failure cases. Strict comparison checks whole response structure
and native identities. Corruption controls reject changed geometry, ownership,
constraints and resource measurements.

Request and callback pipe writes share the original deadline through a bounded
writer thread. Stalled-reader tests cover ordinary requests and replay callbacks.
On Linux, timeout cleanup terminates the private launcher process group and its
native descendants. The regression passed in Linux CI.

The combined root journal/revision build passed matched headless Windows debug
resource gates. Median combined working memory was 79.65 MiB baseline and
80.09 MiB replacement; peak memory was 80.14 and 80.76 MiB. Warm startup was
622.56 and 631.07 ms. Six samples of at least five idle seconds measured zero
CPU. The workload uses a 12-hole Pad, 16 geometry items, 47 constraints, 64
accepted actions and 32 action Undo/Redo operations. These warm-cache small-part
measurements do not establish viewport speed or moderate-part performance.
Real desktop acceptance of the ownership extraction remains open.

## Native refraction and tangent/perpendicular via a point

The shared Rust compiler exposes native SnellsLaw creation and datum/expression
editing, and native Tangent/Perpendicular constraints through a third selected
point. Native preparation adds missing relationships, solves between preparation
steps and preserves native constraint/expression renumbering. Spline via-point
preparation removes only the native procedure's matching point-on-spline
constraints after substitution, preserving unrelated constraints.

Twelve refraction workflows, 42 via-point workflows and 35 compatibility cases
pass. Coverage includes signed ratios, reference/inactive constraints, native
quantity behavior, external and transformed parents, consumed profiles, failed
replacement recovery, action/document history and reopen. An independently
rejected native spline fixture is recorded as a native limitation, not support
for arbitrary spline refraction. AngleViaPoint and axis-curve selection remain
outside this change.

Independent combined review found no demonstrated P0/P1/P2 defect. It verified
two combined native workflows with expression remapping, eleven rejection cases,
two direct FreeCAD geometry/constraint comparisons and 112 headless egui selection
clicks. Coincident Shift selection can add and remove every overlapping handle
while preserving selection order. Real GUI acceptance remains pending.

At commit 2223af0, all 21 public native suites and the Rust checks passed on both
Windows x64 and Linux x64 in CI. The local desktop-enabled Rust suite passed
160 active tests with two ignored native-runtime tests, alongside strict Clippy
and all binary builds. This establishes the listed native/CLI workflows, not
complete FreeCAD application parity.

The next bounded extraction moves gesture tokens, worker-wide leases,
active-sketch edit admission, staged preview ownership and poison state to Rust.
Python retains native replay payloads and sequencing. The desktop-enabled root
build passes 169 active Rust tests, two ignored native-runtime tests, strict
Clippy and five Python callback transport tests. Thirty native getter-order
cases pass with 177 responses. They verify label rejection before getter access,
lazy redo admission, retained snapshot reads and recovery. A two-callback timeout
test verifies the handshake uses one deadline. Independent re-review found no
P0/P1/P2 defect and reproduced the prior getter-order failure as fixed. All 23
current native suites pass on the root lease build. The later internal-point
copy correction has separate acceptance below.

Fresh matched resource gates pass for the actual combined root host and worker.
Combined working memory was 80.35 MiB baseline and 80.43 MiB replacement; peak
memory was 80.86 and 81.06 MiB. Warm startup medians were 627.83 and 625.50 ms.
Transaction, action Undo/Redo, preview, Cancel and Finish latency medians pass
the baseline-times-1.25-plus-2-ms gate. Working and peak memory each stay within
baseline plus 32 MiB. Every metric is finite and nonnegative. Six measured idle
samples of at least five seconds report zero CPU. The workload and headless
Windows debug limits match the earlier journal/revision gate. Root source and
binary hashes remained unchanged during measurement. This does not establish
viewport performance or real desktop acceptance.

Projected standalone vertices now retain their native persistent reference and
flags in snapshots despite the pinned Python Point wrapper dropping extensions.
The correction reads native serialized property metadata into detached points;
it never restores or rewrites the document. Six root native workflows pass,
covering mixed caches, equal coordinates from different sources, parent changes,
missing-source history, shared Rust via-point commands, projection deletion and
independent copies of projected/defining points. Native defining points become
independent internal geometry without changing the source. Unknown metadata
stays explicit and blocks source-dependent destructive edits. Projected-point
GUI picking remains unverified. A separate read-only native verifier checks all
twelve saved root files against raw native XML references/flags and native point
handles. It also checks Missing-source caches and two simulated metadata failures.
All twelve files retain their original hashes. The existing five circular-copy
workflows still pass with the correction.

Independent point review then found two inherited copy gaps. An already-internal
point could retain hidden external identity, and cleaning another materialized
curve could lose an internal point's native ID/construction state. The follow-up
restores full native internal-point records before identity detection and native
cloning. Three self-contained public workflows now pass for a materialized point,
mixed ordinary point and mixed construction point. Raw native metadata checks
compare IDs, modes, constraints, expressions and all other persisted extension
attributes. They also verify source bytes, preview/Cancel, failed-copy rollback,
Undo/Redo and three saved/reopened file pairs. The six projected-point and five
circular-copy workflows still pass after this follow-up. Independent re-review
found no remaining P0/P1/P2 defect. It reproduced both fixes on the original
inputs, checked six saved source/result pairs and exercised unavailable metadata,
rollback recovery and nondefault native geometry flags/layers. Real GUI
acceptance remains open. Cross-platform CI runs all 24 public native suites.

The private AngleViaPoint candidate adds the self-contained
`tests/angle_via_point_workflows.py` suite using the public host and the same Rust
compiler as the editor. Its 31 workflows pass, with 29 direct pinned-native
comparisons. These cover native preparation and curve order, all existing curve
families, one/two-spline cleanup, both-fixed reference mode, axes, exposed points,
signed and multi-turn native datums, expressions/state, failed preparation and
replacement, source updates, consumed profiles, history and reopen. Numeric
comparisons retain the existing public native-workflow tolerance; no broader
tolerance was added. The underconstrained two-spline exploratory fixture showed
native solver drift and is retained in private failed evidence. The deterministic
two-spline fixture fixes both curves and checks native reference/cleanup policy.

The construction-point-only both-axes fixture has a matched native transient
`_ElementMapVersion` sequence: empty on open, `15.70200.5` after preparation,
retained after abort/Undo, empty on reopen. The consumed sketch has native
`Touched` status after abort plus sketch-only solve/recompute, then returns to
Valid after document recompute. Each oracle checks only that exact fixture and
phase transition; unrelated metadata/version/status mutations must fail. Full
graph, property, geometry, constraint and identity comparisons remain enabled.
The final private Rust desktop library run passes 173 tests, with two ignored;
all-target desktop Clippy passes with warnings denied. No actual desktop/Pixel
acceptance, broad regression matrix or resource comparison was run by this
candidate author. Independent review and root integration remain required.

Independent Angle review found one P2 selection-lifecycle defect: selecting the
temporary native Angle row and canceling left its index selected, so a later
unrelated Radius reusing that index inherited the selection. A private follow-up
keeps selection state inside the Angle dialog lifecycle. It checks the native
slot/type/geometry-point references on refresh, preserves a still-valid selected
row on failed or successful Apply, and restores a verified prior row after
Cancel. A replaced or absent native row clears both its selection and name.
Deferred cancellation performs no request and preserves Redo; immediate
reference creation cannot inherit an old row selection. This is not a global
selection reset and does not invent persistent constraint IDs or remap indices.

The exact reported editor sequence fails against the unchanged prior candidate
and passes with the fix. Eight focused tests drive actual editor methods,
headless Escape/Enter, replies, native row selection and revision/owner request
fields using full captured public-host snapshots. Four existing dialog tests
also pass. Four focused native sequences cover Cancel/index reuse, failed
replacement/commit, spline cleanup and deferred/reference creation. A direct
native abort plus sketch-only recompute control checks the exact Touched phase
and restored geometry/IDs/constraints/formulas. No backend/native policy changed,
no full native matrix or resource run was repeated, and desktop/Pixel acceptance
remains pending. The new frozen follow-up requires independent re-review.

## Pad and Pocket termination workflows

The task schema now follows pending modes rather than only a previous successful
native recompute's conditional property flags. Native task setters remain scoped
to Pad/Pocket; computed data and other feature types retain read-only protection.
The property compiler applies controllers before targets and dimensional values.
Pocket retains its pinned native 5 mm default. No expression evaluator, geometry
engine, native history replacement or runtime prototype probing was added.

The three public extrusion suites cover both sides, face/shape termination,
offsets, taper, custom/reference direction, expressions, placements, parent and
topology changes, dormant values, preview replacement/Cancel, failures, Undo/Redo
and reopen. Their saved results are compared with direct native geometry and
properties. Independent review found and verified the fix for an expression-mode
target that remained hidden after a failed incomplete preview. Further schema
and native probes preserve exact expression-clear keys and unrelated read-only
properties. These are headless native/shared-editor-command checks. Desktop
interaction and broader imported/legacy geometry remain unaccepted.

The combined root implementation passes 183 Rust tests, two separately enabled
native editor tests, callback tests, Clippy with warnings denied, the desktop/bin
build and all 29 native workflow suites on Windows. The native editor tests run
the actual form methods, worker actor and headless egui pointer controls for mode
changes, target buttons, exact expression clears, Apply/Cancel and deferred Redo.
They do not initialize OpenGL, open a desktop window or pick geometry on screen.
CI runs both these tests and the native suites on Windows and Linux.

Fresh interleaved debug headless resource checks compare the preserved baseline
with this combined source using the same 12-hole Pad and sketch-action workload.
Working set medians are 80.96 and 80.48 MiB; peak medians are 81.43 and 80.96 MiB.
Warm startup medians are 693.87 and 687.36 ms. All memory/startup/operation latency
gates pass and all six five-second idle samples measure zero CPU. These samples
establish a bounded regression result, not viewport or input-to-frame speed,
moderate complex-part performance, cold startup or real GUI acceptance.
