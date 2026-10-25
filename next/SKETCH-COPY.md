# Sketch Copy, Clone, Move and rectangular array

These typed tasks call pinned FreeCAD 1.1.4 Sketcher methods. Native geometry,
constraints, persistent IDs, expressions, references and recomputation remain
authoritative. This adds bounded application access. It does not complete a
Rust Sketcher replacement or full Sketcher parity.

## Use the editor

1. Open a native document and edit its sketch. Use Select to choose whole
   internal curves or standalone sketch points. Endpoints of other curves do
   not become copied geometry. Axes are references, not sources. Clear the
   selection to open a task with All internal geometry selected.
2. Choose Edit tools, then Copy, Clone, Move or Rectangular array. The existing
   task window owns the operation. All internal geometry switches between the
   captured selection and the sketch's internal geometry.
3. Enter displacement X and Y in the sketch plane. Explicit native quantities
   such as `1 in` and `-2 cm` work. Unitless edits use the document's native
   default length unit. These are action inputs, not stored expression-bound
   displacement properties.
4. For an array, set Columns from 2 to 99 and Rows from 1 to 99. Counts include
   the original. Columns follow the displacement; rows follow FreeCAD's
   clockwise perpendicular `(dy, -dx)`. Clone replaces eligible dimensions
   with native geometric relationships. Constrain inter-element separation
   creates native construction lines and their constraints. Equal
   vertical/horizontal spacing selects a perpendicular scale of 1; clearing
   it selects 0.5. The scale field also accepts other finite native factors.
   A constrained array needs nonzero spacing in every populated direction.
5. Wait for preview or choose Recompute preview. Changing a value replaces the
   candidate from the captured baseline. An error leaves the last accepted
   candidate available; correct the input or Cancel. Apply accepts one sketch
   action. Cancel restores the task baseline. With Redo pending, preview waits
   until Apply; Cancel retains that Redo branch.
6. Use sketch action Undo/Redo. Finish validates the whole model, including a
   consumed profile's downstream features. Repair a failed Finish or Cancel
   the sketch session to restore its session baseline. After Finish, document
   Undo/Redo retains the native edit. Save as a new FCStd and Open it again
   before continuing.
7. Edit the original driving dimension or its native expression after Clone.
   Eligible copied dimensions follow through native Equal/Parallel constraints.
   Copy retains the native independent dimension behavior. These commands are
   sketch actions, not persistent array feature objects with a re-edit dialog.

## Native boundary and limits

The shared Rust task builder and `cad-next-sketch` Edit command emit
`copy_geometry`, `clone_geometry`, `move_geometries` and `rectangular_array`.
The worker uses `addCopy`, `addMove` and `addRectangularArray` in its ordinary
revision-guarded transaction or owned preview. No clipboard text is evaluated.

`indices` contains distinct native internal geometry indices. An empty list
requires explicit `all_geometry:true`; the adapter expands it to all internal
geometry. It rejects axes, projected/external sources, missing IDs, malformed
flags, nonplanar displacement and unsupported parameters before native mutation.
The pinned legacy Copy/Move/array GUI filters non-external sources. Although
the application `addCopy` can read projected geometry, this task does not expose
that additional API behavior. Mixed external/internal whole-curve selections
are rejected explicitly rather than silently dropping the external selection.

Conic and B-spline source selections include the actual InternalAlignment
controls using pinned `getListOfSelectedGeoIds(true)` policy. The legacy typed
Copy/array command itself predates that helper. This task extends its selection
preparation to keep exposed native controls together. Selecting only an exposed
control without its parent still follows the native method's rejection or skip
behavior. All nine supported native geometry classes remain native.

Raw pinned `addMove` mutates geometry before its property change is registered,
so native Undo alone leaves moved geometry behind. The adapter first registers
the existing Geometry property, recovering GeomPoint extensions from native
property serialization in exact property order. It never rebuilds IDs or
matches by coordinates. Exact property, construction-point and ID restoration
was checked on native Undo/Redo.

Array count domains match `SketchRectangularArrayDialog.ui`: Columns minimum 2,
Rows minimum 1, and Qt QSpinBox's default maximum 99. They are GUI input domains,
not a new product resource cap. Signed native geometry/constraint index overflow
and nonfinite translated coordinates are rejected. The native API permits a
broader count domain; this task intentionally presents the pinned GUI domain.

The tasks use typed displacement fields. FreeCAD's pointer destination gesture,
last-selected reference point, Ctrl angle snapping and destination auto
constraints are not supplied by this form. Existing native expression/property
editing remains separate. Clipboard Copy/Paste, Rotate and Scale are outside
this bundle. Desktop interaction and other platforms need separate verification.

## Verification

Bounded direct controls compare exact native properties, IDs, construction,
element maps, Undo/Redo and fresh Open for Copy, Clone, Move and non-square
arrays, with all nine curve types and exposed controls. Actual Editor helper
tests exercise the task menus/widgets, native quantity queries, preview
replacement/failure recovery, deferred Redo, session Cancel and consumed-profile
Finish. These checks run the real worker without opening a desktop window.
Recorded desktop steps remain a separate acceptance task.

The original two Editor tests cover successful consumed-profile Finish, not a
failed Finish. A separate focused shared-host check now covers a consumed Body
with AllowCompound explicitly false: failed Clone Finish followed by action
Undo/correction and successful Finish, and failed Copy Finish followed by whole
session Cancel. Its final saved Open is compared with a baseline saved Open.
Raw native Open before solving is a different phase; the worker solves and
recomputes before publication. No tolerance or broad status normalization is
used to make those phases appear equal.

Admission checks the exact left-associated native source/column/row arithmetic
for line ends, centers, spline poles and points, including intermediate sums
and precomputed perpendicular products. Constrained-array separation uses its
native grouping and finite nonzero native lengths. A detached native arc or
spline copy supplies its post-translation reference point. This check changes
no sketch properties, identities or history. It introduces no magnitude cap;
finite large, tiny and mixed-sign point translations remain admissible when
their required native calculations are finite.

Pinned sources:

- [SketchObject.cpp](https://github.com/FreeCAD/FreeCAD/blob/4fd3bf320d9566a27e60069fc8387448aaa3a094/src/Mod/Sketcher/App/SketchObject.cpp)
- [SketchObjectPyImp.cpp](https://github.com/FreeCAD/FreeCAD/blob/4fd3bf320d9566a27e60069fc8387448aaa3a094/src/Mod/Sketcher/App/SketchObjectPyImp.cpp)
- [CommandSketcherTools.cpp](https://github.com/FreeCAD/FreeCAD/blob/4fd3bf320d9566a27e60069fc8387448aaa3a094/src/Mod/Sketcher/Gui/CommandSketcherTools.cpp)
- [SketchRectangularArrayDialog.ui](https://github.com/FreeCAD/FreeCAD/blob/4fd3bf320d9566a27e60069fc8387448aaa3a094/src/Mod/Sketcher/Gui/SketchRectangularArrayDialog.ui)
- [Qt QSpinBox maximum](https://doc.qt.io/qt-6/qspinbox.html#maximum-prop)
