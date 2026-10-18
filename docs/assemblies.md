# Assemblies and frozen contexts

An assembly is a separate `.cadasm` document containing linked Part Studio snapshots, instances, mates and saved contexts. Inserting the same source part several times reuses its evaluated topology and tessellation. Moving a mate transforms the cached display mesh. Exact source surfaces and curves remain available for STEP export.

Source files are optional links. The embedded parametric document works offline if a linked file is unavailable or has changed. Changes become assembly inputs only through an explicit source update. Missing parts, missing connector references and failed source rebuilds reject the update and preserve the previous document.

The model follows [Onshape's mate connector approach](https://cad.onshape.com/help/Content/Assembly/mates.htm). Each mate connects a local coordinate frame on two instances. Fixing an instance anchors its position in this assembly.

| Mate | Allowed motion in its connector frame |
| --- | --- |
| Fastened | None |
| Revolute | Rotation around Z |
| Slider | Translation along Z |
| Cylindrical | Translation and rotation along Z |
| Planar | Translation along X/Y and rotation around Z |
| Ball | Three rotational coordinates |

`position` uses millimeters and `rotation_deg` uses degrees. `offset` moves or rotates the fixed connector frame independently of the mate's allowed motion. Each allowed coordinate can have minimum and maximum limits. Disallowed motion coordinates and out-of-range values produce errors.

Implicit connectors use stable planar face centers or exact circular edge centers and axes. Circular-edge connectors read the analytic curve, so changing a bore's radius preserves its axis. A connector supplies `face` or `edge`, never both. Straight edge connectors are rejected. An explicit local frame remains available for other placements. Flipping a connector's primary axis or rotating its secondary axes changes that connector's `frame`.

The solver positions tree-shaped mechanisms deterministically from fixed instances. A disconnected component retains its initial root placement and has six additional free coordinates. Consistent rigid Fastened cycles are checked. Articulated closed loops and articulated components with several fixed anchors are rejected explicitly. Collision detection, contact, gear relations, subassemblies and dynamics are not implemented.

Mate operations also have plugin packages in the `mate` category. Bundled packages select the six native operations. A custom Rhai package returns `#{op: "mate", kind: "Slider"}` and may supply `position`, `rotation_deg` or `offset`. Packages are pinned by version and content hash. Changing a saved package requires adopting that change explicitly. Scripts select and parameterize native constraints; numerical solving remains in Rust.

[In-context modeling](https://cad.onshape.com/help/Content/Assembly/modeling_in_context.htm) captures surrounding parts and their solved placements in a frozen snapshot. The editable primary Part Studio stays in its own coordinate system. Other instances appear as reference geometry transformed into that frame.

A context binding identifies the context, source instance, stable face and edited feature. Sketch-plane bindings attach a sketch to an external planar face. Extrude bindings terminate an extrusion at a parallel external plane. These become real parametric feature inputs, saved as `context_plane` or `context_up_to`, together with their reference metadata. An up-to-face extrusion currently rejects a symmetric extrusion, a nonparallel target, or a target behind its direction.

Editing an assembly source or moving an instance does not silently change a frozen reference. Update context captures the current assembly and rebuilds every bound primary feature. The new snapshot, primary source revision and resulting geometry commit together after validation. A missing face or failed rebuild preserves the previous snapshot and primary source. Updating an older context also rejects features that have since been detached or rebound, preserving their current ownership. Saved Part Studios retain their frozen feature inputs and can rebuild independently of the assembly file.

## Desktop workflow

Switch to Assembly in the header. Insert opens a Part Studio and places selected source parts as instances. Select an instance to change its placement or fixed state. Select two planar faces or circular rims, press M, choose an installed mate operation and apply it. J shows the mate connector frames. Ordinary clicks add or remove selections; selection filters remain available above the viewport.

Edit in context opens the selected instance's source against translucent frozen surroundings. Reentering a saved context reuses its snapshot. New context captures a separate snapshot. Sketch plane and up-to-face controls can pick a reference face in the surrounding geometry. Bound features show the referenced instance instead of inactive ordinary plane or depth controls. Detach reference preserves the current sketch plane or converts the current up-to extent into a numeric depth. Choose new plane replaces a detached frozen sketch plane only after a new plane is selected.

Save during context editing commits the edited source snapshot and returns to the assembly. Cancel restores the previous Part Studio. Update sources and Update context are separate explicit actions. The original standalone Part Studio is preserved across the edit.

The assembly toolbar's Export menu lists installed exporters. STEP preserves exact placed solids; STL exports their display tessellation. File dialogs add the selected format's extension before checking whether the destination exists, and require an explicit replacement action for an existing file. Export runs on the worker and preserves the document if it fails.

## Agent interface

`cad-assembly-agent` reads one JSON request from stdin and returns one JSON response. It uses the same Rust geometry and mate code as the desktop application.

```json
{"version":1,"path":"bracket.cadasm","action":{"op":"inspect"}}
```

Responses include the semantic document revision, source snapshots, instance transforms, part bounds and volumes, stable face/edge references, and remaining tree degrees of freedom. Geometry references expose both the instance-qualified `key`, such as `instance1::f2:end`, and the source's `source_key`. Connectors and context references accept either key for their own instance. A key qualified with another instance is rejected. Linked-file change detection is informational; inspect never reloads geometry automatically. Relative source links resolve against the saved assembly's directory.

Supported actions are `create`, `inspect`, `apply`, `reload_source`, `capture_context`, `bind_context`, `update_context`, `reference_plane`, and `export`. Every mutation of an existing document requires `expected_revision`. `apply` supplies a complete candidate assembly document. Writers use a shared OS file lock, recheck the revision under that lock, and atomically replace the file. Agents can work on separate files or retry a shared edit against the current revision.

| Action | Additional fields |
| --- | --- |
| `create` | `document`, a complete assembly; destination must be new |
| `apply` | `expected_revision`, `document` |
| `reload_source` | `expected_revision`, `source` ID |
| `capture_context` | `expected_revision`, `primary` instance ID |
| `bind_context` | `expected_revision`, `context` ID, `binding` |
| `update_context` | `expected_revision`, `context` ID |
| `reference_plane` | `context` ID, `instance` ID, `face` key |
| `export` | `expected_revision`, `format`, `output`, optional `instance` ID |

A binding supplies `feature`, `instance`, `face`, `kind` and optional millimeter `offset`. Its kind is `SketchPlane` or `ExtrudeUpToFace`. An optional request-level `plugins` directory loads installed packages; otherwise the bundled registry is used. The CLI exits with a nonzero status and returns `ok: false` for a rejected request. Content revisions detect conflicting edits; they are not cryptographic signatures.

```json
{
  "version": 1,
  "path": "bracket.cadasm",
  "action": {
    "op": "export",
    "expected_revision": "fnv1a64:revision-from-inspect",
    "format": "step",
    "output": "bracket.step"
  }
}
```

Export includes all instances unless `instance` names one instance. It copies exact B-rep topology into a common arena and applies each rigid transform. It does not reconstruct geometry from display triangles. Export requires a new output file. Requests and saved assemblies are limited to 32 MiB, 128 sources, 1,024 instances, 2,048 mates and 32 contexts.
