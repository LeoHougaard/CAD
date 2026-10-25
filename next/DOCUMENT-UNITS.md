# Native document units

This bundle exposes FreeCAD 1.1.4 document settings and quantity presentation in
the existing Rust editing workflows. FreeCAD still owns units, quantities,
geometry, expressions, solving and history. This is UI and command access, not a
kernel or quantity port to Rust.

## Use the editor

1. Finish or cancel the active feature/sketch task and wait for Ready.
2. Choose **Units > Document units...**. The dialog lists the native document's
   ten choices and selects its current setting.
3. Choose a system and **Apply units**. **Cancel units** or Escape closes the
   unsubmitted dialog without changing the document. Apply is a direct setting
   operation. While it runs, cancellation is disabled.
4. Save to persist the choice in FCStd. Opening another document reads that
   document's own choice. New documents retain native defaults.

Changing units marks the editor dirty and publishes a revision only when the
native selection changes. Applying the current selection preserves revision,
dirty state and history. UnitSystem is not native-transaction undoable. Undo and
Redo continue to operate on native model history and leave this setting in
place. Changing units preserves existing Redo. No history entry is fabricated.

Physical fields use native preferred units for the quantity currently displayed.
A bare number uses that field's displayed native unit. Explicit suffixes are
parsed and dimension-checked by FreeCAD. For example, editing a field showing
`1 in` to `2` produces the same native length as `2 in`. MKS does not force every
length field to metres: native auto preference displays 25.4 mm as 25.40 mm.
Display text is rounded by FreeCAD; edit seeds are separate full-precision native
conversions. Merely displaying a value never writes it back or rounds geometry.
Native expression fields keep their source and existing native semantics.

## Current input routes

| Route | Native context and behavior |
| --- | --- |
| Features / Properties, creation and existing objects | Scalar Quantity metadata, including length, distance, angle, area, volume, speed and other native quantity dimensions; cached schema defaults are reformatted read-only for the current document. |
| MultiTransform child property forms | Same property widgets and document-aware native schema templates. |
| Position / VectorDistance / Placement properties | Native length context per component; rotation quaternions stay unitless. Native generic Vector, Color, numeric lists and scalar ratios retain their unitless contracts. |
| Part Fillet/Chamfer edge-radius lists | Native length metadata for existing radii and newly added/selected edge defaults. Edge indices remain integers. |
| Sketch constraint panel, dimension dialog and viewport labels | Native distance, horizontal/vertical distance, radius, diameter and angle text; native query for new-dimension seeds. Solver values retain mm/radian protocol meanings. Weight and refraction ratios stay unitless. |
| Sketch fillet/chamfer, extension and spline-pole tasks | Native radius, linear/angular increment and X/Y coordinate context. Curve parameters, knots, degrees, weights and indices stay unitless. Split/trim/pointer drawing still submit the original native-coordinate numeric values. |
| New sketch > Attachment options | Native offset lengths and native axis-angle rotation. The rotation axis stays unitless; existing numeric quaternion requests remain supported. |
| Assembly insertion/placement and joint fields | Native placement positions and native joint quantity metadata. Quaternions, flags and ratios remain unitless. |
| Built-in declarative packages | Registry metadata for every current physical number parameter declares `unit: "mm"`. The editor resolves text through native quantities before the existing numeric package compiler. Numeric agent parameters retain their previous base-unit meanings. |
| Measure / Inspect | Existing native schemaTranslate presentation is retained. |

Custom packages can add an optional native `unit` to a number parameter. Packages
without that declaration retain their original numeric contract. Their intended
physical dimension cannot be inferred safely from an arbitrary template. They
are an explicit remaining unit-presentation gap until annotated.

## Shared agent boundary

`document_settings` is a standalone operation, never an ordinary transaction
edit. It requires `document`, `expected_revision` and an integer `unit_system`
from `state.units.choices`. Existing worker-wide/document leases and unavailable
document guards apply before even a same-value request. It sets UnitSystem,
prepares and serializes the full state, checks unchanged history, then publishes
the revision. A failure explicitly restores the setting. Failed restoration
poisons the document and requires Close/reopen. Transaction abort is not used
as a restoration mechanism.

`cad-next-packages --document-units -` compiles `{state,unit_system}` into that
guarded request. Send it through the existing host NDJSON boundary.

`quantities` is a revision-checked, read-only native operation. Each item contains
`unit` plus either a base-unit numeric `value`, or `text` and an optional native
`default_unit`. Results contain `base_value`, `display`, full-precision `input`,
`default_unit`, native unit `signature`, and native internal quantity `value`.
`base_value` follows the requested native unit, including radian sketch values.
The internal quantity value follows FreeCAD's quantity storage, including degrees.

Property metadata adds `quantity` alongside the unchanged numeric `properties`.
Scalar form input can carry `{quantity_input,default_unit}`. Physical vector,
placement and fillet-radius components accept the same additive text wrapper.
UI datum/via-point edits carry `input_default_unit`. Native dimensional checks
resolve those inputs; legacy numeric and explicit native string requests retain
their previous meanings. Expressions are never converted into these wrappers.

The feature-schema cache is built before requests, as before. Document-aware
schema formatting reconstructs native Quantity objects from cached signatures
and values. It creates no scratch document or native feature during editing.

## Evidence and limits

The private implementation receipt records pinned-native behavior, native host
admission/fault checks, six actual Editor/headless-egui workflows, independent
FCStd readback, source/build manifests and exact frozen-archive patch replay.
The pinned native QuantitySpinBox source uses the current quantity's preferred
unit as its default input context. The older native InputField has different
dimensionless-input handling; this UI uses the QuantitySpinBox preferred-unit
rule for bare numbers. It does not clone Qt's text normalization of every mixed
implicit-unit expression. Explicit native quantity text and native expression
mode remain available.

Desktop GUI, Linux, exhaustive imported/custom property schemas and every native
feature/task combination remain open acceptance gates. The historical coverage
ledger is unchanged. This bundle does not establish all-app unit parity, full
Part Studio/Assembly parity, performance acceptance or an all-Rust kernel.
