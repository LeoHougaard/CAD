"""Focused pure identity checks. Native persistence requires the Editor test."""
import importlib.util
import sys
import types
import unittest
from pathlib import Path

sys.modules.setdefault("FreeCAD", types.ModuleType("FreeCAD"))
sys.modules.setdefault("Part", types.ModuleType("Part"))
path = Path(__file__).resolve().parents[1] / "backend" / "vertex_extrusion.py"
spec = importlib.util.spec_from_file_location("vertex_extrusion_identity_subject", path)
subject = importlib.util.module_from_spec(spec)
spec.loader.exec_module(subject)


class Object:
    def __init__(self, document, body, name, identity, type_id):
        self.Document, self.body, self.Name, self.ID, self.TypeId = document, body, name, identity, type_id
        self.PropertiesList = []
        self.status = {}

    def getPropertyStatus(self, prop):
        return self.status[prop]

    def tag(self, owner_name, owner_id, uid):
        for name, value in zip(subject.OWNER_TAGS, (owner_name, owner_id, uid)):
            self.PropertiesList.append(name)
            setattr(self, name, value)
            self.status[name] = ["ReadOnly", "Hidden"]


class Identity(unittest.TestCase):
    def setUp(self):
        self.doc = types.SimpleNamespace(Uid="document-uid")
        self.body = object()
        self.pad = Object(self.doc, self.body, "Pad", 7, "PartDesign::Pad")
        self.binder = Object(self.doc, self.body, "Binder", 8, "PartDesign::SubShapeBinder")
        self.plane = Object(self.doc, self.body, "Plane", 9, "PartDesign::Plane")
        for obj in (self.pad, self.binder, self.plane):
            obj.tag("Pad", 7, "document-uid")
        self.pad.Type = "UpToFace"
        self.pad.Profile, self.pad.UpToFace = (self.binder, [""]), (self.plane, [""])
        self.doc.Objects = [self.pad, self.binder, self.plane]
        self.doc.getObject = lambda name: next((x for x in self.doc.Objects if x.Name == name), None)
        self.owner = lambda obj: obj.body

    def test_same_name_replacement_is_not_owner(self):
        self.assertIsNotNone(subject.owned_helpers(self.pad, self.owner))
        replacement = Object(self.doc, self.body, "Pad", 10, "PartDesign::Pad")
        replacement.tag("Pad", 7, "document-uid")
        replacement.Type = "UpToFace"
        replacement.Profile, replacement.UpToFace = self.pad.Profile, self.pad.UpToFace
        self.doc.Objects[0] = replacement
        self.assertIsNone(subject.owned_helpers(replacement, self.owner))
        self.assertIsNone(subject.helper_owner(self.binder, self.owner))
        with self.assertRaises(subject.VertexExtrusionError):
            subject.remove_owned(self.doc, replacement, self.owner)

    def test_tag_tampering_fails_closed(self):
        for name, value in [("VertexExtrusionOwner", "Other"), ("VertexExtrusionOwnerId", 99),
                            ("VertexExtrusionDocumentUid", "other-document")]:
            old = getattr(self.plane, name)
            setattr(self.plane, name, value)
            self.assertIsNone(subject.owned_helpers(self.pad, self.owner))
            self.assertIsNone(subject.helper_owner(self.plane, self.owner))
            with self.assertRaises(subject.VertexExtrusionError):
                subject.remove_owned(self.doc, self.pad, self.owner)
            setattr(self.plane, name, old)
        self.plane.status["VertexExtrusionOwnerId"] = ["Hidden"]
        self.assertIsNone(subject.owned_helpers(self.pad, self.owner))


if __name__ == "__main__":
    unittest.main()
