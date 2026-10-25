# Measure and inspect

Measure reads the current native BRep. It does not open a transaction, recompute
the document, advance its revision, add history, save, or release a modeling
gesture. Native FreeCAD 1.1.4 and OCCT 7.8.1 remain authoritative.

## Use the editor

1. Open an FCStd document or create native geometry. Click **Measure** beside the
   View controls, or press **Ctrl+M** while a text field has no focus.
2. Click an edge, face or vertex in the viewport. Use View's native picking filter
   for small vertices or overlapping elements. Click a tree row to measure the
   whole object. Bodies, nested Part containers and native Links are supported.
3. Shift-click adds or removes a second element. Ctrl/Shift-click adds or removes
   tree objects. While Measure is open, additive selection also keeps selections
   across the tree and viewport, allowing a whole object plus a subelement.
4. Read selectable results in the floating Measure / Inspect window. One selection
   reports applicable native length, boundary length, area, solid volume, center
   of gravity or vertex position, geometry type, topology counts and native
   radius/diameter, axis or direction where available. The native total boundary
   length of a solid follows FreeCAD's Shape.Length semantics, including repeated
   boundary occurrences. It is not a sum invented from display edges.
5. Two selections also report closest distance and expandable native witness pairs.
   A witness is an XYZ point in document world coordinates. Text can be selected
   and copied. Hover a value for its full numeric value in mm, mm², mm³ or degrees.
   Display text uses the document's native UnitSystem, including imperial units.
6. Straight edges provide their native parameter directions. Planar faces provide
   native oriented normals. Two lines or two planes report the angle between
   those directions in 0–180 degrees. A line and plane report the acute line to
   plane angle in 0–90 degrees. Arbitrary curved surfaces, curves and vertices
   show an explicit reason why no unique angle is reported.
7. Change the selection to recompute automatically. More than two elements shows
   a selection error. **Clear selection** removes native and sketch picks and
   clears the results. **Recompute** retries the current native references.
8. Press **Escape** or close the Measure window to dismiss it. A pending read may
   finish, but its canceled result is discarded. Dismissal preserves any open
   feature preview or sketch session. Use that modeling task's own Apply, Finish
   or Cancel afterward.

Measurements clear when a model operation starts or the selected revision changes.
Native viewport references are invalidated by the existing display refresh. Whole
tree objects can be remeasured by stable native name. An older in-flight response
cannot populate a different selection, revision or document. Measurement failures
stay in the Measure window and preserve the model's existing error and dirty state.

Discarded in-flight reads release their own request attempt so returning to that
selection automatically measures again. A native error for the current selection
stays cached until the selection changes or Recompute is pressed; it does not
queue a retry on every frame.

Link viewport workflows require the reviewed assembly display inclusion and native
frame changes. The standalone public display baseline cannot certify them.
[The mandatory combined integration gate](MEASUREMENT-INTEGRATION.md) checks actual
Link scene inclusion, headless viewport picks and native witness frames before
root can promote this workflow. The native resolver's Link scope is preserved.

During a feature preview, Measure reads the accepted native candidate. During a
sketch session, choose Select and pick native sketch curves or exposed points.
Construction geometry is supported. Non-active solids are cached context until
Finish, so Measure rejects them with a request to finish the sketch first. It
does not silently recompute downstream solids or show them as current geometry.
Infinite sketch axes have no finite edge length. Origin and other exposed native
points remain measurable.

## Native boundary

`measure` takes `document`, required `expected_revision`, and one or two selections.
For example:

```json
{"op":"measure","document":"PartStudio","expected_revision":17,"selections":[{"object":"Pad","subelement":"Face1"},{"object":"Outer","subelement":"Inner.Instance.Edge2"}]}
```

An empty subelement selects the whole object. Native subobject paths and mapped
element names are passed to getSubObject. References are scoped to their revision;
they are not detached persistent FaceN/EdgeN identities. A sketch selection can use
`{"object":"Sketch","subelement":"","sketch_geometry":{"index":0,"point":2}}`.
The native signed geometry index and exposed point ID follow the existing sketch
selection contract. `point:0` selects the finite native curve.

The response includes document/revision, preview and sketch ownership metadata,
the native document unit system, and measured selections. Scalar metrics include
`key`, `label`, `value`, `unit` and native formatted `display`. Points include
world-coordinate `value` and native formatted `display`. Optional distance includes
its scalar metric and native witness pairs. Optional angle includes a scalar
metric or an explicit `angle_unavailable` reason.

Parent global placement initializes native getSubObject traversal. The native call
owns the object's placement and any nested Link transforms. Whole App::Part shapes
use native Part.getShape aggregation. No Link.getGlobalPlacement call, rendering
mesh, tessellation length, local approximation or second document is used.

## Verification and limits

Run `tests/measurement_workflows.py --host HOST --runtime RUNTIME --output NEW_DIR`
against the pinned runtime. It builds independent native controls, compares all
declared metrics and witnesses, tests read-only snapshots and owner guards, and
checks units and save/reopen. It covers rotated nested Part/Body/Link frames,
LinkTransform and Link-to-Link cases, line/plane angles, curved-angle rejection,
mixed face/edge/vertex selection, missing/deleted references and active gestures.

The ignored native Rust test
`editor::measurements::tests::native_measure_button_shortcut_clear_cancel_recompute_and_gesture_reads`
uses actual Editor/model-thread methods and headless egui input. Set
`CAD_MEASUREMENT_RUNTIME` to a runtime JSON and `CAD_MEASUREMENT_OUTPUT` to a fresh
directory. It exercises button/shortcut entry, clearing, recomputation,
cancellation, stale in-flight replies and reads during real owned gestures.

The three ignored `editor::measurements::tests::scheduling_tests::` cases add
deterministic native Editor coverage for returning to discarded selections,
revision changes, rapid toggles, cached failures, explicit recomputation, clearing,
Escape and the actual window close button. The assembly-dependent scene test is
separate and intentionally fails on the standalone baseline.

These are bounded native and headless checks. Real Windows/Pixel GUI interaction,
clipboard use, display layout and arbitrary imported/external-link documents have
not been verified. There is no witness viewport overlay and no full Part Studio,
Assembly, FreeCAD or Rust-kernel parity claim.
