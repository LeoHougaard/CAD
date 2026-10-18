# Tutorial verification

Evidence recorded on 2026-10-17. These tests exercise saved documents, sketch constraints, regeneration and geometry. They do not establish that every tutorial can be completed through the native UI. Source dimensions use inches, converted with `inch = 25.4 mm`.

## Cases and limits

| Source | Verified work | Adaptations and remaining gaps | Evidence and example |
| --- | --- | --- | --- |
| Matthew Jourden, [Extrude Part 1: Single Profile Extrude](https://drive.google.com/file/d/1PUrUjtnXYu7C9-piphr1rg4BUoxOE2WZ/view), pages 4-11 | Fully constrained six-edge L profile, 4in extrusion, overall width edit from 5 to 4in. Volumes are 45 and 33in³. Solved endpoints, eight faces, topology, closed mesh and save/reopen checked. | Connected lines replace the source's overlapping rectangles and trim. The trim workflow is unavailable. | [Test](../tests/tutorial_lblock.rs), [report](../output/tutorials/lblock/report.json), [edited part](../output/tutorials/lblock/lblock-4in.cadpart) |
| Jourden, [Extrude Part 2: Adding Material and Cutting Material](https://resources.finalsite.net/images/v1722986235/brightonk12com/bg0jjfkr34hujkzkp4oq/tutorialextrudepartadd_cutmaterial.pdf), pages 2-7 | Attached riser cut, margin edit widening it from 2 to 3in, fused 1in³ boss, parent width edit. All sketches have zero degrees of freedom; attachment frames, volumes, topology, mesh closure and file regeneration checked. | Inherits the trim substitution. One variable drives both cut margins. The boss corner placement follows the diagram's edge coincidences. | [Test](../tests/tutorial_add_cut.rs), [report](../output/tutorials/add-cut/report.json), [completed part](../output/tutorials/add-cut/completed-add-cut.cadpart) |
| Jourden, [Revolve Solid Model](https://resources.finalsite.net/images/v1722986533/brightonk12com/ywtdpiqjzrokemitsmv4/tutorialonshaperevolvesolidmodel_1.pdf), pages 2-7 | Fully constrained profile, 360° and 180° revolutions, analytic volumes, diameter edit and restore, angle-history reuse. First bolt hole and its diameter/bolt-circle edits checked. | Sketch V axis replaces an arbitrary construction axis. Hole support uses an offset plane; the cut has explicit depth. The eight-hole circular feature pattern is unavailable. Later rounds and fillets remain untested. | [Test](../tests/tutorial_revolve.rs), [report](../output/tutorials/revolve/report.json), [first-hole part](../output/tutorials/revolve/revolve-one-hole.cadpart) |
| Jourden, [Extrude Part 3: Holes and Arcs](https://resources.finalsite.net/images/v1722986241/brightonk12com/nocj9hngnnoxr8c3h2fl/tutorialextrudepartholesandarcs.pdf) | Dimensioned blind top cut, side cut, whole-circle addition before the side cut, history reorder, zero-DOF sketches and analytic volumes. Selected source chamfer and three identifiable fillet edges pass. Split coplanar supports retain the original sketch frame. | Hole tool, mate connectors, drill tip and arc/trim option are omitted. Adding the circle after the side hole fails exact geometry and preserves the prior solid. Reordering produces a different result; it does not fix that failure. Fillet coverage is partial. | [Tests](../tests/tutorial_holes.rs), [report](../output/tutorials/holes/report.json), [reordered part](../output/tutorials/holes/circle-reordered.cadpart), [failed source order](../output/tutorials/holes/circle-after-side-hole-failed.cadpart) |
| Alejandro Reyes, [The Housing, Beginner's Guide to SolidWorks 2006](https://static.sdcpublications.com/pdfsample/1-58503-262-x-2.pdf), printed pages 9-50 | Functional housing with 29 timeline entries, 13 fully constrained sketches and 75 faces. Rounded body and pocket, four bosses, orthogonal bores, flange/lid drills and base mounts. Six parent edits, attachment frames, analytic volume, closed mesh, rollback, suppression, valid and invalid reorders, and save/reopen checked. | Base finishing fillet is preserved but suppressed because split edges share an ambiguous reference. Final finishing fillets are untested. Equivalent pocket-edge fillets replace sketch fillets. Dimensioned symmetric sketches replace feature mirrors/patterns. Through-all/up-to-next use parent expressions. Threads, drill cones and tolerance annotations are omitted; holes represent documented tap drills. Full source completion remains blocked. | [Test](../tests/tutorial_complex.rs), [report](../output/tutorials/complex/report.json), [housing](../output/tutorials/complex/housing.cadpart), [resized housing](../output/tutorials/complex/housing-resized.cadpart), [fillet reproduction](../output/tutorials/complex/base-fillet-reproduction.cadpart) |

All five cases include STEP export and import checks through the application's kernel. Separate Gmsh/Open CASCADE checks confirm solid counts and analytic volumes for the [L-block](../output/tutorials/lblock/independent-step-check.json), [add/cut](../output/tutorials/add-cut/independent-step-check.json) and [housing](../output/tutorials/complex/independent-step-check.json). The housing checks also compare bounds. Its baseline volume is 291,974.856919mm³, within 0.000287mm³ of the analytic calculation.

Native Pixel UI testing also exposed a housing edit limit. Changing only the resized Body profile's Height dimension from `#depth` at 76.2mm to 80mm made the body overhang its unchanged base. The quarter-inch fillet then reported a missing edge reference instead of guessing. Undo restored the valid model and the saved source remained unchanged. See the [dimension edit](../output/pixel-housing-dimension-edited-detail.png), [reference error](../output/pixel-housing-edit-error-detail.png) and [undo](../output/pixel-housing-undo-detail.png). This was an explicit failure and recovery, not a successful arbitrary upstream edit; the six coordinated variable edits in the automated test pass.

## Unresolved cylinder boolean

The holes/arcs failure has a [six-feature reproduction](../output/tutorials/holes/minimal-disc-fuse-reproduction.cadpart). [Diagnosis](../output/tutorials/holes/kernel-diagnosis.json) places it in Brepkit 4.2.2's periodic curved-face splitting: the disc's cylindrical face splits into no usable regions. Mesh fallback is rejected; the exact prior solid remains intact.

A separate [kernel experiment](../output/tutorials/holes/kernel-experiment.json) tested removal of duplicate directed section edges. It recovered zero failures; all 12 fallback cases remained. The candidate was rejected. No candidate patch, geometry perturbation or relaxed tolerance was promoted into the application. An upstream periodic-face arrangement correction remains necessary.

## Recorded Pixel measurements

The [native navigation report](../output/navigation-pixel-verification.json) covers a Pixel 10 running native Debian 13 ARM64 with Termux:X11 and VirGL/Zink. These measurements belong to that recorded navigation build, not the housing or an unmeasured later binary.

- The [42-feature panel benchmark](../output/benchmark-pixel.json) recorded a 274.8ms median full rebuild and 296.1ms median last-feature edit across 12 samples, with zero mesh fallbacks.
- The panel process used about 106MiB RSS. Its [15-second idle sample](../output/pixel-panel-idle.json) recorded no CPU timer ticks. Shared graphics-server memory is excluded.
- During 1,200 pointer events over 12.87 seconds, the application consumed 2.46 CPU seconds, about 19.1% of one core. Event rate is not displayed FPS.

No reliable FPS result was obtained. These measurements do not establish performance relative to Onshape, assembly performance or sustained thermal behavior. Native navigation and dimension-edit checks are listed separately in the navigation report; the backend tutorial tests do not replace UI testing.

A separate [headless housing benchmark](../output/benchmark-pixel-housing.json) ran five `cad-agent inspect` processes per model on the Pixel. Median end-to-end wall time was 522ms for the housing and 580ms for its resized variant. Both regenerated 75 faces and 13 zero-DOF sketches successfully. Timing includes process startup, regeneration, tessellation and JSON output. The recorded release CLI predates the final inactive-feature validation and kernel-mutex changes; its hash identifies the measurement build. These are headless timings, with no viewport rendering.

## Reproduce backend checks

With the project's Rust toolchain configured:

```sh
cargo test --release --test tutorial_lblock --test tutorial_add_cut --test tutorial_revolve --test tutorial_holes --test tutorial_complex
```

The tests regenerate their documents, STEP files and JSON reports under `output/tutorials/`. Run native UI checks against those `.cadpart` files separately.
