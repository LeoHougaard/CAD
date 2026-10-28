# External native assembly components

This implementation keeps the native Assembly container, JointGroup, App::Link,
GroundedJoint and Joint proxies. Source FCStd documents are explicit readonly
dependencies of the writable root. No geometry is copied into replacement
features, and no source document can receive a protocol write.

## UI workflow

1. Finish or Cancel the current feature, sketch or gesture. Create a native
   assembly through Assembly > New assembly if needed.
2. Choose Assembly > Insert external component. An unsaved root opens the
   existing Save as browser before starting an insertion draft. Save the root
   as an FCStd, then choose Insert external component again. Canceling Save as
   leaves the root and its history intact.
3. In the Feature task, use Source filepath and Load source, or Browse to choose
   an FCStd. The existing modal file browser accepts only native FCStd files.
   Browser Cancel returns to the task without starting a native edit.
4. Choose a Native source object. Supported shaped Part features, Bodies and
   native Links retain the file's native document UUID, object name, object ID
   and type. Labels are display text. The accepted file SHA256 is visible in
   the task. An unresolved or changed source cannot be inserted.
5. Set Instance name and the native placement fields. Positions use the root's
   current native units. Rotation uses quaternion Qx/Qy/Qz/Qw. Apply creates one
   native instance and one root history step. Cancel or Escape discards the
   unaccepted draft and preserves Redo. These tasks execute on Apply.
6. Use the existing Place component, Ground / unground and New joint tasks on
   the external instance. Select its displayed native face, edge or vertex for
   a connector. Native topology references and the native solver remain in
   charge. Failed Apply keeps the draft and values for correction or Cancel.
7. Save and Save as write only the root. Sources remain readonly. Save rejects
   a changed or missing accepted dependency and cannot overwrite an open source
   document. Undo/Redo retains the native links and accepted source identities.
   A later native Save as failure can change the live root path. The error and
   recovery snapshot show that path; the Editor follows it and remains dirty.
   The destination and its previous native backup bytes are restored when
   filesystem recovery succeeds. Correct the cause and retry Ctrl+S at the same
   destination, or use Save as. A filesystem restoration failure reports the
   retained recovery-copy location explicitly. This is recoverable native state,
   not an atomic native Save as transaction.
   If a native restoring setter also fails, the error lists the pending instance
   identities. Ordinary editing waits in recovery-only state. After correcting
   that native failure, retry Save at the same destination, or select an instance
   and use Update / relink external source with its original accepted source.
   Recovery restores the captured native targets and placements. Same-source
   Relink recovery preserves Undo and Redo without opening an authoring transaction;
   a deliberately different source revision remains a normal authoring edit.
8. For an explicit source update, select an external instance and choose
   Assembly > Update / relink external source. Browse the new FCStd revision
   path and Load source. Its native UUID and object ID must match the original;
   a same-label replacement is rejected. Apply updates every root instance of
   that exact source in one transaction. Instances and joint references retain
   their native identities. Cancel retains the prior revision.
9. A loaded source changed or removed on disk blocks ordinary edits and Save.
   The relink task remains available with the dirty root and history intact.
   Choose a separately saved revision path with the same native UUID/object ID,
   then Apply. No source is saved or overwritten by this action.
10. Close and reopen the root to continue editing. A fresh worker autoloads and
    validates each accepted dependency. The source path, UUID, object ID and
    accepted file revision must still match. Multiple roots can share sources;
    closing one root keeps the other root's references alive. Dependencies stay
    alive for native Undo/Redo until the last root closes.

## Guarded source loading

Source browsing and first preparation run before any root native transaction.
The Rust lease, poison and expected-revision checks run before opening a source.
Browsing returns descriptors and closes only documents it opened, restoring the
previous active document. An owned preview replacement can use an already loaded
source but cannot open or close another document. Source and all autoloaded
dependencies pass the existing native proxy restoration and restored-Hole input
repair, mandatory native Hole regeneration, sketch solving and native validation.
Readonly source bytes remain unchanged by these in-memory checks.

The additive operation `assembly_sources` takes `document`, `expected_revision`
and `path`. Its result has `objects`, each with `name`, `label`, `type` and a
`source` descriptor. A version-1 descriptor contains `path`, `sha256`,
`document_uid`, `object`, `object_id`, `type` and nested `dependencies` revisions.
It is shared by the Editor and public `--native-assembly` compiler.
`dependencies` is required, including an empty list for a source with no external
dependencies. The compiler checks its structure; native admission and continued
checks derive the complete transitive native graph and compare every file UUID
and accepted revision. Missing, shortened, altered and duplicate revision sets
cannot disable validation. Valid complete version-1 descriptors remain valid
without any count field. A redundant `dependency_count` is checked only if supplied
and is never an authority. Saved metadata passes the same graph comparison on Open.
Physical-file identity, including supported hardlink and case/path aliases, is
checked before readonly ownership is claimed. Browsing an alias cannot revoke an
already editable root's ordinary Save permission.
`assembly_insert` accepts `external_source` in place of local `source`.
`assembly_relink` names `assembly`, `object`, `external_source` and the explicit
solver policy. All writes use the existing transaction/preview boundary.

An external-root save failure returns `save_recovery`; Inspect adds a
`save_recovery` object with `previous_path`, live native `path`, `destination`,
`dirty`, `destination_restored`, `history_preserved`, `backup_path` and
`restore_errors`. Revision advances after entering native save/recovery.
The Editor synchronizes its path and marks the root dirty on that rejected
Save. A successful retry clears the recovery object. Existing pinned native
`.FCBak` and legacy `.FCStdN` sidecars participate in the rollback inventory.
When a native restoring setter prevents immediate graph restoration, snapshots
also report `recovery_only` and `save_recovery.pending_edges`, `edge_errors` and
`live_graph_restored`. Native target handles are kept inside the worker; only
their identities and diagnostics cross the boundary. Save and same-source Relink
remain reachable after the fault clears. All affected edges are captured before
any clear, restoration attempts every affected edge, and partial recovery keeps
the remaining exact identities available without discarding root content/history.
Pending recovery reports `available: false`. `recovery_available` is true only
when the unchanged native lease permits repair. The compiler permits only an
identity-matched relink of a captured pending edge in that explicit state;
unrelated unavailable or poisoned states remain blocked. Mesh, topology,
measurement, export and ordinary edits cannot trust the cached shape while
recovery is pending. Successful Save repair validates the native graph and
returns its full repaired snapshot, replacing the Editor's unresolved objects.
Process crashes, disk-full behavior, failed filesystem restoration and concurrent
writers to the same destination are not certified by the bounded fault checks.

## Open gates

In-place refresh of an already loaded path is unsupported. Pinned native
`openDocument` returns the existing document; closing it would invalidate live
root references and undo data. Use a new revision path. A source saved through
native Save as retains its native UUID and object IDs.

A missing or changed dependency during fresh Open fails before publishing a
healthy root. Restore the exact accepted source file at its recorded path,
including nested dependencies, then Open and use the relink task. Offline
recovery without restoring that accepted revision is still an open gate.

Independent native geometry update and assembly solving requires additional
native verification. The pinned AssemblyObject executes a solver during native
recompute when its SolveOnRecompute preference is true. This workflow does not
change global preferences. Native-only update cannot be claimed from skipping
the adapter's later solve call.

Physical desktop interaction, general nested assembly authoring, broad mechanism
and limit combinations, source-version conflict merging and complete Assembly
parity remain open. Headless Editor evidence does not certify physical GUI use.
The frozen workflow audit and its requirement statuses are unchanged.
