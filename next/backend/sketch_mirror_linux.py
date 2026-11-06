"""Pinned Linux access to the native addSymmetric fourth boolean.

Reuse checkpoint's ELF/provider inspection only. No checkpoint calls or objects
are retained here. PyDLL keeps the GIL for the complete synchronous native call.
"""
import ctypes
import json
import os
from pathlib import Path
import struct
import sys
import types

import checkpoint_linux as ABI

SOURCE_COMMIT = '4fd3bf320d9566a27e60069fc8387448aaa3a094'
_libraries = None


def load():
    global _libraries
    if _libraries is not None:
        return _libraries[-1].mirror_native
    arch = ABI.architecture()
    if (sys.platform != 'linux' or sys.implementation.name != 'cpython'
            or struct.calcsize('P') != 8 or arch not in ('aarch64', 'x86_64')):
        raise ValueError('No verified Linux constrained Mirror adapter for this platform')
    import FreeCAD, Part, Sketcher
    folder = Path(__file__).resolve().parent / 'native'
    pins = json.loads((folder / ('sketch_mirror-linux-' + arch + '.json')).read_text())
    version = list(FreeCAD.Version())
    if (pins['architecture'] != arch or list(sys.version_info[:3]) != pins['python']
            or version != pins['freecad_version'] or SOURCE_COMMIT not in version
            or pins['source_commit'] != SOURCE_COMMIT or Part.OCC_VERSION != '7.8.1'
            or pins['occt_version'] != Part.OCC_VERSION):
        raise ValueError('Linux Mirror adapter requires its exact pinned FreeCAD/OCCT/CPython runtime')
    extension = Path(FreeCAD.__file__).resolve()
    runtime = extension.parents[2]
    for role, module in [('freecad', FreeCAD), ('part', Part), ('sketcher', Sketcher)]:
        if Path(module.__file__).resolve() != runtime / pins['extensions'][role]:
            raise ValueError('Unexpected loaded Mirror extension path: ' + role)
    if Path(sys.executable).resolve() != runtime / pins['python_executable']:
        raise ValueError('Unexpected loaded Mirror Python executable')
    for relative, expected in pins['runtime'].items():
        path = runtime / relative
        ABI.verify_elf(path, arch)
        if ABI.digest(path) != expected:
            raise ValueError('Linux Mirror runtime hash mismatch: ' + relative)
    libs = {}
    for role, entry in pins['libraries'].items():
        path = (runtime / entry['path']).resolve()
        if path.relative_to(runtime).as_posix() not in pins['runtime']:
            raise ValueError('Mirror provider is missing its runtime hash: ' + role)
        library = ctypes.PyDLL(None) if entry.get('process') else ctypes.PyDLL(
            str(path), mode=os.RTLD_NOW | os.RTLD_NOLOAD)
        for symbol in entry['symbols']:
            if ABI.symbol_path(library, symbol) != path:
                raise ValueError('Linux Mirror loaded-library mismatch: ' + role)
        libs[role] = library
    artifact = folder / pins['adapter']
    ABI.verify_elf(artifact, arch)
    if ABI.digest(artifact) != pins['adapter_sha256']:
        raise ValueError('Linux Mirror adapter hash mismatch')
    adapter = ctypes.PyDLL(str(artifact))
    # The C++ vector must use the same already-loaded libstdc++ provider.
    if ABI.symbol_path(adapter, '_Znwm') != (runtime / pins['libraries']['stdlib']['path']).resolve():
        raise ValueError('Linux Mirror adapter C++ provider mismatch')
    initialize = adapter.mirror_initialize
    initialize.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    initialize.restype = ctypes.c_int
    if not initialize(libs['app']._handle, libs['sketcher']._handle):
        raise ValueError('Linux Mirror exports unavailable')
    function = adapter.mirror_native
    function.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.POINTER(ctypes.c_int),
        ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_bool, ctypes.c_char_p, ctypes.c_int]
    function.restype = ctypes.c_int
    _libraries = [*libs.values(), adapter]
    return function


def validate(sketch, indices, reference, point):
    """Reject wrappers/proxies and out-of-range native inputs before lookup."""
    import FreeCAD
    method = sketch.addSymmetric
    if (not isinstance(method, types.BuiltinMethodType) or method.__self__ is not sketch
            or method.__name__ != 'addSymmetric'):
        raise TypeError('Native Mirror requires a native bound SketchObject')
    if (not sketch.isDerivedFrom('Sketcher::SketchObject')
            or FreeCAD.getDocument(sketch.Document.Name) is not sketch.Document
            or sketch.Document.getObject(sketch.Name) is not sketch):
        raise TypeError('Native Mirror requires a live native SketchObject')
    minimum = -len(sketch.ExternalGeo)
    if (type(indices) is not list or not indices or len(indices) > 2147483647
            or any(type(i) is not int or not minimum <= i < sketch.GeometryCount
                   or i in (-1, -2) for i in indices)
            or len(set(indices)) != len(indices)
            or type(reference) is not int or not minimum <= reference < sketch.GeometryCount
            or type(point) is not int or not 0 <= point <= 3):
        raise ValueError('Native Mirror inputs are outside the validated sketch range')
    for name in [sketch.Document.Name, sketch.Name]:
        if type(name) is not str or not name or '\0' in name:
            raise ValueError('Native Mirror requires exact native document/object names')
