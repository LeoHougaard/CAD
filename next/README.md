# FreeCAD-based replacement

This is the replacement foundation. The earlier Rust/Brepkit application stays
available while this implementation is verified. It is not the modeling engine
for this version.

The first backend is pinned FreeCAD 1.1.4, including its Part, Sketcher and
PartDesign application modules and Open CASCADE. Rust owns the application-facing
interface. A small, local worker adapter initially calls FreeCAD's supported
Python API. It does not run FreeCAD's GUI or a network service. This interim
version is mixed-language; the eventual target remains Rust throughout.

This is a headless modeling foundation, not the replacement desktop editor.
The [migration order and completion gates](MIGRATION.md) describe the remaining
work. The earlier Windows application has not been replaced by this worker.

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
They do not establish broad feature parity, interactive performance or a usable
replacement editor. CI repeats the native checks on Windows and Linux x64.

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
