# Missing checkpoint creation control

The ignored Editor test `sketch_creation_missing_checkpoint_preserves_redo_before_native_creation` requires a genuinely missing Windows checkpoint adapter and a read-only native-ID observer. The production worker protocol does not add that observer field.

Run `python -B next/tests/checkpoint_preflight_control.py --runtime PATH_TO_PINNED_RUNTIME_JSON --tests PATH_TO_CURRENT_TEST_EXE --output NEW_OUTPUT_DIRECTORY` on Windows. The test executable must be compiled from this source with the desktop feature. No compilation is performed by the control.

The helper copies backend files and the existing lifecycle fixture into its new output directory, omits only `native/checkpoint_restore.dll`, and supplies a native-ID observer through the test runtime's argument array. It never changes an installed runtime, source file, application, preference or old attempt. HOME, USERPROFILE, FREECAD_USER_HOME, APPDATA, LOCALAPPDATA, TMP and TEMP are private to that output. The native process must report BelowNormal priority and private FreeCAD configuration paths. Python bytecode is disabled.

Exactly one test registration is required before execution. The actual Editor test creates a Box with nondefault dimensions, banks its genuine Redo, chooses a sketch plane, verifies complete public state equality after rejection, cancels the creation dialog and redoes the original Box with its original native ID. The helper also requires a genuine missing-DLL capability reason, only the one fixture-setup transaction and zero native Begin requests. Logs, command and input hashes, copied bytes, native replies and failures are retained. Output paths cannot already exist.

This proves only the known-unavailable checkpoint preflight. Other dynamic Begin failures, general new-sketch Cancel/Redo/Restore atomicity, native graceful EOF, integration and tutorial acceptance remain separate gates. A successful test host does not prove graceful native-child exit.
