"""Pinned native Hole task metadata and controller-expression resolution."""

_enumerations = {}
_cut_angles = {}


def initialize(obj):
    # Called only from the worker's startup schema pass. Never create a native
    # document or touch a modeling transaction in response to a task query.
    original = str(obj.ThreadType)
    try:
        for standard in obj.getEnumerationsOfProperty("ThreadType"):
            obj.ThreadType = standard
            _enumerations[standard] = {
                field: list(obj.getEnumerationsOfProperty(field))
                for field in ("ThreadSize", "ThreadClass", "ThreadFit", "HoleCutType")
            }
            _cut_angles[standard] = {}
            for cut in obj.getEnumerationsOfProperty("HoleCutType"):
                # Native editor flags retain prior cut state for missing norm
                # entries. Probe each cut on a fresh startup-only object.
                probe = obj.Document.addObject("PartDesign::Hole", "HoleCutSchema")
                try:
                    probe.ThreadType = standard
                    probe.HoleCutType = cut
                    probe.HoleCutCustomValues = True
                    _cut_angles[standard][cut] = "ReadOnly" not in probe.getEditorMode("HoleCutCountersinkAngle")
                finally:
                    obj.Document.removeObject(probe.Name)
    finally:
        obj.ThreadType = original


def enumerations():
    return _enumerations


def cut_angles():
    return _cut_angles


def prepare_input(obj, field):
    if obj.TypeId == "PartDesign::Hole" and field == "ModelThread" and obj.Threaded and str(obj.ThreadType) != "None":
        # Match TaskHoleParameters::holeTypeChanged: refresh Threaded through
        # its native setter before ModelThread. Native restore can leave the
        # latter's editor flag stale. Never change a readonly/status flag here.
        obj.Threaded = True


def unchanged_forced_input(obj, field, value):
    # Missing preset table entries force this native input true and readonly.
    # A complete candidate may still contain the user's already-selected true.
    # Accept only that exact no-op; a false value still fails native admission.
    return (obj.TypeId == "PartDesign::Hole" and field == "HoleCutCustomValues"
            and value is True and obj.HoleCutCustomValues is True
            and ("ReadOnly" in obj.getEditorMode(field) or "ReadOnly" in obj.getPropertyStatus(field)))


def resolve_controller(obj, path):
    if obj.TypeId == "PartDesign::Hole" and path.lstrip(".").split(".")[0].split("[")[0] in {
            "ThreadType", "Threaded", "ThreadSize", "ThreadClass", "DepthType",
            "ThreadDepthType", "ModelThread", "UseCustomThreadClearance",
            "HoleCutType", "HoleCutCustomValues", "DrillPoint", "Tapered"}:
        # Binding does not immediately evaluate native expressions. Resolve this
        # Hole inside the existing transaction before the next dependent setter
        # checks its native editor lock. No state is published here. A partial
        # feature may still be invalid; the ordinary final whole-document
        # recompute/validation and rollback boundary decides acceptance.
        obj.recompute()


def restore_inputs(doc, path):
    """Restore only serialized Hole inputs corrupted by pinned native restore.

    Called inside guarded Open before publication. Never edits the archive,
    copies Shape, changes property status or creates a document/transaction.
    The caller owns failed-Open cleanup and final native validation.
    """
    restored_holes = [obj for obj in doc.Objects if obj.TypeId == "PartDesign::Hole"]
    holes = {obj.Name: obj for obj in restored_holes}
    if not holes:
        return
    import math
    import xml.etree.ElementTree as ET
    import zipfile

    def scalar(obj, node, field):
        prop = node.find("./Properties/Property[@name='" + field + "']")
        if prop is None or prop.get("type") != obj.getTypeIdOfProperty(field):
            raise ValueError("Missing or mismatched serialized Hole input: " + field)
        kind = prop.get("type")
        if kind == "App::PropertyEnumeration":
            index = int(prop.find("Integer").get("value"))
            saved = prop.find("CustomEnumList")
            choices = ([item.get("value") for item in saved] if saved is not None
                       else list(obj.getEnumerationsOfProperty(field)))
            if not 0 <= index < len(choices) or choices[index] not in obj.getEnumerationsOfProperty(field):
                raise ValueError("Unsupported serialized native Hole enum: " + field)
            return choices[index]
        if kind == "App::PropertyBool":
            value = prop.find("Bool").get("value")
            if value not in {"true", "false", "1", "0"}:
                raise ValueError("Invalid serialized Hole boolean")
            return value in {"true", "1"}
        if kind not in {"App::PropertyLength", "App::PropertyAngle"}:
            raise ValueError("Unsupported serialized Hole input type: " + kind)
        value = float(prop.find("Float").get("value"))
        if not math.isfinite(value):
            raise ValueError("Nonfinite serialized Hole input: " + field)
        return value

    def assign(obj, field, value):
        current = getattr(obj, field)
        current = current.Value if hasattr(current, "Value") else current
        if current == value:
            return
        if "ReadOnly" in obj.getEditorMode(field) or "ReadOnly" in obj.getPropertyStatus(field):
            raise ValueError("Native Hole restore input is read-only: " + field)
        setattr(obj, field, value)

    with zipfile.ZipFile(path) as archive, archive.open("Document.xml") as stream:
        for _, node in ET.iterparse(stream, events=("end",)):
            if node.tag != "Object":
                continue
            obj = holes.get(node.get("name"))
            if obj is not None and node.find("Properties") is not None:
                size = scalar(obj, node, "ThreadSize")
                custom = scalar(obj, node, "HoleCutCustomValues")
                values = {field: scalar(obj, node, field) for field in (
                    "HoleCutCountersinkAngle", "HoleCutDiameter", "HoleCutDepth")}
                # ThreadType's restore callback can select a closest size from
                # stale ThreadDiameter. Reapply the exact saved enum label.
                assign(obj, "ThreadSize", size)
                if custom:
                    # The constructor's temporary readonly flag can reset this
                    # saved input and replace custom dimensions with a preset.
                    assign(obj, "HoleCutCustomValues", True)
                    for field, value in values.items():
                        assign(obj, field, value)
                holes.pop(obj.Name)
            node.clear()
    if holes:
        raise ValueError("Serialized Hole inputs are missing from the native document")
    # Matching scalars do not make a saved BRep authoritative. Invalidate every
    # restored Hole, after all inputs have been restored. The guarded caller's
    # whole-document native recompute orders dependencies and validates the
    # regenerated results before publishing any document state.
    for obj in restored_holes:
        obj.touch()
