# Native Assembly proxy admission

The worker returns `assembly_proxy_restore` when the shipped JointObject module
cannot load for a known native joint, or a known Joint/GroundedJoint object lacks
an instance of its shipped native proxy class. Python subclasses are accepted.
The check reads the pinned serialized property names, types and groups. It does
not infer joint types from object names, assign proxies, rebuild native objects,
or restrict connector values to local references. Unrelated Python objects and
ordinary Parts, Links and empty assemblies do not load JointObject.

Checks run before recompute, model validation and snapshot publication. Save,
export, mesh and topology retain their existing validation and revision/lease
guards. Inspect rejects a missing proxy through the snapshot check. Close stays
available with the current revision for explicit recovery.
Writes to an already damaged open document fail before entering a native
transaction. Stale revision and lease checks still run first. Close and native
preview/sketch cancellation keep their existing cleanup admission.

Open failure closes only documents newly created by that Open and restores the
previous active document. It also handles a native exception after document
creation. Existing documents, their revisions, native paths, model contents and
saved bytes remain intact. No invalid replacement state is published. The
Editor keeps its prior dirty flag and save path on the rejected reply.

After a restore error, fix the native runtime and explicitly reopen the original
FCStd, or restart the worker and reopen it. An already damaged FCStd with missing
serialized proxies stays rejected. The worker does not guess how to repair it.

Run the focused public check with a cached headless Rust host, the pinned native
runtime JSON pointing to this backend, and a new output directory:

```powershell
python -B next/tests/proxy_restore_workflows.py --host HOST --runtime RUNTIME --output NEW_OUTPUT
```

It generates real native fixtures, verifies the 13 declared types against the
native enumeration, tests Python subclasses and native Python feature
containers, and checks missing joint, ground, both and partial restoration.
The exact original meta-path import blocker is retained in
`fixtures/proxy_restore_original_fault_worker.py`. The expected baseline failure
can use fixtures from the successful fixed run:

```powershell
python -B next/tests/proxy_restore_workflows.py --host HOST --runtime SEALED_LAZY_RUNTIME --fixtures NEW_OUTPUT/fixtures --output NEW_BASELINE_OUTPUT --baseline
```

Expected baseline exit is 1 at the original requirement assertion after Open,
Save, clean shutdown and a healthy native process confirming persisted proxy
loss. The fixed path expects an explicit rejection and no published document.
The fault injection files are test-only; the production launcher imports neither.

Other focused checks cover unsaved existing Part content, native path/history,
source-file hashes, failed first authoring, native expressions and references,
Undo/Redo, fresh-process restore, denied Save/export writes, read admission,
stale revision guards, explicit close/reopen and partial Open exceptions. These
are headless native checks. They do not establish GUI behavior, broad Assembly
workflow parity or general performance acceptance.
