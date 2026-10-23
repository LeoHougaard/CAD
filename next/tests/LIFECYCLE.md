Lifecycle failure regressions
=============================

These ignored Rust tests run the actual Editor, headless egui update and model
actor against a native FreeCAD worker. They require a runtime manifest containing
`executable` and `args`, and a fresh output directory. No desktop app is launched.

Set `CAD_LIFECYCLE_RUNTIME` to the manifest and `CAD_LIFECYCLE_OUTPUT` to the output
directory, then run the test filter `editor::sketch::lifecycle_tests::` with desktop
enabled and `--ignored --test-threads=1`. The ordinary production launcher never
loads `tests/fixtures/lifecycle_fault_worker.py`.

The fixture injects validation exceptions after native validation, including a
recovered cleanup Undo failure and poisoned preview/sketch ownership. Its Close
failure occurs at the native close call, after normal revision/lease admission.
Every native request is captured in the supplied output directory. Models are
new synthetic files inside that directory.

Coverage includes failed cleanup followed by same-document Undo or successful
New/Open, subsequent save/Undo dirty protection, New/Close discard confirmation,
normal cancel restoration, unavailable preview/sketch/dimension/task recovery,
Cancel preserving ownership and dirty state, failed Close/retry, empty Close
replies, unchanged saved files and ordinary failed Open/Save retry.

Unavailable recovery deliberately closes and discards the in-memory document.
It never saves automatically. The user must confirm the loss of unsaved changes
and the active operation. Cancel and failed Close retain the document and its
interaction state. Successful native Close clears the UI and makes New/Open
available again. This does not recover edits from a corrupted native document.
