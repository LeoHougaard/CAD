# Native feature suppression

This development candidate adds Suppress/Unsuppress to a native Part Design feature's tree context menu. It is not certified full suppression, downstream repair or desktop parity.

1. Open a native Part Studio and finish the current sketch, feature task or file operation.
2. Right-click the feature in the Features tree and choose Suppress. The native feature remains in the Body history; downstream features recompute through FreeCAD.
3. Use document Undo/Redo to undo or repeat the action. One tree action is one native transaction.
4. Save the `.FCStd` document. Reopen it, right-click the feature and choose Unsuppress. Current parameters and attachments drive native recomputation.
5. If native recomputation rejects the operation, the previous valid model is restored. Correct the native inputs or dependencies and retry. This candidate does not guess a replacement edge or automatically suppress dependents.

The same change is available through native Properties and the shared `--native-history` compiler:

```json
{"state":{"...":"current native snapshot"},"request":{"op":"feature_suppression","object":"Pad","suppressed":true}}
```

Submit the compiled edits through a normal revision-guarded transaction. Every object's `suppression` metadata reports its current state, `can_toggle`, and an unavailable reason. Formula-controlled suppression must be edited through its native expression in Properties. Missing original state in a legacy file disables the tree action; the program cannot recover information that file never saved.

The temporary native core records local placement and the native shape/element map on the same feature while suppressed. On unsuppression it restores the native map as a recompute seed, removes the temporary properties, touches the feature and regenerates through FreeCAD before accepting the result. This preserves references that otherwise remain missing after saving a suppressed source. The properties are hidden/read-only to public edits; they participate in native transactions and serialization. They are not a new geometry kernel or a substitute for current parameters.

Headless actual Editor tests cover the context menu, native history, rejected operations and correction, disabled formula/legacy actions, and fresh reopen. Independent native controls cover placement formulas, attachment updates, profile/dress-up features and projected followers. Physical desktop acceptance, cross-platform checks, resource gates and independent review remain open. Some stock native topology changes still invalidate fillet links; their failed native control is retained, and no automatic repair is claimed.
