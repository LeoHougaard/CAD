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

- `profiles` is the single Faces and sketch regions input. It accepts an ordered
  list mixing `{"kind":"region","sketch":"f1","region":"stable key"}` and
  `{"kind":"face","part":"f2:0","face":"f2:end"}` references. Regions from
  different sketches and planar faces remain selected together. Adjacent cells
  from the same sketch merge before solid creation; selected holes remain exact.
  `{"kind":"sketch","sketch":"f1"}` retains whole-sketch selection semantics.
  Empty lists reject the operation. The inspector shows removable selections
  and Clear; viewport clicks add or remove entries without changing input type.
- Legacy `sketch` identifies an earlier sketch. `regions:null` means the whole sketch;
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
  must lie in every selected profile plane. The older `sketch_line` spelling still works.
  A rectangle side also stores its zero-based `edge` index, so its four sides
  remain distinct axis choices.

New commands begin with empty reference inputs. Deliberately selected geometry
can seed the relevant input; an active sketch can hand its profiles to Extrude
or Revolve. This does not supply an implicit revolve axis or merge scope.
Click an input to activate it, or use Tab and Shift+Tab to cycle the visible
inputs. Clear removes only that input. Apply remains disabled until all required
references are present. These UI rules do not change legacy document semantics.

Each connected Revolve region must stay on one side of the axis; touching it at
the boundary is allowed. This is checked using exact straight-edge endpoints and
bounded circular-arc extrema, rather than display polylines or a signed volume.
Separate regions on opposite sides are checked independently. A crossing region
is rejected atomically, including partial turns. Off-axis arcs whose source-circle
radius reaches its distance from the axis currently require an unsupported
spindle/horn torus path. That path produced an incorrect independently imported
STEP volume, so it is rejected explicitly. Centered spherical arcs and ordinary
ring-torus arcs retain their analytical paths.

Persistent face and edge names use only the features that actually contributed
to that part. Boolean and merge results inherit their input contributors; copies,
patterns and mirrors inherit the source contributors. Unrelated later geometry
cannot supply matching plane or curve names to an earlier part. Arc profiles also
retain the planar closing-edge provenance, and old explicit source-edge aliases
still resolve uniquely within the same contributor scope.

Disconnected coplanar patches have separate identities derived from neighboring
source faces and exact source edges. Coordinates and topology allocation order
are excluded. If that lineage remains ambiguous, reselecting is required.
Attached sketches store a typed `support_ref` with both the part and face owner;
their frame uses the surviving face's outward material normal. A bare legacy
support can resolve only when the active owners agree on one frame.

Old pinned Extrude 1.0/1.1/1.2 and Revolve 1.0/1.1 documents retain their original whole
sketch and single-target behavior when the new inputs are absent. Compatibility
recognizes only the exact previous bundled hashes and exact installed builtin
package. A changed external package requires explicit adoption. Untouched legacy
profile parameters and pins remain unchanged. Explicit unified-profile edits
adopt Extrude 1.3 or Revolve 1.2 on commit, preventing old applications from silently
ignoring the new input. Adoption records the original legacy source and preserves
its cap references while its connected boundary selection is unchanged. New
unified profiles use stable source-and-boundary identities for both parts and
references. Reordering or removing other inputs cannot silently rebind a child to
a different body; removing its own boundary produces a missing-reference error.

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
include a trimmed semicircle extrusion, a bounded ring-torus arc revolution,
one Extrude mixing two sketch regions with an existing planar face, and a Revolve
mixing an annular-cylinder profile with a full circular ring-torus profile.
Collapsed periodic seam edges stay in the exact topology and STEP output; they
have no selectable linear axis or visible point in the viewport. The full torus
uses a 0.01 mm tolerance for OCC's triangulation-based bounding box, whose sampled
points miss its axial extrema by 0.00592 mm. Six exact surface projections verify
those extrema within 1e-7 mm, while the independent volume tolerance stays 1e-7:


```text
cargo test --no-default-features --features step --release --test geometry_controls --test sketch_trim --test provenance_revolve --test unified_profiles
cargo test --release --features step --lib tangent_arc_profile_has_exact_step_geometry
python tools/verify_geometry_controls_step.py
```

Some mixed and annular analytical Booleans still enter the kernel's mesh fallback.
Those results are rejected and the previous precise parts remain available. A
reproducible annular Revolve Add case remains a negative regression. Arc extrusion
uses exact bounded curves; arc sweeps and lofts currently report unsupported
before the legacy profile code can drop their boundaries.
