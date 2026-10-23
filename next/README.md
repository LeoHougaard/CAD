# FreeCAD-based replacement

This is the replacement foundation. The earlier Rust/Brepkit application stays
available while this implementation is verified. It is not the modeling engine
for this version.

The first backend is pinned FreeCAD 1.1.4, including its Part, Sketcher and
PartDesign application modules and Open CASCADE. Rust owns the application-facing
interface. A small, local worker adapter initially calls FreeCAD's supported
Python API. It does not run FreeCAD's GUI or a network service. This interim
version is mixed-language; the eventual target remains Rust throughout,
including the geometry kernel. FreeCAD App, Sketcher, PartDesign, Open CASCADE
and the Python adapter remain native. Rust presentation and command construction
do not constitute replacements for those components.

FreeCAD supplies the functional reference while components move completely to
Rust one at a time. Each replacement must cover its full declared behavior and
pass matched verification before promotion.
UX customization and performance tuning follow functional Rust completion.
Resource checks remain regression gates during migration. The pinned native
baseline is the reference; the current Rust UI does not yet expose all of it.

This includes an initial Rust desktop editor and the headless modeling host.
The editor is under workflow verification. It has not replaced the earlier
Windows application. The [migration gates](MIGRATION.md) describe the remaining
work toward a complete Part Studio. The [component inventory](COMPONENTS.md)
records what is Rust, what remains native and what each replacement must prove.

The [Part Studio and Assembly checklist](COVERAGE.md) records 306 scoped Onshape
help topics and 1,378 open requirements. Its implementation evidence is pinned
to `15703c5`; later verified changes are recorded in [VERIFICATION.md](VERIFICATION.md).
The checklist records remaining work and acceptance gates. It does not establish
full feature coverage or Rust modeling parity.

## Run the editor

Build the optional desktop target and use the same pinned runtime configuration:

```sh
cargo build --manifest-path next/Cargo.toml --features desktop --locked
next/target/debug/cad-next-editor --runtime next/target/runtime.json
```

Use `.exe` on Windows. `--open part.FCStd` opens an existing native document.
Run this as a separate application; it does not replace `dist/CAD.exe`.

Choose Sketch, then XY, XZ or YZ for native Body-origin attachment. Canceling
the chooser makes no document writes and preserves Redo. A selected same-Body
datum plane, existing sketch frame or planar solid face supplies native support
directly. Origin roles are resolved by the native Body; generated plane names
are never guessed. Attached reverse/offset values remain available through native
properties. The standalone Sketcher orientation dialog and broader attachment
choices remain open coverage.

Point, Line, Polyline, Rectangle, Circle and three-point Arc draw native geometry. Drawing leaves position and size free for dimensions
and constraints. Endpoints, centers, the origin and curves provide snapping;
lines infer horizontal/vertical constraints. Hold Alt to disable snapping and
inference. Construction or G toggles construction geometry.

Select points or curves, then use Constrain for horizontal, vertical, coincident,
point-on-object, parallel, perpendicular, tangent, equal, symmetric or fixed
constraints. D chooses a length, radius or angle from the selection; the Dimension
menu also offers explicit horizontal/vertical distances and diameter. Explicit
Length measures an arc length or circular boundary clearance; explicit Angle
measures an arc span. Smart Dimension retains its existing selection choices. Double-click
a dimension to edit it. The constraint panel supports native expressions,
driving/reference dimensions, names, activation and deletion. The native solver's
solved geometry and remaining freedom are authoritative.

Tangent and Perpendicular also accept two whole curves and one exposed native
point. The same selection with D or Dimension > Angle creates a native angle
through the point. Click either sketch axis away from other geometry to select
its whole native curve. The origin remains a point selection. Native preparation
and measurement seed the dimension dialog; Apply accepts a datum, expression or
reference angle, and Cancel restores the sketch. Two fixed curves create a
reference angle directly. When Redo exists, preparation waits for Apply so
opening and canceling the dialog preserves it. This workflow has native and
headless Rust checks; desktop interaction remains unverified.

For refraction, select two line endpoints in n1, n2 order and a whole
boundary curve, then choose Refraction ratio n2/n1. Native Sketcher validates
the ratio and prepares missing relationships. At coincident handles, repeated
Shift-clicks visit each handle to add it, then visit each again to remove it.

Shift-click adds or removes selections. Drag a blank area for a selection box;
a right-to-left box also selects crossing curves. Shift adds a box to the current
selection. Drag endpoints, centers or curve handles to edit through native
Sketcher. Delete removes selected geometry; Trim uses the native picked-segment
operation. Edit tools adds native sketch fillet/chamfer, split, line/arc extension,
Carbon Copy and B-spline degree, knot, pole and weight tasks. Select an internal
curve or corner first; Split curve uses the picked position. Task Apply accepts
one sketch action and Cancel restores its baseline. Carbon Copy keeps native
dimension expressions linked to its source sketch. Spline weight changes require
explicitly releasing any conflicting Equal constraints on the native pole circle.
Degree reduction can replace geometry and remove its old constraints; inspect the
new solved snapshot afterward. Pixel GUI checks and independent native reopen
checks cover fillet/chamfer, split/extension, Carbon Copy and spline degree, knot,
pole and weight tasks, including invalid-weight recovery and Cancel/Redo. These
cases do not establish every operation or selection combination.
Ctrl+Z and Ctrl+Shift+Z walk accepted actions within a sketch. Finish
recomputes the whole model and commits one document undo step; Cancel restores
the sketch session baseline. An invalid consumed profile remains in edit after
Finish fails, so it can be repaired, undone or canceled.

After Finish, choose Extrude or Pocket. Single-click selects a tree object;
Ctrl/Shift-click adds or removes objects. Double-click reopens a sketch or feature
for editing. Properties opens native parameters for Bodies and reference objects
as well. Fillet uses selected native edges. Feature values normally create native previews.
Apply commits one undo step; Cancel or Escape restores the baseline. An invalid
value cannot be applied. When native redo history exists, feature recomputation
is deferred until Apply so Cancel preserves redo. See
[preview ownership and native limits](PROTOCOL.md).

Pad and Pocket use native one-side, symmetric and two-side modes, with each
side's length, first/last or through-all extent, face/shape target, offset and
taper as supported by the selected native feature. Select a native target and
use its field's selection button. Direction can use a reference axis or custom
vector. Mode changes retain inactive values, links and expressions. Controller
expressions keep dependent inputs available until native evaluation resolves
their mode. Required fields remain accessible after an incomplete preview fails.
These workflows have native and shared-command verification; real GUI acceptance
and broader imported/legacy target coverage remain open.

Selecting a Body makes it the active modeling target; selecting a feature keeps
that target. A profile sketch outside the target Body offers Independent copy,
Dependent copy and Cross-reference. Independent creates a native sketch with its
internal constraints and expressions, removes external geometry and attachment,
and leaves the source unchanged. Intentional expression links to other objects
remain live. Dependent creates a native ShapeBinder; Cross-reference links the
original sketch directly. All three choices share the feature preview and one
Apply transaction. Cancel restores the baseline. Unsupported independent copies
fail atomically. Defining and intersection curves are materialized as native
internal geometry without copied external identities. Circular fixtures cover
this behavior; broader curve combinations and custom dynamic properties remain
open coverage. Choose Dependent or Cross-reference when appropriate.

The multi-Body code now provides New body, active Body selection, all visible
unconsumed native Body tips, face/edge multiselection and history grouped by Body.
Boolean supports Union, Subtract and Intersect with a target Body and multiple
native Group-linked tool Bodies. Apply/Cancel and editing an existing Boolean's
Type/tool group use native feature transactions. Real Pixel workflows and
independent native saved-file checks verify body creation, previews, cancellation,
multiple tools, history, reopening and consumed-tool edits; broader coverage
remains in the [verification record](VERIFICATION.md).

The feature tree follows native Body history order, including datum and binder
members. Right-click a solid feature for Set tip here, or its Body for Roll back
to start. Successors remain in the document. Reorder opens an Apply-only task;
Before/After and its target use native Body insertion, with an optional final Tip.
Cancel preserves redo and invalid dependencies restore the previous history.
Reordering a MultiTransform child row does not reorder the parent's transformation
steps; use the MultiTransform task for that. Suppression remains incomplete.

Right-drag orbits, middle-drag pans, and the wheel zooms. F fits the model.
View provides seven standard directions, shaded/edge/wireframe display, native
face/edge/vertex filters and Show/Hide. The tree includes references and guarded
leaf deletion. Native planes and axes use bounded presentation graphics, preserving
their actual native identities for attachment and selection.
Open, Save as, Import and Export use a modal file browser with folder navigation,
format filters and destination replacement confirmation. Native documents use
FCStd; geometry exchange uses STEP/IGES. Canceling the browser starts no native
operation. Pixel GUI checks cover filtered picking, folder navigation, native
Save as, cancellation, unsaved-change protection and STEP/IGES exchange browsing.
Native Save as uses the browser's replacement confirmation. Export retains a
separate explicit replacement checkbox because the browser returns a path without
an overwrite-authorization flag.
Ctrl+S saves the document; Ctrl+Shift+S opens the FCStd Save as browser.
Ctrl+Z and Ctrl+Shift+Z use native Undo/Redo.
The editor asks before discarding unsaved changes. Native calls and display
extraction run on a model thread; camera/input do not wait for recomputation.
The viewport uses OpenGL depth testing and native face/edge identities. Geometry
and history remain entirely in the native document.

The [local feature package interface](PACKAGES.md) is available to both Rust
callers and `cad-next-packages`. The editor's Packages menu exposes loaded
declarative features, including their parameters. `--packages DIR` loads a
caller-selected collection. Executable plugins and custom UI extensions are
future work.

The Features menu now opens native property forms for the declared Part and
PartDesign families, including revolve/groove, lofts, sweeps, helices, holes,
dress-ups, patterns, primitives and reference geometry. Properties edits existing
objects. The pinned native schema supplies property types, choices, documentation
and status; the form writes only user overrides and expressions. Use Find
property or All properties to reach additional native options. Contextual seeds
use selected native profiles, subelements and Body origin references.

These forms extend functional access; they do not establish complete native task
dialog or feature coverage. [UI-COVERAGE.md](UI-COVERAGE.md) records the missing
workflows, including remaining sketch tools and external-reference workflows,
history editing, assemblies, broader exchange and executable extensions. Each
advanced feature's GUI acceptance remains separate from native workflow fixtures.
General documents and moderate-part resource behavior need broader gates.
The [verification record](VERIFICATION.md) separates real GUI checks, independent
native saved-file checks and remaining work.

The same Rust discovery and edit compilers are available to agents:

```sh
cad-next-packages --feature-catalog
cad-next-packages --feature-seed - < selection-context.json
cad-next-packages --native-properties - < property-changes.json
```

Feature seed requests contain `type`, `state`, `name` and optional `body`,
`selected_object` and `selections`. Property requests use the editor's
`{type,name,body,create,properties,expressions}` structure. Existing-feature tasks
also capture `tip` and `tip_visibility` to preserve downstream history display. These commands return
properties or native edit arrays; callers submit them through the same guarded
document boundary. They do not mutate documents themselves.

## Run the verified foundation

Python 3 and Cargo are needed for setup and the Rust host. Runtime setup downloads
the official portable FreeCAD distribution and verifies its pinned SHA256 before
extracting it into the checkout's ignored `.tools` directory. It does not install
anything globally or require FUSE. Supported runtime platforms are Windows x64,
Linux x64 and Linux ARM64.

```sh
cargo build --manifest-path next/Cargo.toml --locked
python next/tools/setup_runtime.py --output next/target/runtime.json
python next/tests/native_workflows.py --host next/target/debug/cad-next --runtime next/target/runtime.json --output next/target/native-documents
```

The sketch command compiler and edit-session suites run through the same host:

```sh
python next/tests/sketch_commands_workflows.py --host next/target/debug/cad-next --compiler next/target/debug/cad-next-sketch --runtime next/target/runtime.json --output next/target/sketch-command-documents
python next/tests/sketch_editor_workflows.py --host next/target/debug/cad-next --runtime next/target/runtime.json --output next/target/sketch-editor-documents
python next/tests/feature_workflows.py --host next/target/debug/cad-next --compiler next/target/debug/cad-next-packages --runtime next/target/runtime.json --output next/target/feature-documents
cargo test --manifest-path next/Cargo.toml --features desktop --locked
```

On Windows, add `.exe` to the host and compiler paths. Launcher configuration
contains `executable`, `args` and `script`. For interactive NDJSON use, pass those
to `cad-next --executable PATH --arg ARG --script PATH`, repeating `--arg` as needed.
The pinned bundled Python runs the worker. `FreeCADCmd` does not execute this
worker's `__main__` entry point reliably and is not the configured launcher.

The full official distribution is an evaluation dependency. Its download and
disk size are substantial. It is not the intended lightweight shipping package.
Runtime slimming and packaging optimization are deferred until functional Rust
replacement is complete. The pinned native distribution remains the behavioral
reference while components are ported.

## Migration rules

- Keep one authoritative native document. Preserve FreeCAD's object links,
  expressions, attachment, topology naming, transactions and recomputation.
- Plugins and agents use the same versioned command boundary. Features compose
  native document operations; the frontend does not implement geometry rules.
- Complete one Rust component at a time behind that boundary. Promote it and
  remove its old implementation only after its full declared functional scope
  passes matched geometry, workflow, failure and resource regression checks.
- Measure startup, working memory, idle CPU, preview latency and regeneration
  throughout migration. Defer performance tuning and UX redesign until the
  functional application and its components are complete in Rust. Converting
  code to Rust is not evidence of a speed improvement.
- Save new documents as native FCStd during the initial migration. Import of
  the old cadpart format must be explicit and must report unsupported features.
  Never silently flatten editable history into a mesh or overwrite an original.

## First acceptance gate

Prove real native feature chains before a replacement editor:

1. Constrained sketch, pad, face attachment, constrained sketch, pocket.
2. Change the parent dimensions and topology, recompute, save, reopen and
   continue editing the same attached sketch.
3. Tangent fillet propagation, fillet termination at another surface, and
   a three-edge corner with adjoining details.
4. Reject an impossible radius without publishing a broken model; undo and
   redo complete feature transactions.

These headless checks pass on Windows x64 and the Pixel's native Linux ARM64.
They include empty sketch creation, constrained dragging, construction toggling,
deletion back to empty, arcs, native dimension expressions, unit rejection,
attachment remapping from Face6 to Face7, and continued editing after reopen.
They do not establish broad feature parity or interactive performance. CI repeats
native, package and preview checks on Windows and Linux x64 and compiles/tests
the optional desktop target.

Compare actual shapes, intended placement and reference ownership, validity,
volume, topology and saved history. Include native Pixel interaction once the
backend gate passes. FreeCAD itself has unsupported cases; this is a measured
baseline, not a claim that every fillet must succeed.

## Implementation boundary

The worker is a persistent child process. Rust exchanges newline-delimited
JSON through pipes. One request owns a native document transaction. FreeCAD
diagnostics stay separate from protocol output. No Python or C++ shape handles
cross the boundary; documents and objects have stable names. Display meshes are
requested separately from modeling and are never authoritative geometry.

An in-process CXX bridge is a later option if measured pipe overhead matters.
It is not a prerequisite for verifying the modeling foundation.

## Sources

- [Gradual replacement](https://martinfowler.com/bliki/StranglerFigApplication.html)
  and [replacement behind a common interface](https://martinfowler.com/bliki/BranchByAbstraction.html).
- [Firefox's Rust/C++ integration](https://firefox-source-docs.mozilla.org/writing-rust-code/index.html)
  and [CXX interoperation](https://cxx.rs/).
- [Pinned FreeCAD release](https://github.com/FreeCAD/FreeCAD/releases/tag/1.1.4).
- [FreeCAD document API](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/App/Document.h),
  [attachment](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/Part/App/AttachExtension.cpp),
  [fillet execution](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/PartDesign/App/FeatureFillet.cpp)
  and [shape naming](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/Part/App/TopoShape.h).


Advanced sketch tools include native conics and open/periodic B-splines. Curves
remain unconstrained until dimensioned or constrained. For ellipse axes or
spline poles, select the curve and use Curves > Expose internal geometry, then
edit the native construction lines or circle centers. Spline clicks collect
poles; Enter creates the curve. External projects native edges/vertices and can
mark them defining or intersecting. MultiTransform edits its native ordered
child steps in the parent task. Pixel GUI checks cover creation, cancellation,
child edits, reordering/removal, Undo/Redo and editing after reopen, with independent
native checks of saved geometry and links. Import/Export supports native STEP/IGES;
selected root export uses native local placement. These additions are under workflow
verification; [UI-COVERAGE.md](UI-COVERAGE.md) records remaining functionality.
