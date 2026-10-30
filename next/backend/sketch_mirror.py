"""FreeCAD 1.1.4 Sketcher_Symmetry command, without a GUI or geometry math."""
import ctypes
import hashlib
import json
from pathlib import Path
import sys
import platform
import struct
import xml.etree.ElementTree as ET

import FreeCAD as App
import Part

_bridge = None
_availability = None


def _digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def _load_bridge():
    global _bridge
    if _bridge is None:
        if sys.platform != "win32" or struct.calcsize("P") != 8 or platform.machine().lower() not in ("amd64", "x86_64"):
            raise ValueError("No verified constrained Mirror adapter for this platform; Windows x64 is currently required")
        # This adapter is explicitly bound to the shipped Windows x64 ABI.
        # Fail before calling native code on another runtime or missing build.
        folder = Path(__file__).resolve().parent / "native"
        manifest = json.loads((folder / "sketch_mirror.json").read_text())
        runtime = Path(sys.executable).resolve().parents[1]
        for relative, digest in manifest["runtime"].items():
            if _digest(runtime / relative) != digest:
                raise ValueError("Mirror symmetry bridge requires the pinned FreeCAD 1.1.4 Windows runtime")
        library = folder / "sketch_mirror.dll"
        if _digest(library) != manifest["bridge_sha256"]:
            raise ValueError("Mirror symmetry bridge does not match its build manifest")
        # Keep the GIL while FreeCAD updates properties and invokes Python hooks.
        _bridge = ctypes.PyDLL(str(library)).mirror_native
        _bridge.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.POINTER(ctypes.c_int),
            ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_bool, ctypes.c_char_p, ctypes.c_int]
        _bridge.restype = ctypes.c_int


def capabilities():
    global _availability
    if _availability is None:
        try:
            _load_bridge()
            _availability = {"available": True, "reason": None}
        except FileNotFoundError as error:
            _availability = {"available": False, "reason": "Constrained Mirror adapter/runtime file is missing: " + Path(error.filename).name}
        except Exception as error:
            _availability = {"available": False, "reason": "Constrained Mirror adapter unavailable: " + str(error)}
    return {"default_copy": {"available": True, "reason": None},
            "create_symmetry_constraints": dict(_availability)}


def preflight(edit):
    if edit.get("create_symmetry_constraints") is True:
        status = capabilities()["create_symmetry_constraints"]
        if not status["available"]:
            from sketch_operations import OperationError
            raise OperationError(status["reason"], "sketch_mode_unavailable")


def _constrained_copy(sketch, indices, reference, point):
    preflight({"create_symmetry_constraints": True})
    error = ctypes.create_string_buffer(1024)
    result = _bridge(sketch.Document.Name.encode(), sketch.Name.encode(),
        (ctypes.c_int * len(indices))(*indices), len(indices), reference, point, True, error, len(error))
    if result < 0:
        raise ValueError(error.value.decode("utf-8", errors="replace") or "Native symmetry operation unsuccessful")


def _coincident_components(sketch):
    # Native identity only: graph edges are the original Coincident constraints.
    parent = {}
    def find(node):
        while parent[node] != node:
            node = parent[node]
        return node
    for index, constraint in enumerate(sketch.Constraints):
        if constraint.Type != "Coincident":
            continue
        try:
            flags = ET.fromstring(constraint.Content).attrib
            if flags.get("IsActive") != "1" or flags.get("IsDriving") != "1":
                continue
        except Exception:
            return {}
        if (constraint.FirstPos in (1, 2, 3)
                and constraint.SecondPos in (1, 2, 3)):
            a = (constraint.First, constraint.FirstPos)
            b = (constraint.Second, constraint.SecondPos)
            parent.setdefault(a, a)
            parent.setdefault(b, b)
            parent[find(a)] = find(b)
    return {node: find(node) for node in parent}


def _repair_axis_endpoint_symmetry(sketch, sources, axis, old_geometries, old_constraints, components):
    # Only newly created source-to-copy symmetric constraints on this native line.
    axis_components = {components[node] for node in ((axis, 1), (axis, 2)) if node in components}
    repairs = []
    for index, constraint in enumerate(sketch.Constraints):
        if (index >= old_constraints and constraint.Type == "Symmetric"
                and constraint.First in sources and constraint.First >= 0
                and constraint.FirstPos in (1, 2)
                and old_geometries <= constraint.Second < sketch.GeometryCount
                and constraint.SecondPos in (1, 2)
                and constraint.Third == axis and constraint.ThirdPos == 0):
            source = (constraint.First, constraint.FirstPos)
            if source in components and components[source] in axis_components:
                repairs.append((index, source, (constraint.Second, constraint.SecondPos)))
    if repairs:
        import Sketcher
        for index, _, _ in reversed(repairs):
            sketch.delConstraint(index)
        for _, source, copy in repairs:
            if sketch.addConstraint(Sketcher.Constraint("Coincident", *source, *copy)) < 0:
                raise ValueError("Native endpoint Coincident constraint could not be added")


def apply(sketch, edit):
    from sketch_operations import OperationError, flag, integer
    allowed = {"op", "object", "indices", "reference", "delete_original", "create_symmetry_constraints"}
    if set(edit) - allowed:
        raise OperationError("Unsupported Mirror parameter")
    indices = edit.get("indices")
    if not isinstance(indices, list) or not indices:
        raise OperationError("Select native sketch geometries to mirror")
    # ExternalGeo stores axes first; native external IDs start at -3.
    minimum = -len(sketch.ExternalGeo)
    indices = [integer(i, minimum, sketch.GeometryCount - 1, "Geometry index") for i in indices]
    if any(i in (-1, -2) for i in indices) or len(set(indices)) != len(indices):
        raise OperationError("Select each native source geometry once; axes are references")
    reference = edit.get("reference")
    if not isinstance(reference, dict) or set(reference) != {"index", "point"}:
        raise OperationError("Choose a native Mirror reference")
    ref = integer(reference["index"], minimum, sketch.GeometryCount - 1, "Reference index")
    point = integer(reference["point"], 0, 3, "Reference point")
    geo = sketch.Geometry[ref] if ref >= 0 else sketch.ExternalGeo[-ref - 1]
    if point == 0:
        if not isinstance(geo, Part.LineSegment):
            raise OperationError("Native Mirror reference must be a line or an exposed point")
    elif not (ref == -1 and point == 1):
        import sketch_geometry
        handles = sketch_geometry.geometry_entry(geo, ref, False, sketch=sketch)["handles"]
        if not any(h["point"] == point for h in handles):
            raise OperationError("Choose an exposed native Mirror point")
    delete = flag(edit, "delete_original", False)
    constrained = flag(edit, "create_symmetry_constraints", False)
    if delete and constrained:
        raise OperationError("Delete originals and create symmetry constraints are mutually exclusive")
    preflight(edit)
    # CommandSketcherTools::getListOfSelectedGeoIds(true). Expand only actual
    # native InternalAlignment links; preserve supplied order and source IDs.
    for index in list(indices):
        geo = sketch.Geometry[index] if index >= 0 else sketch.ExternalGeo[-index - 1]
        if isinstance(geo, (Part.Ellipse, Part.ArcOfEllipse,
                Part.ArcOfHyperbola, Part.ArcOfParabola, Part.BSplineCurve)):
            for constraint in sketch.Constraints:
                if constraint.Type == "InternalAlignment" and constraint.Second == index and constraint.First not in indices:
                    indices.append(constraint.First)
    if constrained:
        repair = ref >= 0 and point == 0 and ref not in indices
        components = _coincident_components(sketch) if repair else None
        old_geometries, old_constraints = sketch.GeometryCount, sketch.ConstraintCount
        _constrained_copy(sketch, indices, ref, point)
        if repair:
            _repair_axis_endpoint_symmetry(sketch, set(indices), ref, old_geometries, old_constraints, components)
    else:
        sketch.addSymmetric(indices, ref, point)
    if delete:
        sketch.delGeometries(indices)
    return {"object": sketch.Name}
