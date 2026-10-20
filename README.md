# CAD

The replacement is being built in [`next`](next/README.md), using native FreeCAD
modeling behind a Rust interface. The application described below is the earlier
prototype. It remains available while the replacement is verified. Its source is
preserved at [`archive/rust-prototype-e9040be`](https://github.com/LeoHougaard/CAD/tree/archive/rust-prototype-e9040be).

A local, offline Rust Part Studio with an editable parametric feature history. Dark UI, native OpenGL viewport, background geometry worker, and local feature packages. Tested on Windows and native Debian ARM64 on a Pixel 10.

## Run

Open `dist/CAD.exe`. There is no server, account, browser runtime, or network requirement. Keep the adjacent `plugins` and `examples` folders with the executable.

From source, run `./build.ps1 -Run` in PowerShell. The script uses the project-local Rust toolchain if present, otherwise your installed Cargo. Standard Rust builds also work:

```sh
cargo run --release --bin cad
cargo run --release --bin cad -- --demo
cargo run --release --bin cad -- examples/mounting-plate.cadpart
```

Linux builds use the same Cargo commands and local feature packages. The Pixel test installation is `/home/leo/projects/rust-cad/dist`; launch it with `pixel-gpu /home/leo/projects/rust-cad/dist/cad`. Other Linux machines can launch `cad` directly with an OpenGL 3.3-capable driver. Linux x86-64 builds and headless tests run in CI; native desktop interaction is tested on Pixel ARM64.

## Sketch and make a part

1. Click **Sketch**, then choose Top, Front, Right, or a tracked planar face.
2. Roughly draw a rectangle, circle, or closed chain of lines. There is no grid. Geometric inferences snap to the origin, endpoints, centers, and horizontal/vertical alignments.
3. Press **D**, click an edge or circle, move the pointer, and click empty space to place the dimension. Enter a value and press Enter. Select two points for distances, or two lines for a distance or angle.
4. Double-click a dimension label to edit it. Expressions accept units and variables, including `25.4`, `2 * inch`, `width / 2`, and `=#width / 2`. Define variables in the top bar.
5. Click points or edges to add them to the selection, then apply geometric constraints. Click again to deselect; Space or empty space clears. Blue geometry can move; white geometry is fully constrained; turquoise marks selection, purple marks hover, and red indicates a constraint problem. Drag geometry to explore remaining freedom. Construction geometry is excluded from solid profiles.
6. Finish the sketch, then Extrude or Revolve it. Shift+E finishes a valid sketch and opens Extrude. Extrude supports individual [sketch regions](docs/sketch-regions.md), New, Add, Remove, Intersect, and symmetric or reversed directions.
7. Select a tracked planar face for an attached sketch. A reversed Remove extrusion makes a pocket or through hole.

Dimensions and geometric relationships are stored constraints. Under-constrained geometry keeps its saved starting position and remains editable. Reference dimensions measure the solved geometry without driving it.

The upper-right navigation cube selects face, edge, and corner views. Its labels follow the cube faces. Drag it to orbit; its arrows roll in 15 degree steps, or 90 with Shift and 5 with Ctrl. Home restores an isometric view. The camera uses orthographic projection.

Right drag orbits freely around the point under the initial cursor, or the visible model's volume center when starting over empty space. A small ring shows the pivot during rotation. Alt + right drag keeps the vertical axis upright. Middle drag, Shift + right drag, or Ctrl + right drag pans. The wheel zooms toward the cursor, and **F** fits the view. Double-click a history row to edit it. Right-click it to rename, suppress, reorder, roll back, or delete. Ctrl+Z and Ctrl+Y undo and redo. Save editable documents as `.cadpart`. STEP and STL are available in the File menu.

The arrow collapses the feature tree. The feature editor overlays the viewport only while editing, and less-used tools are under More. Fit uses the current viewport width and height, including narrow windows and open editors. [Keyboard defaults](docs/shortcuts.md) follow Onshape for implemented tools. Tab moves between fields. S opens the shortcut tools, and Alt+C opens tool search.

SpaceMouse support on Windows uses foreground Raw Input in object mode, with six-axis navigation, a dead zone, and a Fit button. Reports are integrated at their timestamps with a short exponential response, so motion does not depend on irregular redraw timing. Rotation uses the visible model's volume center when it fits, or the surface near the view center in a close-up. The View menu shows device detection and speed/reversal controls. Sketch editing keeps the camera aligned while allowing pan and zoom. The user confirmed the corrected axis mapping; the latest smoothing still needs physical feedback. Linux SpaceMouse input is not implemented yet.

[Settings and search](docs/settings-and-search.md) provide local preferences and keyboard search across commands, history and parts. The toolbar shows installed tools directly, with overflow for narrow windows.

Selection uses ordinary additive clicks in the tree, viewport and sketch. Choose faces, edges, parts or sketch regions from the compact Select menu. The grave accent key opens Select other for obscured geometry; press it again to cycle, Shift+grave to reverse, and Enter to select. Drag right to enclose geometry or left to select across it. Ctrl-drag removes box hits. Selected solids keep depth-tested turquoise highlights, while the hovered candidate is purple. The background and panels are nearly black.

## Current scope

The initial solid tools include extrusion, revolve, Boolean union/subtract/intersect, sweep, ruled loft, fillet, chamfer, shell, linear part patterns, mirror, and translation. Fillet and chamfer accept selected edges or an explicit All edges choice. Pattern and mirror currently create separate parts. [Sweep and loft](docs/sweep-loft.md) documents their supported path/profile combinations and current limits.

[Construction planes](docs/construction-planes.md) provide reusable offsets, parallel or angular midplanes, and planes angled about exact axes. Sketches reference plane features and follow upstream edits. Origins and construction planes use the same 100 × 100 mm display and selection bounds.

[Numeric expressions](docs/numeric-expressions.md) support variables, powers, roots, units and degree trigonometry in dimensions and feature fields. New documents use format 2; older documents retain their original math semantics.

Sketches support points, lines, rectangles, circles, center and three-point arcs, trim, dimensions, construction geometry, coincidence, midpoint, horizontal/vertical, parallel/perpendicular, equality, concentricity, endpoint tangency, and fixing geometry. Dragging uses temporary solver targets; dimension edits persist the solved geometry. See [sketch workflows](docs/sketch-workflows.md). Splines, extend, offset, cross-sketch projection, advanced constraint diagnostics, and Onshape FeatureScript are not implemented yet.

Persistent references cover generated extrusion planes and supported analytical revolve caps and rims. [Geometry inputs](docs/geometry-controls.md) describes planar-face profiles, explicit operation scopes and sketch/origin/solid axes. Curved faces and new blend faces are not stable sketch supports yet. Missing or ambiguous references report an error instead of choosing an arbitrary face. Complex blends and booleans can still fail. Earlier valid parts are preserved when a feature fails; exporting a failed model is blocked in the app.

[Assemblies](docs/assemblies.md) use offline source snapshots, repeated part instances, six mate types, and explicit in-context references. Planar faces and analytic circular edges supply mate connectors. Sketch planes and parallel up-to-face extrusions can reference frozen external geometry. Context updates rebuild those relationships only when requested. General articulated loops, nested subassemblies, collision detection, and arbitrary up-to-surface extents remain unsupported.

## Feature packages

`plugins/*.json` declares a feature's parameters, editor, and local Rhai script. `build(params)` returns one operation or a list of operations. The separate `spacer` package demonstrates composing a new editable feature without a host feature variant. STEP and STL commands are exporter packages.

Use Plugins to disable packages or reload local changes. Documents pin package version and contents. Editing a feature offers explicit adoption if its package changed. Disabled or missing packages leave their history data intact.

Scripts compose the available Rust kernel operations. Adding a kernel operation, editor type, or export codec still requires Rust code and a rebuild. Disabling a package removes its loaded definition and command; it does not remove already linked kernel code. This is the initial extension system, not a complete native plugin ABI or FeatureScript implementation.

## Working with agents

`cad-agent` provides a local JSON interface to inspect solved geometry, preview edits, change dimensions and parameters, create separate designs, and export. Agents can edit separate files or submit guarded changes to the same document. Disjoint changes can merge; conflicting changes are rejected. Each request runs once and exits, with no background service.

The desktop app shares the CLI's file lock and refuses to overwrite changes made since its last load or save. Ctrl+R reloads external edits. Save as preserves a separate copy of unsaved work. `cad-assembly-agent` provides revision-guarded assembly edits, context updates and exact placed STEP exports. See [assemblies](docs/assemblies.md) and [the part agent API](docs/agent-api.md) for the protocol, concurrency rules, and validated Pixel workflow.

## Build and verify

```sh
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --release --workspace
cargo run --release --bin cad-check -- --bench
```

`./build.ps1 -Check`, `-Test`, and `-Run` are Windows shortcuts. `./package.ps1` builds the portable app. The benchmark creates example documents and `output/benchmark.json`. It measures a fixed 42-feature panel and its last feature edit. It is not a comparison against Onshape.

The tests cover solved sketch geometry and degrees of freedom, analytic volumes, topology validation, upstream regeneration, missing references, bounded script execution, failed-feature transactions, interrupted writes, and save/reopen/export. `tools/verify_step.py` optionally checks exported test fixtures using Gmsh/Open CASCADE as an independent verifier. That verifier is a development tool and is not used or shipped by the app.

[Ten practice projects](docs/end-user-projects.md) cover dimensioned brackets, a bearing block, a pulley, an enclosure, a cable comb, mirrored clevises, patterned standoffs, a swept handle, a reducer and a hollow lofted duct. Each is checked after an upstream edit and save/reopen. All twenty baseline/edited STEP exports passed independent Open CASCADE volume and bounds checks. Original failed feature orders have saved reproductions, with positive checks for corrected workflows and explicit rejection checks for unsupported geometry.

[Earlier tutorial verification](docs/tutorial-verification.md) includes a 29-feature housing with 13 fully constrained sketches and 75 faces. It also records incomplete source workflows and their limitations.

[Workflow comparison](docs/workflow-comparison.md) records the FreeCAD and SolidCraft source review, generated-face continuation, edit transactions, installed plugin inputs, bounded blends, and remaining differences.

[Readiness checks](docs/readiness.md) distinguish native application testing, automated geometry checks, performance measurements and remaining limits. Benchmarks include rebuild times for all ten projects and cached pose updates for 24 moderately detailed assembly instances. These local measurements are not a comparison against Onshape. The independent Open CASCADE verifier is a development tool and is not used or shipped by the app.

## Implementation and license

The application, geometry kernel, and constraint solver are Rust. Geometry uses [Brepkit 4.2.2](https://github.com/andymai/brepkit), pinned to our [reviewed solid-boundary corrections](https://github.com/LeoHougaard/brepkit/commit/a534b99229b7c2c196fdb1f723e532db2b785c6e). All eleven kernel packages use that same source revision. The fork corrects blend material domains and exact analytic contacts, preserves historical operand topology, and refines supported connected same-domain boundaries. Curved-outline planar face merging remains deferred until its loop classification handles exact curves. The local sketch solver derives from Brepkit's GCS, retaining its original tests and adding dimension constraints and mobility analysis. The UI uses [egui/eframe](https://github.com/emilk/egui); feature definitions use [Rhai](https://rhai.rs).

The project is AGPL-3.0-only. See `LICENSE`, `Cargo.lock`, and `crates/sketch-solver/UPSTREAM.md`. Onshape is the workflow reference; its proprietary code and assets are not included.
