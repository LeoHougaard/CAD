# Assembly verification

The independent review suite checks mate-order independence, a rigid triangle
with a moving hinge branch, frozen context frames, duplicate context bindings,
wrong document types, reused identity counters and concurrent guarded saves.
CLI link tests run from a different working directory containing a misleading
file with the same basename. Inspection and explicit reload must use the linked
file beside the assembly.

```text
cargo test --no-default-features --features step --release --test assembly_review --test assembly_performance
python tools/verify_assembly_step.py
```

The STEP checker uses independent Open CASCADE imports to verify every solid's
volume and placed bounding box. Fresh fixtures cover three translated or rotated
bearing blocks, three analytic annular cylinders rotated about different axes,
and a primary extrusion whose height is bound to a frozen reference face.
Eight separated solids must remain distinct after export and import.

The performance case has twenty-four instances of the ten-feature bearing block
from the end-user projects. Each source mesh has 1,944 triangles. Twenty-four
pose updates change translations and quarter-turn rotations, retain the same
evaluated source object and perform zero source rebuilds after warmup. Vertex
positions are checked against independent quarter-turn formulas. Volumes remain
unchanged, and the fastened assembly has zero remaining degrees of freedom.

On an AMD Ryzen 7 5700G with Radeon Graphics running Windows 11 Pro x86-64,
an optimized build with LTO disabled and sixteen code
generation units measured 2.26 ms median and 2.44 ms at the 95th percentile for
those updates. The test process used 17.75 MB working set, peaking at 20.25 MB.
Its copied display buffers total 2.98 MB. These measurements include CPU copies
and transformations of display buffers. They exclude the application UI, GPU
uploads and interactive frame latency. The current renderer does not use GPU
instancing, so these figures do not establish an application frame rate.

Machine-readable timing, resource and independent STEP reports are written to
`target/test-output/assembly-performance/`. Hardware changes can alter timings;
the test requires correct placement and cache reuse without a fixed speed limit.
