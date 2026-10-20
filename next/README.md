# FreeCAD-based replacement

This is the replacement foundation. The earlier Rust/Brepkit application stays
available while this implementation is verified. It is not the modeling engine
for this version.

The first backend is pinned FreeCAD 1.1.4, including its Part, Sketcher and
PartDesign application modules and Open CASCADE. Rust owns the application-facing
interface. A small, local worker adapter initially calls FreeCAD's supported
Python API. It does not run FreeCAD's GUI or a network service. This interim
version is mixed-language; the eventual target remains Rust throughout.

This includes an initial Rust desktop editor and the headless modeling host.
The editor is under workflow verification. It has not replaced the earlier
Windows application. The [migration gates](MIGRATION.md) describe the remaining
work toward a complete Part Studio.

## Run the editor

Build the optional desktop target and use the same pinned runtime configuration:

```sh
cargo build --manifest-path next/Cargo.toml --features desktop --locked
next/target/debug/cad-next-editor --runtime next/target/runtime.json
```

Use `.exe` on Windows. `--open part.FCStd` opens an existing native document.
Run this as a separate application; it does not replace `dist/CAD.exe`.

Create a sketch, choose Rectangle or Circle, then click two points in the
viewport. Profiles have native dimensions and constraints. Their solved values
appear in the sketch dimension panel. Set explicit dimensions there, then choose
Extrude or Pocket. Selecting a planar solid face before Sketch creates native
face support. The feature list reopens sketches for dimension edits. Double-click
a solid feature to edit its length or radius. Fillet uses a selected visible edge.

Feature values create native previews. Apply commits one undo step; Cancel or
Escape restores the baseline. An invalid value cannot be applied. Existing
native redo history blocks previews until a committed edit branches history or
Redo consumes it. See [preview ownership and native limits](PROTOCOL.md).

Right-drag orbits, middle-drag pans, and the wheel zooms. F fits the model.
Ctrl+S saves the document; Ctrl+Shift+S opens the FCStd Save as dialog.
Ctrl+Z and Ctrl+Shift+Z use native Undo/Redo.
The editor asks before discarding unsaved changes. Native calls and display
extraction run on a model thread; camera/input do not wait for recomputation.
The viewport uses OpenGL depth testing and native face/edge identities. Geometry
and history remain entirely in the native document.

The [local feature package interface](PACKAGES.md) is available to both Rust
callers and `cad-next-packages`. The editor's Packages menu exposes loaded
declarative features, including their parameters. `--packages DIR` loads a
caller-selected collection. Executable plugins and custom UI extensions are
future work.

This is a bounded editor implementation, not complete Onshape parity. Current
interactive tools cover rectangles, circles, their dimensions, extrusion,
pockets and single-edge fillets. Arbitrary line/arc creation, interactive
constraint authoring, sketch dragging, multiple displayed Bodies, assemblies,
rollback-tree editing and robust file locking between processes remain
unfinished. General documents and moderate-part performance need broader gates.
The [verification record](VERIFICATION.md) lists the tested workflows and limits.

## Run the verified foundation

Python 3 and Cargo are needed for setup and the Rust host. Runtime setup downloads
the official portable FreeCAD distribution and verifies its pinned SHA256 before
extracting it into the checkout's ignored `.tools` directory. It does not install
anything globally or require FUSE. Supported runtime platforms are Windows x64,
Linux x64 and Linux ARM64.

```sh
cargo build --manifest-path next/Cargo.toml --locked
python next/tools/setup_runtime.py --output next/target/runtime.json
python next/tests/native_workflows.py --host next/target/debug/cad-next --runtime next/target/runtime.json --output next/target/native-documents
```

On Windows, use `next/target/debug/cad-next.exe` for `--host`. Launcher configuration
contains `executable`, `args` and `script`. For interactive NDJSON use, pass those
to `cad-next --executable PATH --arg ARG --script PATH`, repeating `--arg` as needed.
The pinned bundled Python runs the worker. `FreeCADCmd` does not execute this
worker's `__main__` entry point reliably and is not the configured launcher.

The full official distribution is an evaluation dependency. Its download and
disk size are substantial. It is not the intended lightweight shipping package.
Removing unused GUI modules and packaging a smaller native runtime comes after
the native behavior is established.

## Migration rules

- Keep one authoritative native document. Preserve FreeCAD's object links,
  expressions, attachment, topology naming, transactions and recomputation.
- Plugins and agents use the same versioned command boundary. Features compose
  native document operations; the frontend does not implement geometry rules.
- Replace one component at a time behind that boundary. Remove its old
  implementation only after matched geometry and workflow checks pass.
- Measure startup, working memory, idle CPU, preview latency and regeneration.
  Converting code to Rust is not evidence of a speed improvement.
- Save new documents as native FCStd during the initial migration. Import of
  the old cadpart format must be explicit and must report unsupported features.
  Never silently flatten editable history into a mesh or overwrite an original.

## First acceptance gate

Prove real native feature chains before a replacement editor:

1. Constrained sketch, pad, face attachment, constrained sketch, pocket.
2. Change the parent dimensions and topology, recompute, save, reopen and
   continue editing the same attached sketch.
3. Tangent fillet propagation, fillet termination at another surface, and
   a three-edge corner with adjoining details.
4. Reject an impossible radius without publishing a broken model; undo and
   redo complete feature transactions.

These headless checks pass on Windows x64 and the Pixel's native Linux ARM64.
They include empty sketch creation, constrained dragging, construction toggling,
deletion back to empty, arcs, native dimension expressions, unit rejection,
attachment remapping from Face6 to Face7, and continued editing after reopen.
They do not establish broad feature parity or interactive performance. CI repeats
native, package and preview checks on Windows and Linux x64 and compiles/tests
the optional desktop target.

Compare actual shapes, intended placement and reference ownership, validity,
volume, topology and saved history. Include native Pixel interaction once the
backend gate passes. FreeCAD itself has unsupported cases; this is a measured
baseline, not a claim that every fillet must succeed.

## Implementation boundary

The worker is a persistent child process. Rust exchanges newline-delimited
JSON through pipes. One request owns a native document transaction. FreeCAD
diagnostics stay separate from protocol output. No Python or C++ shape handles
cross the boundary; documents and objects have stable names. Display meshes are
requested separately from modeling and are never authoritative geometry.

An in-process CXX bridge is a later option if measured pipe overhead matters.
It is not a prerequisite for verifying the modeling foundation.

## Sources

- [Gradual replacement](https://martinfowler.com/bliki/StranglerFigApplication.html)
  and [replacement behind a common interface](https://martinfowler.com/bliki/BranchByAbstraction.html).
- [Firefox's Rust/C++ integration](https://firefox-source-docs.mozilla.org/writing-rust-code/index.html)
  and [CXX interoperation](https://cxx.rs/).
- [Pinned FreeCAD release](https://github.com/FreeCAD/FreeCAD/releases/tag/1.1.4).
- [FreeCAD document API](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/App/Document.h),
  [attachment](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/Part/App/AttachExtension.cpp),
  [fillet execution](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/PartDesign/App/FeatureFillet.cpp)
  and [shape naming](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/Part/App/TopoShape.h).
