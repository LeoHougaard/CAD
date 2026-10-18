# Geometry inputs and exact operations

[Onshape Boolean](https://cad.onshape.com/help/Content/booleanparts.htm) uses
Union, Subtract and Intersect, with Keep tools available in each mode. The local
Boolean feature follows those selection rules. Union and Intersect take at least
two `tools` part identities. Subtract takes nonempty `targets` and `tools`, with
no overlap. Retained tools remain unchanged. Without Keep tools, Union and
Intersect retain the first input part identity; Subtract retains each target.

[Revolve](https://cad.onshape.com/help/Content/PartStudio/revolve.htm) accepts
sketch regions or a planar face and a geometrical axis. Extrude and Revolve share
the local profile resolver and operation scopes:

- `sketch` identifies an earlier sketch. `regions:null` means the whole sketch;
  an explicit array selects those region identities. An empty array is an error.
- `profile_face:{"part":"f2:0","face":"f2:end"}` selects one persistent planar
  face, including its exact inner hole wires, and takes precedence over `sketch`.
- `targets` is an explicit array for Add, Remove or Intersect. Clearing it reports
  an error. Add merges its scope into one part; Remove and Intersect modify each
  target independently. Parts outside the scope remain unchanged.
- Revolve `axis_ref` accepts `{"kind":"origin","axis":"Z"}`,
  `{"kind":"sketch_entity","sketch":"f1","entity":3}`, or
  `{"kind":"edge","part":"f2:0","edge":"persistent edge identity"}`.
  Sketch lines supply their direction; circles and arcs supply their center and
  plane normal. Straight and circular edges supply their exact axes. The axis
  must lie in the profile plane. The older `sketch_line` spelling still works.

Old pinned Extrude 1.0/1.1 and Revolve 1.0 documents retain their original whole
sketch and single-target behavior when the new inputs are absent. Compatibility
recognizes only the exact previous bundled hashes and exact installed builtin
package. A changed external package requires explicit adoption.

The general blend trimmer and polygon chamfer fail on some planar caps containing
circular holes. The local exact setback path handles convex straight edges with
planar adjacent faces and perpendicular end caps. It rebuilds the affected outer
wires, preserves inner hole wires and unrelated analytical surfaces, and adds the
bevel plane. Curved edges and other junctions remain with the general kernel.
Setbacks that collapse an edge are rejected atomically.
Full circular cap rims on plain cylindrical or conical bands use the kernel's
exact rim/cone chamfer builder. The general trimmer previously produced open
shells for an ordinary cylinder rim. Full analytical Revolve results also retain
source-profile cap and rim references for later sketches and edge blends.

The regression suite covers Boolean modes and tool retention, multiple targets,
stale scopes, face profiles, all axis families, upstream edits, reopen, coplanar
part naming, rotated plates, multiple holes, and 1/2 mm blends. It verifies valid
closed topology and analytical volumes. Fresh STEP exports also undergo
independent Open CASCADE volume, placement and solid-count checks. The exports
include a trimmed semicircle extrusion with an exact bounded circular wall:

```text
cargo test --no-default-features --features step --release --test geometry_controls
python tools/verify_geometry_controls_step.py
```

Some mixed and annular analytical Booleans still enter the kernel's mesh fallback.
Those results are rejected and the previous precise parts remain available. A
reproducible annular Revolve Add case remains a negative regression. Arc extrusion
uses exact bounded curves; arc sweeps and lofts currently report unsupported
before the legacy profile code can drop their boundaries.
