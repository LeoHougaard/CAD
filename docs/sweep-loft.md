# Sweep and loft

Both tools are local feature packages under **More**. Their sketch references and operation settings stay in the editable history. New, Add, Remove and Intersect use the existing exact-solid transaction checks. An unsupported operation reports an error and preserves the earlier solid.

## Sweep

Create a closed profile sketch and a separate path sketch, then choose **More > Sweep**. Select the profile and path. The profile must be perpendicular to the path at its start, with its sketch plane passing through that endpoint. The tool preserves the profile's position.

Supported paths are:

- A straight line, including profiles with holes.
- An open, connected planar chain of lines with one polygon profile. Corners use mitres. Short segments, reversals, excessive mitres and nearby nonadjacent runs are rejected. A circular tube cannot currently follow a sharp line-chain bend.
- One complete circle with one connected profile, including an annulus. Place the profile on a radial plane and its outer-region centroid on the path circle. Drive the path radius and profile location from the same variable to keep them together when edited. Profiles that reach the rotation axis are rejected.

Paths can contain construction geometry, which is ignored. General arcs, splines, three-dimensional paths, twist and scale controls are not supported yet.

## Loft

Create two or more profile sketches on distinct parallel planes. **More > Loft** opens an ordered profile list. Add each sketch in section order. The arrows reorder sections; the remove button changes only this selection, leaving the sketch in history.

The initial loft supports 2 to 32 sections with either:

- Coaxial circles, producing analytic cylinders and conical bands.
- Strictly convex polygons with matching vertex counts, producing ruled sides. Line-drawn profiles keep their boundary order; rectangle primitives use a common corner convention across reversed planes. Transitions that collapse or lose convexity are rejected. Warped sides use bilinear surfaces.

Each section must have one closed region without holes. Sections must advance monotonically through their planes. Mixed circle/polygon sections, concave sections, guide curves, vertex connector controls, smooth tangent conditions and surface lofts are not supported yet.

## Verification

[Geometry tests](../tests/sweep_loft.rs) cover profile and path dimension changes, section offsets, save/reopen, STEP export, closed topology and rejection recovery. [Generated fixtures and reports](../output/sweep-loft) include independent Open CASCADE imports. These are analytic test models based on the workflows in Onshape's [Sweep](https://cad.onshape.com/help/Content/PartStudio/sweep.htm) and [Loft](https://cad.onshape.com/help/Content/PartStudio/loft.htm) documentation.

The B-rep retains analytic or exact ruled geometry. Reported volume can still use numerical integration for curved surfaces. In particular, the hollow-torus fixture's kernel-reported volume is approximate; the independent STEP importer verifies its analytic volume separately.
