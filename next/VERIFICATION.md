# Editor verification

Verified October 20, 2026. This is an initial editor milestone, not a complete
Part Studio or a replacement release for the earlier Windows application.

## Real Pixel desktop workflow

The native Linux ARM64 editor ran as a separate application beside the existing
prototype. The documented `pixel-desktop-tool` client supplied real mouse and
keyboard input. Screenshots were inspected during the workflow.

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

Windows x64 and Pixel Linux ARM64 pass the Rust checks and all three real-native
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
- Rust tests cover transport failure, actor document preservation after failed
  Open, committed edits followed by display failure, depth-aware picking,
  selection invalidation, sketch coordinates and transformed normals. Lifecycle
  fixtures do not substitute for native geometry checks.

## Remaining gates

Interactive arbitrary lines/arcs, constraint authoring, constrained dragging,
multi-Body display, assemblies, feature-tree rollback/reorder and executable
plugins remain unfinished. Broader selection and attachment cases need GUI
checks. Moderate parts need controlled input latency, regeneration and memory
measurements. The full portable native runtime remains an evaluation dependency;
slim packaging and matched Rust component replacements have not been promoted.
