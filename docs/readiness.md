# Readiness checks

This iteration targets offline part modeling, linked assemblies, and explicit in-context updates. It also preserves Onshape-style selection and supported shortcuts while keeping the viewport large and the application idle when nothing changes.

The reference baseline is commit `6ff8ac2`. Its Windows and Linux CI passed. Later fixes and new features require their own verification before release.

## Acceptance checks

- Ten distinct completed modeling projects with stated dimensions, source workflows, upstream edits, save/reopen checks, closed solids, analytic volume checks, and independent STEP checks.
- Assembly instances share source geometry. Mates preserve their intended degrees of freedom, honor limits, and report conflicting constraints. Source edits and missing references cannot silently move the wrong component.
- In-context references remain frozen until an explicit update. Updates regenerate the referenced part and preserve the old document on failure.
- Native Pixel Linux checks cover sketch creation, dimensions, selection, region extrusion, finishing, settings, search, and assembly editing. Automated construction of a model does not count as native interaction testing.
- Windows and Linux tests, strict Clippy, portable packaging, and CI pass for the published commit.
- Idle CPU, memory, regeneration time, and cached pose-update time are measured on named hardware. No claim of being faster than Onshape follows from local execution alone.

## Failure ledger

| Category | Observed failure | Change | Verification |
| --- | --- | --- | --- |
| Native keyboard events | Shift+1 arrived as punctuation | Normalize shifted physical number keys while preserving logical letter shortcuts | Unit tests and native Pixel Front/Top/Isometric checks passed |
| Curved topology | Lens edges could not be selected or referenced | Keep analytic source-curve provenance through region extrusion | Native lens-rim fillet and five reference regression tests passed |
| Selection display | Count text disappeared against selected faces | Dark count badge, muted selection and hover colors | Native Pixel screenshots checked |
| View fitting | Selected-region preview cropped the rest of the sketch | Fit the preview and the full input sketch together | Projection regression and native Pixel check passed |
| Region classification | Horizontal ray through a circle intersection misclassified a cell | Consistent half-open endpoint rule | Dense independent region-classification regressions passed |
| Resource use | Whole-program LTO linked every optimized test harness | Disable LTO for tests only, retain shipping release profile | Baseline CI passed both operating systems |
| Circular rim picking | Oblique rims lost to the face under the pointer | Match Part Studio's screen-space depth tolerance | Regression covers displaced rim clicks; final native verification recorded separately |
| In-context feature controls | Bound extents still showed inactive ordinary depth inputs | Show reference status and a geometry-preserving detach action | Plane and extent detach regression; final native verification recorded separately |
| Export file replacement | Extensionless names checked the wrong destination | Normalize the target path before displaying replacement controls | Part Studio and Assembly file-dialog regressions |
| Context ownership | Updating an older context overwrote a newer binding or restored a detached reference | Reject an update unless each feature still belongs to that saved binding | Regression checks both cases and an unchanged assembly on rejection |

Pending entries are updated from results, not from a successful compile.

## Selection baseline on Pixel

The selection baseline was rebuilt from a matching source fingerprint on Windows and Pixel. The initial full Pixel release run passed 188 tests; the final selection-focused run passed 59. These runs precede settings and assemblies and do not validate those later additions. [Native evidence](../output/selection-pixel-verification.json) records the hashes and individual checks.

With the 29-feature housing open, the process used 122,984 KiB RSS and no swap. It accumulated zero CPU seconds during a 10.002-second idle sample. The graphics server is excluded. Physical SpaceMouse feel still needs the user's feedback because automated input cannot reproduce moving the device.

## Interaction regression pass

The starting point is `f341bb3`, with 254 passing checks on Windows and native Pixel Linux and passing CI on both systems. User testing still exposed basic workflow failures. This pass measures improvements against those workflows, including related cases, rather than treating the existing test count as readiness.

| Shared area | Regression set | Acceptance |
| --- | --- | --- |
| Sketch solving | Redundant dimensions, free rectangles, rigid joined line chains, multiple selected entities, constrained circle/arc dragging | Minimum necessary motion, hard constraints retained, no permanent drag constraints, unrelated geometry unchanged, save/reopen agrees with the display |
| Sketch curves | Center and three-point arcs, CW/CCW/major spans, tangency, construction, trimming lines/circles/arcs/rectangles | Exact curves, stable editable dimensions, transactional trim with valid surviving references, closed profiles extrude to analytical volume |
| Feature inputs | Boolean modes and keep-tools; separate scopes; region/face profiles; sketch, origin and solid axes | Only the active input changes; preview and saved feature agree; unsupported references preserve the old model |
| Camera and editing | Open/edit/cancel old and new features in landscape/portrait | Editor overlays the viewport; model position/scale stay fixed; history, affected result and input geometry visibly identify the edit |
| Reference planes | Zoom/fit/orbit all origin planes from both sides | Fixed 100 by 100 mm model-space footprint, same hit bounds, readable projected labels |
| Geometry | Planar and curved Boolean examples, plate with through-hole, one/multiple edge blends, unrelated coplanar parts | Closed exact B-reps, analytical measures and independent STEP imports; no silent mesh fallback or reference reassignment |

Each final native run must use the same frozen source fingerprint as the shipping binaries. The Pixel owns GUI verification; Windows interaction is limited to preserving the old session and handing over the new build. Remaining unsupported sketch tools and kernel cases must be listed explicitly after verification.

### Shared edit lifecycle

An editor owns a snapshot of its surrounding workspace. All features use the same
preview, commit and cancel path. Preview applies the same feature replacement as
commit to a cloned document, then limits evaluation to the edited feature. It
never removes later feature definitions or mutates the saved rollback bar.
Ending the session restores camera, visibility, display flags and selection
filter, and returns the full scene while regeneration runs. Cancel restores the
previous selection; commit selects the edited feature. Committing an edit keeps
an existing manual rollback bar; inserting at that bar advances it one feature.
Even accepting an unchanged feature rebuilds the surrounding history.

Feature selection has one active manifest input. Its values and missing references
belong to the feature, while viewport selection is only that input's current
view. Switching inputs restores that input's values rather than copying the same
selection into every compatible field. Extrude and Revolve share profile, axis
and operation-scope resolution. Geometry operations preserve part identities and
source provenance so later tools refer to the same analytical geometry.
