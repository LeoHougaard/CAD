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
