"""Headless native STEP/IGES exchange. Source documents and files stay authoritative."""

import math
import os
from pathlib import Path
import tempfile

import Import
import Part


class ExchangeError(ValueError):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def path_for(value, importing):
    if not isinstance(value, str) or not value.strip() or "\0" in value:
        raise ExchangeError("invalid_request", "Choose a STEP or IGES file path")
    path = Path(value).expanduser().resolve()
    extension = path.suffix.lower()
    if extension not in {".step", ".stp", ".iges", ".igs"}:
        raise ExchangeError("unsupported_format", "Only native STEP and IGES exchange is supported")
    if importing and not path.is_file():
        raise ExchangeError("not_found", "Exchange source file does not exist")
    if not importing and not path.parent.is_dir():
        raise ExchangeError("not_found", "Export destination directory does not exist")
    return path, "STEP" if extension in {".step", ".stp"} else "IGES"


def finite_shape(obj):
    if not hasattr(obj, "Shape") or obj.Shape.isNull():
        return False
    shape = obj.Shape
    box = shape.BoundBox
    limits = (box.XMin, box.YMin, box.ZMin, box.XMax, box.YMax, box.ZMax)
    return (box.isValid() and all(math.isfinite(value) and abs(value) < 1e50 for value in limits)
            and shape.isValid() and bool(shape.Vertexes))


def import_into(doc, request):
    """Call inside the worker's ordinary atomic native transaction."""
    path, format_name = path_for(request.get("path"), True)
    before = {obj.Name for obj in doc.Objects}
    try:
        # Import.insert preserves native STEP assembly hierarchy. Its IGES
        # OCAF path can silently produce no objects; Part.insert is the native
        # surface importer and handles that format correctly in headless mode.
        if format_name == "STEP":
            Import.insert(str(path), doc.Name, useLinkGroup=False)
        else:
            Part.insert(str(path), doc.Name)
    except Exception as error:
        raise ExchangeError("exchange_failed", str(error)) from error
    added = [obj for obj in doc.Objects if obj.Name not in before]
    if not any(finite_shape(obj) for obj in added):
        raise ExchangeError("exchange_failed", "Native import produced no finite valid geometry")
    return {"objects": [obj.Name for obj in added], "path": str(path), "format": format_name}


def export_from(doc, request):
    """Call after revision and worker-wide preview/sketch ownership guards."""
    path, format_name = path_for(request.get("path"), False)
    overwrite = request.get("overwrite", False)
    if type(overwrite) is not bool:
        raise ExchangeError("invalid_request", "Export overwrite must be an explicit boolean")
    if path.exists() and not overwrite:
        raise ExchangeError("file_exists", "Destination exists; explicitly choose replacement or another path")
    names = request.get("objects")
    if not isinstance(names, list) or not names or any(not isinstance(item, str) for item in names):
        raise ExchangeError("invalid_request", "Export requires an explicit nonempty native object list")
    if len(set(names)) != len(names):
        raise ExchangeError("invalid_request", "Select each export object once")
    objects = []
    for name in names:
        obj = doc.getObject(name)
        if obj is None:
            raise ExchangeError("not_found", "Export object does not exist: " + name)
        if not finite_shape(obj):
            raise ExchangeError("no_shape", "Export object has no finite valid native geometry: " + name)
        objects.append(obj)
    descriptor, temporary = tempfile.mkstemp(prefix=".cad-exchange-", suffix=path.suffix, dir=path.parent)
    os.close(descriptor)
    try:
        # Keep selected root objects' native local placements. Enclosing App::Part
        # selection retains its hierarchy. Do not clone or flatten native shapes.
        Import.export(objects, temporary, keepPlacement=True, exportHidden=False, legacy=False)
        if not Path(temporary).is_file() or Path(temporary).stat().st_size == 0:
            raise ExchangeError("exchange_failed", "Native exporter produced no geometry file")
        if overwrite:
            os.replace(temporary, path)
        else:
            # Same-directory publication must fail atomically if another writer
            # creates this destination after preflight. Never overwrite by race.
            os.link(temporary, path)
        return {"path": str(path), "format": format_name, "objects": names,
                "placement": "selected_root_local"}
    except FileExistsError as error:
        raise ExchangeError("file_exists", "Destination was created by another writer") from error
    except ExchangeError:
        raise
    except Exception as error:
        raise ExchangeError("exchange_failed", str(error)) from error
    finally:
        Path(temporary).unlink(missing_ok=True)
