# Native sketch Mirror

Mirror uses FreeCAD 1.1.4 `Sketcher_Symmetry`, including the native
`SketchObject::addSymmetric` operation. Rust builds the task and commands.
FreeCAD creates geometry, copies eligible constraints, maintains native geometry
extensions and solves the sketch. This is interface access to the native tool,
not a Rust geometry-kernel replacement.

## Use the task

1. Edit a native sketch. Select whole curves with Shift-click, or select a
   standalone sketch point. Endpoint/axis picks mixed with whole geometries are
   ignored as sources, as in the native command. Endpoints alone are not sources.
2. Choose **Edit tools > Mirror**. The task captures the selected native IDs and
   solved sketch baseline. It initially previews about the native X axis.
3. Choose X axis, Y axis, origin, an internal/external straight line, or an
   exposed native point from the reference list. Entries carry actual snapshot
   geometry and point IDs. A source line can also be the reference. Original
   source selections remain fixed for this task.
4. Optionally choose **Delete original geometries** or **Create symmetry
   constraints**. Choosing either clears the other, as in the pinned command.
   Constrained mode is disabled with the native availability reason when its
   verified adapter is absent. Default copy remains usable.
5. Apply accepts one sketch action. Cancel restores its baseline. Recompute
   preview retries a candidate after an error. An invalid replacement retains
   the last accepted preview and keeps the task editable.
6. Sketch Undo/Redo walks accepted actions. Finish recomputes the whole document
   and commits one document history entry. A failed consumed-profile Finish
   leaves the session editable. Sketch Cancel restores the session baseline.
   When Redo exists, native computation waits for Apply; task Cancel preserves
   Redo. Apply branches that history through the existing native journal.

## Shared compiler

`cad-next-sketch` accepts the task through its existing `edit` request. Supply
the actual native object JSON as `snapshot`, `kind:"mirror"`, whole-geometry
`selection:[{index:0,point:0}]`, and these parameters:

```json
{"reference":{"index":-2,"point":0},"delete_original":false,"create_symmetry_constraints":true}
```

The compiler returns `mirror_geometry` with ordered source `indices` and a native
`reference:{index,point}`. Submit it under the current revision and sketch token.
Axes are references `(-1,0)` and `(-2,0)`; the origin is `(-1,1)`. Standalone
point sources also accept their supplied `point:1` identity. The compiler rejects
missing snapshot identities, endpoint-only sources, non-line whole references,
unknown fields and conflicting options. Revision/lease checks remain authoritative
when edits reach the worker.

The adapter expands actual InternalAlignment constraints for selected conics and
B-splines as `getListOfSelectedGeoIds(true)` does. It does not expose new controls,
renumber selections, recreate constraints or synthesize expressions. Native Mirror
decides which source constraints can be copied. For example, its copy omits
directional DistanceX/DistanceY constraints and does not attach the source
dimension expression to a newly copied constraint. Existing source expressions
remain intact unless native deletion removes their owning geometry/constraints.

## Native API boundary

Default copy and delete modes call the pinned Python `addSymmetric` and
`delGeometries` APIs. The 1.1.4 Python binding omits the C++ boolean that creates
symmetry constraints. `backend/native/sketch_mirror.cpp` calls that same exported
C++ method through the pinned Windows x64 ABI. It uses native Application/Document
lookup, a release MSVC vector and no geometry math. The worker keeps the GIL and
verifies FreeCADApp, Sketcher and adapter hashes before its first constrained
copy. Missing/mismatched binaries fail the task. Existing native transactions
own rollback.

Rebuild the adapter with a Windows MSVC release compiler:

```powershell
python next/tools/build_sketch_mirror.py --runtime .tools/next-runtime-windows.json --vcvars "<VS>/VC/Auxiliary/Build/vcvars64.bat" --compiler "<VS>/VC/Tools/MSVC/14.44.35207/bin/Hostx64/x64/cl.exe" --output "<private-build-directory>"
```

This produces the DLL and its hash manifest beside the C++ source with one
BelowNormal compile. It does not rebuild FreeCAD or Cargo dependencies. Other
platforms need a verified native adapter for the symmetry-constraints option.
Default Python copy does not require this adapter.

Availability is public in `capabilities.sketch_tasks.mirror` and in each native
sketch snapshot's `sketch.task_capabilities.mirror`. Constrained requests fail
with `sketch_mode_unavailable` before native transaction/history/revision changes
if the platform, adapter/runtime hashes or load check fails. The checkbox displays
the reason and remains disabled, including when Apply would branch Redo.

Accepted sketch actions now retain opaque whole-object native checkpoints in
the Rust journal. Native `dumpContent` and the audited
[checkpoint restore boundary](CHECKPOINT-RESTORE.md) restore the selected
action or valid preview inside the same native document and transaction. The
worker does not re-execute accepted geometry creation during later task Cancel
or history moves. Separate property restores lost an expression in the native
API probe and are not used. No IDs or native XML are rewritten. The general fix
also covers accepted circle/ellipse-add actions, rather than special-casing Mirror.

## Evidence and limits

The original delivery's eight direct native controls and matching CLI/worker files reopened with exact
native geometry XML, extension IDs, constraint XML, construction flags, expressions
and degrees of freedom. Cases cover axes, an oblique line, origin, endpoint and
circle-center references, deletion and symmetry constraints, with lines, circles,
circular arcs, ellipses, elliptical arcs, parabola/hyperbola arcs, points and a
B-spline with exposed internal controls.

The fixed candidate adds the review's original before/after repro, native direct
controls for plain circle/ellipse-add followed by a later task Cancel, exact
Geometry/Constraints/ExpressionEngine XML and ElementMap comparisons, and a
follower external persistent-reference check. Actual headless Editor cases cover
preview replacement/failure/Cancel, action and document history, deferred Redo,
consumed Finish failure, session Cancel and fresh reopen. Missing-adapter native
capability/preflight/UI cases verify that default copy still works.

This change expands the shared session/journal boundary. Each accepted action
retains a base64-encoded native dump, including undo/redo history until branching
or session end. The checkpoint callback transfers only the selected action's
dump, rather than every earlier action. Capturing/compressing/restoring these
native bytes has memory, CPU and transport costs. Capability validation hashes
runtime artifacts through a bounded stream and loads the pinned adapter once.
The parent's separate resource result does not establish this candidate's
resource acceptance; matched resource validation remains an integration gate.

Headless Editor checks exercise the real form, model thread and worker. They do
not establish physical desktop interaction or visual acceptance. No curve
blacklist is added. Native solve/recompute errors remain errors. A mirrored circle
about its own center with symmetry constraints is one tested native rejection.
External geometry, periodic/rational splines and every constraint combination
need further matched acceptance cases. Offset, patterns and rigid transforms
are outside this change.

Pinned official sources:

- [Command and selection expansion](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/Sketcher/Gui/CommandSketcherTools.cpp)
- [Handler and options](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/Sketcher/Gui/DrawSketchHandlerSymmetry.h)
- [Native geometry and constraints](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/Sketcher/App/SketchObject.cpp)
- [Python API binding](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/Sketcher/App/SketchObjectPyImp.cpp)
