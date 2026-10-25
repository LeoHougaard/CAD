# Sketch plane and attachment options

Choose **Sketch** for the ordinary plane chooser. Selecting a same-Body plane,
sketch frame or planar face first retains the direct native attachment workflow.

For explicit creation settings, right-click **Sketch**, then choose
**Create sketch with options**. Select the support and expand **Attachment
options**. All values are staged locally until **Create sketch**.

- **Reverse normal** uses the native MapReversed property.
- **Offset X/Y/Z** is a placement in the support's local frame, in millimeters.
- **Normal offset expression** replaces the numeric Z offset with a native
  length expression. Examples include `Base.Height / 2` and `0.5 in`.
- **Rotation axis** and **Angle** rotate the local offset frame. Angle uses
  degrees; a nonzero angle requires a nonzero finite axis.
- **Unattached** evaluates the chosen native support and offset, retains the
  resulting placement, then clears the support and deactivates mapping. It stops
  following that support. Its Body placement still applies, and native offset
  values/expressions remain as dormant native data.

**Create sketch** submits one guarded native transaction, including a new Body
when required. The native attachment engine determines the frame. The editor
then enters the normal sketch session. Draw geometry and add dimensions and
constraints. **Finish** validates and accepts the session. **Cancel** restores
the session baseline and removes an unaccepted newly created sketch through the
existing single creation Undo. Opening/canceling the chooser makes no native
edit and preserves Redo.

A rejected creation keeps the chooser and entered values available. Correct the
value or expression and retry, or Cancel. A native failure after object creation
rolls the whole creation transaction back. No partially attached sketch remains.

For an existing sketch, finish any active operation, select the sketch and open
**Properties**. The native `MapReversed`, `MapMode`, `AttachmentSupport`,
`AttachmentOffset` and placement/expression fields retain their existing editing
paths. Native support scope and planarity validation remain authoritative.

The public native attachment suite compares all three Body-origin planes,
reverse/offset/rotation, native length formulas, face support, detached frames,
transformed Bodies, parent updates, solid creation, Undo/Redo and saved reopen
with direct pinned native controls. Actual Editor/headless tests cover chooser
Cancel, reverse/detach controls, native session creation/cancellation and typed
expression failure/correction. Physical desktop picking and broader attachment
modes are separate open gates. This extends native access; it does not replace
FreeCAD's attachment engine with a Rust kernel.
