"""Native expression serialization and dependencies after object restoration.

Pinned native Undo/Redo can restore expression values without their dependency
edges on a recreated object. Rebind only such newly restored objects through
native setters. Existing objects, document history and geometry stay native.
"""
import io
import xml.etree.ElementTree as ET
import zipfile


def entries(obj):
    # Native serialization retains comments omitted by the Python value. Read
    # in memory and never extract archive paths to the filesystem.
    with zipfile.ZipFile(io.BytesIO(bytes(obj.dumpPropertyContent("ExpressionEngine")))) as archive:
        for name in archive.namelist():
            if name.lower().endswith(".xml"):
                return [dict(entry.attrib) for entry in ET.fromstring(archive.read(name)).iter("Expression")]
    raise ValueError("Native expression serialization is unavailable")


def repair_new(doc, previous_names):
    for obj in doc.Objects:
        if obj.Name in previous_names or not obj.ExpressionEngine:
            continue
        for entry in entries(obj):
            obj.setExpression(entry["path"], None)
            obj.setExpression(entry["path"], entry["expression"], entry.get("comment", ""))
