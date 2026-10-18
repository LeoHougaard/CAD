# Keyboard and selection

The defaults follow [Onshape's keyboard map](https://cad.onshape.com/help/Content/Home/keyboard_shortcuts_and_hotkeys.htm) for implemented tools. Unsupported Onshape tools keep their keys unbound.

| In a sketch | Key |
| --- | --- |
| Line | L |
| Corner rectangle | G |
| Center rectangle | R |
| Circle | C |
| 3 point arc | A |
| Line / tangent arc transition | Shift+A |
| Trim | M |
| Dimension | D |
| Coincident | I |
| Horizontal / vertical | H / V |
| Parallel / perpendicular | B / Shift+L |
| Equal | E |
| Concentric | Shift+O |
| Tangent | T |
| Fix | Shift+J |
| Construction | Q |
| Suppress automatic inferences while placing | Hold Shift |

After placing a line, Shift+A starts a tangent arc from its endpoint. Moving away and back near that endpoint also switches to tangent arc. Placing the arc returns to Line; Shift+A can continue with another tangent arc. The standalone Tangent arc tool accepts a line or arc endpoint, or a circle rim.

Press a tool's key again to exit it. Escape cancels the current gesture or tool. Typing in a dimension or other text field does not activate modeling tools. Relationship tools work with preselected geometry or with subsequent clicks. D accepts one curve or two selected entities before placing the dimension. R creates construction diagonals so its center remains constrained when dimensions change.

| Part Studio or view | Key |
| --- | --- |
| Extrude / revolve | Shift+E / Shift+W |
| New sketch | Shift+S, outside sketch editing |
| Rename selected feature | Shift+N |
| Edge fillet | Shift+F, outside sketch editing |
| Front / back / left / right | Shift+1 / Shift+2 / Shift+3 / Shift+4 |
| Top / bottom / isometric | Shift+5 / Shift+6 / Shift+7 |
| Fit / normal to plane or face | F / N |
| Zoom out / in | Z / Shift+Z |
| Rotate 15 / 5 / 90 degrees | Arrow / Ctrl+Arrow / Shift+Arrow |
| Pan | Ctrl+Shift+Arrow |
| Clear selection | Space |
| Delete selection | Delete or Backspace |
| Undo / redo | Ctrl+Z / Ctrl+Y, also Ctrl+Shift+Z |
| Shortcuts / tool search | S / Alt+C |
| Keyboard help | Shift+/ |
| Hide selected or hovered part/sketch | Y |
| Show hidden parts/sketches | Shift+Y |
| Hide all sketches and planes | Shift+P |
| Toggle all sketches / planes | Shift+H / P |

[Hide/Show](https://cad.onshape.com/help/Content/View/hide_show.htm) applies to parts and sketches. [Onshape's glossary](https://cad.onshape.com/help/Content/Home/glossary.htm) leaves Shell, Mirror, Transform, Linear pattern, Sweep and Loft unassigned. Open these from the toolbar or the S menu. There is no default shortcut for collapsing the feature panel; Tab remains available for moving between fields.

[Feature dialogs](https://cad.onshape.com/help/Content/Home/dialogs.htm) accept Enter to finish and Shift+Enter to finish and start the same command again. A dimension's own numeric editor accepts Enter locally, leaving the sketch open.

Selection follows [Onshape's toggle behavior](https://cad.onshape.com/help/Content/Home/selection.htm). Click entities to add them. Click a selected entity to remove it. Click empty space or press Space to clear. Turquoise marks selected geometry; purple marks the entity under the pointer.

In sketches, drag from empty space left to right to select only enclosed curves. Drag right to left to select curves the box crosses. Hold Ctrl while dragging to subtract those curves from the selection. Individual rectangle edges can be selected. Construction geometry participates in sketch selection and constraints but does not add solid regions.

A draws a three-point arc by start, end, then a point on its span. Center arc uses center, start, then end; its Clockwise option selects the opposite span. M removes the exact curve interval under the pointer. No intersection removes the entire curve. Trimming preserves valid surviving constraints and reports removed dimensions or endpoint references.

Q toggles each selected curve independently. Pressing Q twice restores a mixed
regular/construction selection. Selecting one rectangle side changes that side;
the shared structural replacement preserves surviving dimensions and joins.
The Construction checkbox sets all selected curves to its checked state. With no
selection, Q switches construction mode for subsequent drawing.
