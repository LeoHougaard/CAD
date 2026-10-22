"""Native PartDesign Body Tip and history insertion tasks.

Caller owns revision guards, leases, transactions, recomputation and rollback.
Ordering uses pinned FreeCAD Body.removeObject/insertObject; Group is never
assigned as a substitute for native BaseFeature rewiring.
"""

import re


OPERATIONS = ("body_tip", "body_reorder")


class HistoryError(ValueError):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def error(code, message):
    raise HistoryError(code, message)


def object_in(doc, value):
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", value):
        error("invalid_request", "History references require a native object name")
    obj = doc.getObject(value)
    if obj is None:
        error("not_found", "Native history object does not exist: " + value)
    return obj


def owner_body(obj):
    parent = obj.getParentGeoFeatureGroup()
    if parent is not None and parent.isDerivedFrom("PartDesign::Body") and obj in parent.Group:
        return parent
    # Use native membership, never a client feature graph or a stale _Body link.
    return next((body for body in obj.Document.Objects if body.isDerivedFrom("PartDesign::Body")
                 and obj in body.Group), None)


def transform_step(obj):
    return any(parent.isDerivedFrom("PartDesign::MultiTransform") and obj in parent.Transformations
               for parent in obj.InList)


def flags(obj, body):
    member = body is not None and obj != body and obj in body.Group
    # Match Body.execute: non-PD features and empty shapes fail
    # recomputation. A non-solid, nonempty native Feature is not rejected here.
    can_tip = (member and obj.isDerivedFrom("PartDesign::Feature") and hasattr(obj, "Shape")
               and not obj.Shape.isNull())
    # Match native MoveFeatureInTree selection and base-feature exclusion.
    can_reorder = (member and obj != body.BaseFeature and any(obj.isDerivedFrom(kind) for kind in
                   ("Part::Feature", "App::DatumElement", "App::LocalCoordinateSystem")))
    return {"can_tip": bool(can_tip), "can_reorder": bool(can_reorder),
            "transform_step": transform_step(obj)}


def set_tip(body, value):
    tip = None if value is None else object_in(body.Document, value)
    if tip is not None and tip not in body.Group:
        error("history_scope", "Tip must be a native member of the selected Body")
    if body.Tip == tip:
        return
    # Native Body.execute is the final eligibility authority after recompute.
    body.Tip = tip
    if tip is not None:
        tip.Visibility = True
        # Match native Set Tip -> SHOW -> PartDesign ViewProvider.onChanged.
        # The headless worker has no ViewProvider to hide sibling features.
        for feature in body.Group:
            if feature != tip and feature.isDerivedFrom("PartDesign::Feature"):
                feature.Visibility = False


def check_order(body):
    features = [obj for obj in body.Group if obj.isDerivedFrom("PartDesign::Feature")]
    order = {obj.Name: index for index, obj in enumerate(features)}
    for index, feature in enumerate(features):
        try:
            # Detect direct expression/link cycles too, before asking native
            # recomputation to traverse a candidate cyclic model.
            list(feature.OutListRecursive)
            # Native GUI MoveFeatureInTree dependency-order check. Direct PD
            # links include BaseFeature and owned MultiTransform steps; the
            # native Body methods already rewire the solid chain.
            for dependency in feature.OutList:
                if dependency.isDerivedFrom("PartDesign::Feature"):
                    continue
                for ancestor in [dependency, *dependency.OutListRecursive]:
                    if order.get(ancestor.Name, -1) > index:
                        error("history_dependency", feature.Name + " depends on later native feature " + ancestor.Name)
        except HistoryError:
            raise
        except Exception as exception:
            error("history_dependency", "Native dependency traversal rejected the reordered history: " + str(exception))


def apply(doc, edit):
    op = edit.get("op", edit.get("kind"))
    obj = object_in(doc, edit.get("object"))
    if op == "body_tip":
        if not obj.isDerivedFrom("PartDesign::Body") or "tip" not in edit:
            error("invalid_request", "Body Tip requires a native Body and explicit tip name or null")
        set_tip(obj, edit["tip"])
        return {"object": obj.Name, "tip": obj.Tip.Name if obj.Tip else None}
    if op != "body_reorder":
        error("unsupported_edit", "Unsupported native history operation: " + str(op))
    body = owner_body(obj)
    if body is None or not flags(obj, body)["can_reorder"]:
        error("history_scope", "Choose a movable native member of one Body")
    if "target" not in edit or type(edit.get("after")) is not bool:
        error("invalid_request", "Reorder requires an explicit target name or null and boolean after")
    target = None if edit["target"] is None else object_in(doc, edit["target"])
    if target == obj or target is not None and target not in body.Group:
        error("history_scope", "Reorder target must be another native member of the same Body")
    if "tip" in edit:
        tip = None if edit["tip"] is None else object_in(doc, edit["tip"])
        if tip is not None and tip not in body.Group:
            error("history_scope", "Final tip must remain in the reordered Body")
    body.removeObject(obj)
    # Native null target semantics: after=True inserts at beginning; False at end.
    body.insertObject(obj, target, edit["after"])
    check_order(body)
    if "tip" in edit:
        set_tip(body, edit["tip"])
    return {"object": obj.Name, "body": body.Name, "tip": body.Tip.Name if body.Tip else None}
