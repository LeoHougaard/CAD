"""Pinned immutable-bytes restore boundary, without the leaking Py_buffer API."""
import ctypes
import hashlib
import json
import os
from pathlib import Path
import platform
import struct
import sys
import types

_restore = None

class CheckpointUnavailable(RuntimeError):
    pass

def capabilities():
    try:
        require_runtime()
        return {'available':True,'reason':None}
    except CheckpointUnavailable as error:
        return {'available':False,'reason':str(error)}

def _digest(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def require_runtime():
    global _restore
    if _restore is not None:return
    try:
        if sys.platform=='linux':
            import checkpoint_linux
            _restore=checkpoint_linux.load()
            return
        if sys.platform!='win32' or sys.implementation.name!='cpython' or struct.calcsize('P')!=8 or platform.machine().lower() not in ('amd64','x86_64'):
            raise ValueError('No verified checkpoint restore adapter for this platform; Windows x64 CPython is required')
        folder=Path(__file__).resolve().parent/'native'
        manifest=json.loads((folder/'checkpoint_restore.json').read_text())
        if list(sys.version_info[:3])!=manifest['python'] or manifest['platform']!=sys.platform:
            raise ValueError('Checkpoint restore requires the pinned CPython runtime')
        kernel=ctypes.PyDLL('kernel32.dll')
        kernel.GetModuleHandleW.argtypes=[ctypes.c_wchar_p];kernel.GetModuleHandleW.restype=ctypes.c_void_p
        kernel.GetModuleFileNameW.argtypes=[ctypes.c_void_p,ctypes.c_wchar_p,ctypes.c_uint];kernel.GetModuleFileNameW.restype=ctypes.c_uint
        runtime=Path(sys.executable).resolve().parents[1]
        for relative,digest in manifest['runtime'].items():
            expected=(runtime/relative).resolve();handle=kernel.GetModuleHandleW(expected.name)
            path=ctypes.create_unicode_buffer(32768)
            length=kernel.GetModuleFileNameW(handle,path,len(path)) if handle else 0
            if not length or length>=len(path) or os.path.normcase(str(Path(path.value).resolve()))!=os.path.normcase(str(expected)) or _digest(expected)!=digest:
                raise ValueError('Checkpoint restore runtime/module pin mismatch: '+expected.name)
        library=folder/'checkpoint_restore.dll'
        if _digest(library)!=manifest['adapter_sha256']:raise ValueError('Checkpoint restore adapter hash mismatch')
        function=ctypes.PyDLL(str(library)).checkpoint_restore_native
        function.argtypes=[ctypes.c_char_p,ctypes.c_char_p,ctypes.c_char_p,ctypes.c_size_t,ctypes.c_char_p,ctypes.c_int]
        function.restype=ctypes.c_int
        _restore=function
    except Exception as error:
        raise CheckpointUnavailable('Native checkpoint restore unavailable: '+str(error)) from error

def restore_bytes(sketch, data):
    # Writable buffers, views, subclasses and arbitrary forwarding callables are
    # deliberately outside this boundary. No Py_buffer is acquired or retained.
    if type(data) is not bytes:raise TypeError('Native checkpoint restore requires exact immutable bytes')
    method=sketch.restoreContent
    if not isinstance(method,types.BuiltinMethodType) or method.__self__ is not sketch or method.__name__!='restoreContent':
        raise TypeError('Native checkpoint restore requires a native bound SketchObject')
    if not sketch.isDerivedFrom('Sketcher::SketchObject') or sketch.Document.getObject(sketch.Name) is not sketch:
        raise TypeError('Native checkpoint restore requires a live native SketchObject')
    require_runtime()
    error=ctypes.create_string_buffer(512)
    # PyDLL holds the GIL. ctypes keeps these exact bytes alive for this complete
    # synchronous call; native std::istringstream owns and releases its copy.
    if not _restore(sketch.Document.Name.encode(),sketch.Name.encode(),data,len(data),error,len(error)):
        raise OSError(error.value.decode('utf-8') or 'Unable to restore content')
