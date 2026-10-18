# CAD

A local, offline Rust Part Studio with an editable parametric feature history. Dark UI, native OpenGL viewport, background geometry worker, and local feature packages. Tested on Windows and native Debian ARM64 on a Pixel 10.

## Run

Open `dist/CAD.exe`. There is no server, account, browser runtime, or network requirement. Keep the adjacent `plugins` and `examples` folders with the executable.

From source, run `./build.ps1 -Run` in PowerShell. The script uses the project-local Rust toolchain if present, otherwise your installed Cargo. Standard Rust builds also work:

```sh
cargo run --release --bin cad
cargo run --release --bin cad -- --demo
cargo run --release --bin cad -- examples/mounting-plate.cadpart
```

Linux builds use the same Cargo commands and local feature packages. The Pixel test installation is `/home/leo/projects/rust-cad/dist`; launch it with `pixel-gpu /home/leo/projects/rust-cad/dist/cad`. Other Linux machines can launch `cad` directly with an OpenGL 3.3-capable driver. Linux x86-64 has not been tested yet.

## Sketch and make a part

1. Click **Sketch**, then choose Top, Front, Right, or a tracked planar face.
2. Roughly draw a rectangle, circle, or closed chain of lines. There is no grid. Geometric inferences snap to the origin, endpoints, centers, and horizontal/vertical alignments.
3. Press **D**, click an edge or circle, move the pointer, and click empty space to place the dimension. Enter a value and press Enter. Select two points for distances, or two lines for a distance or angle.
4. Double-click a dimension label to edit it. Expressions accept units and variables, including `25.4`, `2 * inch`, `width / 2`, and `=#width / 2`. Define variables in the top bar.
5. Ctrl-click points or edges and apply geometric constraints in the sketch panel. Blue geometry can move; white geometry is fully constrained; red indicates a constraint problem. Drag geometry to explore remaining freedom. Construction geometry is excluded from solid profiles.
6. Finish the sketch, then Extrude or Revolve it. Extrude supports New, Add, Remove, and Intersect, plus symmetric and reversed directions.
7. Select a tracked planar face for an attached sketch. A reversed Remove extrusion makes a pocket or through hole.

Dimensions and geometric relationships are stored constraints. Under-constrained geometry keeps its saved starting position and remains editable. Reference dimensions measure the solved geometry without driving it.

The upper-right navigation cube selects face, edge, and corner views. Its labels follow the cube faces. Drag it to orbit; its arrows roll in 15 degree steps, or 90 with Shift and 5 with Ctrl. Home restores an isometric view. The camera uses orthographic projection.

Right drag orbits freely around the point under the initial cursor, or the visible model's volume center when starting over empty space. A small ring shows the pivot during rotation. Alt + right drag keeps the vertical axis upright. Middle drag, Shift + right drag, or Ctrl + right drag pans. The wheel zooms toward the cursor, and **F** fits the view. Double-click a history row to edit it. Right-click it to rename, suppress, reorder, roll back, or delete. Ctrl+Z and Ctrl+Y undo and redo. Save editable documents as `.cadpart`. STEP and STL are available in the File menu.

Tab collapses the feature tree. The properties panel appears only while editing, and less-used tools are under More. Fit uses the current viewport width and height, including narrow windows and open editors.

SpaceMouse support on Windows uses foreground Raw Input in object mode, with six-axis navigation, a dead zone, and a Fit button. Rotation uses the visible model's volume center when it fits, or the surface near the view center in a close-up. The View menu shows device detection and speed/reversal controls. Sketch editing keeps the camera aligned while allowing pan and zoom. Physical SpaceMouse Pro input is confirmed; the corrected directions still need user feedback. Linux SpaceMouse input is not implemented yet.

## Current scope

The initial solid tools include extrusion, revolve, sweep, ruled loft, fillet, chamfer, shell, linear part patterns, mirror, and translation. Fillet and chamfer accept selected edges or an explicit All edges choice. Pattern and mirror currently create separate parts. [Sweep and loft](docs/sweep-loft.md) documents their supported path/profile combinations and current limits.

Sketches support lines, rectangles, circles, dimensions, construction geometry, coincidence, horizontal/vertical, parallel/perpendicular, equality, concentricity, line-circle tangency, and fixing geometry. Arcs, splines, trim/extend, offset, cross-sketch projection, advanced constraint diagnostics, assemblies, and Onshape FeatureScript are not implemented yet.

Persistent face references cover generated extrusion planes. Curved faces and new blend faces are not stable sketch supports yet. Missing or ambiguous references report an error instead of choosing an arbitrary face. Complex blends and booleans can still fail. Earlier valid parts are preserved when a feature fails; exporting a failed model is blocked in the app.

## Feature packages

`plugins/*.json` declares a feature's parameters, editor, and local Rhai script. `build(params)` returns one operation or a list of operations. The separate `spacer` package demonstrates composing a new editable feature without a host feature variant. STEP and STL commands are exporter packages.

Use Plugins to disable packages or reload local changes. Documents pin package version and contents. Editing a feature offers explicit adoption if its package changed. Disabled or missing packages leave their history data intact.

Scripts compose the available Rust kernel operations. Adding a kernel operation, editor type, or export codec still requires Rust code and a rebuild. Disabling a package removes its loaded definition and command; it does not remove already linked kernel code. This is the initial extension system, not a complete native plugin ABI or FeatureScript implementation.

## Working with agents

`cad-agent` provides a local JSON interface to inspect solved geometry, preview edits, change dimensions and parameters, create separate designs, and export. Agents can edit separate files or submit guarded changes to the same document. Disjoint changes can merge; conflicting changes are rejected. Each request runs once and exits, with no background service.

The desktop app shares the CLI's file lock and refuses to overwrite changes made since its last load or save. Ctrl+R reloads external edits. Save as preserves a separate copy of unsaved work. See [the agent API](docs/agent-api.md) for the protocol, concurrency rules, and validated Pixel workflow.

## Build and verify

```sh
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --release --workspace
cargo run --release --bin cad-check -- --bench
```

`./build.ps1 -Check`, `-Test`, and `-Run` are Windows shortcuts. `./package.ps1` builds the portable app. The benchmark creates example documents and `output/benchmark.json`. It measures a fixed 42-feature panel and its last feature edit. It is not a comparison against Onshape.

The tests cover solved sketch geometry and degrees of freedom, analytic volumes, topology validation, upstream regeneration, missing references, bounded script execution, failed-feature transactions, interrupted writes, and save/reopen/export. `tools/verify_step.py` optionally checks exported test fixtures using Gmsh/Open CASCADE as an independent verifier. That verifier is a development tool and is not used or shipped by the app.

[Tutorial verification](docs/tutorial-verification.md) records five source-based modeling cases and their adaptations. The largest tested housing has 29 history entries, 13 fully constrained sketches, and 75 faces. Six upstream size changes, rollback, suppression, reordering, save/reopen, and independent STEP checks are covered. Some source finishing operations remain blocked, including an ambiguous fillet edge reference and a perpendicular-cylinder boolean. These failures have saved reproductions; the application rejects them rather than accepting approximate solids.

The release suite passed 152 checks on both Windows and native ARM64 Linux. Seventeen sweep/loft STEP fixtures also passed independent Open CASCADE volume and bounds checks. [Release verification](output/release-verification.json) records the build hashes, final native UI checks and remaining limits.

An earlier navigation build passed 120 release checks on both platforms, plus eight independent STEP imports. Pixel desktop checks covered orbit pivots before and after pan, face pivots, cube labels, zoomed circular holes, extrusion editing and undo. Its 42-feature, 6,336-triangle panel measured 275 ms median full rebuild and 296 ms median last-feature edit over 12 samples. The four-feature model used about 105 MiB resident memory and 91 MiB private memory; a 15-second idle sample recorded 0.00 CPU seconds. These process-memory figures exclude the shared graphics server. Results and selected screenshots are in `output/`.

## Implementation and license

The application, geometry kernel, and constraint solver are Rust. Geometry uses [Brepkit 4.2.2](https://github.com/andymai/brepkit). The local sketch solver derives from Brepkit's GCS, retaining its original tests and adding dimension constraints and mobility analysis. The UI uses [egui/eframe](https://github.com/emilk/egui); feature definitions use [Rhai](https://rhai.rs).

The project is AGPL-3.0-only. See `LICENSE`, `Cargo.lock`, and `crates/sketch-solver/UPSTREAM.md`. Onshape is the workflow reference; its proprietary code and assets are not included.
