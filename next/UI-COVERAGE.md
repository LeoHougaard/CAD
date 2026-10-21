# UI functional coverage

The current application is incomplete. A native feature appearing in the catalog,
or a property being editable, establishes access to that interface. It does not
establish a complete creation/editing workflow or parity with FreeCAD's task
dialogs. Rust implements the editor and command construction; the native engine
still owns modeling, constraints, references and history.

| Area | Current access | Remaining functional acceptance |
| --- | --- | --- |
| Document lifecycle | New, FCStd Open, Save/Save as, unsaved-change protection, native Undo/Redo and guarded writes. | Broader files, failure/recovery, multiple-document interaction, and cross-process locking. |
| Sketch creation and editing | Native face support; Point, Line, Polyline, Rectangle, Circle and Arc; snapping/inference, construction, boxes, dragging, trim and deletion. | Splines, ellipses/conics, additional sketch tools, external geometry and their editable saved references. |
| Sketch constraints | Common geometric constraints, length/coordinate/radius/diameter/angle dimensions, expressions, names, reference/driving and activation controls. | Remaining native constraint families and combinations; full selection, conflict and recovery coverage. |
| Sketch sessions | Action Undo/Redo, gesture previews, Finish validation and Cancel to baseline, including repair of consumed profiles. | Broader histories, topology changes and every newly added sketch operation. |
| Feature property forms | Native schema supplies defaults, enums, documentation, status and property types. Forms edit explicit overrides and expressions, including references, lists, vectors and placements. | Every declared type's required inputs and conditional options; task-specific behavior that generic writable fields cannot provide. |
| Extrusions and revolutions | Pad/Pocket and Revolution/Groove forms expose native profile, extent/direction and available second-side options. | Complete termination modes, offsets, axes, parent changes, invalid selections, reopen and GUI workflows. |
| Lofts, sweeps and helices | Additive/subtractive feature forms expose profiles, sections, paths, axes and native modes. | Section/path ordering, multiple sections, guide/orientation modes and topology-edit recovery through the UI. |
| Holes and dress-ups | Hole, Fillet, Chamfer, Draft and Thickness forms expose native parameters and references. | Hole/thread variants, face/edge selection rules, neutral planes, shell modes and difficult blends. |
| Patterns and transforms | Linear, Polar, Mirrored, Scaled and MultiTransform forms expose native references and options. | Complete direction/axis/plane workflows; creation/editing of MultiTransform steps and history ownership. |
| Primitives and Part operations | Part and additive/subtractive primitive catalogs; Part CSG, extrusion, revolution, fillet and chamfer forms. | Each native operation's creation, edit, failure and persistence workflow, including specialized edge data. |
| Datum and reference geometry | Plane, Line, Point, CoordinateSystem, ShapeBinder and SubShapeBinder interfaces. | Support selection, attachment modes/offsets, binder-specific support editing and linked reference updates. |
| Bodies and Booleans | Active/new Bodies, native tip display, grouped history and linked Union/Subtract/Intersect operations. | Broader GUI coverage, multiple tools, nested references and native transformed-Group behavior. |
| Object/history management | Native properties provide labels, visibility and placements; guarded leaf removal protects dependents. | Complete suppression, rollback/reorder, dependency editing and document organization workflows. |
| Viewport and input | Native face/edge/vertex identities, depth-aware multiple-object selection, camera navigation and model-thread separation. | Interaction checks for every new geometry/reference kind and complete tool/gesture cancellation behavior. |
| Import/export and migration | Native FCStd persistence; earlier prototype and original documents retained. | STEP/IGES and other exchange workflows, explicit `.cadpart` migration, and unsupported-feature reporting. |
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
