# Editor verification

Verified October 20, 2026. The Rust editor now includes interactive native
sketch editing. This record does not establish complete Part Studio, FreeCAD
sketch-tool or Onshape parity, or a replacement release for the earlier Windows
application.

## Real Pixel desktop workflow

The native Linux ARM64 editor ran as a separate application beside the existing
prototype. The documented `pixel-desktop-tool` client supplied real mouse and
keyboard input. Screenshots were inspected during the workflow.

### Sketch editing

The current sketch editor was exercised with real Pixel input:

- Drew Point, Line, Polyline, Rectangle, Circle and three-point Arc, including
  construction geometry. These tools retain native freedom for later dimensions
  and constraints rather than fixing every coordinate during creation.
- Created a 40 mm dimension through the interactive dimension dialog and applied
  a vertical constraint through Constrain.
- Dragged an endpoint, a circle center and a circle radius through native
  Sketcher, using solved results as the displayed geometry.
- Used a selection box to delete geometry and restored it with session Undo.
  Trim and its Undo were also exercised.
- Finished a sketch and created a native 10 mm pad, then created and applied a
  10 mm pocket.
- Confirmed that Return applies a dimension, bare values receive native length
  units, and Delete on a selected constraint removes the constraint rather than
  its highlighted geometry. Cancel restored the starting sketch, and canceling
  a new sketch removed its temporary feature.
- Exercised sketch entry/Cancel with native redo present and committed a feature
  edit after Undo; Undo/Redo remained usable.
- Reopened a consumed profile, deleted a required segment and confirmed that
  Finish rejected the open contour. Undo repaired the sketch and Finish then
  succeeded.

An independent native worker reopened an earlier GUI-created sketch document.
The saved sketch contained four lines, eight constraints and four remaining
degrees of freedom, including its 40 mm dimension. Its 10 mm pad was valid with
volume 13723.55643507148 cubic millimeters. Reopen preserved the editable
geometry and constraints. A separate copy was edited to 45 mm, passed native
Undo/Redo, saved and reopened with volume 15062.713394201584 cubic millimeters.
The source document's hash remained unchanged. The private report is retained
locally; the source model and screenshots are not published by these checks.

The later all-tools GUI file also passed an independent native check: two
sketches, a Pad and a Pocket; its attached sketch contains eight geometries,
eight constraints and 18 degrees of freedom. The normal circle cuts the solid;
the construction arc, point and five lines do not. Support remains
`Extrude4.Face6` at z=10 mm. The valid pocket volume, 13144.699350942195 cubic
millimeters, matches the pad volume minus the analytical circular cut. Its
original file hash remained unchanged.
The interactive checks above do not establish GUI coverage of every constraint
selection form, every expression/state control or difficult trim/drag case.

### Earlier feature-chain baseline

These earlier GUI checks remain useful evidence for solid modeling and face
attachment. The former rectangle tool fixed its dimensions at creation; current
drawing leaves freedom for explicit authoring.

- Created a native, fully constrained 40 by 30 mm rectangle with two clicks.
- Previewed and applied a 10 mm extrusion.
- Selected its top face and created a native face-attached circular sketch.
- Created a pocket, then reopened it and changed its depth to 5 mm.
- Reopened the attached sketch and set its radius to 3 mm.
- Reopened the base sketch and changed its width to 50 mm. The pocket stayed
  attached and recomputed from the changed parent.
- Selected an outer edge and previewed a 2 mm fillet. A 100 mm candidate failed;
  Apply stayed disabled and Cancel restored the original unfilleted body.
- Created and applied the fillet again. Ctrl+Z removed the entire feature;
  Ctrl+Shift+Z restored it. Ctrl+Shift+S saved a separate native FCStd file, and
  Ctrl+S saved subsequent edits.
- Reopened native files in the editor. Orbit, pan, Top, Iso and Fit changed the
  view without changing the native document revision.

An independent native worker reopened the GUI-created final FCStd. Both sketches
had zero remaining freedom. Native support was `Extrude2.Face6` at z=10 mm.
The pocket volume matched `50*30*10 - pi*3^2*5`. The final filleted volume matched
that result minus `50*2^2*(1-pi/4)`, at 14815.707963267949 cubic millimeters.
The native solid was valid. Its display mesh had 644 triangles, with one oriented
native normal per display vertex. This checks the saved model independently of
what the editor drew.

The debug editor used 82508 KiB RSS for this part. Its native Python worker used
97580 KiB, plus a 2768 KiB runtime launcher. All three measured zero CPU over
five idle seconds and zero swap. These figures describe one small part. They
are not a moderate-part performance gate or an Onshape speed comparison.

The earlier Windows application and the existing Pixel prototype were left
running. Their executables and original documents were not replaced.

## Automated checks

Windows x64 and Pixel Linux ARM64 pass the Rust checks and all five real-native
workflow scripts. CI repeats the Rust checks, including the optional desktop
target, and native workflows on Windows and Linux x64.

- `native_workflows.py`: sketches, construction, dragging, arcs, native units
  and expressions, attachment remapping, transformed Bodies, save/reopen,
  fillets, failures and native undo/redo. Display checks cover outward and inward
  oriented normals, sphere poles, placement transforms and curve detail.
- `preview_workflows.py`: candidate replacement from baseline, revision and
  owner rejection, worker-wide lease, failed candidate recovery, one-step undo,
  cancel and save/reopen. Native redo and rollback limitations are explicit in
  [PROTOCOL.md](PROTOCOL.md).
- `package_workflows.py`: typed package expansion through the same native
  document, constrained profiles, attached pocket, parent edits, fillet volume,
  failure atomicity, undo/redo and continued editing after reopen.
- `sketch_commands_workflows.py`: all six Rust drawing tools and 37 authored
  constraint cases run through the actual compiler and native solver. Checks
  compare solved freedom, native constraint types and typed dimensional values,
  including point/axis symmetry, tangency and zero-valued dimension seeds.
- `sketch_editor_workflows.py`: reference/driving, active/virtual and named
  constraints; native points and trim Undo/Redo; consumed profiles with open
  intermediate contours; failed Finish recovery; session ownership and drag
  previews; action Undo/Redo; cancellation after more than 30 actions; one native
  Undo entry on Finish; save/reopen and attached-pocket dimension edits.
- Rust tests cover transport failure, actor document preservation after failed
  Open, committed edits followed by display failure, depth-aware picking,
  selection invalidation, sketch coordinates and transformed normals. Sketch
  checks include native handle/origin picking, multiselection, exact box/segment
  crossing, dimension label/line hits, attached-coordinate snapping and arc
  preview interpolation. Lifecycle fixtures do not substitute for native geometry
  checks.

Run the two sketch suites after building the host and compiler:

```sh
cargo build --manifest-path next/Cargo.toml --locked
python next/tests/sketch_commands_workflows.py --host next/target/debug/cad-next --compiler next/target/debug/cad-next-sketch --runtime next/target/runtime.json --output next/target/sketch-command-documents
python next/tests/sketch_editor_workflows.py --host next/target/debug/cad-next --runtime next/target/runtime.json --output next/target/sketch-editor-documents
cargo test --manifest-path next/Cargo.toml --features desktop --locked
```

Use `.exe` for the host and compiler paths on Windows. Runtime setup is described
in [README.md](README.md).

## Remaining gates

The interactive tools above do not include splines, external sketch geometry or
all FreeCAD sketch tools. Advanced features, multi-Body display, assemblies,
feature-tree rollback/reorder and executable plugins remain open. Broader
constraint, selection, trim, dragging and attachment cases need GUI checks.
Feature previews defer recomputation until Apply when existing native redo would
otherwise be lost; a live preview is unavailable in that case, and Cancel
preserves redo.

Moderate parts need controlled input latency, regeneration and memory
measurements. The resource figures above remain the earlier small-part baseline;
they were not remeasured as a performance gate for the expanded sketch editor.
The full portable native runtime remains an evaluation dependency; slim packaging
and matched Rust component replacements have not been promoted.
