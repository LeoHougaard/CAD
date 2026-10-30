# Tutorial input replay

Extract a tutorial's visible keyboard/mouse overlay, review its inputs against the video, map them to CAD controls, and replay them through the real Editor. Video analysis and review are silent. Nothing uploads the video.

The trace keeps source time, evidence, pointer samples and the original input order. Target mappings are separate, so moving a toolbar does not require changing the reference. You can correct every input manually.

## Start

Python 3.11+ is sufficient for validation and mapping. Extraction also uses the packages in `requirements.txt`. Use a private virtual environment rather than changing a global installation.

```powershell
python -m venv .tools/tutorial-replay-env
.tools/tutorial-replay-env/Scripts/python -m pip install -r tools/tutorial_replay/requirements.txt
.tools/tutorial-replay-env/Scripts/python -m tools.tutorial_replay extract receiver.mp4 --profile tools/tutorial_replay/profiles/receiver-1080p.json --start 484.8 --end 487.5 --output .tools/receiver-input-attempt1
```

The profile contains the 95 inspected key/button regions from Receiver. It requires the original 1920×1080 video. Other layouts/resolutions need a separate calibration. The three cursor templates cover an arrow and two sketch crosshairs. Tracking confidence is measured, but it is not a guarantee that the detected shape is the cursor.

Extraction produces `trace.json`, event frame images and a decoder log. Every proposed event starts `pending`. Brightness changes are also retained in `signals`; cursor positions and rejected/ambiguous detections are retained in `cursor`.

## Review and map

Open `review.html` in a browser. Load the trace and its local video. The video stays muted. Use single-frame stepping to inspect each event. Click the frame to correct the cursor hotspot. Add missed/repeated inputs with **Add missed input**. Edit the event JSON to preserve modifiers, button, path, exact text or an unresolved gap. Approve only after inspection.

Load a screenshot of the intended CAD window and click the matching target. This creates an explicit per-event pixel override. For menus and geometry, a named binding is usually more reusable:

```json
{
  "version": 1,
  "target": {"width": 1280, "height": 850},
  "bindings": {
    "front": {"kind": "plane", "role": "front"},
    "sketch-button": {"kind": "label", "text": "Sketch", "scope": "shortcut"},
    "origin": {"kind": "sketch", "xy": [0, 0]},
    "drawing": {"kind": "rect", "source": [400, 100, 1000, 800], "target": [200, 50, 500, 400]}
  },
  "overrides": {}
}
```

`rect` maps every recorded sample proportionally within two calibrated rectangles. `affine` uses exactly three noncollinear `{source:[x,y],target:[x,y]}` pairs, allowing translation, scale and rotation. `pixel` overrides a fixed point; `path` records all target waypoints. `label`, `plane` and `sketch` resolve against the current Editor frame, not stale coordinates. Labels must be unique in their explicit scope.

For a drag, retain every reviewed sample with relative `dt`. **Adopt detected path** refuses intervals with missing/ambiguous tracking. The editor also supports manually adding a path point at the current video frame. A drag cannot compile from just a click point. A skipped presentation action needs a reason and remains in the plan's audit.

Save both the trace and mapping, then compile:

```powershell
python -m tools.tutorial_replay validate trace-reviewed.json
python -m tools.tutorial_replay compile trace-reviewed.json mapping.json --start 464 --end 487 --output .tools/receiver-plan1.json
python -m tools.tutorial_replay record .tools/receiver-plan1.json --output .tools/receiver-dry-run1.json
```

Compilation refuses unapproved inputs, unresolved intervals, unmapped points and unbalanced key holds. It does not infer physical key-up from overlay fading, silently add a camera gesture, or substitute a modeling command for a click. Outputs use fresh filenames to preserve prior attempts. `record` is a dry run, not UI verification.

## Actual Editor adapter

`adapters/egui.rs` is a test module for the existing `next` lifecycle test driver. Copy the selected immutable CAD source to a new private validation directory, include this module from `editor/sketch/lifecycle_tests.rs`, and compile that crate's tests against its pinned cached dependencies. Product source remains unchanged. Set:

```text
CAD_TUTORIAL_PLAN=<absolute compiled plan>
CAD_LIFECYCLE_RUNTIME=<isolated native runtime manifest>
CAD_LIFECYCLE_OUTPUT=<fresh output directory>
```

Run the ignored `tutorial_replay_from_json` test. Each mapped action goes through actual `Editor::update`/egui input frames. Named targets resolve afresh before input. `wait_for` lists the native replies that must succeed after that action. `expect` uses JSON pointers into `{native,sketch,ui}` for exact checkpoints. The output records resolved points, inputs, native replies and model state. Example scope starts with an empty native document; cloud document-list creation is outside that adapter.

Once the adapter test binary is built, replay it directly:

```powershell
python -m tools.tutorial_replay replay .tools/receiver-plan1.json --executable <test-exe> --sha256 <expected-exe-hash> --runtime <isolated-runtime.json> --output .tools/receiver-run1
```

The adapter implements click, sampled move/drag paths, scroll, key, key_down, key_up, text and checkpoint. Modifier holds persist across actions and every recorded path point reaches a real input frame. Overlapping paths and other inputs are rejected until interleaved replay is implemented. It preserves recorded order and waits for native completion; it does not reproduce wall-clock timing. egui normalizes keypad digits/Enter to logical digit/Enter keys. Exact expectations preserve JSON number representation, so a native floating value uses `62.0` rather than integer `62`. This is headless real-Editor/native verification, not physical Windows or Pixel verification. A desktop adapter must use the existing approved computer-use APIs and current window observations. No global input hook or SendInput replacement is installed.

## What video can establish

The Receiver key overlay remains bright roughly half a second after a press. Repeated presses may merge, and actual release/hold duration is not encoded. Mouse-button cells alone do not distinguish a click from a held drag. Cursor templates can confuse scene geometry with a crosshair. Scroll amount, gestures without visible button indication, occlusion and cuts require explicit manual correction or a blocked interval.

Keep full tutorial completion separate from extracted events and partial route passes. A successful replay also needs model/geometry checks. The tool's default claim fields remain false.

Run focused tests with `python -B -m unittest discover -s tools/tutorial_replay/tests -v`.

## Guarded Pixel step adapter

`adapters/pixel.py` translates one reviewed input to the existing phone-native desktop API. It does no device I/O. Call `preflight` on a fragment before starting, then resolve and guard each action against a newly inspected screenshot, the sole device lease and the exact owned process/window. The caller must also check the native replies and checkpoint expectations. Preflight does not replace those checks.

This adapter supports unmodified clicks and pointer moves, ordinary key chords, keypad keys and case-sensitive `F1` through `F24`. A visible uppercase S label maps to lowercase X11 `s`, while an explicit Shift modifier stays explicit. It refuses held modifiers on pointer input, raw key holds, paths/drags, scroll and text instead of dropping or substituting them. Unsupported later actions block the fragment before its first input.

## Selected visible-input videos

Silent frame inspection confirmed mouse-button/keyboard overlays and a blank geometry start in these additional Too Tall Toby Onshape videos:

- [FOOT, 17:42](https://www.youtube.com/watch?v=EGNJEvfMlGo), model 24WC-07. New-document dialog at 5:00 and default geometry with Parts(0) at 6:00. Keyboard-camera and M1/M2 overlay.
- [STEPPED PIN, 13:13](https://www.youtube.com/watch?v=QvRE2bTSoFY). Synthetic keyboard/mouse overlay, including a visibly pressed mouse button at 3:05.
- [Parametric BRACELET, 24:09](https://www.youtube.com/watch?v=B1j1JsFzr2k). New document at 3:20, variables and Parts(0) at 8:00. Keyboard-camera and M1/M2 overlay.

These are format-qualified candidates, not complete extracted traces or tutorial passes. Their 720p layouts need their own calibration rather than applying the Receiver 1080p coordinates.

## Current verification

The first Receiver proof covers M07–M16: Front click, S, Sketch click, N, S, Line click, origin/left/upper clicks, keypad6, keypad2 and keypadEnter. The extractor found all12 onsets in693 frames over23.1seconds. Six source pointer proposals needed manual correction or insertion. The target's rough upper point is explicitly adapted to avoid the floating panel; the same click/order and native62mm driving value remain.

Actual headless Editor replay passed those12 actions, exact native62.0 geometry/constraint checkpoints and persistent IDs1/2. A separate headless adapter replay passed Ctrl hold/release, a three-sample pointer move, a three-sample drag and scroll. Twenty-two focused tests cover compilation, mapping and Pixel translation. The offline review editor passed eight interaction checks at desktop and mobile widths.

The same 12 source inputs were then executed through the Pixel's native agent in one isolated current Editor, with fresh screenshot and process/hash guards before input. The recorded native state has two lines, five constraints, IDs 1/2 and a 62.0 mm vertical driving dimension. Finish and Save were additional persistence controls, followed by normal owned Close. The rough first horizontal length is 36.22234725952149 mm; the source pointer mapping was an origin translation, not metric calibration. This does not verify the source's full pointer path or later camera gesture. The observer reconstructed all 13 main request/reply pairs and 155 callback pairs without pending IDs or drops; graceful EOF/child exit remains uncertified. Complete tutorial credit remains zero.

Independent stock FreeCAD readback reopened that saved file with the same endpoints, IDs and driving length. A separate solve retained one degree of freedom.

The local headless replay now reconstructs the next two views from source landmarks, with measured alignment errors of 0.7512 and 0.21375 pixels. Navigation leaves the native model state unchanged. The physical camera buttons, wheel steps and pointer path remain unverified. The mapped route draws the profile, mirrors it and enters the S-menu Dimension tool. Exact native checks confirm the first 80 mm width and 100 mm endpoint-to-baseline height. This is 43 verified actions within a fixed 75-action segment, including four reconstructed camera actions. The 182 mm step was incorrectly committed as an angle; later accepted transactions do not count as semantic passes. The next repair targets the missing Horizontal constraint on the earlier snapped top edge. All failed attempts remain retained. `VALIDATION.json` keeps these cohorts separate from complete tutorial acceptance.

Live `sketch-element` targets use persistent native IDs and verify the actual hit before clicking. Quantity validation replies are consumed when a numeric field opens, before waiting for its commit. Current focused tool checks pass 29 cases. The latest source checkpoint passed 255 active tests and five focused native cases, with independent reruns of the mirrored-width and retained-selection controls. Physical Pixel acceptance of these later fixes remains open.
