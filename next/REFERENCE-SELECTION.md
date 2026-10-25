# Union menus and native reference selection

The Part operations menu distinguishes its two native union operations:

- Features > Part operations > Union of two objects uses Base and Tool.
- Features > Part operations > Union of multiple objects uses the Shapes list.

Their existing feature IDs and native types remain unchanged. Existing files
and their native history are not converted or renamed.

## Choose and correct a native reference

1. Finish or cancel a sketch/task, then select a native object or its visible
   faces/edges. Shift adds or removes viewport selections; Ctrl/Shift toggles
   tree objects. A tree row selects the whole object.
2. Open the intended feature, or select an existing feature and Properties.
   Find the intended native reference field. Check its current object and
   subelement names before replacing it.
3. Use selection replaces that field only when the selected native object
   cardinality is valid. A Link field takes one distinct object, even when
   several selected faces belong to that same object. A LinkSub field takes
   one distinct object and retains its selected subelements in order.
4. For a list field, select all intended objects/subelements, then Use selection.
   Inspect its object rows and use Up, Down or Remove to correct order or scope.
   Repeated identical picks are deduplicated. Whole-object and face picks from
   the same object remain present in a subelement list. Native FreeCAD merges
   repeated object entries and preserves the selected empty whole-object
   subnames alongside face names; this UI follows that policy.
5. If a singular field has picks from multiple objects, Use selection is
   disabled and the field shows "Select one native object for this field.
   Clear picks from other objects and retry." Remove the other objects and
   retry. A missing object instead asks you to clear the stale selection.
   Neither rejection changes the field, task input or last accepted preview.
   Profile and Pad/Pocket termination fields also reject missing identities in
   their original reference selection. They do not silently accept the surviving
   subset. The edited feature and its temporary profile copy are intentional
   exclusions; selecting either does not hide a missing third object.
6. Wait for an accepted native preview, inspect the result, then Apply. Cancel
   restores the task baseline. When Redo exists, the task defers native work
   until Apply; opening, rejected picking and Cancel preserve that Redo.
7. If a referenced edge/face no longer works for the consuming feature, the
   native preview can reject it while retaining the previous valid candidate.
   Correct the picks and retry, or Cancel. The task does not invent geometry
   or globally reject native subnames that a reference property can retain.
8. After Apply, verify Undo/Redo. Save as to a new FCStd, reopen it, select the
   feature and Properties, inspect its native references, then edit or Cancel.

For example, an existing PartDesign Fillet can use several edges of its one
Base feature. Select those edges and use the Base field's selection button.
Selecting an edge from another object at the same time is ambiguous for Base
and leaves its existing link unchanged. Generic LinkSub cardinality does not
limit the valid same-object selection to one edge or face.

Pad/Pocket termination fields retain their separate Use native selection
buttons and operation-specific target rules. This change does not broaden
every native operation's permitted geometry or replace native validation.

## Verification and limits

The regression tests drive real egui widgets and the actual Editor menu. The
explicit native test runs the actual Editor, actor, worker, reference property
assignments, previews and Apply/Cancel. It compares nine Link/LinkSub/list
assignments with independent direct FreeCAD assignments. It also checks stale
objects, a rejected stale fillet edge, correction, history, Redo-preserving
Cancel and editing after saved reopen. Independent saved-file inspection
compares the retained two-edge fillet with a native control.

Native reference properties may retain unresolved subnames such as Face999.
That is not evidence that a geometric feature can use them. Native feature
recomputation remains authoritative and must reject invalid geometry atomically.

These are bounded headless Windows/native checks. No desktop window, physical
mouse delivery, Pixel interaction, Linux run, resource benchmark or full feature
parity is certified by this change.

Build the desktop-enabled test binary, then run the focused native workflow
with that binary, the existing opaque runtime configuration and a new output
directory. The workflow requires no private fixtures:

```sh
cargo test --manifest-path next/Cargo.toml --features desktop --locked --no-run
python next/tests/reference_selection_workflows.py --tests PATH_TO_CAD_NEXT_TEST_BINARY --runtime next/target/runtime.json --output next/target/reference-selection
```

The test binary path is printed by Cargo. On Windows use its `.exe` path.
Keep the runtime executable and argument list unchanged. The Python workflow
runs native preparation, the explicit Editor test and independent native
comparison sequentially; retain failed outputs when correcting a failed run.
