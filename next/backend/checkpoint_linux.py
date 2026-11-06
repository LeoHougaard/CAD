"""Verified Linux runtime dispatch for the native immutable checkpoint boundary."""
import ctypes
import hashlib
import json
import os
from pathlib import Path
import platform
import struct
import sys

_libraries = None  # Loaded libraries/functions only; never objects or payloads.

def architecture():
    return {'arm64':'aarch64','aarch64':'aarch64','amd64':'x86_64','x86_64':'x86_64'}.get(platform.machine().lower())

def digest(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def verify_elf(path, arch):
    with path.open('rb') as stream:header=stream.read(20)
    machine={'aarch64':183,'x86_64':62}[arch]
    if header[:6]!=b'\x7fELF\x02\x01' or int.from_bytes(header[18:20],'little')!=machine:
        raise ValueError('Checkpoint ELF architecture mismatch: '+str(path))

class DlInfo(ctypes.Structure):
    _fields_=[('filename',ctypes.c_char_p),('base',ctypes.c_void_p),('symbol',ctypes.c_char_p),('address',ctypes.c_void_p)]

def symbol_path(library, symbol):
    api=ctypes.PyDLL(None)
    api.dladdr.argtypes=[ctypes.c_void_p,ctypes.POINTER(DlInfo)];api.dladdr.restype=ctypes.c_int
    info=DlInfo();address=ctypes.cast(getattr(library,symbol),ctypes.c_void_p)
    if not api.dladdr(address,ctypes.byref(info)) or not info.filename:
        raise ValueError('Cannot resolve the loaded native library for '+symbol)
    return Path(os.fsdecode(info.filename)).resolve()

def load():
    global _libraries
    if _libraries is not None:return _libraries[-1].checkpoint_restore_native
    arch=architecture()
    if sys.platform!='linux' or sys.implementation.name!='cpython' or struct.calcsize('P')!=8 or arch not in ('aarch64','x86_64'):
        raise ValueError('No verified Linux checkpoint adapter for this platform')
    import FreeCAD
    folder=Path(__file__).resolve().parent/'native'
    manifest=json.loads((folder/('checkpoint_restore-linux-'+arch+'.json')).read_text())
    if manifest['architecture']!=arch or list(sys.version_info[:3])!=manifest['python'] or list(FreeCAD.Version())!=manifest['freecad_version']:
        raise ValueError('Linux checkpoint adapter requires its exact pinned FreeCAD/CPython runtime')
    extension=Path(FreeCAD.__file__).resolve();runtime=extension.parents[2]
    if extension!=runtime/manifest['freecad_extension']:
        raise ValueError('Unexpected loaded FreeCAD extension path')
    for relative,expected in manifest['runtime'].items():
        path=runtime/relative
        verify_elf(path,arch)
        if digest(path)!=expected:raise ValueError('Linux checkpoint runtime hash mismatch: '+relative)
    if Path(sys.executable).resolve()!=runtime/manifest['python_executable']:
        raise ValueError('Unexpected loaded native Python executable')
    libs={}
    for role,entry in manifest['libraries'].items():
        path=(runtime/entry['path']).resolve()
        library=ctypes.PyDLL(None) if entry.get('process') else ctypes.PyDLL(str(path),mode=os.RTLD_NOW|os.RTLD_NOLOAD)
        if symbol_path(library,entry['symbol'])!=path:
            raise ValueError('Linux checkpoint loaded-library mismatch: '+role)
        libs[role]=library
    artifact=folder/manifest['adapter']
    verify_elf(artifact,arch)
    if digest(artifact)!=manifest['adapter_sha256']:raise ValueError('Linux checkpoint adapter hash mismatch')
    adapter=ctypes.PyDLL(str(artifact))
    initialize=adapter.checkpoint_initialize
    initialize.argtypes=[ctypes.c_void_p,ctypes.c_void_p];initialize.restype=ctypes.c_int
    if not initialize(libs['app']._handle,libs['base']._handle):raise ValueError('Linux checkpoint exports unavailable')
    function=adapter.checkpoint_restore_native
    function.argtypes=[ctypes.c_char_p,ctypes.c_char_p,ctypes.c_char_p,ctypes.c_size_t,ctypes.c_char_p,ctypes.c_int]
    function.restype=ctypes.c_int
    _libraries=[*libs.values(),adapter]
    return function
