# Receiver tutorial checkpoint

The target is Too Tall Toby's [Advanced Part Design tutorial](https://www.youtube.com/watch?v=pcU5-8PkhGg), starting with a genuinely empty document. A video-only copy was inspected silently. Known modeling inputs, rough selections and typed dimensions are recorded separately from unidentified camera gestures.

This checkpoint adds an optional native line-length prompt after a Line endpoint. The first numeric key activates the field; separate 6, 2 and Enter events create a native 62 mm driving constraint. The chain resumes from the solved native endpoint. Geometry identity, document revision and sketch-session ownership guard the prompt. Escape in the sketch-name field cancels that field before the line prompt can consume it.

The earlier S menu and Workspace units fixes remain. Empty-sketch framing uses the available rectangle when fitting instead of the old fixed scale, which lets the Receiver's next two rough points remain visible without an extra Fit or zoom action. This does not guarantee a 250 mm span after subsequent layout changes.

Verification is bound to source manifest `ceeecfa95f86ed92b4efe66e10839301271622269b105dc9b3f73838c7feceae`, 295 files. The production inputs are byte-identical to the full source10 build; source11 and source12 change only test code.

- The full production library, Editor, test build, formatting and Clippy batch passed all nine commands. All 253 active tests passed. The 155 ignored tests are not counted as passes.
- The original combined source10 batch remains 18 passed and 1 failed. Its continuing-line control projected a point before layout settled. The native solver then moved an unconstrained coordinate. Those raw inputs and the failed assertion remain retained.
- The corrected input test waits for layout, verifies the chain seed, and retains the original exact prior-endpoint assertion. It also requires exact post-solve coincidence and the native Coincident constraint. Source12's fresh production-worker run passed, including native Undo/Redo and save/reopen.
- A camera-dependent axis test initially clicked a sketch endpoint. Its matched older-build control passed. The corrected fixture chooses a visible point that actually hits the same native H axis, then retains the original preview, Enter, history and reopen assertions. This passed with the current production code.
- Independent stock FreeCAD matched all nine source10 native geometry phases exactly. A separate four-phase source11 replay also matched geometry and constraint XML, Content, native IDs, DOF and status exactly. Source12's actual input edits and raw geometry phases equal those four source11 phases. Each run's opaque session token is authenticated independently; tokens are not expected to equal across processes.
- The fresh independent combined review retains its original 3 passed/1 failed batch and the matched older-build pass. Its two corrected-fixture checks passed. It authenticated the one-assert source12 strengthening and Root's fresh passing run. No new confirmed product defect emerged within that bounded review scope.

The strict recorded-input route reaches source M18 at 494.4 seconds, then stops before an unidentified camera translation at 496.1 seconds. A separate modeling-only continuation explicitly omits that navigation and reaches eight native sketch segments with the 62 mm dimension intact. It then stops at a confirmed next mismatch: one Escape dismisses the passive length prompt but keeps Line active, while the tutorial returns to selection. Box selection, Mirror and later operations remain untested in that continuation.

Complete tutorial verification remains **0 of the requested 100**. Two fixed source UI checks improved from 0/2 to 2/2; that small cohort cannot establish an overall tutorial success rate or support a completion extrapolation.

Current-source physical desktop acceptance remains open. The Pixel attempt stopped before Linux entry at its lock/readiness guard and released its lease and hold. Windows computer-use reports its native pipe unavailable. These results are actual headless Editor input and native-worker checks, not physical desktop runs.

Other open gates include the Workspace mass-unit mismatch with the source's Gram setting, S-menu Dimension, the remaining modeling sequence and camera gaps, independent final solid geometry, atomic Sketch Begin/Cancel/Redo/Restore, graceful native-child EOF, current-head CI and performance. Existing failed attempts and original-source evidence remain attributed to their own trees. No binaries, media, credentials, private clipboard text or raw process inventories are newly published.
