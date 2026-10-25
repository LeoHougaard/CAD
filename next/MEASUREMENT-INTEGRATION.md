# Measurement integration gate

The native measurement resolver and its finite BRep scope are unchanged by the
scheduling fix. Independent review confirmed the native math and found a shared
display prerequisite. The public baseline omits App::Link from displayed objects
and supplies identity world_from_shape for Links, which lack getGlobalPlacement.
Do not promote the Link measurement workflow until the reviewed assembly changes
and the combined gate below are integrated and pass.

## Required assembly integration hunks

These describe the reviewed assembly candidate's existing work. They are not
copied into the standalone measurement source or imported by its tests.

1. In `src/body_commands.rs`, include `App::Link` alongside the standalone
   `Part::` shape display filter. Preserve validity, finite bounds, visibility,
   Body ownership/consumption and duplicate filters. A Link must reach the actual
   model-thread mesh/topology requests and viewport topology references.
2. In `backend/assembly.py`, retain the native shipped UtilsAssembly traversal:
   `link_global_placement(obj)` returns
   `UtilsAssembly.getGlobalPlacement((obj, [""]), obj)`.
   `link_world_from_shape(obj)` returns that placement multiplied by
   `obj.Placement.inverse()`. This compensates the own placement already present
   in the native Shape; it does not approximate Link geometry.
3. In `backend/freecad_worker.py`, dispatch Link `world_from_shape` through
   `AssemblyAuthoring.link_world_from_shape(obj)` before the existing GeoFeature
   branch. Mesh and topology must both carry this transform. In object snapshots,
   add a Link `global_placement` branch using
   `AssemblyAuthoring.link_global_placement(obj)` when getGlobalPlacement is absent.
   Preserve the assembly module import/registration and all other native guards.
4. Merge the independent measurement operation and Editor state/read reply hunks
   with the assembly changes. Keep `measurements.resolve` unchanged. Its native
   getSubObject call already applies the object's and nested Link transforms
   using the parent's native global initial matrix. Do not apply the new display
   transform to measurement output or apply the Link placement a second time.

Root's concurrent attachment documentation in PROTOCOL.md is intentional and
must be preserved. Neither this fix nor its replay writes the shared checkout.

## Run the combined gate

Build the test binary and host from the same merged source. Its runtime JSON must
point `script` to that merged source's backend/freecad_worker.py, with the pinned
executable/args left opaque. With a Cargo build, for example:

```powershell
python MERGED_NEXT/tests/measurement_scene_workflows.py --host MERGED_HOST --runtime MERGED_RUNTIME --editor-tests MERGED_TEST_BINARY --output NEW_PRIVATE_OUTPUT
```

For Cargo test users, first run the script without `--editor-tests` to generate
the native fixture and check public-host frames. Then set:

```text
CAD_MEASUREMENT_RUNTIME=MERGED_RUNTIME
CAD_MEASUREMENT_OUTPUT=NEW_PRIVATE_OUTPUT/editor
CAD_MEASUREMENT_SCENE_FIXTURE=NEW_PRIVATE_OUTPUT/fixture/scene.FCStd
CAD_MEASUREMENT_SCENE_CONTROLS=NEW_PRIVATE_OUTPUT/fixture/controls.json
```

Run the exact ignored test
`editor::measurements::tests::scene_tests::native_measure_link_scene_selection_and_witness_frames`.
Use jobs1/test-threads1 and a fresh output directory. The gate asserts that the
compiled CARGO_MANIFEST_DIR worker path matches runtime.script, so an old worker
cannot accidentally certify a new test binary.

The fixture is generated independently through the configured native launcher.
It contains translated/rotated nested Parts, a placed Box, LinkTransform=true and
false instances, and a Link targeting a Link. The true-Link vertex also has an
explicit analytic native placement-composition control.

The public-host check compares exact native measurements with topology and mesh
world transforms, native global-placement metadata and vertex-pair witnesses.
The Editor test asserts actual display_objects inclusion and native viewport
identities, then sends real headless egui clicks at the independent native world
points. It checks picked object/subelement, measurement coordinates and a
Shift-selected pair's distance/witnesses. Reads must preserve model state and
dirty state. This catches omitted Link scenes and incorrect or double transforms.

`--measure-only` is available to verify standalone native measurement without the
assembly display patch. It reports `promotable_combined_gate:false` and is never
a substitute for the combined gate. A host-only pass is also insufficient to
certify Editor display inclusion or picking. The combined script returns failure
when a requested frame or Editor assertion fails and retains its observations.

The standalone measurement candidate is expected to fail this P1 display gate.
No assembly source was imported to make that limitation disappear. Root must
retain a passing combined gate receipt before promoting the workflow. This is
headless native acceptance only; real desktop picking/layout/clipboard, Linux,
scaled/reflected and arbitrary external-link documents remain separate gates.
