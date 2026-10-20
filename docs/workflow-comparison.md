# Parametric workflow comparison

The product direction remains an offline, lightweight Rust Part Studio with
Onshape-style interaction and installed feature scripts. FreeCAD and SolidCraft
are references for modeling contracts, not interfaces to copy wholesale.

## Sources and scope

This review used FreeCAD revision
[`33adf618`](https://github.com/FreeCAD/FreeCAD/tree/33adf618dcebfb167795a219175e7dbb18f7a353)
and SolidCraft revision
[`885d2aaf`](https://github.com/tangency-labs/solidcraft/tree/885d2aafbb0194ab7a2d96c13604a0711c64936a).
The comparison includes their source and documented behavior, our shipping
baseline `67bd212`, automated modeling chains and native Pixel interaction.
It does not establish general feature parity or a speed advantage over either
application. Shared-browser snapshots timed out twice, so no hands-on browser
comparison is claimed.

FreeCAD's [generated-element mapping](https://github.com/FreeCAD/FreeCAD/blob/33adf618dcebfb167795a219175e7dbb18f7a353/src/Mod/Part/App/TopoShapeExpansion.cpp#L3634)
and SolidCraft's [composed lineage](https://github.com/tangency-labs/solidcraft/blob/885d2aafbb0194ab7a2d96c13604a0711c64936a/crates/doc/src/lineage.rs)
show why a valid intermediate solid is insufficient: later features need its
surviving and generated geometry to retain meaningful identities. Our references
must remain exact or become explicitly unresolved; proximity alone cannot repair
a split or ambiguous face.

## Shared failures addressed

| Contract | Verified baseline failure | Implementation |
| --- | --- | --- |
| Continue modeling from generated geometry | A valid mitred sweep had eight untracked planar sides and twenty untracked edges. The circular loft's three edges were untracked. | Generated roles derive from source section, curve and operation identities. Exact support and surviving boundary witnesses establish ownership. Planar frames follow copied occurrences. |
| Preserve state across Undo | Choosing Front then Undo changed sketch parameters to Top while the camera and plane highlight remained Front. | One editor snapshot includes explicit plane-choice state; restoring it updates support selection and automatic camera ownership. Geometry-only Undo retains manual navigation. |
| Treat timeline gestures as transactions | Context rollback recorded Undo, but slider and End did not. | All paths share one transaction; a slider gesture records one history action and a no-op does not discard Redo. |
| Give installed plugins the same input behavior | Conditional visibility depended on built-in feature IDs; optional selections could prevent Apply. | Declarative conditions and optional inputs share validation, routing and agent schemas. Existing package bytes and pins stay unchanged. |
| Respect bounded material when blending | A 100 mm fillet on a 10 by 8 by 20 mm solid succeeded and expanded its bounds and volume. | Plane/plane fillet and chamfer contacts must share a continuous positive material interval on supported finite faces. Holes and concave gaps are checked before assembly. |

The finite-contact check partitions at exact linear and complete circular
boundary intersections. It allows endpoint clipping at tapered supports and
junctions. Mixed arc, ellipse and NURBS boundaries retain their previous trimming
path and are not certified by this new check.

## Verification chains

- Loft or mitred sweep, select a generated side, create a plane and attached
  sketch, cut, change an upstream dimension, serialize and regenerate.
- Pattern or mirror a generated solid, attach to the selected occurrence, and
  resize the source without borrowing another occurrence's references.
- Create multiple loft operations in one external script, deliberately fail
  its second operation, repair it, and recover the same outputs and identities.
- Install an external conditional feature, change modes, leave optional inputs
  empty, reject incomplete edits atomically, change upstream dimensions, and
  reopen with the same pinned packages.
- Undo/Redo plane selection and rollback through the menu, shortcut and slider
  paths while retaining downstream history and deliberate camera navigation.

Four new STEP exports have independent analytic mass and bounds expectations.
The existing verifier imports them using development-only Gmsh/Open CASCADE.
The Rust application has no C++ kernel dependency.

## Remaining differences

FreeCAD [continues independent branches after a failure](https://github.com/FreeCAD/FreeCAD/blob/33adf618dcebfb167795a219175e7dbb18f7a353/src/App/Document.cpp#L2952).
Our evaluator still blocks subsequent features conservatively. Safely changing
this requires tracking dependencies and failed writers: simply continuing would
let a dependent feature consume the preserved pre-failure solid.

Closed periodic sweep patch naming, arbitrary split/merge rebinding,
tangent-chain propagation, curved-path sweeps, guided lofts, and the fused-loft
through-bore failure remain limits. The earlier [ten-project results](end-user-projects.md)
describe the supported workflows and their adaptations.

SolidCraft's [static feature enum and commands](https://github.com/tangency-labs/solidcraft/blob/885d2aafbb0194ab7a2d96c13604a0711c64936a/crates/doc/src/document.rs#L525)
are not a replacement for our extension contract. Installed scripts continue
to compose Rust operations without a host rebuild. New native operations,
codecs and editor types still require host support; no dynamically loadable
Rust algorithm-provider ABI is claimed.
