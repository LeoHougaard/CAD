"""Pinned FreeCAD Assembly authoring. No local solver or copied component shapes."""
import math
import re
import FreeCAD as App
import Assembly
import UtilsAssembly

OPERATIONS = {"assembly_create", "assembly_insert", "assembly_placement", "assembly_ground",
              "assembly_joint", "assembly_remove", "assembly_solve"}
# The pinned task interface, also declared by Rust's assembly_commands. Loading
# the shipped joint proxies imports Qt and Coin; ordinary Part Studios and Link
# display do not need those modules. Actual joint creation still uses the native
# module's types, connector preparation and solver unchanged.
JOINT_TYPES = ("Fixed", "Revolute", "Cylindrical", "Slider", "Ball", "Distance",
               "Parallel", "Perpendicular", "Angle", "RackPinion", "Screw", "Gears", "Belt")


def joint_module():
    import JointObject
    return JointObject
JOINT_PROPERTIES = {"Offset1", "Offset2", "Distance", "Distance2", "Angle", "Suppressed",
                    "EnableLengthMin", "EnableLengthMax", "LengthMin", "LengthMax",
                    "EnableAngleMin", "EnableAngleMax", "AngleMin", "AngleMax"}


class AssemblyError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def reject(message, code="invalid_assembly"):
    raise AssemblyError(code, message)


def identifier(value):
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", value):
        reject("Choose a native object name")
    return value


def object_in(doc, value):
    obj = doc.getObject(identifier(value))
    if obj is None:
        reject("Native assembly object is absent: " + value)
    return obj


def unique(doc, value):
    value = identifier(value)
    if doc.getObject(value):
        reject("Object name already exists: " + value, "name_conflict")
    return value


def assemblies(doc):
    return [obj for obj in doc.Objects if obj.TypeId == "Assembly::AssemblyObject"]


def owner(obj):
    for assembly in assemblies(obj.Document):
        if obj in assembly.Group or obj in joint_group(assembly).Group:
            return assembly
    return None


def joint_group(assembly):
    groups = [obj for obj in assembly.Group if obj.TypeId == "Assembly::JointGroup"]
    if len(groups) != 1:
        reject("Assembly requires exactly one native JointGroup")
    return groups[0]


def validate_proxies(doc):
    # Identify the pinned serialized properties, not names or connector values.
    # Do not load Qt/Coin for Parts, Links, empty assemblies or unrelated proxies.
    required = []
    for obj in doc.Objects:
        if "Proxy" not in obj.PropertiesList:
            continue
        properties = obj.PropertiesList
        if (all(prop in properties for prop in ("JointType", "Reference1", "Reference2"))
                and obj.getTypeIdOfProperty("JointType") == "App::PropertyEnumeration"
                and all(obj.getTypeIdOfProperty(prop) == "App::PropertyXLinkSub" for prop in ("Reference1", "Reference2"))
                and obj.getGroupOfProperty("JointType") == "Joint"):
            required.append((obj, "Joint"))
        elif ("ObjectToGround" in properties
                and obj.getTypeIdOfProperty("ObjectToGround") in ("App::PropertyLinkGlobal", "App::PropertyLink")
                and obj.getGroupOfProperty("ObjectToGround") == "Ground"):
            required.append((obj, "GroundedJoint"))
    if not required:
        return
    try:
        native = joint_module()
    except Exception as error:
        reject("Native Assembly proxy runtime is unavailable: " + str(error)
               + ". Close and reopen the document after repairing the runtime.", "assembly_proxy_restore")
    for obj, kind in required:
        if not isinstance(obj.Proxy, getattr(native, kind)):
            reject(obj.Name + ": shipped native " + kind + " proxy did not restore. "
                   "Close and reopen the original document after repairing the runtime.", "assembly_proxy_restore")


def local_instance(assembly, value):
    obj = object_in(assembly.Document, value)
    if obj.TypeId != "App::Link" or obj not in assembly.Group:
        reject("Choose a component instance in this assembly")
    source = obj.LinkedObject
    if source is None or source.Document != assembly.Document:
        reject("External or missing component references are preserved but cannot be authored here")
    return obj


def placement(value):
    if not isinstance(value, dict) or set(value) != {"position", "rotation"}:
        reject("Placement requires position and quaternion rotation")
    p, q = value["position"], value["rotation"]
    if not isinstance(p, list) or len(p) != 3 or not isinstance(q, list) or len(q) != 4:
        reject("Placement requires three position and four quaternion values")
    if any(isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v) for v in p + q):
        reject("Placement values must be finite")
    if sum(v * v for v in q) < 1e-24:
        reject("Placement quaternion cannot be zero")
    return App.Placement(App.Vector(*p), App.Rotation(*q))


def reference(assembly, value):
    obj = local_instance(assembly, value["link"])
    subs = value.get("subelements")
    if not isinstance(subs, list) or len(subs) != 2 or any(not isinstance(s, str) for s in subs):
        reject("Joint connectors require native element and origin/vertex identities")
    if bool(subs[0]) != bool(subs[1]):
        reject("Choose both connector elements, or the whole component origin")
    for sub in subs:
        if sub:
            # Resolve through the native link, retaining its native persistent names.
            if "." in sub or "?" in sub:
                reject("Choose a direct face, edge or vertex of this instance")
            try:
                element = obj.Shape.getElement(sub)
            except (IndexError, ValueError):
                reject("Connector element does not exist: " + sub)
            if element is None or element.isNull():
                reject("Connector element does not exist: " + sub)
    return (obj, subs)


def solve_document(doc):
    """Only called at native recomputation boundaries, never from read requests."""
    solved = False
    for assembly in reversed(assemblies(doc)):
        joints = [j for j in joint_group(assembly).Group if hasattr(j, "JointType") and not j.Suppressed]
        for joint in joints:
            for prop in ("Reference1", "Reference2"):
                ref = getattr(joint, prop)
                if ref is None or ref[0] is None or not ref[1] or any("?" in sub for sub in ref[1]):
                    reject(joint.Name + ": broken native connector reference")
        if not joints:
            continue
        code = assembly.solve()
        solved = True
        if code != 0:
            reject(assembly.Name + ": native Assembly solver returned " + str(code), "assembly_solver")
    return doc.recompute() if solved else True


def metadata(obj):
    if obj.TypeId == "Assembly::AssemblyObject":
        group = joint_group(obj)
        return {"assembly_state": {"members": [x.Name for x in obj.Group if x not in group.Group],
                                   "joints": [x.Name for x in group.Group],
                                   "joint_types": list(JOINT_TYPES)}}
    assembly = owner(obj)
    if assembly is None:
        return {}
    data = {"assembly": assembly.Name}
    if obj.TypeId == "App::Link":
        source = obj.LinkedObject
        data["instance_state"] = {"source": None if source is None else source.Name,
                                  "source_document": None if source is None else source.Document.Name,
                                  "source_path": None if source is None else source.Document.FileName,
                                  "local": source is not None and source.Document == obj.Document,
                                  "grounded": bool(assembly.isPartGrounded(obj))}
    return data


def link_global_placement(obj):
    # App::Link does not expose GeoFeature.getGlobalPlacement. Use the shipped
    # Assembly reference traversal, which includes enclosing native Parts.
    return UtilsAssembly.getGlobalPlacement((obj, [""]), obj)


def link_world_from_shape(obj):
    return link_global_placement(obj).multiply(obj.Placement.inverse())


def apply(doc, edit):
    op = edit["op"]
    if op == "assembly_create":
        key = unique(doc, edit["name"])
        group_key = unique(doc, edit["joints"])
        if key == group_key:
            reject("Assembly and JointGroup require different names")
        assembly = doc.addObject("Assembly::AssemblyObject", key)
        assembly.Type = "Assembly"
        assembly.newObject("Assembly::JointGroup", group_key)
        return {"object": key}
    assembly = object_in(doc, edit["assembly"])
    if assembly.TypeId != "Assembly::AssemblyObject":
        reject("Choose a native Assembly container")
    group = joint_group(assembly)
    if op == "assembly_insert":
        key = unique(doc, edit["name"])
        source = object_in(doc, edit["source"])
        if source.TypeId == "App::Link":
            source = source.getLinkedObject(True)
        if source.Document != doc or source == assembly or assembly in source.OutListRecursive:
            reject("Component must be a local document source without an assembly dependency")
        if not (source.isDerivedFrom("Part::Feature") or source.TypeId == "PartDesign::Body"):
            reject("Choose a local native Part feature or Body as the component source")
        if not hasattr(source, "Shape") or source.Shape.isNull():
            reject("Component source requires a native shape")
        link = assembly.newObject("App::Link", key)
        link.setLink(source)
        if "placement" in edit:
            link.LinkPlacement = placement(edit["placement"])
        return {"object": key, "source": source.Name}
    if op == "assembly_solve":
        code = assembly.solve()
        if code != 0:
            reject("Native Assembly solver returned " + str(code), "assembly_solver")
        return {"object": assembly.Name, "solver_code": code}
    if op == "assembly_placement":
        obj = local_instance(assembly, edit["object"])
        obj.LinkPlacement = placement(edit["placement"])
        return {"object": obj.Name}
    if op == "assembly_ground":
        obj = local_instance(assembly, edit["object"])
        grounds = [j for j in group.Group if getattr(j, "ObjectToGround", None) == obj]
        if type(edit["grounded"]) is not bool:
            reject("Grounded must be a boolean")
        if edit["grounded"] and not grounds:
            JointObject = joint_module()
            ground = group.newObject("App::FeaturePython", unique(doc, edit["name"]))
            JointObject.GroundedJoint(ground, obj)
        elif not edit["grounded"]:
            for ground in grounds:
                doc.removeObject(ground.Name)
        return {"object": obj.Name, "grounded": edit["grounded"]}
    if op == "assembly_joint":
        JointObject = joint_module()
        kind = edit["joint_type"]
        if kind not in JointObject.JointTypes:
            reject("Unsupported native joint type")
        refs = [reference(assembly, edit[k]) for k in ("reference1", "reference2")]
        if refs[0][0] == refs[1][0]:
            reject("Choose two distinct component instances")
        key = identifier(edit["name"])
        if edit["create"]:
            joint = group.newObject("App::FeaturePython", unique(doc, key))
            JointObject.Joint(joint, JointObject.JointTypes.index(kind))
        else:
            joint = object_in(doc, key)
            if joint not in assembly.Joints or not isinstance(joint.Proxy, JointObject.Joint):
                reject("Choose a native joint in this assembly")
            joint.Proxy.setJointType(joint, kind)
        properties = edit.get("properties", {})
        if not isinstance(properties, dict) or set(properties) - JOINT_PROPERTIES:
            reject("Unsupported joint task property")
        # The shipped proxy owns connector placement, presolve and solver calls.
        # Assign both native references before task properties so offsets are live.
        joint.Proxy.setJointConnectors(joint, refs)
        for prop, value in properties.items():
            setattr(joint, prop, placement(value) if prop.startswith("Offset") else value)
        for prop, expression in edit.get("expressions", {}).items():
            if prop.split(".")[0] not in JOINT_PROPERTIES:
                reject("Unsupported joint expression property")
            joint.setExpression(prop, expression)
        return {"object": key}
    if op == "assembly_remove":
        obj = object_in(doc, edit["object"])
        if obj not in assembly.Group and obj not in group.Group:
            reject("Choose a member or joint of this assembly")
        if obj.TypeId == "Assembly::JointGroup":
            reject("The assembly's native JointGroup must remain")
        dependents = [x.Name for x in obj.InList if x not in (assembly, group)]
        if dependents:
            reject("Remove dependent native joints or references first: " + ", ".join(dependents), "dependent_objects")
        doc.removeObject(obj.Name)
        return {"object": edit["object"], "removed": True}
    reject("Unknown assembly operation")
