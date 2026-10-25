# Fit the camera to a selection

1. Open a model and wait for its native display geometry.
2. Select a visible face, edge, vertex or displayed object. Use Shift/Ctrl for
   multiple native elements or multiple rows in Features. Every selected Body uses its currently
   displayed native tip, rather than resurrecting a hidden history feature.
3. Choose View > Fit selection. The camera centers and scales the selected
   display geometry while retaining its current orientation and ordered picks.
4. Continue with Measure or an existing modeling command. Camera fitting does
   not change native properties, revisions, history, visibility or saved files.
5. Choose View > Fit all, or press the existing F shortcut, to frame everything.

Fit selection is disabled when the current scene contains no matching display
geometry. Missing native names do not fall back to a different object. Hidden
objects are not revealed. Datums use their bounded native display graphics;
this does not imply finite mathematical geometry. The camera retains existing
display scale limits. Sketch solver-handle fitting, arbitrary group expansion
and zoom by a screen window remain separate workflows.

This addresses the zoom-to-selection route associated with OSH-0061-R02. All
original coverage acceptance gates remain open. Headless interaction and native
state checks are distinct from physical desktop, platform and resource gates.
