# Editor verification

Verified October 20, 2026. The Rust editor now includes interactive native
sketch editing. This record does not establish complete Part Studio, FreeCAD
sketch-tool or Onshape parity, or a replacement release for the earlier Windows
application.

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
target, and native workflows on Windows and Linux x64. The verified sketch-task
and browser batch has 110 Rust tests: 108 passed and two ignored. Formatting,
strict all-target Clippy and Windows desktop/all-binary builds pass. All eleven
native suites listed below pass on Windows x64 and Pixel Linux ARM64.

- `native_workflows.py`: sketches, construction, dragging, arcs, native units
  and expressions, attachment remapping, transformed Bodies, save/reopen,
  fillets, failures and native undo/redo. Display checks cover outward and inward
  oriented normals, sphere poles, placement transforms and curve detail.
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
feature-tree rollback/reorder and executable plugins remain open. Broader
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
millimeter box used 96348 KiB RSS, the Rust host 2708 KiB and the native worker
99388 KiB. All three measured zero CPU during five idle seconds with the browser
closed and no swap. This is another small-part observation, not a moderate-part
resource gate or a comparison against a different document/client size.
