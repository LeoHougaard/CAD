# Migration decision

Keep the native FreeCAD modeling implementation while replacing components in
stages. The eventual target remains entirely Rust. Rewriting the geometry core
first would discard the complex-operation behavior that motivated this rebuild.

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

1. Preserve the prototype and establish a pinned native baseline. The persistent
   Rust host, native worker, runtime setup and initial workflows are implemented.
2. Build the Rust editor and feature package interface around that document.
   Implement Onshape selection, sketch editing, feature previews and rollback
   as transactions against native state. Reopening a file must restore editable
   geometry and constraints without a separate frontend history.
   The initial editor now covers constrained rectangle/circle profiles, native
   dimensions, face attachment, extrusion, pockets, fillet previews, undo/redo
   and FCStd reopening. Declarative feature packages share its command boundary.
   See [tested workflows and remaining gates](VERIFICATION.md). General sketch
   interaction, complete feature editing and executable plugins remain open.
3. Measure complete user actions on the Pixel. Keep input, camera movement and
   drawing independent of model recomputation. Reduce copying, unnecessary
   recomputes and unused dependencies before translating working algorithms.
   A temporary preview must never publish a failed or stale result.
4. Replace bounded components with Rust only when they preserve required
   behavior and improve measured cost or maintainability. Keep the native
   implementation for matched evaluation until promotion.
5. Replace coupled geometry, topology/history, attachment and document components
   together where their contracts demand it. Remove the last native dependency
   only after Rust passes the same modeling and editing gates.

Local pipes keep native handles and failure ownership in one worker process and
require no server. Change to an in-process bridge only if profiling shows pipe
traffic matters. [CXX](https://cxx.rs/) is a possible bridge, not a geometry port.

## Promotion rules

Use matched native and candidate models to check intended geometry, tolerances,
validity, orientation, solid counts and persistent reference ownership. Include
splits, merges, touching and near-tangent Booleans, blend termination and corners,
parent edits, suppressed/reordered history, save/reopen, undo/redo and constrained
dragging. Unsupported operations must leave the previous valid model intact.

Tests provide evidence for this contract. Passing a fixed example count does not
establish general robustness. Inspect difficult results in the real viewport and
include independent geometric measures where available.

Performance gates cover cold and warm startup, working and peak memory, idle CPU,
input-to-frame latency, preview cancellation and complete regeneration on
representative moderate parts. Compare the same workloads and hardware. Rust
alone is not evidence of a speedup.

The interface requires revision tokens for every mutation. Agents can work
independently in separate documents; shared document mutations are serialized
and stale revisions rejected. Cross-process locking, branching and merging are
unfinished work. There is no automatic old `.cadpart` migration yet. Originals
must remain intact until an explicit importer can report which editable features
it preserves.
