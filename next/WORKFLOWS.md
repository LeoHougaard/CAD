# Current Part Studio and Assembly workflows


This guide audits the replacement `next/` at `dbb932af266021f7aed579daf9c957e16247f063` on 24 October 2026. It maps all 306 existing topic IDs and 1378 requirement IDs without changing [the original checklist](coverage/onshape.json), which remains pinned to `15703c5`. The [machine-readable ledger](coverage/workflows.json) holds each exact requirement and its current family route or explicit missing operation. Every requirement remains open. Counts are inventory, never completion percentages.


The guide describes source-defined controls and bounded recorded evidence. No native, Cargo, GUI or device test was run for this audit. FreeCAD 1.1.4 and OCCT 7.8.1 remain authoritative through the Python adapter; Rust owns presentation and guarded command construction. Native feature/property access is not full task parity.


## Read a workflow


Start with the topic index, follow its shared family, then check its requirement IDs in the ledger. A candidate route is nearby implementation to inspect, not a claim that it supplies every Onshape choice. A missing family has no current menu action. Its steps describe the contract an implementer must add. Official source URLs remain in the original checklist and ledger; this audit did not refetch or reinterpret those sources.


The sequence for a usable operation is prerequisites, entry, native selection, all active inputs, preview, Apply/Cancel, existing-object edit, parent/reference change, history and saved reopen. Rejected input must have a correction or Cancel path. An operation remains open until the original applicable G1-G8 gates pass for its full declared scope.


## Start from an empty document


1. Start the replacement editor with the existing runtime configuration as documented in [README](README.md). Keep its executable and argument list opaque. Wait for Ready.
2. Choose New. If the current document is dirty, Cancel retains it; Discard permits the requested replacement.
3. Choose Sketch. The first sketch automatically creates a Body. Choose XY Plane, XZ Plane or YZ Plane, then Create sketch. Chooser Cancel makes no native edit.
4. Choose Rectangle and click opposite corners, or Circle and click center then radius. Switch to Select. Select curves/handles and add dimensions/constraints using the families below.
5. Choose Finish. Repair any solver/downstream failure while still in edit, or Undo/Cancel.
6. Choose Extrude, check Profile and active Body, enter Length, wait for a valid preview, then Apply.
7. Select a native planar face, then Sketch for an attached sketch; draw/Finish and Pocket. Select native edges before Fillet.
8. To create another solid Body choose New body, select its active Body, then Sketch and the feature chain. Boolean requires two eligible solid Bodies.
9. Save as to a new FCStd, Open it, double-click a sketch/feature, change one input, Finish/Apply, then Undo/Redo and save another copy. Independent native readback is an acceptance step; saving alone is not validation.


If most tools are disabled, wait for the native call and finish/cancel the current task, dimension, drag, support chooser or file browser. Hover disabled geometric commands for a selection reason. For a singular generic link, clear other picks before Use selection. Pad/Pocket instead validates singular picks. An unavailable document has a separate confirmed Close/discard route; Cancel retains its owner and a rejected Close allows retry.


## Shared workflow families


<a id="wf-document"></a>

### document: Local document lifecycle


Current access: bounded-ui-access.


Entry: New; Open; Save; Save as; Undo; Redo; Native document unavailable > Close/discard confirmation.


Prerequisites

- Start the replacement editor with its existing opaque runtime configuration. Wait for Ready. Only one document is exposed at a time.


Selection

- No geometry selection is needed.


Declared inputs

- New; Open FCStd; Save; Save as; folder navigation; FCStd filter; destination filename; replace confirmation; unsaved Discard/Cancel confirmation.


1. Start. Choose New for an empty native document, or Open and pick an FCStd file. Cancel the file browser to abandon the request. If prompted about unsaved changes, Cancel returns to the current document; Discard permits replacement.

2. Inputs. Save first opens Save as when no native path exists. Browse to a writable directory, enter a new FCStd filename and accept the browser. Confirm replacement only for an intentional existing destination. Ctrl+S saves; Ctrl+Shift+S opens Save as.

3. Preview. There is no modeling preview for New/Open/Save. A browser/confirmation is pending UI only; canceling it starts no native file operation.

4. Apply. Accept the selected file in the browser, or confirm Discard for a requested replacement. New/Open replace the exposed document only after native success. Save has no separate feature Apply button.

5. Cancel. Cancel the file browser, overwrite or unsaved confirmation to retain the current document. Native writes already in flight cannot be canceled by closing a chooser.

6. Edit. Open an FCStd, then double-click an existing sketch/feature or select Properties. No document properties/branch editor is implied.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. Undo/Redo accepted native modeling edits, not file-browser navigation. Save changes revision but is not a replacement for model history. Verify unsaved state after document replacement.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No multi-tab document UI, cloud document service, versions/branches/merge, explicit old .cadpart importer or cross-process locking contract.


Evidence: source-audit, rust-record, native-record, pixel-earlier, pixel-wave1, pixel-wave2. Acceptance stays open.


<a id="wf-selection"></a>

### selection: Native picking and ordered selection


Current access: bounded-ui-access.


Entry: View / Fit / Iso; Model tree > right-click > Set tip here / Roll back to start / Reorder; View > Selection > Faces and edges; View > Selection > Faces; View > Selection > Edges; View > Selection > Vertices.


Prerequisites

- A Ready document with visible selectable geometry. Enter a sketch for sketch entities.


Selection

- Solid tree rows select whole native objects. Ctrl/Shift-click toggles additional tree objects.
- View > Selection chooses Faces and edges, Faces, Edges or available Vertices. Click a visible native subelement; Shift adds/removes.
- In a sketch click exposed point handles separately from whole curves; Shift adds/removes. Repeated Shift-clicks cycle coincident handles. Origin is a point; axes are whole native curves away from geometry.
- Drag blank sketch space for a contained box; right-to-left also crosses curves. Shift adds the box. Selection order is retained by command construction.
- Link-list fields have Up, Down, Remove, Add object and Use selection. Explicitly inspect this list for section, original and support order.


Declared inputs

- Selection filter; Shift/Ctrl; ordered native object/subelement list; no inferred detached mesh identities.


1. Start. Choose Select in a sketch, or a selection filter in View outside it.

2. Inputs. Make one point/curve/edge/face/object pick at a time. Clear unwanted picks. Use a field selection button only after confirming its intended native owner. For singular generic fields Use selection currently takes the first selected link; clear all but one. Pad/Pocket targets instead reject multiple singular picks.

3. Preview. Hover/selection highlights are display feedback, not a native modeling candidate.

4. Apply. A pick updates selection immediately. There is no Apply for selection itself. The consuming task selection button copies native identities into its input.

5. Cancel. Toggle a selected pick off with the additive modifier, clear selection with the relevant Select interaction, or abandon the consuming task. Escape ends active picking tools; it does not undo previously accepted sketch actions.

6. Edit. Replace the native picks and use the consuming field selection button again. Use Up/Down on lists to change order.

7. Parent update. Refresh selections after native recompute/rollback/history. Never keep an old face/edge index as a substitute for a native saved link.

8. History. Selections themselves have no document Undo entry. Verify history through their consuming action.

9. Save reopen. Selection state is not a saved modeling operation. Verify that the consuming feature retains exact native links after FCStd reopen.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No general Select other/hidden-hit chooser, Create selection rules, full triad, command search, touch workflow or complete selection variants.


Evidence: source-audit, rust-record, pixel-wave2. Acceptance stays open.


<a id="wf-view"></a>

### view: Camera, display and visibility


Current access: bounded-ui-access.


Entry: View / Fit / Iso; View > Front; View > Back; View > Left; View > Right; View > Top; View > Bottom; View > Isometric; View > Shaded; View > Shaded with edges; View > Wireframe; View > Hide selected; View > Show selected; View > Show all.


Prerequisites

- Open a native document; Fit may be used on an empty model.


Selection

- Select owning objects or faces/edges before Hide selected/Show selected. Visibility operates on native owners.


Declared inputs

- Front, Back, Left, Right, Top, Bottom, Isometric; Fit all; Shaded, Shaded with edges, Wireframe; selection filters; Hide selected, Show selected, Show all.


1. Start. Right-drag to orbit, middle-drag to pan, wheel to zoom. F fits; Iso uses the isometric direction.

2. Inputs. Choose the View direction/display mode. Use explicit visibility actions outside an active modeling task. Reopen a sketch to align to its native frame.

3. Preview. Camera/display changes update presentation immediately and do not open a modeling preview.

4. Apply. Choose the view/display action directly. Show/Hide changes native owner visibility through explicit commands; there is no view Apply button.

5. Cancel. Stop the navigation gesture or choose a different view/display mode. No camera-preview Cancel exists.

6. Edit. Choose another direction/display mode or Show/Hide action. Native Properties intentionally excludes Visibility from parameter previews.

7. Parent update. Recompute native geometry then Fit. Check moved coordinate frames and displayed Body Tips without resurrecting hidden historical solids.

8. History. Native Visibility is Output/NoModify and does not create its own native Undo entry. Do not promise camera/visibility Undo from modeling transactions.

9. Save reopen. Save/reopen can check native visibility and displayed Body Tips. Camera/display-session persistence is not certified by this audit.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No named/previous/perspective/section view, view cube, isolate/transparency task, tangent/hidden-edge controls, high-quality analysis or AR workflow.


Evidence: source-audit, pixel-earlier. Acceptance stays open.


<a id="wf-exchange"></a>

### exchange: STEP/IGES import and export


Current access: bounded-ui-access.


Entry: Import; Export.


Prerequisites

- Ready native document; no active sketch or preview owner. Keep originals intact.


Selection

- Export checks explicit native shape roots. Select the enclosing Part when its hierarchy/placement is required.


Declared inputs

- Import path; Export path; Browse; STEP/IGES filter; export object checkboxes; Replace existing file when exporting.


1. Start. Choose Import or Export in the toolbar. Browse navigates folders and filters .step/.stp/.iges/.igs.

2. Inputs. For Import choose the source then Import. For Export choose the destination format, check intended objects and set the separate replacement checkbox only when intentional. A path returned by Browse does not itself authorize export replacement.

3. Preview. Import/export has no shape preview in this dialog. Browse is file picking only.

4. Apply. Choose Import or Export in its dialog after assigning path and required objects. Import commits guarded native document edits; Export publishes a temporary output atomically without changing document/revision.

5. Cancel. Cancel the idle exchange dialog or its browser before submission. A canceled browser starts no native exchange operation.

6. Edit. Imported native objects can use supported Properties. Export has no editable feature/history object; repeat it with a new destination and selection.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. Import uses native document Undo/Redo. Export is a read/output operation and creates no document history step.

9. Save reopen. Save imported documents as new FCStd and reopen them. Independently open exported STEP/IGES, compare native shapes/placements and preserve all source files.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No other exchange formats, Lidar, mixed modeling, cloud translation/download jobs or editable legacy .cadpart migration. Exchange geometry does not establish source parametric history.


Evidence: source-audit, native-record, pixel-earlier. Acceptance stays open.


<a id="wf-sketch-create"></a>

### sketch-create: Create, finish and reopen a native sketch


Current access: bounded-ui-access.


Entry: Sketch; Sketch edit > Finish / Cancel.


Prerequisites

- New produces an empty document. Sketch creates a Body automatically when none exists; New body is optional. Choose the active Body for later sketches.


Selection

- With no valid support picked, choose XY Plane, XZ Plane or YZ Plane. Same-Body datum plane/sketch-frame or planar solid face can supply support. Selected valid support starts creation directly.


Declared inputs

- Sketch plane chooser; origin role; existing same-Body datum/sketch frame; Create sketch; Cancel. Reverse and offset require later native Properties, not a creation-time orientation dialog.


1. Start. Choose Sketch. Without a selected support, choose an origin plane and Create sketch. Canceling the chooser makes no document write. A supported selected face/frame bypasses the chooser.

2. Inputs. Draw and constrain native geometry, inspect remaining freedom and errors. Finish validates all downstream features. On failed Finish, remain in edit and repair, Undo or Cancel. Cancel of a newly created sketch also attempts cleanup Undo; do not assume it removed an existing Body.

3. Preview. The support chooser opens no transaction or geometry preview. Native creation occurs only after Create sketch, or immediately for a valid selected support.

4. Apply. Choose Create sketch, then wait for the attached sketch edit session. Finish validates the whole document and accepts its actions; there is no separate creation Apply.

5. Cancel. Before creation, chooser Cancel changes no model and preserves Redo. During sketch edit, session Cancel restores its baseline; a new-sketch cleanup Undo can also remove its creation. On cleanup failure the UI clears pending cleanup and refreshes state.

6. Edit. Double-click the sketch tree row to edit its geometry. Use Properties for available native attachment reverse/offset values; baseline has no creation-time options task.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No standalone sketch/orientation/reverse/offset creation dialog, foreign-Body support chooser, connector support, Disable imprinting or whole-versus-split region selection.


Evidence: source-audit, rust-record, native-record, pixel-wave1. Acceptance stays open.


<a id="wf-sketch-draw"></a>

### sketch-draw: Point, line, rectangle, circle and arc drawing


Current access: bounded-ui-access.


Entry: Sketch edit > Point / Line / Polyline / Rectangle / Circle / Arc; Sketch edit > Construction or G; Sketch edit > Finish / Cancel; Sketch edit > Select; Sketch edit > Point; Sketch edit > Line; Sketch edit > Polyline; Sketch edit > Rectangle; Sketch edit > Circle; Sketch edit > Arc.


Prerequisites

- An active native sketch session from Sketch or a sketch tree double-click.


Selection

- Point: one click. Line: start then end. Polyline: successive connected endpoints; Escape ends the current chain.
- Rectangle: two opposite corners. Circle: center then point on radius. Arc: start, point on the arc, end, all distinct and noncollinear.


Declared inputs

- Point positions in the sketch frame; Construction/G; endpoint/center/origin/curve snapping; inferred horizontal/vertical; Alt disables snapping/inference.


1. Start. Choose the drawing tool, then click its ordered positions. Pointer graphics are feedback; only returned native solved geometry is authoritative.

2. Inputs. Switch to Select after drawing. Drag native endpoints or centers to edit. Delete selected internal geometry, then constrain dimensions explicitly. Preview of a new line/circle does not fix its position or size.

3. Preview. Unfinished click input draws a pointer overlay only. The native solver receives the completed curve; dragging existing geometry uses guarded native preview when allowed, or deferred release when Redo must remain intact.

4. Apply. The final required click creates a basic/conic curve; Enter/Create curve creates a spline. No per-curve Apply button exists. Check the returned solved geometry before the next action.

5. Cancel. Escape abandons unfinished click input or the current drag/tool. Accepted curves remain until action Undo or whole-session Cancel. Finish accepts the sketch session only after downstream validation.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No Midpoint line, center/aligned rectangle, three-point circle, tangent/center-point arc, polygon, text, slot or Bezier drawing workflow.


Evidence: source-audit, native-record, pixel-wave1. Acceptance stays open.


<a id="wf-sketch-curves"></a>

### sketch-curves: Conics and B-splines


Current access: bounded-ui-access.


Entry: Sketch edit > Curves; Sketch edit > Edit tools; Sketch edit > Curves > Ellipse; Sketch edit > Curves > Elliptic arc; Sketch edit > Curves > B-spline; Sketch edit > Curves > Periodic B-spline; Sketch edit > Curves > Hyperbolic arc; Sketch edit > Curves > Parabolic arc; Sketch edit > Curves > Expose internal geometry; Sketch edit > Create curve (Enter).


Prerequisites

- An active native sketch session.


Selection

- Ellipse: center, major-axis endpoint, minor-axis endpoint. Elliptic/hyperbolic arc: those three, then start and end.
- Parabolic arc: vertex, focus, start, end. B-spline: click poles, at least two open or three periodic, then Create curve or Enter.


Declared inputs

- Ordered curve clicks; open/periodic choice; Construction; Expose internal geometry for native axes/pole-circle controls.


1. Start. Choose the curve under Curves, then follow its displayed click hint. Enter creates a spline; Escape cancels unfinished point input.

2. Inputs. Select the native curve then Curves > Expose internal geometry. Select its native controls for supported constraints or Edit tools spline tasks. No artificial endpoint or pole identity is added.

3. Preview. Unfinished click input draws a pointer overlay only. The native solver receives the completed curve; dragging existing geometry uses guarded native preview when allowed, or deferred release when Redo must remain intact.

4. Apply. The final required click creates a basic/conic curve; Enter/Create curve creates a spline. No per-curve Apply button exists. Check the returned solved geometry before the next action.

5. Cancel. Escape abandons unfinished click input or the current drag/tool. Accepted curves remain until action Undo or whole-session Cancel. Finish accepts the sketch session only after downstream validation.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No general Bezier, interpolation/fit-point workflow, arbitrary curve conversion or every spline/Onshape conic option.


Evidence: source-audit, native-record, pixel-earlier. Acceptance stays open.


<a id="wf-sketch-reference"></a>

### sketch-reference: Project and manage native sketch references


Current access: bounded-ui-access.


Entry: Sketch edit > External; Sketch edit > Construction or G; Sketch edit > External > Project edge or vertex; Sketch edit > External > Apply flags to selected references.


Prerequisites

- Active target sketch; visible source edge/vertex and valid native dependency.


Selection

- External > Project edge or vertex, then click native source geometry. Projected handles constrain internal geometry but cannot be dragged.
- Select projected geometry to change flags or Delete it; refresh signed geometry and constraint indices after changes.


Declared inputs

- Defining profile geometry; Intersection; Apply flags to selected references; source object/subelement from native picking.


1. Start. Set Defining profile geometry and Intersection as intended, activate Project edge or vertex, then click each source. Escape ends projection.

2. Inputs. Select existing reference curves/points and Apply flags to selected references to change their modes. Delete removes the persistent projection through native commands. Edit the source, then inspect target updates before Finish.

3. Preview. External picking has no separate property-preview window. The native action returns its solved projection/reference metadata.

4. Apply. Clicking a source adds the reference as one native sketch action. Apply flags to selected references changes existing flags directly. No general feature Apply is used.

5. Cancel. Escape ends external picking; already accepted projections remain. Use action Undo to remove the last action, or session Cancel to restore the session baseline.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: Trackable silhouettes, every binder/projection combination and all transformed/projected-point desktop variants remain unaccepted. Missing metadata must fail source-dependent edits explicitly.


Evidence: source-audit, native-record, pixel-wave2. Acceptance stays open.


<a id="wf-sketch-modify"></a>

### sketch-modify: Trim, split, fillet/chamfer, extend and spline tasks


Current access: bounded-ui-access.


Entry: Sketch edit > Edit tools; Sketch edit > Trim; Sketch edit > Edit tools > Fillet; Sketch edit > Edit tools > Chamfer; Sketch edit > Edit tools > Extend; Sketch edit > Edit tools > Carbon Copy; Sketch edit > Edit tools > Spline degree; Sketch edit > Edit tools > Spline knots; Sketch edit > Edit tools > Spline pole position; Sketch edit > Edit tools > Spline pole weight; Sketch edit > Edit tools > Split curve.


Prerequisites

- Active native sketch session. Select internal geometry or an exposed native control, not a projected curve.


Selection

- Fillet/Chamfer: one internal endpoint, or two whole curves with picked positions. Extend: line/arc endpoint. Split: activate Split curve and pick the curve position.
- Spline tasks: whole internal spline, or exposed pole circle for weight editing. Carbon Copy uses another same-document sketch.


Declared inputs

- Fillet/Chamfer: Radius, each of two curve pick positions when applicable, Trim original curves, Preserve construction corner.
- Extend: Length extension for a line or Angular extension for an arc. Carbon Copy: Source sketch, Construction geometry.
- Spline degree: Degree change. Knots: Insert knot toggles parameter/multiplicity versus native 1-based knot index/multiplicity change.
- Spline pole position: 1-based Pole selector and X (mm)/Y (mm) controls in the sketch frame. Weight: Pole, Dimensionless weight, explicitly selected native Equal constraints to release.


1. Start. Choose Edit tools task after selecting its inputs. Trim and Split instead pick the segment/position directly. Escape ends the picking tool.

2. Inputs. Edit every relevant task field, inspect Recompute preview, then Apply one sketch action or Cancel to its baseline. Radius seeds a sketch fillet; add a native radius dimension to drive it later. Native spline degree reduction/knot reduction may replace or approximate geometry and remove constraints; inspect the returned snapshot.

3. Preview. Inspect the native candidate and error message. Recompute preview retries a complete candidate. If the task says Apply updates the model, Cancel keeps Redo, no native preview has run; Apply still requires native validation.

4. Apply. Task forms Apply accepts one native sketch action. Trim and Split instead commit the clicked segment/position directly, with no task Apply. Inspect reindexed native geometry and constraints.

5. Cancel. Task Cancel restores its captured baseline. Escape stops Trim/Split picking, retaining earlier accepted actions until action Undo or session Cancel. In-flight calls temporarily disable task submissions.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Tasks require an idle active sketch and compatible internal picks. Weight editing with Redo requires internal controls already exposed; otherwise the UI asks to expose them first. Expose is an accepted action, so choose that history branch deliberately. Invalid task selections report an error before opening the task.


Remaining operation: Sketch offset, mirror/pattern/rigid transform, slot, text, DXF/DWG and image insertion have no task. Broader native split/spline cases still need acceptance.


Evidence: source-audit, native-record, pixel-earlier. Acceptance stays open.


<a id="wf-sketch-constraint"></a>

### sketch-constraint: Geometric sketch constraints


Current access: bounded-ui-access.


Entry: Sketch edit > Constrain; Sketch edit > Constraints panel; Sketch edit > Constrain > Horizontal; Sketch edit > Constrain > Vertical; Sketch edit > Constrain > Coincident; Sketch edit > Constrain > Point on object; Sketch edit > Constrain > Parallel; Sketch edit > Constrain > Perpendicular; Sketch edit > Constrain > Tangent; Sketch edit > Constrain > Equal; Sketch edit > Constrain > Symmetric; Sketch edit > Constrain > Fix.


Prerequisites

- Active native sketch session, Select tool and native selections from the current snapshot.


Selection

- Select whole curves for Horizontal/Vertical, parallel/perpendicular/tangent/equal; select exposed points for point relations. Native command validation reports the required compatible combination.
- Coincident accepts point relations; Point on object takes a point and whole curve. Symmetric needs a supported native point/axis combination. Fix maps to native Block.
- Tangent and Perpendicular also accept two whole curves plus an exposed point in any point position; curve order remains selected order.


Declared inputs

- Horizontal; Vertical; Coincident; Point on object; Parallel; Perpendicular; Tangent; Equal; Symmetric; Fix. Existing rows: Name/Rename, Active, native-supported Driving, Delete constraint.


1. Start. Select the compatible native points/curves, open Constrain, hover a disabled command for its reason, then choose the enabled relation.

2. Inputs. The relation commits one atomic native action. Inspect remaining freedom and constraints. Select a constraint row to highlight its actual entities; edit its name/state or delete it. Correct solver conflicts in the current sketch before Finish.

3. Preview. Geometric constraints use one atomic native action and no dimension or preview dialog. Native solver output is authoritative.

4. Apply. Choose the enabled Constrain command to commit it directly. Rename, Active, Driving and Delete constraint also submit native actions; there is no separate relation Apply.

5. Cancel. Before choosing the command, change the picks or dismiss the menu. After success use sketch-action Undo or session Cancel. A failed relation leaves the accepted sketch available for repair.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No dedicated Concentric, Midpoint, Normal, Pierce or Curvature command. Coincident centers are only a bounded alternative, not full concentric parity. Selection/conflict combinations remain open.


Evidence: source-audit, native-record. Acceptance stays open.


<a id="wf-sketch-dimension"></a>

### sketch-dimension: Dimensions, expressions and measured angles


Current access: bounded-ui-access.


Entry: Sketch edit > Dimension or D; Sketch edit > Constraints panel; Sketch edit > Dimension > Length; Sketch edit > Dimension > Horizontal distance; Sketch edit > Dimension > Vertical distance; Sketch edit > Dimension > Radius; Sketch edit > Dimension > Diameter; Sketch edit > Dimension > Angle; Sketch edit > Dimension > Refraction ratio n2/n1; Sketch edit > Dimension > Smart dimension.


Prerequisites

- Active native sketch, no drag/task/dimension submission; compatible selected entities.


Selection

- D/Smart dimension chooses a supported length, radius or angle. Explicit menu choices are Length, Horizontal distance, Vertical distance, Radius, Diameter, Angle and Refraction ratio n2/n1.
- Length also measures arc length or circle-boundary clearance. Angle measures an arc span or two curves. Angle through a point uses two whole curves and one exposed point; native X/Y axes are curve identities.
- Refraction: two straight-ray endpoints in n1 then n2 order and a whole boundary curve. Repeated Shift-click selects coincident handles.


Declared inputs

- Datum text with compatible native units. Ordinary new dimensions first accept a datum; reopen the created dimension for its Expression checkbox. Existing dimensions and AngleViaPoint preparation expose Expression where applicable.
- AngleViaPoint uses a native measured seed and exposes Reference. Reference disables direct value/expression editing. Existing rows expose Name/Rename, Active, Driving, Edit value and Delete constraint. A reference dimension must switch to Driving before value editing.


1. Start. Select entities, press D or choose the explicit Dimension command. Inspect the dialog seed and enter a datum. To add an ordinary expression, create the dimension first, then double-click its annotation/row or choose Edit value, and enable Expression.

2. Inputs. Apply or Enter submits once. Rejected dimensions retain entered text and expression mode for correction; pending calls disable duplicate Apply/Cancel. Angle preparation may own a native preview; Cancel must restore it. Both fixed curves can create a reference angle directly. With Redo, preparation waits until Apply.

3. Preview. Ordinary new/edit dimensions submit on Apply without a live dimension preview. AngleViaPoint can prepare a native candidate and measured seed before Apply; existing Redo defers that preparation. Both-fixed reference mode can create directly.

4. Apply. Wait until the worker is idle and Apply is enabled. Apply commits one native transaction or one sketch action. Confirm the returned shape, identities and history before continuing.

5. Cancel. Choose the task Cancel, or Escape where supported. Confirm the task closes and its native baseline returns. Cancel of a sketch session is a separate action. An in-flight native call temporarily disables submission.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: Full signed/reflex/multiturn desktop breadth, label placement, tolerances/precision and every native reference/activation variant remain open. SnellsLaw has native unsupported spline cases and broader GUI creation remains unaccepted.


Evidence: source-audit, rust-record, native-record, pixel-wave1, pixel-wave2. Acceptance stays open.


<a id="wf-native-form"></a>

### native-form: Shared native feature/property task


Current access: bounded-ui-access.


Entry: Properties or feature tree double-click; Features > Additive > Pad / Extrude; Features > Additive > Revolve; Features > Additive > Additive loft; Features > Additive > Additive sweep; Features > Subtractive > Pocket; Features > Subtractive > Groove / Revolved cut; Features > Subtractive > Subtractive loft; Features > Subtractive > Subtractive sweep; Features > Subtractive > Hole; Features > Dress-up > Fillet; Features > Dress-up > Chamfer; Features > Dress-up > Draft; Features > Dress-up > Shell / Thickness; Features > Transform > Linear pattern; Features > Transform > Circular pattern; Features > Transform > Mirror; Features > Transform > Multiple transforms; Features > Transform > Scaled pattern; Features > Reference > Datum plane; Features > Reference > Datum line; Features > Reference > Datum point; Features > Document > Part container; Features > Part primitives > Box; Features > Part operations > Chamfer; Features > Part operations > Intersect; Features > Part primitives > Cone; Features > Part operations > Subtract; Features > Part primitives > Cylinder; Features > Part operations > Extrusion; Features > Part operations > Fillet; Features > Part operations > Union; Features > Part operations > Revolution; Features > Part primitives > Sphere; Features > Part primitives > Torus; Features > Additive primitives > Additive Box; Features > Additive primitives > Additive Cone; Features > Additive primitives > Additive Cylinder; Features > Additive primitives > Additive Ellipsoid; Features > Additive > Additive Helix; Features > Additive primitives > Additive Prism; Features > Additive primitives > Additive Sphere; Features > Additive primitives > Additive Torus; Features > Additive primitives > Additive Wedge; Features > Body operations > Body Boolean; Features > Reference > Datum coordinate system; Features > Reference > Shape binder; Features > Reference > Subshape binder; Features > Subtractive primitives > Subtractive Box; Features > Subtractive primitives > Subtractive Cone; Features > Subtractive primitives > Subtractive Cylinder; Features > Subtractive primitives > Subtractive Ellipsoid; Features > Subtractive > Subtractive Helix; Features > Subtractive primitives > Subtractive Prism; Features > Subtractive primitives > Subtractive Sphere; Features > Subtractive primitives > Subtractive Torus; Features > Subtractive primitives > Subtractive Wedge.


Prerequisites

- Ready document. New body before PartDesign features; a compatible existing solid is needed for subtractive/dress-up operations. A valid sketch or native reference may be required.


Selection

- Select profile/object/subelements before opening the task to seed native fields. The active Body is the modeling target. Inspect each resulting link explicitly.
- For native link lists Add object/Use selection, then Up/Down/Remove defines order. For a singular generic link Use selection uses the first pick; clear additional picks. Type comma-separated native subelements only when actual identities are known.


Declared inputs

- Every declared preferred field is listed in the native-task inventory below. Find property searches displayed native properties; All properties exposes additional native-schema fields subject to read-only protection.
- Enums use native choices; quantities accept native text; Bool is a checkbox; vectors are x/y/z; Color has four numeric components; Placement is position plus quaternion x/y/z/w; lists have Add value/Remove and maps have editable key/text plus Add entry/Remove.
- fx enables a native expression and disables the direct value. Expression paths accepts Property or component path plus Expression, Set expression and Clear expression.
- The live native schema owns additional/conditional fields, defaults, units and documentation. This audit ran no native schema discovery; unlisted runtime properties are an explicit input-audit gap, not accepted options.


1. Start. Choose Features > group > operation, or select an existing object then Properties. Choose the correct native form using the inventory; do not treat neighboring names as interchangeable.

2. Inputs. Fill every required link/controller and dimension. Hover field names for native documentation/status. Show All properties and search for a required field. An unavailable required field is a blocker; do not force a protected computed field. Recompute preview after correcting inputs.

3. Preview. Inspect the native candidate and error message. Recompute preview retries a complete candidate. If the task says Apply updates the model, Cancel keeps Redo, no native preview has run; Apply still requires native validation.

4. Apply. Wait until the worker is idle and Apply is enabled. Apply commits one native transaction or one sketch action. Confirm the returned shape, identities and history before continuing.

5. Cancel. Choose the task Cancel, or Escape where supported. Confirm the task closes and its native baseline returns. Cancel of a sketch session is a separate action. An in-flight native call temporarily disables submission.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: A catalog/native field is interface access only. Task-specific selection inference, manipulators, native dialog preferences, every conditional schema and each documented Onshape semantic contract remain unaccepted.


Evidence: source-audit, native-record. Acceptance stays open.


<a id="wf-extrusion"></a>

### extrusion: Pad and Pocket with native terminations


Current access: bounded-ui-access.


Entry: Features > Additive > Pad / Extrude; Features > Subtractive > Pocket; Properties or feature tree double-click.


Prerequisites

- Create/Finish a native profile sketch. Activate the intended Body. Pocket needs compatible existing material. For a new separate solid, New body and a suitable sketch/copy are required.


Selection

- Select the profile. Select exactly one native face/axis reference for UpToFace/ReferenceAxis. UpToShape accepts a native object/subelement list including a whole object. Use that field's Use native selection button.
- A foreign Sketch profile exposes Independent copy, Dependent copy and Cross-reference. Choose intentionally; independent retains supported internal constraints/expressions but removes external geometry and attachment. Native incompatibility rejects atomically.


Declared inputs

- Profile; SideType: One side, Symmetric, Two sides. Type/Type2 native extent enums, Length/Length2, Offset/Offset2, UpToFace/UpToFace2, UpToShape/UpToShape2, TaperAngle/TaperAngle2.
- UseCustomVector selects Direction x/y/z versus ReferenceAxis; AlongSketchNormal is available for compatible reference/custom length cases; Reversed depends on mode.
- Type2/second-side fields require Two sides. Length requires Length mode; taper requires Length or ThroughAll; face/shape link matches that extent; offsets apply to UpToFace/Shape/First/Last. Inactive values/links/expressions remain retained.
- Controller expressions keep possibly needed fields reachable even after incomplete preview failure. Native enum choices and valid geometry differ for Pad/Pocket; unsupported native modes fail explicitly.


1. Start. Finish the sketch, then Extrude/Pocket, or choose the Pad/Pocket native menu entry. Check the selected Profile and active Body.

2. Inputs. Choose SideType and each side's extent before entering targets/dimensions. Set custom/reference direction and offset/taper where active. Use native selection on the intended field, inspect the preview, Apply or Cancel. Repeat modes without losing dormant values.

3. Preview. Inspect the native candidate and error message. Recompute preview retries a complete candidate. If the task says Apply updates the model, Cancel keeps Redo, no native preview has run; Apply still requires native validation.

4. Apply. Wait until the worker is idle and Apply is enabled. Apply commits one native transaction or one sketch action. Confirm the returned shape, identities and history before continuing.

5. Cancel. Choose the task Cancel, or Escape where supported. Confirm the task closes and its native baseline returns. Cancel of a sketch session is a separate action. An in-flight native call temporarily disables submission.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No complete Onshape solid/surface/thin result mode, New/Add/Remove/Intersect merge-scope workflow, arbitrary sketch-region/vertex/connector termination, start-entity offset or viewport manipulator. Broader imported/legacy targets and transformed formulas remain unaccepted.


Evidence: source-audit, rust-record, native-record, pixel-wave1, pixel-wave2. Acceptance stays open.


<a id="wf-revolve"></a>

### revolve: Revolution and Groove native forms


Current access: bounded-ui-access.


Entry: Features > Additive > Revolve; Features > Subtractive > Groove / Revolved cut; Features > Part operations > Revolution; Properties or feature tree double-click.


Prerequisites

- Finished valid profile; active Body for PartDesign; solid for Groove; native axis.


Selection

- Profile plus explicit native ReferenceAxis or Part Source/AxisLink. Confirm whole native axis/subelement ownership.


Declared inputs

- Catalog fields for the selected type; Angle/Angle2, Type, Midplane/Reversed where native; Part Solid/Symmetric/Axis/Base where declared.


1. Start. Select the profile, choose Features > Additive > Revolve or Subtractive > Groove / Revolved cut. Part operations > Revolution is a different native type.

2. Inputs. Assign Profile/Source and native reference axis, mode and angular extents. Inspect angle units and expression state; use the shared native task lifecycle.

3. Preview. Inspect the native candidate and error message. Recompute preview retries a complete candidate. If the task says Apply updates the model, Cancel keeps Redo, no native preview has run; Apply still requires native validation.

4. Apply. Wait until the worker is idle and Apply is enabled. Apply commits one native transaction or one sketch action. Confirm the returned shape, identities and history before continuing.

5. Cancel. Choose the task Cancel, or Escape where supported. Confirm the task closes and its native baseline returns. Cancel of a sketch session is a separate action. An in-flight native call temporarily disables submission.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No complete thin/surface modes, selected region workflow, merge scopes or revolution manipulators; every axis/second-angle GUI combination remains open.


Evidence: source-audit, native-record. Acceptance stays open.


<a id="wf-loft"></a>

### loft: Ordered native loft sections


Current access: bounded-ui-access.


Entry: Features > Additive > Additive loft; Features > Subtractive > Subtractive loft; Properties or feature tree double-click.


Prerequisites

- At least the required valid profile sketches; active target Body, compatible base solid for subtractive loft.


Selection

- Profile is first section; Sections is an ordered native link list. Add object or Use selection, then inspect Up/Down order explicitly.


Declared inputs

- Profile, Sections, Ruled, Closed; additional fields require live schema inspection.


1. Start. Choose Features > Additive > Additive loft or Subtractive > Subtractive loft.

2. Inputs. Set first Profile and remaining Sections in traversal order. Adjust Ruled/Closed and supported additional fields. Recompute preview and inspect correspondence and validity before Apply.

3. Preview. Inspect the native candidate and error message. Recompute preview retries a complete candidate. If the task says Apply updates the model, Cancel keeps Redo, no native preview has run; Apply still requires native validation.

4. Apply. Wait until the worker is idle and Apply is enabled. Apply commits one native transaction or one sketch action. Confirm the returned shape, identities and history before continuing.

5. Cancel. Choose the task Cancel, or Escape where supported. Confirm the task closes and its native baseline returns. Cancel of a sketch session is a separate action. An in-flight native call temporarily disables submission.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No complete guides/continuity/end-condition/connection editor, point profiles, thin/surface modes, path section counts or documented nested-loop restrictions. Ordered list editing alone does not establish loft parity.


Evidence: source-audit, native-record. Acceptance stays open.


<a id="wf-sweep"></a>

### sweep: Native sweep and helix forms


Current access: bounded-ui-access.


Entry: Features > Additive > Additive sweep; Features > Subtractive > Subtractive sweep; Features > Additive > Additive Helix; Features > Subtractive > Subtractive Helix; Properties or feature tree double-click.


Prerequisites

- Finished profile; path/axis; active target Body, existing material for subtractive operation.


Selection

- Profile then Spine/native axis. AuxiliarySpine is a separate native reference. Inspect selected subelement chain/order; no path-chain builder is claimed.


Declared inputs

- Pipe: Profile, Spine, Transition, Transformation, AuxiliarySpine. Helix: Profile, ReferenceAxis, Mode, Pitch, Height, Angle, Placement.


1. Start. Choose Additive/Subtractive sweep or Helix under Features.

2. Inputs. Fill Profile and path/axis, then the native transition/transformation or helix mode dimensions. Use shared property lists and preview lifecycle; verify native direction/frame after parent edits.

3. Preview. Inspect the native candidate and error message. Recompute preview retries a complete candidate. If the task says Apply updates the model, Cancel keeps Redo, no native preview has run; Apply still requires native validation.

4. Apply. Wait until the worker is idle and Apply is enabled. Apply commits one native transaction or one sketch action. Confirm the returned shape, identities and history before continuing.

5. Cancel. Choose the task Cancel, or Escape where supported. Confirm the task closes and its native baseline returns. Cancel of a sketch session is a separate action. An in-flight native call temporarily disables submission.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No complete guide/orientation-lock/twist/thin/surface/scale/trim-end UI or curve-pattern workflow. Native supported modes and topology failure behavior need separate cases.


Evidence: source-audit, native-record. Acceptance stays open.


<a id="wf-dressup"></a>

### dressup: Native fillet, chamfer, draft and thickness forms


Current access: bounded-ui-access.


Entry: Features > Dress-up > Fillet; Features > Dress-up > Chamfer; Features > Dress-up > Draft; Features > Dress-up > Shell / Thickness; Features > Part operations > Chamfer; Features > Part operations > Fillet; Properties or feature tree double-click.


Prerequisites

- A compatible valid solid; active Body for PartDesign.


Selection

- Select native edges for fillet/chamfer; faces and native neutral plane/pull direction for Draft; faces to remove for Thickness. Keep selected edges on the intended Base owner.
- Part Fillet/Chamfer Edges uses Add edge/Remove and native edge data; foreign owner picks must reject, even when Edge numbers coincide.


Declared inputs

- Fillet Base/Radius/UseAllEdges; Chamfer Base/ChamferType/Size/Size2/Angle/FlipDirection/UseAllEdges; Draft Base/Angle/NeutralPlane/PullDirection/Reversed; Thickness Base/Value/Mode/Join/Intersection/Reversed; Part Base/Edges.
- Part Edges rows expose Native edge index, start radius, end radius, Add edge, Remove and Use selected edges. Keep the edge owner consistent with Base. These raw native controls do not establish every variable-blend workflow.


1. Start. Select the intended native edges/faces, then Fillet or the exact Features > Dress-up/Part operations entry.

2. Inputs. Inspect Base and subelements, set native size/mode/plane/direction values, then use shared preview. An impossible radius/invalid shell must leave the last valid result available for correction.

3. Preview. Inspect the native candidate and error message. Recompute preview retries a complete candidate. If the task says Apply updates the model, Cancel keeps Redo, no native preview has run; Apply still requires native validation.

4. Apply. Wait until the worker is idle and Apply is enabled. Apply commits one native transaction or one sketch action. Confirm the returned shape, identities and history before continuing.

5. Cancel. Choose the task Cancel, or Escape where supported. Confirm the task closes and its native baseline returns. Cancel of a sketch session is a separate action. An in-flight native call temporarily disables submission.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No complete variable fillet/face blend, body draft, modify fillet, direct-face editing or every difficult native corner/shell workflow. A composed parent-height fillet failure also reproduces in pinned native FreeCAD and must reject atomically.


Evidence: source-audit, native-record, pixel-earlier. Acceptance stays open.


<a id="wf-hole"></a>

### hole: Native Hole form


Current access: bounded-ui-access.


Entry: Features > Subtractive > Hole; Properties or feature tree double-click.


Prerequisites

- Active Body with compatible solid and a native locating profile.


Selection

- Select the native Profile/locating sketch; inspect actual centers/points and target Body.


Declared inputs

- Profile, Diameter, DepthType, Depth, Threaded, HoleCutType; live All properties/Find property for native thread/cut options and dependent choices.


1. Start. Choose Features > Subtractive > Hole.

2. Inputs. Set locating Profile, cut/depth/thread controllers before their dependent sizes. Recompute to refresh live metadata, then inspect unlocked fields. A required field hidden after a rejected incomplete Hole preview is an unresolved task-specific audit, not a confirmed route.

3. Preview. Inspect the native candidate and error message. Recompute preview retries a complete candidate. If the task says Apply updates the model, Cancel keeps Redo, no native preview has run; Apply still requires native validation.

4. Apply. Wait until the worker is idle and Apply is enabled. Apply commits one native transaction or one sketch action. Confirm the returned shape, identities and history before continuing.

5. Cancel. Choose the task Cancel, or Escape where supported. Confirm the task closes and its native baseline returns. Cancel of a sketch session is a separate action. An in-flight native call temporarily disables submission.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: Native generic metadata is not an Onshape hole standard/fit/PEM/multi-part workflow. Thread tables, starting-depth fallback, standards resets, tap-depth shortening messages, tolerances and every conditional field remain unaccepted.


Evidence: source-audit, native-record. Acceptance stays open.


<a id="wf-pattern"></a>

### pattern: Native feature patterns and ordered MultiTransform


Current access: bounded-ui-access.


Entry: Features > Transform > Linear pattern; Features > Transform > Circular pattern; Features > Transform > Mirror; Features > Transform > Multiple transforms; Features > Transform > Scaled pattern; Properties or feature tree double-click.


Prerequisites

- Active Body, valid native original features and native direction/axis/plane.


Selection

- Originals is an ordered native feature list. Direction/Axis/MirrorPlane use actual native object/subelement links.
- MultiTransform Add step, select step, Up/Down and Remove change the parent's ordered native transformations. Tree Reorder on a child is not step reordering.


Declared inputs

- Linear: Originals/Direction/Length/Occurrences/Reversed. Circular: Originals/Axis/Angle/Occurrences/Reversed. Mirror: Originals/MirrorPlane. Scaled: Originals/Factor/Occurrences.
- MultiTransform: Originals, Add step among native supported types, step fields, All step properties, expressions, Up/Down/Remove; Retry step properties after schema error.


1. Start. Choose Features > Transform > exact operation, or Multiple transforms.

2. Inputs. Set Originals and reference links. For multiple transforms add each child, edit its fields and arrange order. Parent preview/Apply/Cancel owns all children as one native transaction.

3. Preview. Inspect the native candidate and error message. Recompute preview retries a complete candidate. If the task says Apply updates the model, Cancel keeps Redo, no native preview has run; Apply still requires native validation.

4. Apply. Wait until the worker is idle and Apply is enabled. Apply commits one native transaction or one sketch action. Confirm the returned shape, identities and history before continuing.

5. Cancel. Choose the task Cancel, or Escape where supported. Confirm the task closes and its native baseline returns. Cancel of a sketch session is a separate action. An in-flight native call temporarily disables submission.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No curve pattern, sketch pattern, assembly pattern, complete face/part result scopes, reapply semantics or every transform reference/GUI combination.


Evidence: source-audit, native-record, pixel-earlier. Acceptance stays open.


<a id="wf-boolean"></a>

### boolean: Multiple Bodies and native Boolean


Current access: bounded-ui-access.


Entry: New body; Active Body chooser or Body tree row; Boolean; Properties or feature tree double-click; Features > Part operations > Intersect; Features > Part operations > Subtract; Features > Part operations > Union; Features > Body operations > Body Boolean.


Prerequisites

- At least two eligible unconsumed native solid Bodies. New body then sketch/primitive and feature chain creates each.


Selection

- Choose target Body; select additional Body tools. Consumed tools retain editable native history but cannot be freely reused in another Boolean.


Declared inputs

- Operation Union/Subtract/Intersect; Target chooser; Tools (count) menu with Body checkboxes; Swap target and tool only when creating with exactly one tool. Existing Boolean Target is disabled to preserve ownership.


1. Start. Choose target active Body, then Boolean. Set Operation, Target and Tools. Opening with selected Bodies can seed the tools.

2. Inputs. Inspect multi-tool native preview and Apply or Cancel. Reopen the Boolean to change Type/tool group. Edit consumed tool sketches/features through their tree history and inspect recomputed result.

3. Preview. Inspect the native candidate and error message. Recompute preview retries a complete candidate. If the task says Apply updates the model, Cancel keeps Redo, no native preview has run; Apply still requires native validation.

4. Apply. Wait until the worker is idle and Apply is enabled. Apply commits one native transaction or one sketch action. Confirm the returned shape, identities and history before continuing.

5. Cancel. Choose the task Cancel, or Escape where supported. Confirm the task closes and its native baseline returns. Cancel of a sketch session is a separate action. An in-flight native call temporarily disables submission.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No complete part/surface scope merge workflow, broad nested references or Assembly components. Native Common with multiple tools follows combined tool-region semantics; do not invent all-tools intersection.


Evidence: source-audit, native-record, pixel-earlier. Acceptance stays open.


<a id="wf-reference"></a>

### reference: Datum, Binder and native placement forms


Current access: bounded-ui-access.


Entry: Features > Reference > Datum plane; Features > Reference > Datum line; Features > Reference > Datum point; Features > Document > Part container; Features > Reference > Datum coordinate system; Features > Reference > Shape binder; Features > Reference > Subshape binder; Properties or feature tree double-click.


Prerequisites

- Ready native document; active Body for PartDesign references; valid same-document support.


Selection

- Pick native source objects/subelements. Binder Support is a native sublink list; its task-specific edit supports that otherwise protected property.


Declared inputs

- AttachmentSupport, MapMode, AttachmentOffset; coordinate-system Placement; ShapeBinder Support/TraceSupport; SubShapeBinder Support/BindMode/Relative/BindCopyOnChange/Offset/MakeFace/Fuse/Refine; Part Group/Placement.


1. Start. Choose Features > Reference > required datum/binder or Document > Part container.

2. Inputs. Use the shared property task to set support, native map mode and offset. Preview then Apply; use Properties to edit. For foreign sketch profiles use the separate native reference-choice radios in the consuming feature.

3. Preview. Inspect the native candidate and error message. Recompute preview retries a complete candidate. If the task says Apply updates the model, Cancel keeps Redo, no native preview has run; Apply still requires native validation.

4. Apply. Wait until the worker is idle and Apply is enabled. Apply commits one native transaction or one sketch action. Confirm the returned shape, identities and history before continuing.

5. Cancel. Choose the task Cancel, or Escape where supported. Confirm the task closes and its native baseline returns. Cancel of a sketch session is a separate action. An in-flight native call temporarily disables submission.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No Onshape Mate Connector inference/ownership UI, Derived external document/version/configuration updater, general transform triad or complete attachment/binder workflow.


Evidence: source-audit, native-record, pixel-wave2. Acceptance stays open.


<a id="wf-history"></a>

### history: Tree, Body Tip and reorder


Current access: bounded-ui-access.


Entry: Model tree > right-click > Set tip here / Roll back to start / Reorder; Properties or feature tree double-click.


Prerequisites

- Ready document, no active sketch/draft. Native Body history_flags determine eligibility.


Selection

- Select tree objects; Body selection changes active modeling target. Feature rows preserve the target.


Declared inputs

- Set tip here; Roll back to start; Reorder Before/After, native target row or Start/End of Body, optional Set tip to moved feature; guarded leaf Delete; Properties Label/Placement.


1. Start. Right-click eligible tree row and choose Set tip here, Body Roll back to start, or Reorder.

2. Inputs. Reorder is Apply-only. Choose Before/After and target, optionally final Tip; Apply asks native Body insertion to validate dependencies. Cancel must preserve Redo. Successors remain in the document after tip rollback. Leaf deletion rejects dependents/nonempty Bodies rather than silently cascading.

3. Preview. Tip rollback, leaf delete and visibility act directly. Reorder has no native preview; its form compiles locally until Apply and preserves Redo while open.

4. Apply. Choose Set tip here/Roll back to start/Delete directly when eligible. Reorder uses Apply after Before/After, target and optional Tip are set. Native dependency validation can still reject the requested reorder.

5. Cancel. Cancel an open Reorder to retain the baseline/Redo. Direct accepted tip/deletion commands use document Undo; visibility follows native non-modeling-history semantics.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No complete suppression, folders/organization, branching/merge history or downstream repair UI. Native suppression placement persistence remains unresolved.


Evidence: source-audit, native-record, pixel-earlier, pixel-wave2. Acceptance stays open.


<a id="wf-packages"></a>

### packages: Declarative local packages


Current access: bounded-ui-access.


Entry: Packages.


Prerequisites

- A loaded declarative package registry; existing opaque runtime and optional caller-chosen package collection.


Selection

- Parameters use native object identifiers and the package's declared types; package UI is not executable extension UI.


Declared inputs

- All six checked-in package declarations and parameter schemas are retained in package_tasks. Custom loaded packages depend on their own declaration.


1. Start. Open Packages, choose the loaded label, then enter all declared parameters shown by its task.

2. Inputs. Compile/preview uses the same revision and lease boundary. Apply/Cancel follows the shared feature lifecycle. Body Boolean package redirects to its dedicated native Body task.

3. Preview. Inspect the native candidate and error message. Recompute preview retries a complete candidate. If the task says Apply updates the model, Cancel keeps Redo, no native preview has run; Apply still requires native validation.

4. Apply. Wait until the worker is idle and Apply is enabled. Apply commits one native transaction or one sketch action. Confirm the returned shape, identities and history before continuing.

5. Cancel. Choose the task Cancel, or Escape where supported. Confirm the task closes and its native baseline returns. Cancel of a sketch session is a separate action. An in-flight native call temporarily disables submission.

6. Edit. Select the saved object in the tree and choose Properties, or double-click its feature row. A sketch row reopens the sketch session. Confirm links and expressions before changing a field.

7. Parent update. Edit a referenced parent through its existing task. Finish and recompute. Inspect downstream errors and native link ownership. Topology failure must restore the previous valid document; old edge/face numbers are not a persistent reference scheme.

8. History. After acceptance, Undo then Redo. Sketch edits first use action history while the session is open; after Finish they use document history. Re-read native state and selection identities after either.

9. Save reopen. Finish or cancel all tasks, Save as to a new FCStd path, then Open that path and repeat the existing-object edit. Native saved-file readback must inspect editable history, constraints, expressions, links and geometry for acceptance.

10. Failure. Keep the task and entered input after rejection where implemented. Correct the value or select a valid native reference and retry; otherwise Cancel. Refresh native state after rollback. If unavailable, use the explicit Close/discard confirmation; Cancel retains ownership and a failed Close permits retry.

11. Availability. Busy calls, another feature draft, sketch drag/dimension/task, support chooser or file browser can disable commands. Finish or cancel the owner and wait for Ready. Invalid selections show command-specific disabled hover reasons where implemented.


Remaining operation: No executable plugins, custom UI contributions, FeatureScript/Feature Studio editing or complete custom-feature service contract.


Evidence: source-audit, native-record. Acceptance stays open.


<a id="wf-interaction-missing"></a>

### interaction-missing: Command search, triad and richer interaction


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add command search, Select other and native-aware manipulators; input fields and camera gestures do not supply these actions.


Evidence: source-audit. Acceptance stays open.


<a id="wf-view-missing"></a>

### view-missing: Additional view tools


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add the named/previous/perspective/section/isolate/transparency or specialized view action named by the topic, with its own state and persistence policy.


Evidence: source-audit. Acceptance stays open.


<a id="wf-measurement-missing"></a>

### measurement-missing: Measurement and mass properties


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add a read-only measurement task for explicit native selections, frame/units, result types and witnesses. Topology lengths/areas and snapshot volume exist as data; no Measure or Mass Properties UI exists. Minimum distance/angles, density/inertia and pinned witnesses require exact native results and separate contracts.


Evidence: source-audit. Acceptance stays open.


<a id="wf-analysis-missing"></a>

### analysis-missing: Geometry analysis


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis.


Evidence: source-audit. Acceptance stays open.


<a id="wf-sketch-tool-missing"></a>

### sketch-tool-missing: Remaining sketch authoring tools


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool.


Evidence: source-audit. Acceptance stays open.


<a id="wf-constraint-missing"></a>

### constraint-missing: Remaining constraint commands


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add the named geometric constraint and native compatible-selection validation. Existing Constrain choices do not expose it.


Evidence: source-audit. Acceptance stays open.


<a id="wf-solid-missing"></a>

### solid-missing: Remaining solid and direct-modeling operations


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation.


Evidence: source-audit. Acceptance stays open.


<a id="wf-surface-missing"></a>

### surface-missing: Surfaces and 3D curve authoring


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling.


Evidence: source-audit. Acceptance stays open.


<a id="wf-data-missing"></a>

### data-missing: Part metadata, materials and tables


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables.


Evidence: source-audit. Acceptance stays open.


<a id="wf-configuration-missing"></a>

### configuration-missing: Configurations and variables


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Define and implement configuration inputs, suppression/visibility conditions, propagation and saved variant identity. Native expressions are present; configuration/variable-table UI is absent.


Evidence: source-audit. Acceptance stays open.


<a id="wf-frame-missing"></a>

### frame-missing: Frame authoring


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add native-backed frame members, profile selection, trim/gusset/end-cap and cut-list workflows. Managed library provenance remains an explicit local/cloud contract.


Evidence: source-audit. Acceptance stays open.


<a id="wf-sheet-metal-missing"></a>

### sheet-metal-missing: Sheet metal and flexible PCB


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids.


Evidence: source-audit. Acceptance stays open.


<a id="wf-assembly-missing"></a>

### assembly-missing: Assembly authoring and mechanisms


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints.


Evidence: source-audit. Acceptance stays open.


<a id="wf-cloud-contract"></a>

### cloud-contract: Document and service contracts


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient.


Evidence: source-audit. Acceptance stays open.


<a id="wf-platform-contract"></a>

### platform-contract: Platform-dependent tools


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Define the platform/device/print/import contract for this topic. The desktop mouse editor does not expose this operation. This audit performed no platform or device actions.


Evidence: source-audit. Acceptance stays open.


<a id="wf-extension-missing"></a>

### extension-missing: Executable extensions and simulation


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Define the named extension/simulation authoring, execution, permission, native result and persistence contract. Declarative packages do not implement it.


Evidence: source-audit. Acceptance stays open.


<a id="wf-performance-missing"></a>

### performance-missing: Performance inspection


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: Add actual diagnostic UI and representative measured workloads. Existing asynchronous model calls and historical resource results are evidence for bounded cases, not a performance panel or pause-regeneration workflow.


Evidence: source-audit. Acceptance stays open.


<a id="wf-shared-overview"></a>

### shared-overview: Shared modeling concepts beyond current routes


Current access: missing.


Entry: No current route.


Prerequisites

- Read the mapped original requirement and its required gates. The named operation is not available in this baseline.


Selection

- Define the exact native point/edge/face/object or ordered/multiple selection contract before adding UI.


Declared inputs

- The mapped original requirement is authoritative. Enumerate task inputs and conditional options when implementation is chosen; none are invented here.


1. Start. No current toolbar/menu route exists for the full named operation.

2. Inputs. Use the original stable requirement ID to implement the missing action and record native API limits explicitly.

3. Preview. Unavailable in this baseline. Implement a reversible native candidate with revision and lease guards before exposing a preview.

4. Apply. Unavailable in this baseline. No Apply route exists for this operation. Implement atomic native acceptance without a second geometry/history model.

5. Cancel. Unavailable in this baseline. No operation-specific Cancel route exists. The future task must restore its captured baseline and preserve Redo when no edit was accepted.

6. Edit. Unavailable in this baseline. No saved operation editor exists. Add one using stable native object/reference identities.

7. Parent update. Unavailable in this baseline. Define dependency, transformed-frame and topology-update behavior before accepting a workflow.

8. History. Unavailable in this baseline. Define native document and applicable sketch-action Undo/Redo behavior.

9. Save reopen. Unavailable in this baseline. Define an editable native persistence contract and independent saved-file check.

10. Failure. Unavailable in this baseline. Reject unsupported operations explicitly and preserve the current valid document.

11. Availability. Unavailable in this baseline. There is no toolbar/menu action to enable. A nearby property or object type is not this operation.


Remaining operation: The overview includes multiple documented operations. Follow explicit supported families, then implement each remaining requirement; the overview itself is not a command or acceptance claim.


Evidence: source-audit. Acceptance stays open.


## Native task field inventory


These are all native forms declared by the Rust catalog at the audited commit. Use the shared native-form lifecycle plus the specialized family. The field list records preferred Rust task fields, not a complete native schema. Find property, All properties and hover documentation expose further runtime properties. This audit did not run schema discovery, so unenumerated native fields/enums/conditional choices remain explicit work. Neither a writable field nor a catalog label certifies a workflow.


| Entry | Native type | Declared preferred fields |
| --- | --- | --- |
| Features > Additive > Pad / Extrude | `PartDesign::Pad` | `Profile`, `SideType`, `Type`, `Length`, `Type2`, `Length2`, `Offset`, `Offset2`, `UpToFace`, `UpToFace2`, `UpToShape`, `UpToShape2`, `TaperAngle`, `TaperAngle2`, `UseCustomVector`, `ReferenceAxis`, `Direction`, `AlongSketchNormal`, `Reversed` |
| Features > Additive > Revolve | `PartDesign::Revolution` | `Profile`, `ReferenceAxis`, `Type`, `Angle`, `Angle2`, `Midplane`, `Reversed` |
| Features > Additive > Additive loft | `PartDesign::AdditiveLoft` | `Profile`, `Sections`, `Ruled`, `Closed` |
| Features > Additive > Additive sweep | `PartDesign::AdditivePipe` | `Profile`, `Spine`, `Transition`, `Transformation`, `AuxiliarySpine` |
| Features > Subtractive > Pocket | `PartDesign::Pocket` | `Profile`, `SideType`, `Type`, `Length`, `Type2`, `Length2`, `Offset`, `Offset2`, `UpToFace`, `UpToFace2`, `UpToShape`, `UpToShape2`, `TaperAngle`, `TaperAngle2`, `UseCustomVector`, `ReferenceAxis`, `Direction`, `AlongSketchNormal`, `Reversed` |
| Features > Subtractive > Groove / Revolved cut | `PartDesign::Groove` | `Profile`, `ReferenceAxis`, `Type`, `Angle`, `Angle2`, `Midplane`, `Reversed` |
| Features > Subtractive > Subtractive loft | `PartDesign::SubtractiveLoft` | `Profile`, `Sections`, `Ruled`, `Closed` |
| Features > Subtractive > Subtractive sweep | `PartDesign::SubtractivePipe` | `Profile`, `Spine`, `Transition`, `Transformation`, `AuxiliarySpine` |
| Features > Subtractive > Hole | `PartDesign::Hole` | `Profile`, `Diameter`, `DepthType`, `Depth`, `Threaded`, `HoleCutType` |
| Features > Dress-up > Fillet | `PartDesign::Fillet` | `Base`, `Radius`, `UseAllEdges` |
| Features > Dress-up > Chamfer | `PartDesign::Chamfer` | `Base`, `ChamferType`, `Size`, `Size2`, `Angle`, `FlipDirection`, `UseAllEdges` |
| Features > Dress-up > Draft | `PartDesign::Draft` | `Base`, `Angle`, `NeutralPlane`, `PullDirection`, `Reversed` |
| Features > Dress-up > Shell / Thickness | `PartDesign::Thickness` | `Base`, `Value`, `Mode`, `Join`, `Intersection`, `Reversed` |
| Features > Transform > Linear pattern | `PartDesign::LinearPattern` | `Originals`, `Direction`, `Length`, `Occurrences`, `Reversed` |
| Features > Transform > Circular pattern | `PartDesign::PolarPattern` | `Originals`, `Axis`, `Angle`, `Occurrences`, `Reversed` |
| Features > Transform > Mirror | `PartDesign::Mirrored` | `Originals`, `MirrorPlane` |
| Features > Transform > Multiple transforms | `PartDesign::MultiTransform` | `Originals`, `Transformations` |
| Features > Transform > Scaled pattern | `PartDesign::Scaled` | `Originals`, `Factor`, `Occurrences` |
| Features > Reference > Datum plane | `PartDesign::Plane` | `AttachmentSupport`, `MapMode`, `AttachmentOffset` |
| Features > Reference > Datum line | `PartDesign::Line` | `AttachmentSupport`, `MapMode`, `AttachmentOffset` |
| Features > Reference > Datum point | `PartDesign::Point` | `AttachmentSupport`, `MapMode`, `AttachmentOffset` |
| Features > Document > Part container | `App::Part` | `Group`, `Placement` |
| Features > Part primitives > Box | `Part::Box` | `Height`, `Length`, `Width`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Part operations > Chamfer | `Part::Chamfer` | `Base`, `Edges` |
| Features > Part operations > Intersect | `Part::Common` | `Base`, `Tool`, `Refine` |
| Features > Part primitives > Cone | `Part::Cone` | `Height`, `Angle`, `Radius1`, `Radius2`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Part operations > Subtract | `Part::Cut` | `Base`, `Tool`, `Refine` |
| Features > Part primitives > Cylinder | `Part::Cylinder` | `Height`, `Angle`, `Radius`, `FirstAngle`, `SecondAngle`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Part operations > Extrusion | `Part::Extrusion` | `Base`, `DirMode`, `Dir`, `DirLink`, `LengthFwd`, `LengthRev`, `Solid`, `Symmetric`, `TaperAngle`, `TaperAngleRev` |
| Features > Part operations > Fillet | `Part::Fillet` | `Base`, `Edges` |
| Features > Part operations > Union | `Part::Fuse` | `Base`, `Tool`, `Refine` |
| Features > Part operations > Union | `Part::MultiFuse` | `Shapes`, `Refine` |
| Features > Part operations > Revolution | `Part::Revolution` | `Source`, `Axis`, `AxisLink`, `Base`, `Angle`, `Solid`, `Symmetric` |
| Features > Part primitives > Sphere | `Part::Sphere` | `Radius`, `Angle1`, `Angle2`, `Angle3`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Part primitives > Torus | `Part::Torus` | `Radius1`, `Radius2`, `Angle1`, `Angle2`, `Angle3`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Additive primitives > Additive Box | `PartDesign::AdditiveBox` | `Height`, `Length`, `Width`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Additive primitives > Additive Cone | `PartDesign::AdditiveCone` | `Height`, `Angle`, `Radius1`, `Radius2`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Additive primitives > Additive Cylinder | `PartDesign::AdditiveCylinder` | `Height`, `Angle`, `Radius`, `FirstAngle`, `SecondAngle`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Additive primitives > Additive Ellipsoid | `PartDesign::AdditiveEllipsoid` | `Radius1`, `Radius2`, `Radius3`, `Angle1`, `Angle2`, `Angle3`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Additive > Additive Helix | `PartDesign::AdditiveHelix` | `Profile`, `ReferenceAxis`, `Mode`, `Pitch`, `Height`, `Angle`, `Placement` |
| Features > Additive primitives > Additive Prism | `PartDesign::AdditivePrism` | `Height`, `Polygon`, `Circumradius`, `FirstAngle`, `SecondAngle`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Additive primitives > Additive Sphere | `PartDesign::AdditiveSphere` | `Radius`, `Angle1`, `Angle2`, `Angle3`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Additive primitives > Additive Torus | `PartDesign::AdditiveTorus` | `Radius1`, `Radius2`, `Angle1`, `Angle2`, `Angle3`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Additive primitives > Additive Wedge | `PartDesign::AdditiveWedge` | `Xmin`, `Xmax`, `X2min`, `X2max`, `Ymin`, `Ymax`, `Zmin`, `Zmax`, `Z2min`, `Z2max`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Body operations > Body Boolean | `PartDesign::Boolean` | `Group`, `Type`, `Refine` |
| Features > Reference > Datum coordinate system | `PartDesign::CoordinateSystem` | `AttachmentSupport`, `MapMode`, `AttachmentOffset`, `Placement` |
| Features > Reference > Shape binder | `PartDesign::ShapeBinder` | `Support`, `TraceSupport` |
| Features > Reference > Subshape binder | `PartDesign::SubShapeBinder` | `Support`, `BindMode`, `Relative`, `BindCopyOnChange`, `Offset`, `MakeFace`, `Fuse`, `Refine` |
| Features > Subtractive primitives > Subtractive Box | `PartDesign::SubtractiveBox` | `Height`, `Length`, `Width`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Subtractive primitives > Subtractive Cone | `PartDesign::SubtractiveCone` | `Height`, `Angle`, `Radius1`, `Radius2`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Subtractive primitives > Subtractive Cylinder | `PartDesign::SubtractiveCylinder` | `Height`, `Angle`, `Radius`, `FirstAngle`, `SecondAngle`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Subtractive primitives > Subtractive Ellipsoid | `PartDesign::SubtractiveEllipsoid` | `Radius1`, `Radius2`, `Radius3`, `Angle1`, `Angle2`, `Angle3`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Subtractive > Subtractive Helix | `PartDesign::SubtractiveHelix` | `Profile`, `ReferenceAxis`, `Mode`, `Pitch`, `Height`, `Angle`, `Placement` |
| Features > Subtractive primitives > Subtractive Prism | `PartDesign::SubtractivePrism` | `Height`, `Polygon`, `Circumradius`, `FirstAngle`, `SecondAngle`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Subtractive primitives > Subtractive Sphere | `PartDesign::SubtractiveSphere` | `Radius`, `Angle1`, `Angle2`, `Angle3`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Subtractive primitives > Subtractive Torus | `PartDesign::SubtractiveTorus` | `Radius1`, `Radius2`, `Angle1`, `Angle2`, `Angle3`, `AttachmentSupport`, `MapMode`, `Placement` |
| Features > Subtractive primitives > Subtractive Wedge | `PartDesign::SubtractiveWedge` | `Xmin`, `Xmax`, `X2min`, `X2max`, `Ymin`, `Ymax`, `Zmin`, `Zmax`, `Z2min`, `Z2max`, `AttachmentSupport`, `MapMode`, `Placement` |


The two Part operations > Union entries have the same visible label. Source distinguishes `Part::Fuse` from `Part::MultiFuse`; users currently lack that distinction in the menu. Q08 tracks it.


## Package input inventory


The loaded registry can contain caller-provided packages. These are all checked-in declarations; each parameter keeps its exact type/range/choice schema in the ledger. Body Boolean redirects to its dedicated task.


| Package | Declared inputs |
| --- | --- |
| Boolean `body.boolean` | `name` (identifier), `target` (identifier), `tool` (identifier), `operation` (string) |
| Fillet `feature.fillet` | `name` (identifier), `body` (identifier), `source` (identifier), `edge` (edge), `radius` (number) |
| Extrude `feature.pad` | `name` (identifier), `body` (identifier), `profile` (identifier), `length` (number) |
| Remove `feature.pocket` | `name` (identifier), `body` (identifier), `profile` (identifier), `length` (number) |
| Circle `sketch.circle` | `object` (identifier), `x` (number), `y` (number), `radius` (number), `index` (integer) |
| Rectangle `sketch.rectangle` | `object` (identifier), `x` (number), `y` (number), `width` (number), `height` (number), `index` (integer) |


## Evidence boundaries


- source-audit [implementation]. Archived committed next source at dbb932a. Source presence/declared controls only; no runtime execution in this audit.

- rust-record [headless]. Recorded 194 active Rust tests and 11 explicit native Editor cases, including actual actor/headless egui controls. This audit did not rerun them. Headless egui is not physical desktop delivery.

- native-record [native]. Recorded 31 native suites on Windows/Linux, specific saved-file/native comparisons and intentional failures. Test definitions and public verification record support only their declared cases. No new execution.

- pixel-earlier [desktop]. Earlier documented Pixel sketch editing, Carbon Copy, spline tasks, multi-Body Boolean, transforms and file-browser cases. Bounds and source versions remain those recorded; no blanket current desktop acceptance.

- pixel-wave1 [desktop]. Reviewed 23 October first wave: drawing/construction/drag, dimension rejection/correction/Cancel, whole-object Pad/Pocket picking, history, save/reopen, unavailable-document Close Cancel/retry. Reviewer incorrectly expected 11760 for native empty-target Pad 12000; that verifier exited 1 and is not called a pass. No full workflow acceptance.

- pixel-wave2 [desktop]. Reviewed 23 October second wave: coincident Shift selection, AngleViaPoint measured seed and Cancel/index reuse, label/expression correction, Undo/Redo/save/reopen, native X-axis/both-axis reference, transformed external-point picking, transformed Pad whole-target and Pocket Cancel/history. Reported shape differences zero for saved solid controls. Windows reviewer passed three solids but exited 1 on missing SketchObject.isDriving while collecting two sketches; those Windows sketch checks are not passes. Retained ARM64/XML/screenshots support bounded angle gates.


The reviewed Pixel waves add bounded evidence after older public prose that still says the entire AngleViaPoint or Pad/Pocket desktop candidate is unverified. They do not close the full requirement. Pending desktop work includes creation-time MissingPreparation expression failure and preview ownership, separate Y-axis, transformed parent/formula updates, spline cleanup and more signed/reflex/multiturn values. No new device use is authorized by this audit.


## In-progress owner candidates after the audit baseline


These owner reports arrived during the audit. They describe unreviewed implementation in progress, not dbb932a routes or accepted parity. Keep source review, native/headless evidence and actual desktop acceptance separate.


### candidate-assembly-20261024


Owner reports real native headless Assembly plus all 13 JointTypes, new modules/UI, and tests in progress.


Exact UI labels were not supplied; do not invent routes or merge them into dbb932a access.


Queue link: Q01. Missing checks

- Independent source/native review; exact supported joint API and result comparison.
- Empty assembly creation, Insert/repeated instances, native connector selection, local Fix distinct from mates, every joint input and limits.
- Preview/Apply/Cancel, constraint conflicts, edit and parent/reference update, Undo/Redo, Save/reopen and independent native readback.
- Real desktop steps and original Assembly requirements remain open, including cloud/linked-version contracts.


### candidate-measurement-20261024


Owner reports read-only authoritative native metrics and UI implemented.


Exact UI labels were not supplied; baseline has no Measure route.


Queue link: Q02. Missing checks

- Independent native/source review for metric definitions, selections and transformed coordinate frames.
- No document/redo/lease mutation, stale selection/read rejection, unit/precision behavior and empty/invalid selections.
- Actual UI picking and result correction; persistent witnesses, density/mass/inertia and full requirement breadth only if explicitly implemented.


### candidate-sketch-options-20261024


Ordinary Sketch stays one click. Right-click Sketch offers Create sketch with options. Attachment options expose Reverse normal, Unattached, XYZ offsets, native normal-offset expression and axis/angle rotation.


These are reported candidate labels, absent from the frozen dbb932a route registry. Native detach_sketch preserves computed support frame via positionBySupport before clearing support.


Queue link: Q03. Missing checks

- Origin planes, selected face/datum/sketch frame and empty-document automatic Body creation; ordinary click behavior unchanged.
- Reverse, XYZ offset, normal-offset quantity/expression, nonzero axis/angle and invalid units/vectors, including mode changes.
- Unattached preserves native computed world/support frame before detach, with rotated/moved Body and source support.
- Chooser Cancel before creation writes nothing and preserves Redo; rejected creation keeps correction input and publishes no partial objects.
- Native expressions, constraints/references and Tip ownership through parent change, Undo/Redo, Save/reopen and independent native inspection.
- Independent review and actual desktop interaction; full standalone/connector/region-imprinting requirements remain open.


## Correctness and benchmark planning


Only after verified functional parity. Do not begin performance optimization from finite workflow passes or catalog coverage. Correctness, native-baseline comparisons and representative resource/reliability regression gates remain required while functionality is built.


- All applicable original G1-G8 gates for every declared requirement, including actual UI delivery, parent changes, failure correction, native history and independent saved reopen.
- Keep authoritative native geometry/solver, revision/lease guards, atomic failures, saved data and source preservation.
- Record exact source/runtime/platform/workload and matched native baseline before measurement.
- Plan cold/warm startup, working/peak memory, idle CPU, preview cancellation, regeneration and input-to-frame latency on representative moderate parts/assemblies.
- Keep headless timing separate from desktop interaction. Report variance and native unsupported cases. Regression measurement does not authorize hillclimbing.


## Prioritized implementation queue and ownership seams


### Q01 P1: Assembly insert/fix/mate authoring is absent


Add a bounded native-backed Assembly/Insert/Fix task and stable instance references before mates. No existing native Assembly API has been verified by this audit; inspect exact pinned APIs and report missing solver/API behavior rather than using Body placement as a joint.


Own new assembly modules and explicit protocol additions. Coordinate editor.rs toolbar/panels and snapshot dispatch with root; do not replace Body ownership or world_from_shape.


Owner: Assembly owner. Source: [next/src/editor.rs:722](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/editor.rs#L722), [next/src/feature_commands.rs:24](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/feature_commands.rs#L24), [next/backend/freecad_worker.py:36](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/backend/freecad_worker.py#L36).


Candidate link: candidate-assembly-20261024. Owner reports implementation in progress. Baseline absence remains accurate; coordinate review and missing gates rather than duplicating candidate implementation.


### Q02 P1: No read-only Measure UI or authoritative measurement command


Add revision-scoped native selection measurement, units and a result panel. Existing edge.length/face.area/shape.volume only seed bounded modes. Exact distance/angle/center/inertia APIs require focused native investigation; no approximated mesh measure.


Prefer new measurement module and read-only dispatch. Consume viewport native picks/world_from_shape and revision; no document transaction/preview lease for reads. Coordinate toolbar insertion with root.


Owner: Measurement owner. Source: [next/src/editor/view.rs:12](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/editor/view.rs#L12), [next/backend/freecad_worker.py:288](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/backend/freecad_worker.py#L288), [next/backend/freecad_worker.py:437](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/backend/freecad_worker.py#L437).


Candidate link: candidate-measurement-20261024. Owner reports implementation in progress. Baseline absence remains accurate; coordinate review and missing gates rather than duplicating candidate implementation.


### Q03 P1: Sketch creation lacks standalone orientation/support controls


Preserve automatic first Body and atomic origin-plane creation. Add declared support/orientation/reverse/offset choices only against native attachment semantics. Keep chooser Cancel write-free and Redo-safe.


Root owns sketch_creation.rs and sketch_creation_commands.rs. Empty-document creation is already implemented; avoid adding a second root-sketch path. Other owners should consume existing native SketchObject IDs.


Owner: Root sketch owner. Source: [next/src/editor/sketch_creation.rs:55](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/editor/sketch_creation.rs#L55), [next/src/sketch_creation_commands.rs:53](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/sketch_creation_commands.rs#L53).


Candidate link: candidate-sketch-options-20261024. Owner reports implementation in progress. Baseline absence remains accurate; coordinate review and missing gates rather than duplicating candidate implementation.


### Q04 P1: Missing regions, thin/surface result modes, start offsets and merge scopes


Complete one declared extrusion mode with explicit profile region selection, reference choice and native supported geometry. Do not equate Pad/Pocket enum exposure with every Onshape extent/result contract.


Keep native controllers, dormant values, singular-target guards, profile reference compiler and preview lease. New topology identities must be native-owned.


Owner: Feature task owner. Source: [next/src/extrusion_commands.rs:34](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/extrusion_commands.rs#L34), [next/src/editor/properties.rs:256](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/editor/properties.rs#L256).


### Q05 P1: Everyday sketch offset, mirror, patterns and rigid transform absent


Add one exact native sketch action and UI task at a time; preserve source constraints/expressions and action history. Native command constructor/API availability is not proved by this audit.


Use shared sketch command compiler, task baseline and accepted-action journal. Avoid edits to root support-creation code.


Owner: Sketch tool owner. Source: [next/src/editor/sketch.rs:305](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/editor/sketch.rs#L305), [next/src/sketch_operation_commands.rs:28](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/sketch_operation_commands.rs#L28).


### Q06 P1: Hole conditional input access after failed incomplete previews needs audit


Enumerate native Hole schema and controllers, then test reaching every required thread/cut field after an incomplete preview fails. Only Pad/Pocket have the explicit pending-controller task schema. Do not claim a Hole defect until reproduced.


Task-specific presentation may extend Hole only with exact native metadata; preserve generic computed/read-only protections.


Owner: Feature task owner. Source: [next/src/editor/properties.rs:256](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/editor/properties.rs#L256), [next/src/extrusion_commands.rs:34](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/extrusion_commands.rs#L34).


### Q07 P2: Suppression and downstream dependency repair incomplete


Resolve pinned native suppression placement persistence before UI promotion; add clear failure/repair routes and preserve saved graph/history.


Keep Body insertion/BaseFeature rewiring native. MultiTransform child order belongs to its parent task.


Owner: History owner. Source: [next/src/editor/history.rs:61](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/editor/history.rs#L61), [next/PROTOCOL.md:444](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/PROTOCOL.md#L444).


### Q08 P2: Two visible Part operations Union entries are indistinguishable


Give two-input Part::Fuse and list-based Part::MultiFuse distinct visible labels while retaining stable feature IDs and existing files.


Small catalog-label seam, coordinate generated ledger routes after integration.


Owner: Feature catalog owner. Source: [next/src/feature_commands.rs:360](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/feature_commands.rs#L360), [next/src/feature_commands.rs:367](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/feature_commands.rs#L367).


### Q09 P2: Generic singular Use selection silently takes the first pick


Add explicit singular-selection validation/reason to match Pad/Pocket behavior, while retaining deliberate link-list ordering.


Shared widget also serves datum, dress-up, sweep and patterns; use native property type and do not discard valid whole-object selection.


Owner: Native property owner. Source: [next/src/editor/properties.rs:694](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/editor/properties.rs#L694).


### Q10 P2: Materials, mass density, variable/configuration/table workflows absent


Separate saved material/density and expressions from configuration identity and managed libraries. Add one usable local native-backed workflow, then its parent/edit/history/reopen gates.


Coordinate measurement density/override contract without inventing mass from volume alone.


Owner: Part data owner. Source: [next/src/editor/properties.rs:256](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/editor/properties.rs#L256), [next/src/editor.rs:846](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/editor.rs#L846).


### Q11 P2: Surface/direct-face, frame and folded/flat models absent


Choose exact pinned native API per feature; state concrete unavailable APIs or required extension dependencies. Preserve mature geometry; never substitute mesh/linear approximations.


New native task modules and exact allowlist changes; no blanket FreeCAD workbench/addon parity assumption.


Owner: Surface/frame/sheet-metal owners. Source: [next/src/feature_commands.rs:24](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/src/feature_commands.rs#L24).


### Q12 P2: Recorded Pixel gates cover only bounded cases


After Leo releases a device, check MissingPreparation expression failure/preview owner, separate Y-axis, transformed formulas/parent updates, spline cleanup and signed/reflex/multiturn dimension breadth. This audit authorizes no GUI/device action.


Keep desktop evidence separate from headless/native. Preserve current user session, files and controls.


Owner: Desktop verification owner. Source: [next/VERIFICATION.md:931](https://github.com/LeoHougaard/CAD/blob/dbb932af266021f7aed579daf9c957e16247f063/next/VERIFICATION.md#L931).


## Full topic index


All 306 IDs are listed once. Full option requirements, exact IDs, official sources, remaining contracts and original gates are in the ledger. Shared families avoid inventing 1,378 near-identical instructions. A bounded route leaves the entire documented workflow unaccepted.


| Topic | Family | Current access | Remaining operation |
| --- | --- | --- | --- |
| [OSH-0005 Performance Considerations](https://cad.onshape.com/help/Content/Home/performance_considerations.htm) | [performance-missing](#wf-performance-missing) | missing | Add actual diagnostic UI and representative measured workloads. Existing asynchronous model calls and historical resource results are evidence for bounded cases, not a performance panel or pause-regeneration workflow. |
| [OSH-0006 Performance Panel](https://cad.onshape.com/help/Content/Home/performance_panel.htm) | [performance-missing](#wf-performance-missing) | missing | Add actual diagnostic UI and representative measured workloads. Existing asynchronous model calls and historical resource results are evidence for bounded cases, not a performance panel or pause-regeneration workflow. |
| [OSH-0011 Keyboard Shortcuts and Hotkeys](https://cad.onshape.com/help/Content/Home/keyboard_shortcuts_and_hotkeys.htm) | [selection](#wf-selection) | bounded-ui-access | Only implemented shortcuts are documented here; shortcut customization and full Onshape key mappings are absent. |
| [OSH-0012 Search Tools](https://cad.onshape.com/help/Content/Home/search_tools.htm) | [interaction-missing](#wf-interaction-missing) | missing | Add command search, Select other and native-aware manipulators; input fields and camera gestures do not supply these actions. |
| [OSH-0013 Selection](https://cad.onshape.com/help/Content/Home/selection.htm) | [selection](#wf-selection) | bounded-ui-access | No general Select other/hidden-hit chooser, Create selection rules, full triad, command search, touch workflow or complete selection variants. |
| [OSH-0014 Create Selection](https://cad.onshape.com/help/Content/Home/create_selection.htm) | [interaction-missing](#wf-interaction-missing) | missing | Add command search, Select other and native-aware manipulators; input fields and camera gestures do not supply these actions. |
| [OSH-0015 Select Other](https://cad.onshape.com/help/Content/Home/select_other.htm) | [interaction-missing](#wf-interaction-missing) | missing | Add command search, Select other and native-aware manipulators; input fields and camera gestures do not supply these actions. |
| [OSH-0016 Triad Manipulator](https://cad.onshape.com/help/Content/Home/triad_manipulator.htm) | [interaction-missing](#wf-interaction-missing) | missing | Add command search, Select other and native-aware manipulators; input fields and camera gestures do not supply these actions. |
| [OSH-0017 Dialogs](https://cad.onshape.com/help/Content/Home/dialogs.htm) | [native-form](#wf-native-form) | bounded-ui-access | Feature, sketch and file tasks have distinct lifecycles; generic fields do not establish dialog parity or remembered preferences. |
| [OSH-0018 Numeric Fields](https://cad.onshape.com/help/Content/Home/numeric_fields.htm) | [native-form](#wf-native-form) | bounded-ui-access | Native quantities/expressions exist. Full Onshape numeric-field shortcuts, tolerances, precision, configurations and input gestures remain open. |
| [OSH-0019 Context Menus](https://cad.onshape.com/help/Content/Home/context_menus.htm) | [history](#wf-history) | bounded-ui-access | Tree history/visibility/delete context actions exist; a context menu for every geometry/tool and every documented action does not. |
| [OSH-0020 Colors](https://cad.onshape.com/help/Content/Home/colors.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0021 Error Indicators](https://cad.onshape.com/help/Content/Home/error_indicators.htm) | [history](#wf-history) | bounded-ui-access | Native errors and solver state are displayed. No full downstream repair, warning classification or document-wide error-navigation task. |
| [OSH-0022 Printing](https://cad.onshape.com/help/Content/Home/printing_part_studios_and_assemblies.htm) | [platform-contract](#wf-platform-contract) | missing | Define the platform/device/print/import contract for this topic. The desktop mouse editor does not expose this operation. This audit performed no platform or device actions. |
| [OSH-0025 Documents](https://cad.onshape.com/help/Content/Document/documents.htm) | [document](#wf-document) | bounded-ui-access | No multi-tab document UI, cloud document service, versions/branches/merge, explicit old .cadpart importer or cross-process locking contract. |
| [OSH-0026 Document Tabs](https://cad.onshape.com/help/Content/Document/document_tabs.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0027 Document Panel](https://cad.onshape.com/help/Content/Document/document_panel.htm) | [document](#wf-document) | bounded-ui-access | One model tree and native Properties exist. No Onshape tabbed document panel contract. |
| [OSH-0028 Document Toolbar](https://cad.onshape.com/help/Content/Document/document_toolbar.htm) | [document](#wf-document) | bounded-ui-access | Local lifecycle/edit commands exist. Cloud/version/tab toolbar controls are absent. |
| [OSH-0029 Document Menu](https://cad.onshape.com/help/Content/Document/document_menu.htm) | [document](#wf-document) | bounded-ui-access | New/Open/Save/Save as exist as toolbar actions. No corresponding complete Onshape document menu. |
| [OSH-0030 Setting Default Units](https://cad.onshape.com/help/Content/Document/setting_default_units_per_workspace.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0031 Documents Page](https://cad.onshape.com/help/Content/Document/documents_page.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0032 List View](https://cad.onshape.com/help/Content/Document/list_view.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0033 Structure View](https://cad.onshape.com/help/Content/Document/structure_view.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0034 Create Menu](https://cad.onshape.com/help/Content/Document/create_menu.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0035 Folders](https://cad.onshape.com/help/Content/Document/folders.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0036 Labels](https://cad.onshape.com/help/Content/Document/labels.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0037 Filters](https://cad.onshape.com/help/Content/Document/filters.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0038 Integrations](https://cad.onshape.com/help/Content/Document/doc_integrations.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0039 Advanced Search](https://cad.onshape.com/help/Content/Document/advanced_search.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0040 Panels](https://cad.onshape.com/help/Content/Document/panels.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0041 Trash](https://cad.onshape.com/help/Content/Document/trash.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0042 Document Management](https://cad.onshape.com/help/Content/Document/document_management.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0043 Versions, Branches, and History](https://cad.onshape.com/help/Content/Document/versions_and_history.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0044 Comparing](https://cad.onshape.com/help/Content/Document/compare.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0045 Repairing](https://cad.onshape.com/help/Content/Document/repairing.htm) | [history](#wf-history) | bounded-ui-access | Atomic rejection and sketch correction exist. No general reference-repair/task dependency repair tool. |
| [OSH-0046 Merging](https://cad.onshape.com/help/Content/Document/merging.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0047 Restoring](https://cad.onshape.com/help/Content/Document/restoring.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0048 Workspace Protections](https://cad.onshape.com/help/Content/Document/workspace_protections.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0049 Document Notes](https://cad.onshape.com/help/Content/Document/document_notes.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0050 Where Used](https://cad.onshape.com/help/Content/Document/where_used.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0051 Properties (Metadata)](https://cad.onshape.com/help/Content/Home/properties_metadata.htm) | [native-form](#wf-native-form) | bounded-ui-access | Native Label and supported properties can be edited; configured metadata, custom property schemas and managed publications are absent. |
| [OSH-0052 Publications](https://cad.onshape.com/help/Content/Document/publications.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0053 Model Visualization Tools](https://cad.onshape.com/help/Content/View/model_visualization_tools.htm) | [view](#wf-view) | bounded-ui-access | No named/previous/perspective/section view, view cube, isolate/transparency task, tangent/hidden-edge controls, high-quality analysis or AR workflow. |
| [OSH-0054 View Navigation and the View Cube](https://cad.onshape.com/help/Content/View/view_navigation_and_the_view_cube.htm) | [view](#wf-view) | bounded-ui-access | No named/previous/perspective/section view, view cube, isolate/transparency task, tangent/hidden-edge controls, high-quality analysis or AR workflow. |
| [OSH-0055 Mobile Touch Interface Navigation](https://cad.onshape.com/help/Content/Mobile/mobile_touch_interface_videos.htm) | [platform-contract](#wf-platform-contract) | missing | Define the platform/device/print/import contract for this topic. The desktop mouse editor does not expose this operation. This audit performed no platform or device actions. |
| [OSH-0056 Camera and Render Options](https://cad.onshape.com/help/Content/View/camera_and_render_options.htm) | [view](#wf-view) | bounded-ui-access | No named/previous/perspective/section view, view cube, isolate/transparency task, tangent/hidden-edge controls, high-quality analysis or AR workflow. |
| [OSH-0057 Isometric, Dimetric, and Trimetric Projections](https://cad.onshape.com/help/Content/View/isometric_dimetric_trimetric_projections.htm) | [view](#wf-view) | bounded-ui-access | Only Isometric among these axonometric projection choices is exposed; no Dimetric/Trimetric chooser. |
| [OSH-0058 Graphics Preferences](https://cad.onshape.com/help/Content/View/graphics_preferences.htm) | [view-missing](#wf-view-missing) | missing | Add the named/previous/perspective/section/isolate/transparency or specialized view action named by the topic, with its own state and persistence policy. |
| [OSH-0059 Named Views](https://cad.onshape.com/help/Content/View/named_views.htm) | [view-missing](#wf-view-missing) | missing | Add the named/previous/perspective/section/isolate/transparency or specialized view action named by the topic, with its own state and persistence policy. |
| [OSH-0060 Previous View](https://cad.onshape.com/help/Content/View/previous_view.htm) | [view-missing](#wf-view-missing) | missing | Add the named/previous/perspective/section/isolate/transparency or specialized view action named by the topic, with its own state and persistence policy. |
| [OSH-0061 Zoom to Fit, Window, and Selection](https://cad.onshape.com/help/Content/View/zoom_to_fit_window_and_selection.htm) | [view](#wf-view) | bounded-ui-access | Fit/F/Fit all exist. Window/selection zoom variants are absent. |
| [OSH-0062 Perspective View](https://cad.onshape.com/help/Content/View/perspective_view.htm) | [view-missing](#wf-view-missing) | missing | Add the named/previous/perspective/section/isolate/transparency or specialized view action named by the topic, with its own state and persistence policy. |
| [OSH-0063 Orient Normal to Sketch Plane](https://cad.onshape.com/help/Content/View/orient_normal_to_sketch_on_edit.htm) | [sketch-create](#wf-sketch-create) | bounded-ui-access | Entering sketch edit aligns to its frame. A separate orient-normal command for arbitrary selected sketch is absent. |
| [OSH-0064 Shaded, Unshaded, and Translucent](https://cad.onshape.com/help/Content/View/shaded_unshaded__and_translucent.htm) | [view](#wf-view) | bounded-ui-access | Shaded/Shaded with edges/Wireframe exist; translucent per-part states are absent. |
| [OSH-0065 Hidden Edges Visible and Removed](https://cad.onshape.com/help/Content/View/hidden_edges_visible_and_removed.htm) | [view-missing](#wf-view-missing) | missing | Add the named/previous/perspective/section/isolate/transparency or specialized view action named by the topic, with its own state and persistence policy. |
| [OSH-0066 Tangent Edges Visible, Phantom, and Removed](https://cad.onshape.com/help/Content/View/tangent_edges_visible_phantom_and_removed.htm) | [view-missing](#wf-view-missing) | missing | Add the named/previous/perspective/section/isolate/transparency or specialized view action named by the topic, with its own state and persistence policy. |
| [OSH-0067 View in High Quality](https://cad.onshape.com/help/Content/View/view_in_high_quality.htm) | [view-missing](#wf-view-missing) | missing | Add the named/previous/perspective/section/isolate/transparency or specialized view action named by the topic, with its own state and persistence policy. |
| [OSH-0068 Highlight Boundary Edges](https://cad.onshape.com/help/Content/View/highlight_boundary_edges.htm) | [view-missing](#wf-view-missing) | missing | Add the named/previous/perspective/section/isolate/transparency or specialized view action named by the topic, with its own state and persistence policy. |
| [OSH-0069 Section View](https://cad.onshape.com/help/Content/View/section_view.htm) | [view-missing](#wf-view-missing) | missing | Add the named/previous/perspective/section/isolate/transparency or specialized view action named by the topic, with its own state and persistence policy. |
| [OSH-0070 Hide/Show](https://cad.onshape.com/help/Content/View/hide_show.htm) | [view](#wf-view) | bounded-ui-access | No named/previous/perspective/section view, view cube, isolate/transparency task, tangent/hidden-edge controls, high-quality analysis or AR workflow. |
| [OSH-0071 Isolate](https://cad.onshape.com/help/Content/View/isolate.htm) | [view-missing](#wf-view-missing) | missing | Add the named/previous/perspective/section/isolate/transparency or specialized view action named by the topic, with its own state and persistence policy. |
| [OSH-0072 Make Transparent](https://cad.onshape.com/help/Content/View/make_transparent.htm) | [view-missing](#wf-view-missing) | missing | Add the named/previous/perspective/section/isolate/transparency or specialized view action named by the topic, with its own state and persistence policy. |
| [OSH-0073 Viewing Models with Augmented Reality](https://cad.onshape.com/help/Content/Mobile/viewing_models_with_augmented_reality.htm) | [platform-contract](#wf-platform-contract) | missing | Define the platform/device/print/import contract for this topic. The desktop mouse editor does not expose this operation. This audit performed no platform or device actions. |
| [OSH-0074 Viewing Models with AR View](https://cad.onshape.com/help/Content/Mobile/viewing_models_with_ar_view.htm) | [platform-contract](#wf-platform-contract) | missing | Define the platform/device/print/import contract for this topic. The desktop mouse editor does not expose this operation. This audit performed no platform or device actions. |
| [OSH-0075 Viewing Models with Apple Vision Pro](https://cad.onshape.com/help/Content/Mobile/viewing_models_with_apple_vision_pro.htm) | [platform-contract](#wf-platform-contract) | missing | Define the platform/device/print/import contract for this topic. The desktop mouse editor does not expose this operation. This audit performed no platform or device actions. |
| [OSH-0076 Model Evaluation Tools](https://cad.onshape.com/help/Content/View/model_evaluation_tools.htm) | [measurement-missing](#wf-measurement-missing) | missing | Add a read-only measurement task for explicit native selections, frame/units, result types and witnesses. Topology lengths/areas and snapshot volume exist as data; no Measure or Mass Properties UI exists. Minimum distance/angles, density/inertia and pinned witnesses require exact native results and separate contracts. |
| [OSH-0077 Measure Tool](https://cad.onshape.com/help/Content/View/measure_tool.htm) | [measurement-missing](#wf-measurement-missing) | missing | Add a read-only measurement task for explicit native selections, frame/units, result types and witnesses. Topology lengths/areas and snapshot volume exist as data; no Measure or Mass Properties UI exists. Minimum distance/angles, density/inertia and pinned witnesses require exact native results and separate contracts. |
| [OSH-0078 Analysis Tools](https://cad.onshape.com/help/Content/View/analysis_tools.htm) | [analysis-missing](#wf-analysis-missing) | missing | Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis. |
| [OSH-0079 Curve/Surface Analysis](https://cad.onshape.com/help/Content/View/curve_surface_analysis.htm) | [analysis-missing](#wf-analysis-missing) | missing | Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis. |
| [OSH-0080 Deviation Analysis](https://cad.onshape.com/help/Content/View/deviation_analysis.htm) | [analysis-missing](#wf-analysis-missing) | missing | Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis. |
| [OSH-0081 Connection Analysis](https://cad.onshape.com/help/Content/View/connection_analysis.htm) | [analysis-missing](#wf-analysis-missing) | missing | Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis. |
| [OSH-0082 Dihedral Analysis](https://cad.onshape.com/help/Content/View/dihedral_analysis.htm) | [analysis-missing](#wf-analysis-missing) | missing | Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis. |
| [OSH-0083 Interference Detection](https://cad.onshape.com/help/Content/View/interference_detection.htm) | [analysis-missing](#wf-analysis-missing) | missing | Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis. |
| [OSH-0084 Zebra Stripes](https://cad.onshape.com/help/Content/View/zebra_stripes.htm) | [analysis-missing](#wf-analysis-missing) | missing | Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis. |
| [OSH-0085 Reflection Analysis](https://cad.onshape.com/help/Content/View/reflection_analysis.htm) | [analysis-missing](#wf-analysis-missing) | missing | Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis. |
| [OSH-0086 Curvature Color Map](https://cad.onshape.com/help/Content/View/curvature_color_map.htm) | [analysis-missing](#wf-analysis-missing) | missing | Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis. |
| [OSH-0087 Draft Analysis](https://cad.onshape.com/help/Content/View/draft_analysis.htm) | [analysis-missing](#wf-analysis-missing) | missing | Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis. |
| [OSH-0088 Thickness Analysis](https://cad.onshape.com/help/Content/View/thickness_analysis.htm) | [analysis-missing](#wf-analysis-missing) | missing | Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis. |
| [OSH-0089 Flatten Surfaces](https://cad.onshape.com/help/Content/View/flatten_surfaces.htm) | [analysis-missing](#wf-analysis-missing) | missing | Add the topic's explicit analysis mode and authoritative calculation, valid selections, options and visual result. Shaded triangles are not curvature, interference, draft or thickness analysis. |
| [OSH-0090 Mass Properties Tool](https://cad.onshape.com/help/Content/View/mass_properties_tool.htm) | [measurement-missing](#wf-measurement-missing) | missing | Add a read-only measurement task for explicit native selections, frame/units, result types and witnesses. Topology lengths/areas and snapshot volume exist as data; no Measure or Mass Properties UI exists. Minimum distance/angles, density/inertia and pinned witnesses require exact native results and separate contracts. |
| [OSH-0091 Shared Modeling Concepts](https://cad.onshape.com/help/Content/Home/shared_modeling_concepts.htm) | [shared-overview](#wf-shared-overview) | missing | The overview includes multiple documented operations. Follow explicit supported families, then implement each remaining requirement; the overview itself is not a command or acceptance claim. |
| [OSH-0092 Modeling in Context](https://cad.onshape.com/help/Content/Assembly/modeling_in_context.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0093 Configurations](https://cad.onshape.com/help/Content/PartStudio/configurations.htm) | [configuration-missing](#wf-configuration-missing) | missing | Define and implement configuration inputs, suppression/visibility conditions, propagation and saved variant identity. Native expressions are present; configuration/variable-table UI is absent. |
| [OSH-0094 Part Studio and Assembly Configurations](https://cad.onshape.com/help/Content/PartStudio/part_studio_and_assembly_configurations.htm) | [configuration-missing](#wf-configuration-missing) | missing | Define and implement configuration inputs, suppression/visibility conditions, propagation and saved variant identity. Native expressions are present; configuration/variable-table UI is absent. |
| [OSH-0095 Variable Studio Configurations](https://cad.onshape.com/help/Content/VariableStudio/variable_studio_configurations.htm) | [configuration-missing](#wf-configuration-missing) | missing | Define and implement configuration inputs, suppression/visibility conditions, propagation and saved variant identity. Native expressions are present; configuration/variable-table UI is absent. |
| [OSH-0096 Configured Properties](https://cad.onshape.com/help/Content/PartStudio/configured_properties.htm) | [configuration-missing](#wf-configuration-missing) | missing | Define and implement configuration inputs, suppression/visibility conditions, propagation and saved variant identity. Native expressions are present; configuration/variable-table UI is absent. |
| [OSH-0097 Managing Configurations](https://cad.onshape.com/help/Content/PartStudio/managing_configurations.htm) | [configuration-missing](#wf-configuration-missing) | missing | Define and implement configuration inputs, suppression/visibility conditions, propagation and saved variant identity. Native expressions are present; configuration/variable-table UI is absent. |
| [OSH-0098 Visibility Conditions](https://cad.onshape.com/help/Content/PartStudio/visibility_conditions.htm) | [configuration-missing](#wf-configuration-missing) | missing | Define and implement configuration inputs, suppression/visibility conditions, propagation and saved variant identity. Native expressions are present; configuration/variable-table UI is absent. |
| [OSH-0099 Input propagation](https://cad.onshape.com/help/Content/PartStudio/input_propagation.htm) | [configuration-missing](#wf-configuration-missing) | missing | Define and implement configuration inputs, suppression/visibility conditions, propagation and saved variant identity. Native expressions are present; configuration/variable-table UI is absent. |
| [OSH-0100 Working with Imported CAD](https://cad.onshape.com/help/Content/Document/working_with_imported_cad.htm) | [exchange](#wf-exchange) | bounded-ui-access | STEP/IGES import plus native placement/property editing exists; import repair/history recovery and all advertised imported-CAD tools are absent. |
| [OSH-0101 Mixed Modeling](https://cad.onshape.com/help/Content/PartStudio/mixed_modeling.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0102 Onshape Labs](https://cad.onshape.com/help/Content/Home/onshape_labs.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0103 FeatureScript Co-Complete](https://cad.onshape.com/help/Content/Home/featurescript_co_complete.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0104 Descriptive Image Search](https://cad.onshape.com/help/Content/Home/descriptive_image_search.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0105 Importing & Exporting Files](https://cad.onshape.com/help/Content/File/importing_and_exporting_files.htm) | [exchange](#wf-exchange) | bounded-ui-access | No other exchange formats, Lidar, mixed modeling, cloud translation/download jobs or editable legacy .cadpart migration. Exchange geometry does not establish source parametric history. |
| [OSH-0106 Supported File Formats](https://cad.onshape.com/help/Content/File/supported_file_formats.htm) | [exchange](#wf-exchange) | bounded-ui-access | Only FCStd and STEP/IGES routes documented here. Every other named format needs its own support/deviation decision. |
| [OSH-0107 Importing Files](https://cad.onshape.com/help/Content/Document/importing_files.htm) | [exchange](#wf-exchange) | bounded-ui-access | No other exchange formats, Lidar, mixed modeling, cloud translation/download jobs or editable legacy .cadpart migration. Exchange geometry does not establish source parametric history. |
| [OSH-0108 Exporting Files](https://cad.onshape.com/help/Content/File/exporting_files.htm) | [exchange](#wf-exchange) | bounded-ui-access | No other exchange formats, Lidar, mixed modeling, cloud translation/download jobs or editable legacy .cadpart migration. Exchange geometry does not establish source parametric history. |
| [OSH-0109 Downloading Files](https://cad.onshape.com/help/Content/File/downloading_files.htm) | [exchange](#wf-exchange) | bounded-ui-access | Export writes a local file. No cloud translation/download queue or package download service. |
| [OSH-0110 Importing a Lidar Scan](https://cad.onshape.com/help/Content/File/importing_a_lidar_scan.htm) | [platform-contract](#wf-platform-contract) | missing | Define the platform/device/print/import contract for this topic. The desktop mouse editor does not expose this operation. This audit performed no platform or device actions. |
| [OSH-0111 Sharing and Collaboration](https://cad.onshape.com/help/Content/Collaboration/sharing_and_collaboration.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0112 Share Documents](https://cad.onshape.com/help/Content/Collaboration/share_documents.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0113 Collaboration](https://cad.onshape.com/help/Content/Collaboration/collaboration.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0114 Comments in Workspaces and Versions](https://cad.onshape.com/help/Content/Collaboration/comments_on_workspaces.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0115 Follow Mode](https://cad.onshape.com/help/Content/Collaboration/follow_mode.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0116 View Only Mode](https://cad.onshape.com/help/Content/Collaboration/view_only_mode.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0117 Using the View Only Toolbar](https://cad.onshape.com/help/Content/Collaboration/using_the_view_only_toolbar.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0118 Transfer Ownership](https://cad.onshape.com/help/Content/Collaboration/transfer_ownership.htm) | [cloud-contract](#wf-cloud-contract) | missing | No corresponding cloud UI/service contract exists. Decide local equivalent versus authorized cloud service for this exact requirement, including document/version identity, access/ownership and failure behavior. Local save/Undo is insufficient. |
| [OSH-0134 Part Studios](https://cad.onshape.com/help/Content/PartStudio/part_studios.htm) | [sketch-create](#wf-sketch-create) | bounded-ui-access | One native document with multiple Bodies is exposed; it is not a multi-tab Onshape Part Studio/document model. |
| [OSH-0135 Customizing Appearance](https://cad.onshape.com/help/Content/PartStudio/customizing_appearances.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0136 Features and Parts Lists](https://cad.onshape.com/help/Content/PartStudio/features_and_parts_lists.htm) | [history](#wf-history) | bounded-ui-access | Native Body Group/Tip tree and guarded leaf delete exist; Part lists, folders, suppression and organization parity are incomplete. |
| [OSH-0137 Customizing Part Materials](https://cad.onshape.com/help/Content/PartStudio/customizing_part_materials.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0138 Onshape Material Library](https://cad.onshape.com/help/Content/Library/onshape_material_library.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0139 Custom Tables](https://cad.onshape.com/help/Content/PartStudio/custom_tables.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0140 Model-Based Definition (MBD)](https://cad.onshape.com/help/Content/PartStudio/model_based_definition.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0141 Tolerance Options](https://cad.onshape.com/help/Content/Home/tolerance_options.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0142 Inspection table](https://cad.onshape.com/help/Content/PartStudio/inspection_table.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0143 Onshape Default Tolerances Library](https://cad.onshape.com/help/Content/Library/onshape_default_tolerances_library.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0144 Default Tolerances Library](https://cad.onshape.com/help/Content/Library/default_tolerances_library.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0145 Variable Table (Part Studio)](https://cad.onshape.com/help/Content/PartStudio/variable_table.htm) | [configuration-missing](#wf-configuration-missing) | missing | Define and implement configuration inputs, suppression/visibility conditions, propagation and saved variant identity. Native expressions are present; configuration/variable-table UI is absent. |
| [OSH-0146 Hole Table](https://cad.onshape.com/help/Content/PartStudio/hole_table.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0147 Surface Modeling](https://cad.onshape.com/help/Content/PartStudio/surface_modeling.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0148 Sketch Basics](https://cad.onshape.com/help/Content/Sketch/sketch_basics.htm) | [sketch-create](#wf-sketch-create) | bounded-ui-access | No standalone sketch/orientation/reverse/offset creation dialog, foreign-Body support chooser, connector support, Disable imprinting or whole-versus-split region selection. |
| [OSH-0149 Sketch Tools](https://cad.onshape.com/help/Content/Sketch/sketch_tools.htm) | [sketch-draw](#wf-sketch-draw) | bounded-ui-access | No Midpoint line, center/aligned rectangle, three-point circle, tangent/center-point arc, polygon, text, slot or Bezier drawing workflow. |
| [OSH-0150 Line](https://cad.onshape.com/help/Content/Sketch/line.htm) | [sketch-draw](#wf-sketch-draw) | bounded-ui-access | No Midpoint line, center/aligned rectangle, three-point circle, tangent/center-point arc, polygon, text, slot or Bezier drawing workflow. |
| [OSH-0151 Midpoint Line](https://cad.onshape.com/help/Content/Sketch/midpoint_line.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0152 Corner Rectangle](https://cad.onshape.com/help/Content/Sketch/corner_rectangle.htm) | [sketch-draw](#wf-sketch-draw) | bounded-ui-access | No Midpoint line, center/aligned rectangle, three-point circle, tangent/center-point arc, polygon, text, slot or Bezier drawing workflow. |
| [OSH-0153 Center Point Rectangle](https://cad.onshape.com/help/Content/Sketch/center_point_rectangle.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0154 Aligned Rectangle](https://cad.onshape.com/help/Content/Sketch/aligned_rectangle.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0155 Center Point Circle](https://cad.onshape.com/help/Content/Sketch/center_point_circle.htm) | [sketch-draw](#wf-sketch-draw) | bounded-ui-access | No Midpoint line, center/aligned rectangle, three-point circle, tangent/center-point arc, polygon, text, slot or Bezier drawing workflow. |
| [OSH-0156 3 Point Circle](https://cad.onshape.com/help/Content/Sketch/3_point_circle.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0157 Ellipse](https://cad.onshape.com/help/Content/Sketch/ellipse.htm) | [sketch-curves](#wf-sketch-curves) | bounded-ui-access | No general Bezier, interpolation/fit-point workflow, arbitrary curve conversion or every spline/Onshape conic option. |
| [OSH-0158 3 Point Arc](https://cad.onshape.com/help/Content/Sketch/3_point_arc.htm) | [sketch-draw](#wf-sketch-draw) | bounded-ui-access | No Midpoint line, center/aligned rectangle, three-point circle, tangent/center-point arc, polygon, text, slot or Bezier drawing workflow. |
| [OSH-0159 Tangent Arc](https://cad.onshape.com/help/Content/Sketch/arc_tangent.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0160 Center Point Arc](https://cad.onshape.com/help/Content/Sketch/center_point_arc.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0161 Elliptical Arc](https://cad.onshape.com/help/Content/Sketch/elliptical_arc.htm) | [sketch-curves](#wf-sketch-curves) | bounded-ui-access | No general Bezier, interpolation/fit-point workflow, arbitrary curve conversion or every spline/Onshape conic option. |
| [OSH-0162 Conic](https://cad.onshape.com/help/Content/Sketch/conic.htm) | [sketch-curves](#wf-sketch-curves) | bounded-ui-access | Ellipse/hyperbola/parabola arcs use native click-defined curves. No general rho/tangent Conic tool. |
| [OSH-0163 Inscribed Polygon](https://cad.onshape.com/help/Content/Sketch/inscribed_polygon.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0164 Circumscribed Polygon](https://cad.onshape.com/help/Content/Sketch/circumscribed_polygon.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0165 Spline](https://cad.onshape.com/help/Content/Sketch/spline.htm) | [sketch-curves](#wf-sketch-curves) | bounded-ui-access | No general Bezier, interpolation/fit-point workflow, arbitrary curve conversion or every spline/Onshape conic option. |
| [OSH-0166 Bezier](https://cad.onshape.com/help/Content/Sketch/bezier.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0167 Spline Control Point](https://cad.onshape.com/help/Content/Sketch/spline_control_point.htm) | [sketch-modify](#wf-sketch-modify) | bounded-ui-access | Pole position/weight/degree/knot tasks exist. No complete Onshape fit/control-point conversion and control-point editing workflow. |
| [OSH-0168 Point](https://cad.onshape.com/help/Content/Sketch/point.htm) | [sketch-draw](#wf-sketch-draw) | bounded-ui-access | No Midpoint line, center/aligned rectangle, three-point circle, tangent/center-point arc, polygon, text, slot or Bezier drawing workflow. |
| [OSH-0169 Text](https://cad.onshape.com/help/Content/Sketch/text.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0170 Use](https://cad.onshape.com/help/Content/Sketch/use.htm) | [sketch-reference](#wf-sketch-reference) | bounded-ui-access | Trackable silhouettes, every binder/projection combination and all transformed/projected-point desktop variants remain unaccepted. Missing metadata must fail source-dependent edits explicitly. |
| [OSH-0171 Intersection](https://cad.onshape.com/help/Content/Sketch/intersection.htm) | [sketch-reference](#wf-sketch-reference) | bounded-ui-access | Native Intersection reference flag exists. This does not establish every 2D intersection or 3D Intersection Curve tool. |
| [OSH-0172 Construction](https://cad.onshape.com/help/Content/Sketch/construction.htm) | [sketch-draw](#wf-sketch-draw) | bounded-ui-access | No Midpoint line, center/aligned rectangle, three-point circle, tangent/center-point arc, polygon, text, slot or Bezier drawing workflow. |
| [OSH-0173 Sketch Chamfer](https://cad.onshape.com/help/Content/Sketch/sketch_chamfer.htm) | [sketch-modify](#wf-sketch-modify) | bounded-ui-access | Native sketch chamfer uses a Radius input and construction arc. Onshape distance/angle variants need an explicit mapping. |
| [OSH-0174 Sketch Fillet](https://cad.onshape.com/help/Content/Sketch/sketch_fillet.htm) | [sketch-modify](#wf-sketch-modify) | bounded-ui-access | Native radius seeds geometry; persistent radius is a separate dimension. Full corner-selection behavior remains open. |
| [OSH-0175 Trim](https://cad.onshape.com/help/Content/Sketch/trim.htm) | [sketch-modify](#wf-sketch-modify) | bounded-ui-access | Sketch offset, mirror/pattern/rigid transform, slot, text, DXF/DWG and image insertion have no task. Broader native split/spline cases still need acceptance. |
| [OSH-0176 Extend](https://cad.onshape.com/help/Content/Sketch/extend.htm) | [sketch-modify](#wf-sketch-modify) | bounded-ui-access | Sketch offset, mirror/pattern/rigid transform, slot, text, DXF/DWG and image insertion have no task. Broader native split/spline cases still need acceptance. |
| [OSH-0177 Sketch Split](https://cad.onshape.com/help/Content/Sketch/sketch_split.htm) | [sketch-modify](#wf-sketch-modify) | bounded-ui-access | Sketch offset, mirror/pattern/rigid transform, slot, text, DXF/DWG and image insertion have no task. Broader native split/spline cases still need acceptance. |
| [OSH-0178 Offset](https://cad.onshape.com/help/Content/Sketch/offset.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0179 Slot](https://cad.onshape.com/help/Content/Sketch/slot.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0180 Sketch Mirror](https://cad.onshape.com/help/Content/Sketch/sketch_mirror.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0181 Linear Sketch Pattern](https://cad.onshape.com/help/Content/Sketch/sketch_linear_pattern.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0182 Circular Sketch Pattern](https://cad.onshape.com/help/Content/Sketch/sketch_circular_pattern.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0183 Transform Sketch](https://cad.onshape.com/help/Content/Sketch/sketch_transform.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0184 Insert DXF or DWG](https://cad.onshape.com/help/Content/Sketch/insert_dwg_or_dxf.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0185 Insert Image](https://cad.onshape.com/help/Content/Sketch/insert_image.htm) | [sketch-tool-missing](#wf-sketch-tool-missing) | missing | Add the named drawing/offset/slot/mirror/pattern/transform/text/import action with native constraints and references. Existing basic geometry is not the requested tool. |
| [OSH-0186 Dimension](https://cad.onshape.com/help/Content/Sketch/dimension.htm) | [sketch-dimension](#wf-sketch-dimension) | bounded-ui-access | Full signed/reflex/multiturn desktop breadth, label placement, tolerances/precision and every native reference/activation variant remain open. SnellsLaw has native unsupported spline cases and broader GUI creation remains unaccepted. |
| [OSH-0187 Coincident](https://cad.onshape.com/help/Content/Sketch/coincident.htm) | [sketch-constraint](#wf-sketch-constraint) | bounded-ui-access | No dedicated Concentric, Midpoint, Normal, Pierce or Curvature command. Coincident centers are only a bounded alternative, not full concentric parity. Selection/conflict combinations remain open. |
| [OSH-0188 Concentric](https://cad.onshape.com/help/Content/Sketch/concentric.htm) | [constraint-missing](#wf-constraint-missing) | missing | Add the named geometric constraint and native compatible-selection validation. Existing Constrain choices do not expose it. |
| [OSH-0189 Parallel](https://cad.onshape.com/help/Content/Sketch/parallel.htm) | [sketch-constraint](#wf-sketch-constraint) | bounded-ui-access | No dedicated Concentric, Midpoint, Normal, Pierce or Curvature command. Coincident centers are only a bounded alternative, not full concentric parity. Selection/conflict combinations remain open. |
| [OSH-0190 Tangent](https://cad.onshape.com/help/Content/Sketch/tangent.htm) | [sketch-constraint](#wf-sketch-constraint) | bounded-ui-access | No dedicated Concentric, Midpoint, Normal, Pierce or Curvature command. Coincident centers are only a bounded alternative, not full concentric parity. Selection/conflict combinations remain open. |
| [OSH-0191 Horizontal](https://cad.onshape.com/help/Content/Sketch/horizontal.htm) | [sketch-constraint](#wf-sketch-constraint) | bounded-ui-access | No dedicated Concentric, Midpoint, Normal, Pierce or Curvature command. Coincident centers are only a bounded alternative, not full concentric parity. Selection/conflict combinations remain open. |
| [OSH-0192 Vertical](https://cad.onshape.com/help/Content/Sketch/vertical.htm) | [sketch-constraint](#wf-sketch-constraint) | bounded-ui-access | No dedicated Concentric, Midpoint, Normal, Pierce or Curvature command. Coincident centers are only a bounded alternative, not full concentric parity. Selection/conflict combinations remain open. |
| [OSH-0193 Perpendicular](https://cad.onshape.com/help/Content/Sketch/perpendicular.htm) | [sketch-constraint](#wf-sketch-constraint) | bounded-ui-access | No dedicated Concentric, Midpoint, Normal, Pierce or Curvature command. Coincident centers are only a bounded alternative, not full concentric parity. Selection/conflict combinations remain open. |
| [OSH-0194 Equal](https://cad.onshape.com/help/Content/Sketch/equal.htm) | [sketch-constraint](#wf-sketch-constraint) | bounded-ui-access | No dedicated Concentric, Midpoint, Normal, Pierce or Curvature command. Coincident centers are only a bounded alternative, not full concentric parity. Selection/conflict combinations remain open. |
| [OSH-0195 Midpoint](https://cad.onshape.com/help/Content/Sketch/midpoint.htm) | [constraint-missing](#wf-constraint-missing) | missing | Add the named geometric constraint and native compatible-selection validation. Existing Constrain choices do not expose it. |
| [OSH-0196 Normal](https://cad.onshape.com/help/Content/Sketch/normal.htm) | [constraint-missing](#wf-constraint-missing) | missing | Add the named geometric constraint and native compatible-selection validation. Existing Constrain choices do not expose it. |
| [OSH-0197 Pierce](https://cad.onshape.com/help/Content/Sketch/pierce.htm) | [constraint-missing](#wf-constraint-missing) | missing | Add the named geometric constraint and native compatible-selection validation. Existing Constrain choices do not expose it. |
| [OSH-0198 Symmetric](https://cad.onshape.com/help/Content/Sketch/symmetric.htm) | [sketch-constraint](#wf-sketch-constraint) | bounded-ui-access | No dedicated Concentric, Midpoint, Normal, Pierce or Curvature command. Coincident centers are only a bounded alternative, not full concentric parity. Selection/conflict combinations remain open. |
| [OSH-0199 Fix](https://cad.onshape.com/help/Content/Sketch/fix.htm) | [sketch-constraint](#wf-sketch-constraint) | bounded-ui-access | No dedicated Concentric, Midpoint, Normal, Pierce or Curvature command. Coincident centers are only a bounded alternative, not full concentric parity. Selection/conflict combinations remain open. |
| [OSH-0200 Curvature](https://cad.onshape.com/help/Content/Sketch/curvature.htm) | [constraint-missing](#wf-constraint-missing) | missing | Add the named geometric constraint and native compatible-selection validation. Existing Constrain choices do not expose it. |
| [OSH-0201 Automatic Inferencing](https://cad.onshape.com/help/Content/Sketch/automatic_inferencing.htm) | [sketch-draw](#wf-sketch-draw) | bounded-ui-access | Endpoint/center/origin/curve snapping and horizontal/vertical inference exist; all documented inference rules, settings and gestures are not exposed. |
| [OSH-0202 Working with Constraints](https://cad.onshape.com/help/Content/Sketch/working_with_constraints.htm) | [sketch-constraint](#wf-sketch-constraint) | bounded-ui-access | No dedicated Concentric, Midpoint, Normal, Pierce or Curvature command. Coincident centers are only a bounded alternative, not full concentric parity. Selection/conflict combinations remain open. |
| [OSH-0203 Troubleshooting Sketch Geometry](https://cad.onshape.com/help/Content/Sketch/troubleshooting_sketch_geometry.htm) | [sketch-constraint](#wf-sketch-constraint) | bounded-ui-access | Native solver errors, action Undo and constraint editing allow correction. Full conflict diagnosis/repair tools remain absent. |
| [OSH-0204 Feature Basics](https://cad.onshape.com/help/Content/PartStudio/feature_basics.htm) | [native-form](#wf-native-form) | bounded-ui-access | A catalog/native field is interface access only. Task-specific selection inference, manipulators, native dialog preferences, every conditional schema and each documented Onshape semantic contract remain unaccepted. |
| [OSH-0205 Feature Tools](https://cad.onshape.com/help/Content/PartStudio/feature_tools.htm) | [native-form](#wf-native-form) | bounded-ui-access | A catalog/native field is interface access only. Task-specific selection inference, manipulators, native dialog preferences, every conditional schema and each documented Onshape semantic contract remain unaccepted. |
| [OSH-0206 Extrude](https://cad.onshape.com/help/Content/PartStudio/extrude.htm) | [extrusion](#wf-extrusion) | bounded-ui-access | No complete Onshape solid/surface/thin result mode, New/Add/Remove/Intersect merge-scope workflow, arbitrary sketch-region/vertex/connector termination, start-entity offset or viewport manipulator. Broader imported/legacy targets and transformed formulas remain unaccepted. |
| [OSH-0207 Revolve](https://cad.onshape.com/help/Content/PartStudio/revolve.htm) | [revolve](#wf-revolve) | bounded-ui-access | No complete thin/surface modes, selected region workflow, merge scopes or revolution manipulators; every axis/second-angle GUI combination remains open. |
| [OSH-0208 Sweep](https://cad.onshape.com/help/Content/PartStudio/sweep.htm) | [sweep](#wf-sweep) | bounded-ui-access | No complete guide/orientation-lock/twist/thin/surface/scale/trim-end UI or curve-pattern workflow. Native supported modes and topology failure behavior need separate cases. |
| [OSH-0209 Loft](https://cad.onshape.com/help/Content/PartStudio/loft.htm) | [loft](#wf-loft) | bounded-ui-access | No complete guides/continuity/end-condition/connection editor, point profiles, thin/surface modes, path section counts or documented nested-loop restrictions. Ordered list editing alone does not establish loft parity. |
| [OSH-0210 Thicken](https://cad.onshape.com/help/Content/PartStudio/thicken.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0211 Enclose](https://cad.onshape.com/help/Content/PartStudio/enclose.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0212 Fillet](https://cad.onshape.com/help/Content/PartStudio/fillet.htm) | [dressup](#wf-dressup) | bounded-ui-access | No complete variable fillet/face blend, body draft, modify fillet, direct-face editing or every difficult native corner/shell workflow. A composed parent-height fillet failure also reproduces in pinned native FreeCAD and must reject atomically. |
| [OSH-0213 Face Blend](https://cad.onshape.com/help/Content/PartStudio/face_blend.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0214 Chamfer](https://cad.onshape.com/help/Content/PartStudio/chamfer.htm) | [dressup](#wf-dressup) | bounded-ui-access | No complete variable fillet/face blend, body draft, modify fillet, direct-face editing or every difficult native corner/shell workflow. A composed parent-height fillet failure also reproduces in pinned native FreeCAD and must reject atomically. |
| [OSH-0215 Draft](https://cad.onshape.com/help/Content/PartStudio/draft.htm) | [dressup](#wf-dressup) | bounded-ui-access | No complete variable fillet/face blend, body draft, modify fillet, direct-face editing or every difficult native corner/shell workflow. A composed parent-height fillet failure also reproduces in pinned native FreeCAD and must reject atomically. |
| [OSH-0216 Body Draft](https://cad.onshape.com/help/Content/PartStudio/body_draft.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0217 Rib](https://cad.onshape.com/help/Content/PartStudio/rib.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0218 Shell](https://cad.onshape.com/help/Content/PartStudio/shell.htm) | [dressup](#wf-dressup) | bounded-ui-access | No complete variable fillet/face blend, body draft, modify fillet, direct-face editing or every difficult native corner/shell workflow. A composed parent-height fillet failure also reproduces in pinned native FreeCAD and must reject atomically. |
| [OSH-0219 Hole](https://cad.onshape.com/help/Content/PartStudio/hole.htm) | [hole](#wf-hole) | bounded-ui-access | Native generic metadata is not an Onshape hole standard/fit/PEM/multi-part workflow. Thread tables, starting-depth fallback, standards resets, tap-depth shortening messages, tolerances and every conditional field remain unaccepted. |
| [OSH-0220 External Thread](https://cad.onshape.com/help/Content/PartStudio/external_thread.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0221 Linear Pattern](https://cad.onshape.com/help/Content/PartStudio/linear_pattern.htm) | [pattern](#wf-pattern) | bounded-ui-access | No curve pattern, sketch pattern, assembly pattern, complete face/part result scopes, reapply semantics or every transform reference/GUI combination. |
| [OSH-0222 Circular Pattern](https://cad.onshape.com/help/Content/PartStudio/circular_pattern.htm) | [pattern](#wf-pattern) | bounded-ui-access | No curve pattern, sketch pattern, assembly pattern, complete face/part result scopes, reapply semantics or every transform reference/GUI combination. |
| [OSH-0223 Curve Pattern](https://cad.onshape.com/help/Content/PartStudio/curve_pattern.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0224 Mirror](https://cad.onshape.com/help/Content/PartStudio/mirror.htm) | [pattern](#wf-pattern) | bounded-ui-access | No curve pattern, sketch pattern, assembly pattern, complete face/part result scopes, reapply semantics or every transform reference/GUI combination. |
| [OSH-0225 Boolean](https://cad.onshape.com/help/Content/PartStudio/boolean.htm) | [boolean](#wf-boolean) | bounded-ui-access | No complete part/surface scope merge workflow, broad nested references or Assembly components. Native Common with multiple tools follows combined tool-region semantics; do not invent all-tools intersection. |
| [OSH-0226 Split](https://cad.onshape.com/help/Content/PartStudio/split.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0227 Transform](https://cad.onshape.com/help/Content/PartStudio/transform.htm) | [reference](#wf-reference) | bounded-ui-access | Native Placement values and primitive transform patterns exist. No full translate/rotate/copy/scale Transform task or triad. |
| [OSH-0228 Wrap](https://cad.onshape.com/help/Content/Sketch/wrap.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0229 Decal](https://cad.onshape.com/help/Content/PartStudio/decal.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0230 Delete Part](https://cad.onshape.com/help/Content/PartStudio/delete_part.htm) | [history](#wf-history) | bounded-ui-access | Guarded leaf removal rejects dependents; it is not a history-based Delete Part operation with all scopes. |
| [OSH-0231 Modify Fillet](https://cad.onshape.com/help/Content/PartStudio/modify_fillet.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0232 Delete Face](https://cad.onshape.com/help/Content/PartStudio/delete_face.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0233 Move Face](https://cad.onshape.com/help/Content/PartStudio/move_face.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0234 Replace Face](https://cad.onshape.com/help/Content/PartStudio/replace_face.htm) | [solid-missing](#wf-solid-missing) | missing | Add the named native-backed operation and task-specific selections/options. No replacement menu route currently supplies this operation. |
| [OSH-0235 Offset Surface](https://cad.onshape.com/help/Content/PartStudio/offset_surface.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0236 Boundary Surface](https://cad.onshape.com/help/Content/PartStudio/boundary_surface.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0237 Fill](https://cad.onshape.com/help/Content/PartStudio/fill.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0238 Move Boundary](https://cad.onshape.com/help/Content/PartStudio/move_boundary.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0239 Ruled Surface](https://cad.onshape.com/help/Content/PartStudio/ruled_surface.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0240 Mutual Trim](https://cad.onshape.com/help/Content/PartStudio/mutual_trim.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0241 Constrained Surface](https://cad.onshape.com/help/Content/PartStudio/constrained_surface.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0242 Plane](https://cad.onshape.com/help/Content/PartStudio/plane.htm) | [reference](#wf-reference) | bounded-ui-access | No Onshape Mate Connector inference/ownership UI, Derived external document/version/configuration updater, general transform triad or complete attachment/binder workflow. |
| [OSH-0243 Helix](https://cad.onshape.com/help/Content/PartStudio/helix.htm) | [sweep](#wf-sweep) | bounded-ui-access | No complete guide/orientation-lock/twist/thin/surface/scale/trim-end UI or curve-pattern workflow. Native supported modes and topology failure behavior need separate cases. |
| [OSH-0244 Projected Curve](https://cad.onshape.com/help/Content/PartStudio/projected_curve.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0245 3D Fit Spline](https://cad.onshape.com/help/Content/PartStudio/3d_fit_spline.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0246 Bridging Curve](https://cad.onshape.com/help/Content/PartStudio/bridging_curve.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0247 Composite Curve](https://cad.onshape.com/help/Content/PartStudio/composite_curve.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0248 Intersection Curve](https://cad.onshape.com/help/Content/PartStudio/intersection_curve.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0249 Trim Curve](https://cad.onshape.com/help/Content/PartStudio/trim_curve.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0250 Isocline](https://cad.onshape.com/help/Content/PartStudio/isocline.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0251 Offset Curve](https://cad.onshape.com/help/Content/PartStudio/offset_curve.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0252 Isoparametric Curve](https://cad.onshape.com/help/Content/PartStudio/isoparametric_curve.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0253 Edit Curve](https://cad.onshape.com/help/Content/PartStudio/edit_curve.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0254 Routing Curve](https://cad.onshape.com/help/Content/PartStudio/routing_curve.htm) | [surface-missing](#wf-surface-missing) | missing | Add the named native-backed surface/curve tool, its ordered references and continuity/trim controls. Existing sweep/loft property access is not full surface modeling. |
| [OSH-0255 Mate Connector](https://cad.onshape.com/help/Content/PartStudio/mate_connector.htm) | [reference](#wf-reference) | bounded-ui-access | Native datum coordinate system is nearby access. Implicit/explicit mate-connector geometry inference, ownership and connector UI are absent. |
| [OSH-0256 Derived](https://cad.onshape.com/help/Content/PartStudio/derived.htm) | [reference](#wf-reference) | bounded-ui-access | Same-document native binder/foreign-sketch copy is nearby access. Derived cross-document versions/configurations, update manager, property-copy policy and circular external dependency checks are absent. |
| [OSH-0257 Variable](https://cad.onshape.com/help/Content/PartStudio/variable.htm) | [configuration-missing](#wf-configuration-missing) | missing | Define and implement configuration inputs, suppression/visibility conditions, propagation and saved variant identity. Native expressions are present; configuration/variable-table UI is absent. |
| [OSH-0258 Query Variable](https://cad.onshape.com/help/Content/PartStudio/query_variable.htm) | [configuration-missing](#wf-configuration-missing) | missing | Define and implement configuration inputs, suppression/visibility conditions, propagation and saved variant identity. Native expressions are present; configuration/variable-table UI is absent. |
| [OSH-0259 Composite Part](https://cad.onshape.com/help/Content/PartStudio/composite_part.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0260 Tag](https://cad.onshape.com/help/Content/PartStudio/tag.htm) | [data-missing](#wf-data-missing) | missing | Add the named editable data/table/material/MBD/tolerance workflow and saved data model. Native Label/property expressions do not supply material libraries, annotation semantics or tables. |
| [OSH-0261 Frame](https://cad.onshape.com/help/Content/PartStudio/frame.htm) | [frame-missing](#wf-frame-missing) | missing | Add native-backed frame members, profile selection, trim/gusset/end-cap and cut-list workflows. Managed library provenance remains an explicit local/cloud contract. |
| [OSH-0262 Onshape Frame Profiles Library](https://cad.onshape.com/help/Content/Library/onshape_frame_profiles_library.htm) | [frame-missing](#wf-frame-missing) | missing | Add native-backed frame members, profile selection, trim/gusset/end-cap and cut-list workflows. Managed library provenance remains an explicit local/cloud contract. |
| [OSH-0263 Frame Trim](https://cad.onshape.com/help/Content/PartStudio/frame_trim.htm) | [frame-missing](#wf-frame-missing) | missing | Add native-backed frame members, profile selection, trim/gusset/end-cap and cut-list workflows. Managed library provenance remains an explicit local/cloud contract. |
| [OSH-0264 Gusset](https://cad.onshape.com/help/Content/PartStudio/gusset.htm) | [frame-missing](#wf-frame-missing) | missing | Add native-backed frame members, profile selection, trim/gusset/end-cap and cut-list workflows. Managed library provenance remains an explicit local/cloud contract. |
| [OSH-0265 End cap](https://cad.onshape.com/help/Content/PartStudio/end_cap.htm) | [frame-missing](#wf-frame-missing) | missing | Add native-backed frame members, profile selection, trim/gusset/end-cap and cut-list workflows. Managed library provenance remains an explicit local/cloud contract. |
| [OSH-0266 Cut List](https://cad.onshape.com/help/Content/PartStudio/cut_list.htm) | [frame-missing](#wf-frame-missing) | missing | Add native-backed frame members, profile selection, trim/gusset/end-cap and cut-list workflows. Managed library provenance remains an explicit local/cloud contract. |
| [OSH-0267 Sheet Metal Model](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_model.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0268 Sheet Metal Flange](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_flange.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0269 Sheet Metal Hem](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_hem.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0270 Sheet Metal Tab](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_tab.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0271 Sheet Metal Bend](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_bend.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0272 Sheet Metal Jog](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_jog.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0273 Sheet Metal Form](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_form.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0274 Onshape Sheet Metal Forms Library](https://cad.onshape.com/help/Content/Library/sheet_metal_form_library.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0275 Sheet Metal Loft](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_loft.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0276 Sheet Metal Make Joint](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_make_joint.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0277 Sheet Metal Corner](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_corner.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0278 Sheet Metal Bend Relief](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_bend_relief.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0279 Sheet Metal Modify Joint](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_joint.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0280 Sheet Metal Corner Break](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_corner_break.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0281 Sheet Metal Table and Flat View](https://cad.onshape.com/help/Content/PartStudio/sheet_metal_table.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0282 Finish Sheet Metal Model](https://cad.onshape.com/help/Content/PartStudio/finish_sheet_metal_model.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0283 Flex PCB Model](https://cad.onshape.com/help/Content/PartStudio/flex_pcb_model.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0284 Finish Flex PCB Model](https://cad.onshape.com/help/Content/PartStudio/finish_flex_pcb_model.htm) | [sheet-metal-missing](#wf-sheet-metal-missing) | missing | Add native-authoritative folded/flat editable model, thickness/bend/joint rules and each named operation. No replacement sheet-metal/flex-PCB task exists; do not substitute thin solids. |
| [OSH-0285 Add Custom Features](https://cad.onshape.com/help/Content/PartStudio/add_custom_features.htm) | [packages](#wf-packages) | bounded-ui-access | Local declarative packages are available; FeatureScript authoring, executable custom features and custom UI/service/library discovery are absent. |
| [OSH-0286 Assemblies](https://cad.onshape.com/help/Content/Assembly/assembly.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0287 Insert Parts and Assemblies](https://cad.onshape.com/help/Content/Assembly/insert_parts_and_assemblies.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0288 Linking Documents](https://cad.onshape.com/help/Content/Document/linking_documents.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0289 Updating References](https://cad.onshape.com/help/Content/Assembly/updating_references.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0290 Instances List](https://cad.onshape.com/help/Content/Assembly/instances_list.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0291 Standard Content](https://cad.onshape.com/help/Content/Assembly/standard_content.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0292 Onshape Standard Content Library](https://cad.onshape.com/help/Content/Library/onshape_standard_content_library.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0293 Bill of Materials (BOM)](https://cad.onshape.com/help/Content/Assembly/bill_of_material.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0294 Working with the BOM Table](https://cad.onshape.com/help/Content/Assembly/working_with_the_bom_table.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0295 BOM Table Formatting](https://cad.onshape.com/help/Content/Assembly/bom_table_formatting.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0296 BOM Table Templates](https://cad.onshape.com/help/Content/Assembly/bom_table_templates.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0297 Mate Connector](https://cad.onshape.com/help/Content/Assembly/assembly_mate_connector.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0298 Mates](https://cad.onshape.com/help/Content/Assembly/mates.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0299 Fastened Mate](https://cad.onshape.com/help/Content/Assembly/fastened_mate.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0300 Revolute Mate](https://cad.onshape.com/help/Content/Assembly/revolute_mate.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0301 Slider Mate](https://cad.onshape.com/help/Content/Assembly/slider_mate.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0302 Planar Mate](https://cad.onshape.com/help/Content/Assembly/planar_mate.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0303 Cylindrical Mate](https://cad.onshape.com/help/Content/Assembly/cylindrical_mate.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0304 Pin Slot Mate](https://cad.onshape.com/help/Content/Assembly/pin_slot_mate.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0305 Ball Mate](https://cad.onshape.com/help/Content/Assembly/ball_mate.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0306 Parallel Mate](https://cad.onshape.com/help/Content/Assembly/parallel_mate.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0307 Tangent Mate](https://cad.onshape.com/help/Content/Assembly/tangent_mate.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0308 Width Mate](https://cad.onshape.com/help/Content/Assembly/width_mate.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0309 Group](https://cad.onshape.com/help/Content/Assembly/group.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0310 Snap Mode](https://cad.onshape.com/help/Content/Assembly/snap_mode.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0311 Show Mates Mode](https://cad.onshape.com/help/Content/Assembly/show_mates_mode.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0312 Replicate](https://cad.onshape.com/help/Content/Assembly/replicate.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0313 Replace Instance](https://cad.onshape.com/help/Content/Assembly/replace_instance.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0314 Assembly Linear Pattern](https://cad.onshape.com/help/Content/Assembly/assembly_linear_pattern.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0315 Assembly Circular Pattern](https://cad.onshape.com/help/Content/Assembly/assembly_circular_pattern.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0316 Assembly Mirror](https://cad.onshape.com/help/Content/Assembly/assembly_mirror.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0317 Relations](https://cad.onshape.com/help/Content/Assembly/relations.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0318 Gear Relation](https://cad.onshape.com/help/Content/Assembly/gear_relation.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0319 Rack and Pinion Relation](https://cad.onshape.com/help/Content/Assembly/rack_and_pinion_relation.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0320 Screw Relation](https://cad.onshape.com/help/Content/Assembly/screw_relation.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0321 Linear Relation](https://cad.onshape.com/help/Content/Assembly/linear_relation.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0322 Named Positions](https://cad.onshape.com/help/Content/Assembly/named_positions.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0323 Display States](https://cad.onshape.com/help/Content/Assembly/display_states.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0324 Create Part Studio in Context](https://cad.onshape.com/help/Content/Assembly/create_part_studio_in_context.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0325 Simulation](https://cad.onshape.com/help/Content/Assembly/simulation.htm) | [extension-missing](#wf-extension-missing) | missing | Define the named extension/simulation authoring, execution, permission, native result and persistence contract. Declarative packages do not implement it. |
| [OSH-0326 Modal Simulation](https://cad.onshape.com/help/Content/Assembly/modal_simulation.htm) | [extension-missing](#wf-extension-missing) | missing | Define the named extension/simulation authoring, execution, permission, native result and persistence contract. Declarative packages do not implement it. |
| [OSH-0327 Asynchronous Simulation](https://cad.onshape.com/help/Content/Assembly/asynchronous_simulation.htm) | [extension-missing](#wf-extension-missing) | missing | Define the named extension/simulation authoring, execution, permission, native result and persistence contract. Declarative packages do not implement it. |
| [OSH-0328 Exploded Views](https://cad.onshape.com/help/Content/Assembly/exploded_views.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0329 Assembly Variable Table](https://cad.onshape.com/help/Content/Assembly/assembly_variable_table.htm) | [assembly-missing](#wf-assembly-missing) | missing | Add an assembly document/instance/reference model, Insert, local Fix, native mate frames, solver-backed mates and relations, update/replace, instance history and persistence. Body Placement and App::Part are not assembly joints. |
| [OSH-0396 Variable Studios](https://cad.onshape.com/help/Content/VariableStudio/variable_studio.htm) | [configuration-missing](#wf-configuration-missing) | missing | Define and implement configuration inputs, suppression/visibility conditions, propagation and saved variant identity. Native expressions are present; configuration/variable-table UI is absent. |
| [OSH-0397 Feature Studios](https://cad.onshape.com/help/Content/FeatureStudio/feature_studios.htm) | [extension-missing](#wf-extension-missing) | missing | Define the named extension/simulation authoring, execution, permission, native result and persistence contract. Declarative packages do not implement it. |


## Validate the ledger


Run from the repository root with Python 3. No native runtime, Cargo build or desktop is needed.


```sh
python next/tools/validate_workflows.py
python next/tests/workflow_ledger_tests.py
```


The structural validator checks the original inventory hash and exact topic/requirement IDs/text, unique route definitions, family prerequisites and lifecycle stages, acceptance gates, native preferred-field declarations, source anchors and unsupported completion claims. It cannot prove native semantics or real UI delivery. New behavior requires a deliberate ledger/evidence update and the relevant original gates.

Historical anchors, native declarations and package declarations resolve from `metadata.source_commit`. The checker reads local Git objects without using the current working files as historical evidence. CI or a shallow checkout must make that commit available locally. The checker never fetches it.

For an exported source tree, pass `--pinned-root BASELINE_SOURCE_ROOT`. That root must contain the pinned `next/` files. Every byte hash in the public [pinned source manifest](coverage/workflow-sources.json), including the package file set, is verified before use. A missing or changed baseline fails; there is no silent fallback to current files. An archive can be prepared with `git archive --format=tar --output=baseline-next.tar dbb932af266021f7aed579daf9c957e16247f063 next` and extracted into a new baseline directory.

Current compatibility is a separate default check. It requires exact native stable IDs, types, groups and declared fields, plus package IDs and parameter schemas. `current_compatibility.presentation_updates` in the ledger records the two intentional label changes by stable ID, each with its historical and current value. Historical `Union` labels, source URLs/anchors and all acceptance remain unchanged. An unlisted rename or an acceptance update fails. A read-only guide may use that mapping to show current names alongside this audit's historical routes; it does not establish current workflow parity.

`--historical-only` explicitly checks historical integrity without claiming current compatibility. It still requires authenticated pinned source. The normal command checks both. The public tests include the historical removed anchor, intentional label updates and negative ID/input/type/group/package/label/acceptance and verified-root cases.

