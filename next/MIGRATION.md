# Migration decision

Preserve FreeCAD's complete functional behavior as the reference, then replace
its components with Rust one at a time. Complete each component's functional
scope and verify it against the native implementation before promotion. The end
state is a fully functional application implemented entirely in Rust, including
the geometry kernel. UX redesign and performance tuning follow that functional
Rust completion.

The current implementation is mixed-language. FreeCAD App, Sketcher, PartDesign,
Open CASCADE and the Python adapter still perform the native modeling work. The
Rust editor, display path, transport and command construction do not replace
those modeling components. The Rust UI exposes only part of FreeCAD's
functionality; complete functional parity has not been achieved.

This follows [gradual replacement](https://martinfowler.com/bliki/StranglerFigApplication.html)
and [replacement behind a common interface](https://martinfowler.com/bliki/BranchByAbstraction.html).
[Firefox's Rust integration](https://firefox-source-docs.mozilla.org/writing-rust-code/index.html)
is a practical example of Rust coexisting with a mature native application.

## Preserve the modeling system together

FreeCAD's fillet feature delegates to its topology wrapper, which invokes Open
CASCADE. Attachment, expressions, document recomputation, native undo and
persistent element naming are additional parts of the modeling behavior.
Copying feature wrappers into Rust would not replace those systems.

The initial worker therefore retains FreeCAD App, Part, Sketcher, PartDesign and
their dependencies. One native document owns the model. Rust, feature packages
and agents use the same command boundary. Display meshes never become the model.

Native sources pinned to the selected release:

- [Document ownership and transactions](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/App/Document.h)
- [Sketch solver integration](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/Sketcher/App/SketchObject.cpp)
- [Face attachment](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/Part/App/AttachExtension.cpp)
- [Fillet execution](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/PartDesign/App/FeatureFillet.cpp)
- [Shape history and element mapping](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/Part/App/TopoShape.h)

## Replacement order

1. Establish and preserve the full functional FreeCAD reference. Keep the pinned
   native runtime and the earlier prototype intact. The existing native workflow
   baseline verifies specific modeling behavior; it does not establish complete
   application coverage. Record the remaining capabilities and their acceptance
   workflows before claiming full functionality.
2. Close functional gaps through the native application boundary while retaining
   one authoritative document. Features, plugins and agents must preserve native
   constraints, attachment, expressions, references, history, transactions and
   recomputation. A functional Rust interface to native operations is application
   access, not a completed Rust port of those operations.
3. Port one well-defined component completely to Rust behind a stable interface.
   Keep the native implementation available for matched evaluation. Promote the
   candidate only after its entire declared functional scope, failure behavior,
   saved data and resource regression checks pass. A partial implementation or a
   Rust command wrapper does not complete the component.
4. Repeat until all components, including the coupled geometry kernel,
   topology/history, attachment and document systems, are functionally complete
   in Rust. Replace coupled components together when their contracts require it.
   Remove native C++ and Python dependencies only after their replacements pass
   the same functional gates. Preserve working native geometry until then.
5. After functional Rust completion, customize the UX toward Onshape workflows
   and tune speed, memory, startup and packaging. Resource measurements during
   earlier stages guard against regressions; they do not move optimization or
   visual redesign ahead of functional replacement.

## Current functional implementation

The editor provides Point, Line, Polyline, Rectangle, Circle, Arc, conics and
open/periodic B-splines, snapping and inference, construction, interactive native
constraints and dimensions, expression/state editing, selection boxes, dragging,
trim and deletion. Native sketch tasks add fillet/chamfer, split/extend, Carbon
Copy and spline degree/knot/pole/weight editing. These interfaces retain native
geometry, constraints, exposed controls and persistent references. Sketch
sessions support action Undo/Redo, whole-model validation on Finish and Cancel
back to the session baseline. Invalid intermediate consumed profiles can be
repaired without publishing broken final solids. Face attachment, extrusion,
pockets, fillet previews and FCStd reopening use the same native document.
Declarative feature packages share its command boundary. When existing redo
prevents a native feature preview, recomputation is deferred until Apply so
cancellation preserves that history.

Multi-Body implementation now includes New body and active Body selection,
display of all visible unconsumed native Body tips, native face/edge
multiselection and feature history grouped by Body. Native Boolean features use
Union, Subtract or Intersect, with one target and multiple linked tool Bodies in
the native Group property. Their previews support Apply/Cancel; existing Boolean
Type and tool groups can be edited. The real Pixel workflow now covers creating
Bodies, Boolean previews, cancellation, committed multiple-tool edits, Undo/Redo,
save/reopen and continued edits to consumed tools. Independent native saved-file
checks validate geometry and retained history. These cases establish bounded
workflow evidence, not complete multi-Body or FreeCAD parity.

Body tip rollback and feature reordering use dedicated native commands. The
Rust tree follows native Group order; Body insertion owns BaseFeature rewiring.
Reorder forms execute only on Apply, so opening and canceling them preserves
redo. Native dependency failures abort atomically. Native suppression has a
separate unresolved placement persistence defect and is not promoted as a
complete workflow.

Sketch creation now resolves native Body-origin roles and same-Body datum,
sketch-frame or face supports through the shared Rust compiler. Foreign Sketch
profiles offer native Independent/Dependent copy and direct Cross-reference in
the feature preview transaction. Their tested cases retain source documents,
constraints, expressions, history and downstream Tip ownership; complete reference
and attachment coverage remains open. Native Undo/Redo expression rebinding is
limited to newly restored objects and retains native transaction ownership.

See [tested workflows and remaining gates](VERIFICATION.md). Full FreeCAD
functionality is not yet exposed by the Rust UI. The Rust feature catalog now
exposes typed native forms for revolutions, lofts, sweeps, holes, dress-ups,
patterns, primitives and references. Conditional property values, expressions,
links and native metadata share the guarded command boundary. Workflow fixtures
cover specific cases, not every task option. Complete sketch/reference variants,
remaining advanced feature workflows, suppression, assemblies and executable
plugins remain open. See [UI-COVERAGE.md](UI-COVERAGE.md). This application work does not constitute a Rust geometry-kernel
replacement.

Local pipes keep native handles and failure ownership in one worker process and
require no server. Change to an in-process bridge only if profiling shows pipe
traffic matters. [CXX](https://cxx.rs/) is a possible bridge, not a geometry port.

Steps 2 and 3 proceed component by component; exposing every native tool in the
new UI is not a prerequisite for a bounded Rust replacement. Full end-to-end
functional Rust coverage is required before customization. The
[component inventory](COMPONENTS.md) records current boundaries and gaps.

## Promotion rules

Use matched native and candidate models to check intended geometry, tolerances,
validity, orientation, solid counts and persistent reference ownership. Include
splits, merges, touching and near-tangent Booleans, blend termination and corners,
parent edits, suppressed/reordered history, save/reopen, undo/redo and constrained
dragging. Unsupported operations must leave the previous valid model intact.

Tests provide evidence for this contract. Passing a fixed example count does not
establish general robustness. Inspect difficult results in the real viewport and
include independent geometric measures where available.

Resource regression gates cover cold and warm startup, working and peak memory,
idle CPU, input-to-frame latency, preview cancellation and complete regeneration
on representative moderate parts. Compare native and candidate implementations
on the same workloads and hardware before promotion. These checks remain part
of functional verification throughout migration. Performance tuning and UX
customization wait until functional Rust completion; Rust alone is not evidence
of a speedup.

The interface requires revision tokens for every mutation. Agents can work
independently in separate documents; shared document mutations are serialized
and stale revisions rejected. Cross-process locking, branching and merging are
unfinished work. There is no automatic old `.cadpart` migration yet. Originals
must remain intact until an explicit importer can report which editable features
it preserves.
