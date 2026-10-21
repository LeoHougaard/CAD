# Component migration inventory

Build a fully parametric Part Studio first, then assemblies. Preserve the pinned
FreeCAD reference while completing functional coverage and replacing components
with Rust behind stable interfaces. Promote each complete component only after
matched verification. The final application includes a Rust geometry kernel.
UX customization and performance tuning follow full functional Rust completion;
resource regression checks remain promotion gates throughout.

Current fixtures establish specific tested cases. Complete modeling parity has
not been established. Third-party addons and unrelated FreeCAD workbenches have
no blanket parity commitment here.

| Component | Current implementation and scope | Matched acceptance gate |
| --- | --- | --- |
| BRep geometry, topology, Booleans and blends | FreeCAD/Open CASCADE C++, accessed through Python. Rust builds commands only. | Shape validity, tolerances, orientation, solid counts, difficult intersections/blends and persistent topology history. |
| Sketch geometry, constraints and solver | Native Sketcher C++; Rust drawing, constraint commands and interaction. | Entire declared geometry/constraint scope, solved freedom, conflicts, drag/trim, construction, external references and units. |
| Document graph, references and attachment | FreeCAD App/Part C++; Python adapter; Rust ownership/display queries. | Linked edits, topology remapping, transformed attachment, suppression, reorder and dependency failures. |
| Expressions and units | Native C++ expression engine and quantities. | Dimensional compatibility, dependency propagation, named constraints, cycles, errors and saved expressions. |
| Transactions, history and recomputation | C++ document transactions; Python preview/sketch-session orchestration; Rust request ownership. | Atomic failure, revision guards, preview ownership, action/document Undo/Redo, Cancel and validated final recomputation. |
| Persistence and interchange | C++ FCStd persistence through Python; legacy prototype preserved separately. | Editable graph and references after reopen; explicit import/export coverage, unsupported-data reporting and original preservation. |
| PartDesign feature families | Native C++ features; selected operations exposed through Python and Rust forms/packages. | Each feature's complete declared options, linked parent edits, failure recovery, history and persistence. |
| Assemblies | No replacement assembly implementation; native C++ Body placement available. | Joint/constraint behavior, component references, transforms, recomputation and saved assemblies. |
| Display, picking and input | Rust editor, OpenGL display and selection; native meshes/topology remain display inputs. | Occlusion, native identities, attached coordinates, multiple Bodies, gesture ownership and input during recomputation. |
| Feature packages and agent protocol | Rust declarative loader, edit builders and persistent NDJSON host; Python native adapter. | Typed requests, errors, revision/ownership rules, equivalent agent/UI edits and extensibility without duplicate document history. |

Functional gates remain for complete extrude/pocket termination and directions;
all revolve/groove, loft, sweep, pattern, shell/thickness and dress-up workflows;
complete native sketch editing/reference tools; history rollback/reorder;
assemblies and executable extensions. The Rust feature catalog and typed native property forms expose these families;
a native type appearing in an allowlist or form does not establish a complete
end-to-end workflow. See [UI-COVERAGE.md](UI-COVERAGE.md).

A next candidate for a complete bounded port is preview and sketch-session
orchestration: gesture ownership, revision checks, replay journals, cancellation
and failure recovery. The existing native workflow suites provide its reference.
A Rust replacement would still use native document transactions and geometry;
it must remove the replaced Python orchestration after matched promotion rather
than keep two owners. Native transaction semantics, sketch solving and the BRep
kernel remain separate migration work.
