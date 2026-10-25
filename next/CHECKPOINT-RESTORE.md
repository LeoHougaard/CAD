# Native checkpoint restore boundary

The pinned FreeCAD 1.1.4 `PersistencePy::restoreContent` binding acquires a
`Py_buffer` with `PyBUF_SIMPLE` and omits `PyBuffer_Release` on success and error.
Every old checkpoint restore therefore retains its freshly decoded bytearray.
The native review reproduction and reference/lifetime evidence confirm the leak.
It is separate from the intentionally retained Rust action-history checkpoints.

`PyBuffer_Release` includes both reference ownership and the exporter's release
callback. The pinned CPython bytearray exporter increments/decrements
`ob_exports`, whereas exact immutable bytes have no release callback. Merely
decrementing a bytearray reference would leave export bookkeeping unbalanced.
The production fix does not decrement references or fabricate/release a lost
`Py_buffer`.

## Native route

`checkpoint_native.restore_bytes` accepts exact immutable `bytes` only and uses
`ctypes.PyDLL` for a synchronous call while retaining the GIL. ctypes keeps the
bytes alive for the complete call. The pinned CPython `c_char_p` conversion uses
`PyBytes_AsString` with ordinary owned argument references, not `GetBuffer`.
Writable buffers, memoryviews, subclasses and forwarding proxies are rejected.
The native object must be a live SketchObject with its actual bound native
method and a matching document lookup. No native object pointer is cached.

`backend/native/checkpoint_restore.cpp` looks up that object synchronously in
the same authoritative native document and calls the exact exported
`Base::Persistence::restoreFromStream(std::istream&)` used by the Python binding.
The primary-base chain from DocumentObject to Persistence is verified in the
pinned official headers. The adapter uses the exact Windows x64/release MSVC
14.44 ABI, `/MD`, `/EHsc` and `_ITERATOR_DEBUG_LEVEL=0`. Its owned string stream
releases its archive copy on both success and exceptions. No `Py_buffer` is
acquired. No Python reference-count adjustment is performed.

FreeCAD still owns ZIP/XML reading, Restore and restoreFinished. The adapter
does not parse or rewrite checkpoint XML, assign IDs, reconstruct geometry or
substitute command replay. All C++ exceptions remain inside the `noexcept` C
boundary. Python raises the original binding's `OSError("Unable to restore
content")` on native restoration failure; existing recovery and poisoning rules
remain in force. The captured native geometry, constraints, expressions, Shape/
ElementMap data and references are restored together.

## Pins and availability

Before loading, the boundary verifies Windows x64 CPython 3.11.14, hashes of the
actually loaded FreeCADBase, FreeCADApp and python311 modules, their expected
runtime paths, and the adapter hash. Other runtimes fail closed. Public
`capabilities.sketch_checkpoints` reports `available` and `reason`; Sketch Begin
rejects `native_checkpoint_unavailable` before acquiring a session or mutating
the document when the adapter/runtime is unavailable.

Linux uses a separate [native checkpoint adapter](CHECKPOINT-LINUX.md), built by
normal runtime setup and pinned to the actual ELF runtime. Its ARM64 headless
correctness and bounded ownership gate has been exercised. Linux x64 execution,
root integration and independent review remain open. The reviewed Windows
implementation and its pins are unchanged.

Build the small adapter with `tools/build_checkpoint_restore.py`, passing the
runtime configuration, MSVC compiler/vcvars paths and a fresh private output
directory. It performs one serial BelowNormal adapter compile and never rebuilds
FreeCAD, CPython or Cargo dependencies. It emits the adapter DLL and pin manifest
beside the source. The separate Mirror C++ adapter is unchanged.

## Verification boundary

The private handover records native success/error reference counts, writable
buffer resize and weak-reference collection checks, exact native state controls,
actual worker recovery and public API cases, and a repeated large-checkpoint
memory probe. Resource sampling requires a root-coordinated quiescent window.
The memory probe uses one fixed checkpoint and one document with no journal or
history growth. Its direct-native arm calls the same pinned native adapter with
already decoded bytes; it is a call-overhead/retention control, not an independent
ABI implementation. Exact geometry/history controls also use direct native
transaction aborts separately.

This is an interim native adapter, not a completed Rust checkpoint component or
full Mirror parity. Physical desktop, cross-platform, root integration and full
resource gates remain separate acceptance requirements.

Sources: FreeCAD 1.1.4 `src/Base/PersistencePyImp.cpp`, `Persistence.cpp`,
`Persistence.h`, the App primary-base headers, and CPython 3.11.14
`Objects/bytesobject.c`, `bytearrayobject.c`, `abstract.c`, and
`Modules/_ctypes/cfield.c`. Their downloaded hashes and the actual binary export
are retained in the private evidence manifest.
