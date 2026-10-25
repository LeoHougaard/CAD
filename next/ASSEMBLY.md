# Local native assembly authoring

This bounded implementation authors FreeCAD `Assembly::AssemblyObject` containers,
`Assembly::JointGroup`, local `App::Link` instances, shipped `GroundedJoint` proxies
and shipped `JointObject.Joint` constraints. FreeCAD owns connector frames,
presolve, constraint solving, topology references, document history and FCStd
persistence. Rust builds commands and presents the task fields.

The menu exposes the pinned native joint types: Fixed, Revolute, Cylindrical,
Slider, Ball, Distance, Parallel, Perpendicular, Angle, RackPinion, Screw, Gears
and Belt. Native/headless checks cover Fixed, Revolute, Slider, Cylindrical and
Ball in the recorded fixtures. Access to the other types does not establish
their complete workflow acceptance or full Assembly parity.

## UI workflow

Use the optional Rust editor with the pinned runtime. A fresh document or native
FCStd can contain both editable source parts and an assembly. Finish any sketch
or feature task before entering Assembly tools. All assembly tasks use the
existing Feature window and execute only on Apply. Cancel or Escape closes an
unaccepted task without native writes, including when Redo exists.

1. Create source components with Features > Part primitives > Box and Cylinder,
   or use existing native Bodies. For the retained plate/pin fixture, set the
   Box to Length 40 mm, Width 30 mm, Height 5 mm. Set the Cylinder to Radius 2 mm
   and Height 12 mm. Apply each source feature. Native source names remain in
   Features and their ordinary Properties forms remain editable.
2. Choose Assembly > New assembly. Keep or edit the `name` and `joints` native
   identifiers. Apply creates the assembly and its native JointGroup in one
   document history step. The Features tree shows the assembly, its components
   and its JointGroup rows.
3. Select the assembly, then Assembly > Insert component. Choose the Assembly
   and Component source, set Instance name, and enter X/Y/Z in millimeters and
   quaternion Qx/Qy/Qz/Qw. Apply creates a native link to that source. Insert the
   second source the same way. Select an existing instance as Component source
   to make another instance of its underlying source. No copied solid replaces
   the native source. A zero quaternion is rejected.
4. Hide source definitions through their existing Features selection and View >
   Hide selected when they obscure the instances. Source geometry and history
   remain in the document. Links retain their own visibility and placement.
5. Select an instance row, choose Assembly > Ground / unground, check Ground
   this component and Apply. This creates the native grounded joint. The row
   shows `grounded`; the native GroundedJoint appears in the JointGroup. Use the
   same task with the checkbox cleared to unground. This follows the runtime's
   actual solver behavior, including its accepted ungrounded cases.
6. Choose Assembly > New joint. Choose the native joint type and two distinct
   component instances. Use component origin clears both connector subnames.
   To attach geometry, pick a displayed instance face, edge or vertex and press
   that connector's Use selected face / edge / vertex button. View > Selection
   selects the native face/edge/vertex picking mode. The picked instance must
   belong to the chosen assembly.
7. Each connector has Element and Vertex or element center fields. Repeating a
   face or edge name in both fields requests its native center. Enter a native
   vertex in the second field to choose a corner. A whole component origin uses
   two empty fields. The tested plate/pin face connectors use Face6/Face6 on the
   Box instance and Face3/Face3 on the Cylinder instance. Native XLinkSub owns
   saved identities and later topology updates. Missing elements reject Apply.
8. Expand Joint values, offsets and limits to override Distance, Distance2,
   Angle, length/angle limits, enable flags, Suppressed or Offset1/Offset2.
   Check a property to write it; unchecked properties retain their native values.
   Scalar fields accept native quantities. Expression fields retain existing
   native expressions until explicitly changed or cleared. Offset placements
   use millimeters and quaternions. Apply solves the complete native document.
9. Double-click an instance, or choose Assembly > Place component, to edit its
   placement. Native constraints determine its final solved placement. Double-
   click a joint, or select its row and choose Assembly > Edit joint, to change
   type, references or explicit task values. Apply accepts one native history
   step. Cancel preserves the prior model. Failed Apply retains the task and
   entered values so they can be corrected and applied or canceled.
10. Edit a source component through its existing Properties form. Its instances
    update through native links, and assembly constraints solve during accepted
    document recomputation. Selecting the assembly and Properties also exposes
    its native container Placement. Moved/rotated containers retain native
    instance identities and the correct mesh/topology display frame.
11. Select a joint or instance, choose Assembly > Remove component or joint and
    Apply. Remove dependent joints and grounds before removing a component.
    Dependencies outside the container/group prevent removal atomically. The
    current tool does not delete an entire assembly container.
12. Use Undo/Redo for accepted authoring, placement, joint edits and removals.
    Ctrl+S or Save as writes the same authoritative FCStd document. Reopen it
    through Open, select an instance or joint and continue editing. Native joint
    Python proxies, local links, references and editable sources are retained.

## Shared compiler and document boundary

`cad-next-packages --native-assembly -` reads `{state,request}` on stdin and
returns an edit array. `state` is a complete current `inspect` snapshot. The
Editor uses the same `assembly_commands::build` function. Compilation changes
no document; submit the array through `transaction` with `expected_revision`.
The same edits also support the existing owned preview boundary.

Operations are `assembly_create`, `assembly_insert`, `assembly_placement`,
`assembly_ground`, `assembly_joint`, `assembly_remove` and `assembly_solve`.
Every operation except creation names its `assembly`. Insert takes local source
`source`, a new `name` and optional native `placement`. Joint takes `name`,
`create`, `joint_type`, `reference1` and `reference2`, plus optional `properties`
and `expressions` maps. A reference is
`{"link":"Instance","subelements":["Face6","Face6"]}`. Ground takes `object`,
`grounded` and a fresh `name` for a new grounded joint. Placement and remove take
`object`. Creation takes fresh `name` and `joints` names.

Snapshots add `assembly_state` for containers, native `assembly` ownership for
members, and `instance_state` with source name, source document, source path,
local/external status and native grounding. This document/path identity remains
available for a later external-file insertion interface. External documents
can be legitimate native component sources for one authoritative assembly.
This candidate's insertion/connector tasks author local document sources only.
They do not implement external file selection, path portability, external source
updates or multi-document save/close ownership. Those are explicit open work.

## Verification and remaining acceptance

The public host/compiler suite covers a plate, pin and repeated pin instance,
source updates, native face identities, placement, moved container display,
five joint types, offsets, grounding, stale revisions, preview ownership,
atomic rejected connectors/dependent removals, history and FCStd reopen/edit.
An actual Editor/model-thread test uses headless egui Apply/Cancel input,
including rejected Apply correction and Cancel with Redo. An independent direct
native construction compares saved shapes, placements, links and joint proxies.

No Windows or Pixel GUI was operated. Real desktop picking, every joint mode
and limit combination, detached/custom connectors, nested/subassemblies,
external file insertion/reference updates, full assembly-container deletion,
simulation and complete native Assembly task parity remain unverified or open.
Finite cases do not certify general Assembly parity. Native C++/Python remains
the intentional modeling and solving implementation.
