# Receiver tutorial checkpoint

The target is Too Tall Toby's [Advanced Part Design tutorial](https://www.youtube.com/watch?v=pcU5-8PkhGg), starting blank. The video-only reference was inspected silently. Complete tutorial verification is still **0 of the requested 100**.

This checkpoint includes the reviewed S menu and Workspace units behavior, optional native line-length entry, and initial empty-sketch framing. It now also ends a passive Line or Polyline command with one Escape, as the source does. Escape inside an edited numeric field keeps its separate cancellation behavior. Native geometry and the sketch session remain unchanged when leaving the drawing tool.

A real editing check found another Escape conflict. In a constraint's Name field, egui surrenders current focus on Escape before the app's drawing handler runs. The old handler therefore also ended Line. The field now has a stable ID tied to its document, sketch, session and constraint index. Its current or previous-frame focus receives priority over drawing Escape. The unchanged real-field test passes; no draft-text reset behavior is claimed.

Verification is bound to source manifest `2dba3a347fa78350e95e5e50e9af15d721adfdf659fbe5d0051af09cc03929a6`, 296 files.

- All nine cached build/check commands passed, including production library and Editor, tests, formatting and Clippy with warnings denied. All 253 active tests passed. The 160 ignored tests are not counted as passes.
- The combined actual headless Editor/native batch passed 25/25. It covers Units, S, Line numeric input, native identity, native history and save/reopen, passive and edited Escape, existing popup/name/busy owners, the real constraint Name field, and native-axis selection. One test asserts the recorded replay's stop boundary; it does not certify the remaining tutorial.
- The passive-Escape component passed four author checks and four independent checks. The obsolete old two-Escape expectation was preserved and separately failed with actual Select versus expected Line. The final composition explicitly updates that first expectation, retaining the second Escape, live-session and native checks.
- The constraint Name review reproduced the old real-field failure, then passed the unchanged case and a current owner control. Full native JSON, revision, session, chain and prompt are preserved through Escape. Its exact 296-file patch replay and 5,088 protected inputs passed authentication.
- Prior independent stock results remain labeled by their original trees: nine source10 phases and four source11 phases matched raw geometry/constraint XML, Content, IDs, DOF and status exactly. Source12's actual phases matched those four source11 phases. No new stock execution on source14 is claimed.

The strict recorded-input replay reaches M18 at source 494.4 seconds, then stops before an unidentified camera gesture at 496.1 seconds. A separate modeling-only continuation explicitly omits unrepresented navigation. It reaches eight native lines, preserves the typed 62 mm dimension, exits Line with the source's single Escape, and selects all eight lines through the actual box drag. Its next mismatch is the missing directly visible Mirror control.

A separate private Mirror candidate adds that control and reaches the intended native constrained Mirror operation in one click. Its source case still fails the native solve and rolls back; that candidate is **not included** here. A diagnostic reproduced nine preceding native phases exactly and identified conflicting endpoint symmetry constraints. Further controlled experiments remain separate, including a failed strict baseline comparison and a grounded result whose native face area equals the stock result but not the literal analytical value. None is a repaired source route or tutorial pass.

Physical acceptance of the current source remains open. Pixel readiness stopped before Linux entry and its lease and hold were released. Windows computer-use reported its native pipe unavailable. These checks use real Editor input processing and native workers, without a physical desktop acceptance claim.

Other open gates include the source's Gram default versus native mass formatting, S-menu Dimension, Mirror, the remaining source operations and camera gaps, the final independent solid and persistence checks, atomic Sketch Begin/Cancel/Redo/Restore, native-child graceful EOF, current-head CI and performance. Two fixed UI checks improved from 0/2 to 2/2; that cohort does not support an overall tutorial-success extrapolation.

All earlier failed runs retain their original source labels. Main, the existing worktree and their indexes remain preserved. No binaries, media, credentials, private clipboard text or raw process inventories are newly published. This is an unmerged WIP checkpoint.
