# Interaction contract

The normal modeling interface follows Onshape's field names, choices, permitted selections and click semantics. Matching only the final geometry is insufficient. This applies to every exposed tool, including feature creation and editing, sketches, reference geometry and assemblies.

- Sketch starts directly in a blank Part Studio. Native Body ownership is automatic implementation detail.
- A reference field activates selection in the graphics area. Eligible geometry fills it immediately. The field shows removable entity chips. It does not require a native-object dropdown, a typed Edge/Face identifier or a separate Use selection action.
- The active field controls eligible geometry and cardinality. A Revolve axis is selected as an axis-defining entity, not any object from the document.
- Ordinary graphics-area clicks toggle and add selection. Blank-space click and Space clear it. Shift is not required to collect multiple edges.
- A visible option must execute the operation its name promises. Renaming a native property is not sufficient proof of equivalence.
- Backend diagnostic/property inspectors are not substitutes for the normal feature tasks. Their native names and internal objects must not leak into those tasks.

Official references: [selection](https://cad.onshape.com/help/Content/Home/selection.htm), [Revolve](https://cad.onshape.com/help/Content/PartStudio/revolve.htm), [Extrude](https://cad.onshape.com/help/Content/PartStudio/extrude.htm). The task catalog records corresponding primary references for the other exposed tools.

Every intended dimension, native geometry, history, persistence and previous failure seal remains required. UI parity and native correctness are both acceptance requirements. Unsupported options and unresolved mappings remain explicit work; they cannot be filled by lookalike controls or waived by successful mock tests.
