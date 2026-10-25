"""Preserve native suppression placement and element-map state in its document."""
from contextlib import contextmanager

import FreeCAD as App

PROPERTY = "_CadSuppressionPlacement"
SHAPE_PROPERTY = "_CadSuppressionShape"


def metadata(obj):
    if not obj.isDerivedFrom("PartDesign::Feature") or "Suppressed" not in obj.PropertiesList:
        return {}
    reason = None
    if PROPERTY in obj.PropertiesList and obj.getTypeIdOfProperty(PROPERTY) != "App::PropertyPlacement":
        reason = "The saved suppression placement has an unsupported native property type"
    elif SHAPE_PROPERTY in obj.PropertiesList and obj.getTypeIdOfProperty(SHAPE_PROPERTY) != "Part::PropertyPartShape":
        reason = "The saved suppression map has an unsupported native property type"
    elif obj.Suppressed and (PROPERTY not in obj.PropertiesList or SHAPE_PROPERTY not in obj.PropertiesList):
        reason = "The original suppression state was not saved by this legacy document"
    elif any(path.removeprefix(".").split(".")[0] == "Suppressed" for path, _ in obj.ExpressionEngine):
        reason = "Suppression is controlled by a native expression; edit it in Properties"
    elif "ReadOnly" in obj.getEditorMode("Suppressed") or "ReadOnly" in obj.getPropertyStatus("Suppressed"):
        reason = "Native suppression is read-only"
    return {"suppression": {"suppressed": obj.Suppressed, "can_toggle": reason is None, "reason": reason}}


class NativeSuppression:
    def __init__(self):
        self.documents = set()
        self.pending = {}
        self.errors = []
        self.replaying = 0

    def attach(self, doc):
        self.documents.add(doc.Name)

    def detach(self, doc):
        name = doc if isinstance(doc, str) else doc.Name
        self.documents.discard(name)
        self.pending = {key: value for key, value in self.pending.items() if key[0] != name}

    @contextmanager
    def history(self):
        self.replaying += 1
        try:
            yield
        finally:
            self.replaying -= 1

    def eligible(self, obj, prop):
        if prop != "Suppressed" or self.replaying:
            return False
        doc = obj.Document
        return (doc is not None and doc.Name in self.documents and not doc.Restoring
                and obj.isDerivedFrom("PartDesign::Feature"))

    def remember_error(self, error):
        if not self.errors:
            self.errors.append(str(error))

    def check(self):
        if self.errors:
            error, self.errors = self.errors[0], []
            raise RuntimeError(error)

    def slotBeforeChangeObject(self, obj, prop):
        try:
            if self.eligible(obj, prop):
                self.pending[(obj.Document.Name, obj.Name)] = (obj.Suppressed, obj.Placement)
        except Exception as error:
            self.remember_error(error)

    def slotChangedObject(self, obj, prop):
        try:
            if not self.eligible(obj, prop):
                return
            before = self.pending.pop((obj.Document.Name, obj.Name), None)
            if before is None or before[0] == obj.Suppressed:
                return
            exists = PROPERTY in obj.PropertiesList
            if exists and obj.getTypeIdOfProperty(PROPERTY) != "App::PropertyPlacement":
                raise RuntimeError("Suppression placement property type collision")
            has_shape = SHAPE_PROPERTY in obj.PropertiesList
            if has_shape and obj.getTypeIdOfProperty(SHAPE_PROPERTY) != "Part::PropertyPartShape":
                raise RuntimeError("Suppression shape property type collision")
            if obj.Suppressed:
                original_shape = obj.Shape
                if not exists:
                    obj.addProperty("App::PropertyPlacement", PROPERTY,
                                    "CAD Native Compatibility",
                                    "Original native placement while suppressed", 5)
                setattr(obj, PROPERTY, before[1])
                obj.setPropertyStatus(PROPERTY, "ReadOnly")
                obj.setPropertyStatus(PROPERTY, "Hidden")
                if not has_shape:
                    obj.addProperty("Part::PropertyPartShape", SHAPE_PROPERTY,
                                    "CAD Native Compatibility",
                                    "Native shape and element map before suppression", 5)
                setattr(obj, SHAPE_PROPERTY, original_shape)
                obj.setPropertyStatus(SHAPE_PROPERTY, "ReadOnly")
                obj.setPropertyStatus(SHAPE_PROPERTY, "Hidden")
            elif exists and has_shape:
                obj.Placement = getattr(obj, PROPERTY)
                # Seed the native element map before regenerating from current
                # native inputs. Touch requires recomputation before publication.
                obj.Shape = getattr(obj, SHAPE_PROPERTY)
                obj.removeProperty(SHAPE_PROPERTY)
                obj.removeProperty(PROPERTY)
                obj.touch()
            else:
                raise RuntimeError("Original native suppression state unavailable in this already-suppressed document")
        except Exception as error:
            self.remember_error(error)


def register():
    observer = NativeSuppression()
    App.addDocumentObserver(observer)
    return observer
