# Construction planes

Plane is a parametric feature in the local feature history. Sketches can use its feature ID as their support, so later edits to a plane or its source update the sketch and downstream solids. Creating a plane adds no solid and does not change persistent solid-face names.

The supported workflows follow the [Onshape Plane documentation](https://cad.onshape.com/help/Content/PartStudio/plane.htm): Offset from a plane or planar face, Midplane between parallel planes or between intersecting planes, and Angle about an axis in a reference plane. Midplane offers the two angular bisectors through Flip alignment. Flip normal reverses the normal while retaining a right-handed sketch frame. Other documented plane families are outside this implementation.

| Input | Meaning |
| --- | --- |
| `mode` | `Offset`, `Midplane`, or `Angle` |
| `reference` | Origin plane, earlier datum, or persistent planar face |
| `offset` | Signed distance along the reference normal, for Offset |
| `second` | Second plane, for Midplane |
| `flip_alignment` | Select the other bisector for nonparallel Midplane |
| `axis_ref`, `angle` | Exact in-plane axis and signed angle in degrees, for Angle |
| `flip_normal` | Reverse the resulting normal |

For parallel planes there is one geometric halfway plane; Flip alignment has no effect. Angle rotates the entire reference frame around the selected axis, including its origin. The axis must lie in the reference plane. Origin axes, referenced sketch lines, and persistent straight edges can supply the axis; circular entity axes are valid only when they lie in the reference plane.

Plane references store identities rather than evaluated coordinates:

```json
{"kind":"origin","plane":"Top"}
{"kind":"datum","feature":"f3"}
{"kind":"face","part":"f2:0","face":"f2:end"}
```

The bundled `plane` plugin is version 1.0.0. Existing documents retain their pins until explicitly adopting a newer input. The public agent protocol accepts `plane_ref` fields and returns evaluated datums as `model.planes: [{id, plane}]`. A sketch with `support: "f3"` uses that datum. Missing, suppressed, later, curved, ambiguous, or unsupported references fail explicitly. Failed features preserve earlier datums and solids; rollback excludes later planes.

New face-attached sketches store `support_ref` as the typed face reference shown
above, retaining the owning part as well as the face identity. Their local frame
uses the face's outward material normal. The legacy `support` string remains
readable, but cannot choose between different placed occurrences. Selecting an
origin or datum clears an earlier face reference.

`tests/construction_planes.rs` exercises chained offsets, opposing face normals, both angular bisectors, exact-axis rotation, upstream edits, reopening, failure preservation, timeline changes, cache reuse, and agent edits with atomic rejection. Geometry checks include closed B-reps, analytic volumes, bounds, and orthonormal frames. The runtime rejects native mesh fallbacks for these workflows as for other modeling operations.
