# Ten verified practice projects

These complete designs use the workflows taught in the linked official Onshape
tutorials and Tech Tips. The dimensions and acceptance calculations are ours.
Their scope is the finished geometry described below, rather than reproduction
of every model shown in an Onshape video. Every example contains editable
dimensions, expressions, constraints, and feature history. Every sketch has zero
remaining degrees of freedom without Fix constraints.

All dimensions are millimeters. Each project has a baseline, an upstream variable
edit, save/reopen regeneration, closed B-rep and oriented display-mesh checks,
independent analytic volume and bounds checks, and baseline/edited STEP exports.
The exact-operation guard rejects every mesh fallback. Independent Gmsh/Open
CASCADE checks all twenty exported files at a relative volume tolerance of 1e-7
and a bounds tolerance of 1e-5 mm.

| Project | Finished design | Upstream edit | Primary workflow source |
| --- | --- | --- | --- |
| [Adjustment bracket](../examples/project-adjustment-bracket.cadpart) | 80 × 40 × 6 base, 40-high upright, two 6-wide rounded slots with 12 between end centers, two 6-diameter mounting holes | Width 80 → 90; slots and base holes follow | [Onshape Slot tutorial](https://www.onshape.com/en/resource-center/tech-tips/tech-tip-how-to-sketch-slots), [Extrude](https://cad.onshape.com/help/Content/PartStudio/extrude.htm) |
| [Bearing block](../examples/project-bearing-block.cadpart) | 80 × 50 × 6 foot, 50 × 30 × 30 pedestal, 18-diameter transverse bearing bore, 22-diameter shoulder 5 deep, four 6-diameter mounting holes | Pedestal height 30 → 36; bore and shoulder remain centered | [Extrude Add/Remove and selected regions](https://cad.onshape.com/help/Content/PartStudio/extrude.htm) |
| [Grooved pulley](../examples/project-grooved-pulley.cadpart) | 48-diameter V-groove pulley, 16-wide rim, 6-high hub extension, 8-diameter bore, four 4-diameter web holes | Rim radius 24 → 26; groove section follows | [Revolve tutorial](https://cad.onshape.com/help/Content/PartStudio/revolve.htm) |
| [Instrument enclosure](../examples/project-instrument-enclosure.cadpart) | 80 × 50 × 25 open box, 2 wall and floor thickness, four 8-diameter hollow bosses 10 high, 4-diameter screw passages, 16 × 8 connector opening | Height 25 → 30; shell regenerates around the existing mounting details | [Shell](https://cad.onshape.com/help/Content/PartStudio/shell.htm), [Shell workflow and failure diagnosis](https://www.onshape.com/en/resource-center/tech-tips/troubleshooting-shell-failures-3d-modeling) |
| [Cable comb](../examples/project-cable-comb.cadpart) | 120 × 24 × 12 stock, five open channels 12 wide at 20 pitch, two 5-diameter mounting holes, 0.5 back-edge chamfer | Width 120 → 130; holes follow the ends | [Chamfer tutorial](https://cad.onshape.com/help/Content/PartStudio/chamfer.htm) |
| [Clevis pair](../examples/project-clevis-pair.cadpart) | Two mirrored 6-thick cheeks, 12 gap, 40 height, 8-diameter pin bores, 20 × 40 × 5 feet and two mounting holes per foot | Height 40 → 46; pin bores follow 10 below the top | [Part Mirror tutorial](https://cad.onshape.com/help/Content/PartStudio/mirror.htm) |
| [Standoff set](../examples/project-standoff-set.cadpart) | Four flanged standoffs at 30 pitch, 20-diameter × 3 flange, 14-diameter stem, 5-diameter through bore, 18 total height, 0.5 top-rim fillet | Height 18 → 22; fillet and all patterned bodies regenerate | [Linear part Pattern](https://cad.onshape.com/help/Content/PartStudio/linear_pattern.htm), [Fillet](https://cad.onshape.com/help/Content/PartStudio/fillet.htm) |
| [Swept handle](../examples/project-swept-handle.cadpart) | 6 × 8 rectangular bar on a three-line U path, 60 span and 30 rise, two 20 × 24 × 4 mounting feet and four screw holes | Rise 30 → 36; both legs and top mitres follow | [Sweep tutorial](https://cad.onshape.com/help/Content/PartStudio/sweep.htm) |
| [Circular reducer](../examples/project-circular-reducer.cadpart) | 50-long hollow reducer with outer radii 16, 10, 6 at three sections, 6-diameter passage, 46-diameter × 5 flange and four 4-diameter bolts on a 38 pitch circle | Length 50 → 60; both tapered sections follow | [Revolve](https://cad.onshape.com/help/Content/PartStudio/revolve.htm), [Loft profile selection](https://cad.onshape.com/help/Content/PartStudio/loft.htm) |
| [Rectangular duct](../examples/project-rectangular-duct.cadpart) | 40-long hollow transition, outside 50 × 30 → 30 × 20, inside 46 × 26 → 26 × 16, 70 × 50 × 4 flange with four mounting holes | Length 40 → 50; outer and inner lofts follow together | [Loft New and Remove tutorials](https://cad.onshape.com/help/Content/PartStudio/loft.htm) |

The clevis pair and standoff set intentionally contain two and four separate
parts. They test mirrored and patterned manufacturing sets in a Part Studio.

## Workflow adaptations and discovered failures

Rounded slots use a rectangular cut followed by circular endpoint cuts. This
creates the same capsule boundary while using the available sketch tools. A
native Slot tool and general tangent line/circle sketch arrangement are missing.

The handle uses a rectangular section and exact planar mitres. Onshape's
[U-bolt tutorial](https://www.onshape.com/en/resource-center/tech-tips/tech-tip-lets-make-a-u-bolt)
uses a circular section along a line/arc path. That U-bolt workflow remains
unsupported here; this example is a different complete handle design. Threads,
helical sweeps, and guided/spline lofts remain outside these project scopes.

Three first attempts exposed kernel failures. Their unsuppressed documents are
preserved in [tests/fixtures/project-rejections](../tests/fixtures/project-rejections).
The shared chamfer fix now passes the original cable-comb ordering, including an
upstream width edit, closed topology and analytical volume. The other two remain
explicit rejections that retain the previous precise solid.

| Rejected workflow | Observed result | Completed project workflow |
| --- | --- | --- |
| Drill enclosure boss centers after fusing annular bosses into the shell floor | Boolean requires a mesh approximation, so the operation is rejected | Drill the floor first, then add annular bosses above it; final geometry is unchanged |
| Bore through a fused circular loft and flange | Boolean requires a mesh approximation, so the operation is rejected | Revolve the complete hollow radial section, then drill flange bolts; the two conical sections and axial passage are explicit in the section |
| Chamfer the comb's long back edge after making the channel and mounting cutouts | Fixed by preserving inner wires in the shared exact setback operation | Both original and alternative orders pass |

The successful alternatives are saved as the examples. Failed features are not
suppressed in those projects. The two Boolean failures remain material limits for designs
that require the original feature ordering.

## Reproduce verification

```text
cargo test --release --test end_user_projects --test project_rejections
python tools/verify_projects_step.py
```

Each project records twenty full rebuilds and twenty last-feature edits, with
50th and 95th percentile wall times. Full rebuilds alternate baseline and upstream
edit in fresh runtimes. Last-feature edits alternate two real numeric parameter
values and must reuse every earlier feature. Timings include engine regeneration
and display-mesh construction. They measure the local build and machine used for
the run; they do not measure interactive frame latency or compare against Onshape.
`CAD_PROJECT_BENCH_SAMPLES` can set 5–100 samples for local diagnosis.

Fresh files and per-project reports are written under
`target/test-output/end-user-projects/<project>/`. The independent checker writes
`independent-step-check.json` alongside those project folders. Native application
interaction with these documents is checked separately.

## Measured optimized rebuilds

AMD Ryzen 7 5700G with Radeon Graphics, Windows 11 Pro x86-64,
twenty samples per operation, release optimization with LTO
disabled and sixteen code generation units. These timings include display mesh
construction. They are local engine measurements, not application frame times.

| Project | Full p50 ms | Full p95 ms | Last feature p50 ms | Last feature p95 ms |
| --- | ---: | ---: | ---: | ---: |
| Adjustment bracket | 27.75 | 28.92 | 12.12 | 12.55 |
| Bearing block | 21.28 | 22.32 | 11.03 | 11.90 |
| Cable comb | 21.12 | 22.09 | 8.58 | 9.15 |
| Circular reducer | 17.87 | 18.88 | 18.17 | 19.06 |
| Clevis pair | 14.10 | 15.09 | 7.28 | 7.59 |
| Grooved pulley | 22.24 | 23.26 | 22.07 | 23.42 |
| Instrument enclosure | 39.08 | 40.26 | 18.10 | 18.86 |
| Rectangular duct | 17.50 | 18.44 | 14.86 | 15.36 |
| Standoff set | 29.83 | 30.89 | 26.15 | 28.17 |
| Swept handle | 18.34 | 19.13 | 15.91 | 16.34 |

All ten baseline and ten upstream-edited STEP files passed independent
Open CASCADE checks for solid count, analytic volume and exact bounding boxes.
