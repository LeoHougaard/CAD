# Sketch regions

Extrude can use a whole sketch or individual enclosed regions. Click regions in
the viewport or choose them in the feature editor. Selecting neighboring regions
removes their shared internal boundary before extrusion. Disjoint selections
produce separate parts for a New operation.

This follows Onshape's [Faces and sketch regions selection workflow](https://cad.onshape.com/help/Content/PartStudio/extrude.htm).
It covers solid extrusion of the line, rectangle, and circle tools available here.

Each enclosed cell is selectable, including the disc inside an annulus. A circle
inside another circle therefore gives an annulus and an inner disc. Crossing
circles, crossing rectangles, shared rectangle sides, and lines dividing closed
contours also produce separate regions. Construction entities do not divide a
region. Open line tails do not create regions.

Circle intersections and the resulting boundaries remain analytic circle arcs.
Display polylines and fill triangles are never used to build solid geometry.
Coincident circles and tangent curve contacts are rejected with a feature error.
The current subdivision limit is 2,000 input boundary curves, 8,192 edges, and a
bounded number of intersection records.

The Extrude plugin is version 1.1.0. Its `regions` parameter is either `null` for
whole-sketch selection or an array of region identity strings. An empty array is
an explicit empty selection and cannot be applied. Missing parameters in older
documents retain whole-sketch behavior, including the previous alternating-hole
rule and exact topology and part ordering for profiles the older version handled.

For agents, get region identities from the resolved sketch regions. Identities
come from source sketch entities, rectangle sides, and analytic intersection
branches. Dimension changes and entity list reordering preserve a region when its
boundary topology stays the same. Removing a boundary, adding an interior hole,
or changing intersection topology can invalidate a selection. The feature reports
that the selected region changed and preserves the saved selection so it can be
repaired. It never substitutes another region by array position.

Extrude also records source edge and intersection-vertex identities for its
boundary edges. Circle walls and lens intersections have distinct references
that survive dimension edits and the independent solid copy used by Fillet.
Selected circular rims on ordinary cylinders and lenses are tested with Fillet.
If another operation produces an edge or face without a persistent reference,
it can still be selected and highlighted. Fillet, Chamfer, and Shell report an
unsupported reference instead of using a transient kernel array index. Existing
All edges operations retain their behavior.
