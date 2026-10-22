# Component migration inventory

Complete a parametric Part Studio before assemblies. Preserve the pinned FreeCAD
reference while replacing components with Rust behind stable interfaces. Promote each complete component only after
matched verification. The final application includes a Rust geometry kernel.
UX customization and performance tuning follow full functional Rust completion;
resource regression checks remain promotion gates throughout.

Fixtures prove tested cases; complete modeling parity remains unverified.
Third-party addons and unrelated FreeCAD workbenches have no blanket parity commitment.

| Component | Current implementation and scope | Matched acceptance gate |
| --- | --- | --- |
| BRep geometry, topology, Booleans and blends | FreeCAD/Open CASCADE C++, accessed through Python. Rust builds commands only. | Shape validity, tolerances, orientation, solid counts, difficult intersections/blends and persistent topology history. |
| Sketch geometry, constraints and solver | Native Sketcher C++; Python task adapter; Rust drawing, constraints and sketch-task command construction/interaction. | Entire declared geometry/constraint scope, solved freedom, conflicts, drag/trim/split/extend, fillet/chamfer, Carbon Copy, spline edits, external references and units. |
| Document graph, references and attachment | FreeCAD App/Part C++; Python adapter; Rust ownership/display queries. | Linked edits, topology remapping, transformed attachment, suppression, reorder and dependency failures. |
| Expressions and units | Native C++ expression engine and quantities. | Dimensional compatibility, dependency propagation, named constraints, cycles, errors and saved expressions. |
| Transactions, history and recomputation | C++ document transactions; Python preview/sketch-session orchestration; Rust request ownership. | Atomic failure, revision guards, preview ownership, action/document Undo/Redo, Cancel and validated final recomputation. |
| Persistence and interchange | C++ FCStd and STEP/IGES through Python; Rust file browser; legacy prototype preserved separately. | Editable graph and references after reopen; explicit import/export coverage, browser cancellation/replacement, unsupported-data reporting and original preservation. |
| PartDesign feature families | Native C++ features; selected operations exposed through Python and Rust forms/packages. | Each feature's complete declared options, linked parent edits, failure recovery, history and persistence. |
| Assemblies | No replacement assembly implementation; native C++ Body placement available. | Joint/constraint behavior, component references, transforms, recomputation and saved assemblies. |
| Display, picking and input | Rust editor, OpenGL display and selection; native meshes/topology remain display inputs. | Occlusion, native identities, attached coordinates, multiple Bodies, gesture ownership and input during recomputation. |
| Feature packages and agent protocol | Rust declarative loader, edit builders and persistent NDJSON host; Python native adapter. | Typed requests, errors, revision/ownership rules, equivalent agent/UI edits and extensibility without duplicate document history. |

Functional gates remain for complete extrude/pocket termination and directions;
all revolve/groove, loft, sweep, pattern, shell/thickness and dress-up workflows;
complete native sketch editing/reference tools; broader history/suppression cases;
assemblies and executable extensions. The Rust feature catalog and typed native property forms expose these families;
a native type appearing in an allowlist or form does not establish a complete
end-to-end workflow. See [UI-COVERAGE.md](UI-COVERAGE.md).

A bounded next port is preview/sketch-session orchestration: ownership, revisions,
replay, cancellation and failure recovery. Match the native workflow suites, then
remove the Python owner. Native document transactions, solving and BRep geometry
remain separate ports.
