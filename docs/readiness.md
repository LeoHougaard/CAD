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
Ending the session restores visibility, display flags and selection filter,
and returns the full scene while regeneration runs. Automatic sketch framing
returns to the previous view unless the user deliberately navigated during the edit. Cancel restores the
previous selection; commit selects the edited feature. Committing an edit keeps
an existing manual rollback bar; inserting at that bar advances it one feature.
Even accepting an unchanged feature rebuilds the surrounding history.
Cached preview sketches enter the controls only when no newer draft is scheduled
or building. Until then the draft retains its locally solved geometry and
selection, including identities remapped by a structural edit.

Feature selection has one active manifest input. Its values and missing references
belong to the feature, while viewport selection is only that input's current
view. Switching inputs restores that input's values rather than copying the same
selection into every compatible field. Extrude and Revolve share profile, axis
and operation-scope resolution. Geometry operations preserve part identities and
source provenance so later tools refer to the same analytical geometry.

The follow-up adds reusable construction-plane references, exact per-edge
construction edits and shared numeric expressions. Native input regression also
covers frame-ordered press/move/release handling shared by Part Studio, Assembly
and the view cube, retaining movement on the release frame and excluding pointer
travel outside the held-button interval.

Logical curve identity is separate from a sketch entity's current representation. Per-edge construction can turn a rectangle into connected lines without renaming surviving region, edge or support references. Genuine splits receive new identities. Part provenance also limits reference candidates to features that actually contributed to that part; adding an unrelated later feature must not rename an earlier face.

## Profile and sketch interaction follow-up

The baseline is `58f2f91`: 361 checks on Windows and Pixel Linux, 58 independently
imported STEP files, and passing CI on both systems. Native retesting reproduces
a face-seeded Extrude whose Clear action leaves face-only picking active. The
replacement uses one profile input for planar faces and sketch regions.

The sketch audit also found a purple input overlay behind all active sketch
curves, selected geometry overwritten by coincident later geometry, and
construction dash patterns restarting at every tessellation segment. The active
sketch owns its geometry rendering. Selection is painted last, selected points
have rings, and construction dashes span whole curves. Native verification uses
the same constrained-chain fixture before and after these changes. Tangent arcs
are checked through persistent tangency constraints, dimension edits, save/reopen
and an independently checked solid profile.

Shared trimmed endpoints anchor analytic intersections directly, avoiding the
quadratic cancellation at tangency. Saved references accept a redundant contact
label only when both original curve endpoints identify the same junction; genuine
splits, crossing branches and part owners remain distinct.

## Onshape workflow and solid-boundary audit

Baseline: `eaf751b`, 390 passing checks and Windows/Linux CI. Subsequent user
testing still found failures. That baseline does not establish Onshape parity.

The reference is observable Onshape behavior, using its
[selection rules](https://cad.onshape.com/help/Content/Home/selection.htm),
[feature dialogs](https://cad.onshape.com/help/Content/Home/dialogs.htm),
[sketch constraints](https://cad.onshape.com/help/Content/Sketch/working_with_constraints.htm),
[Extrude](https://cad.onshape.com/help/Content/PartStudio/extrude.htm),
[Revolve](https://cad.onshape.com/help/Content/PartStudio/revolve.htm),
[Boolean](https://cad.onshape.com/help/Content/PartStudio/boolean.htm) and
[Fillet](https://cad.onshape.com/help/Content/PartStudio/fillet.htm).
Direct comparison in the supplied account is tracked separately from reading
these references; unavailable browser automation is not a completed comparison.

The design contract is independent of the example set. A solid has shared
vertices and edges, connected closed face wires, consistent shell orientation,
and exact supported curve/surface representations. Boolean construction must
remove internal boundaries and retain the ownership of surviving boundaries.
Canonicalization may replace a representation only when equivalence is proved,
not by visually fitting samples. A feature commits a complete valid result or
retains its previous geometry. Tests and independent imports check these
contracts; they are not a substitute for them or a proof of every possible
floating-point geometry case.

| Shared cause | Baseline evidence | Required outcome |
| --- | --- | --- |
| Arbitrary feature inputs | Starting a command fills the most recent sketch, or uses an implicit last body/axis | Use deliberate selections and an explicit active-sketch handoff; empty inputs remain empty |
| Input routing | Sketch roles are omitted from generic selection routing; Revolve uses a separate axis click path | Every exposed reference input uses the same activation, selection, highlighting, clear and restoration rules |
| Rendered sketch selection | Clear can hide an edited feature's source; a selected region behind a body can be toggled off but cannot be clicked back on | Keep the editing source visible independently of current input values; use the same visible sketch set for drawing and profile picking |
| Constraint activation | Missing midpoint; toolbar/search/keyboard differ in applying preselection | Persistent midpoint and point geometry; consistent selection-first and tool-first constraint activation |
| Circular blend boundaries | Inner circular rim cases disagree with analytic volume and independent STEP mass | Correct material side and orientation at both ends and directions; exact boundaries and independent verification |
| Partial geometry results | Blend failures can be ignored when some edges succeed | Commit the requested feature completely or preserve the previous solid and report the failure |
| Boolean result boundaries | Saved model has six redundant connected same-surface faces; remaining disconnected coplanar faces also share references | Verify connected oriented wires, removal of internal boundaries, supported face unification, and downstream edits; do not infer correctness from appearance or face count alone |
| Placed references | A moved body can leave its attached sketch at the original location or lose selectable cap references | Transform reference frames with their owning occurrence; keep copied occurrences distinct through later joins and upstream edits |

Verification must include positive and rejected operations, multiple profiles,
independent scopes, clear/reselect, preview/commit/cancel, upstream edits,
save/reopen, exact solid validity and independent STEP measurements. Pixel owns
native interaction checks. Automated construction, screenshots of generated
models and direct Onshape interaction remain distinct evidence.

Native workflow testing also found two shared camera failures. Fit considered
origin planes before visible sketch geometry, and accepting a normal feature
restored the entry camera even after a deliberate fit. Fit now uses visible
sketches and solids, excluding hidden parts. Editing records automatic sketch
framing separately. It restores the earlier camera only while that automatic
view remains unchanged, preserving deliberate navigation on accept or cancel.

A plane alone is not a face identity. Disconnected coplanar wall patches retain
separate references derived from neighboring source faces and exact source-edge
lineage. The signature excludes coordinates and arena allocation order. If the
known lineage still cannot distinguish patches, the operation requires a repaired
reference instead of choosing one by proximity.

The frozen application source for this pass is
`18f32bddacc3762f007bbac17e83c962c2a6d4858121a450c552a700c41cf4d6`.
Windows passed 438 checks with no failures or ignored tests, strict all-target
Clippy, and the build without desktop features. The ten-project timing table is
updated in [end-user projects](end-user-projects.md).

Native Pixel checks on the preceding UI build covered Clear and reselect,
occluded-region toggling, mixed face/region extrusion, an open Sweep path behind
a body, Apply/Cancel camera behavior, and connected constrained dragging. Pan
translated the image by the requested 60 by 40 pixels; inverse navigation
restored 99.9991% of the viewport pixels. That interaction run used source
`4536953`, before the final kernel update, and does not stand in for the final
build's geometry checks. Physical SpaceMouse feel and direct hands-on Onshape
comparison remain unverified. Tangent-chain fillet propagation, curved-path
U-bolts, guided lofts, and the fused-loft through-bore case remain unsupported.
