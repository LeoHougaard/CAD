# Linux native checkpoint adapter

The Linux path uses the same native `Base::Persistence::restoreFromStream`
engine as the reviewed Windows boundary. It never calls the leaking Python
`restoreContent` binding, acquires a Py_buffer, changes reference counters,
rewrites XML/IDs or recreates geometry through action replay. This is a temporary
C++ application adapter, not a Rust modeling-kernel or checkpoint-component port.

`checkpoint_native.py` dispatches Linux to a separate `checkpoint_linux.py` and
adapter. The reviewed Windows C++ source, binary and pin manifest stay unchanged.
The common immutable-bytes, live SketchObject/document, GIL, exception and
transaction contracts remain in force. Constrained Mirror on Linux remains a
separate unavailable mode; this adapter restores general sketch editing/recovery.

## Setup and build

The normal Linux `tools/setup_runtime.py` command now fetches/verifies the pinned
runtime and invokes its bundled Python to build the tiny adapter. A C++ compiler,
`nm` and `c++filt` must be installed. FreeCAD and its dependencies are not rebuilt.
The setup command still writes only the launcher JSON to stdout; adapter build
diagnostics go to stderr. Receipts use fresh private `.tools/checkpoint-adapter-*`
directories, preserving prior failures.

For an existing configured runtime, invoke `tools/build_checkpoint_linux.py`
through that runtime's `executable` and complete `args`, with `--output` naming a
fresh receipt directory. The command builds only one small shared library and
emits architecture-specific `.so` and JSON files in `backend/native/`.
These artifacts are tied to the exact runtime used to build them.

The builder reads actual defined ELF exports and checks complete demangled
declarations for the application singleton, document/object lookups and native
restore method. The generated header records those exact symbols. The pinned
headers establish the primary-base chain to Persistence. No object offsets or
virtual table slots are guessed. Standard-library stream calls use the same
GNU/Itanium ABI and the already loaded pinned libstdc++.

The loader checks ELF64/little-endian architecture, exact FreeCAD/CPython versions,
runtime and adapter hashes, the loaded FreeCAD extension and Python executable,
and `dladdr` provenance for each already loaded library. Both a shared libpython
and a statically linked Python executable are handled by pinning the actual
provider of `Py_GetVersion`. Missing artifacts, wrong runtimes or unsupported
architectures report a capability reason and reject Sketch Begin before mutation.
No unsafe or leaking fallback is used.

## CI and acceptance gates

Linux x64 must be executed in root CI before promotion. ARM64 results cannot
substitute for that gate. A CI job should install the ordinary C++ tools, then:

```sh
python3 next/tools/setup_runtime.py --output "$RUNNER_TEMP/runtime.json"
cargo build --locked --manifest-path next/Cargo.toml
python3 next/tools/check_checkpoint_linux.py \
  --runtime "$RUNNER_TEMP/runtime.json" --host next/target/debug/cad-next \
  --expected-arch x86_64 --output "$RUNNER_TEMP/checkpoint-linux-x64"
```

The same gate runs on aarch64 with `--expected-arch aarch64`. The native controls
compare exact IDs, geometry/constraint/expression XML, DoF, ElementMap, external
references and fresh reopen against direct native transaction controls. Forty
restores share one immutable native archive larger than 128 KiB after eight
warmups, with no native history growth, balanced references and exception/writable
buffer checks. The same process records RSS every eight restores, with a declared
8 MiB retention limit. This bounded check is not a coordinated resource comparison.
The first restore changes one pending native Session; counts are checked after
that expected transaction becomes nonempty. The
public Rust/native cases cover replacement, failure recovery, Cancel, action and
document Undo/Redo, consumed Finish failure, plain geometry, Carbon Copy live
dependencies and reopen. Missing adapters, wrong architecture, and tampered
runtime/adapter pins must fail before mutation with a public capability reason.

Headless correctness/ownership tests are not physical UI certification or broad
resource acceptance. Tiny adapter compilation and native restores still use CPU
and memory; the owned stream copy has bounded call lifetime. Process-resource
comparisons require separate coordinated measurements. Native core, Linux x64 CI,
root integration and independent review remain explicit promotion gates.
