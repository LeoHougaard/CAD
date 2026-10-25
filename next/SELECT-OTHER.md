# Select an obscured native element

This candidate adds a chooser for faces, edges and vertices already present in
the current native display scene. It does not create geometry or change native
visibility. Hidden objects omitted from the scene and sketch solver handles
continue to use their existing selection workflows.

1. Open a native model and wait for Ready. In View > Selection choose Faces and
   edges, Faces, Edges or Vertices as appropriate.
2. Right-click the viewport where the desired elements overlap. A right-button
   drag still orbits the view.
3. The Select other menu lists matching native object/subelement names from
   front to back. Scroll when the list exceeds the menu height. An empty hit
   list says that no matching face, edge or vertex is at the point.
4. Hover an entry to preview it through the overlapping display geometry.
   Hovering does not change selection, the native document or its revision.
5. Click the entry to select that exact native element. Hold Shift or Ctrl to
   toggle it in the existing ordered selection. Escape closes the menu and
   retains the selection and underlying Measure, preview or projection tool.
6. Use the selection in Measure or an existing native reference field. Native
   reference validation still owns admission to a modeling operation. In the
   active External > Project edge or vertex tool, choosing an edge or vertex
   submits that exact obscured native reference through the same tool as a
   canvas click. No extra canvas click is required.

These steps are also in Help > Current instructions > Select other.

A changed scene, sketch context, selection filter, camera or viewport size
discards the captured list. Help and disabled canvas controls close it. Reopen
the menu after the change. It never replays an old face number against a new
scene. Normal frontmost picking and sketch handle selection remain in place.
The solid-reference projection tool can also use this chooser while editing a
sketch; ordinary sketch drawing does not open it.

Verification remains bounded. Headless viewport interaction tests and a real
Editor/model-thread/native-worker test cover an obscured face and its native
measurement without changing document state/history. An independent stock
FreeCAD readback checks both saved box shapes before and after recompute.
Physical desktop rendering/input, broader datum/hidden-object choice, platform
acceptance and full selection parity remain open.
