//! Analytical construction planes, independent of solid-face provenance.
use super::*;
use crate::model::PlaneReference;

pub(super) fn source_face_key(key: &str) -> &str {
    key.rsplit_once(":patch:").filter(|(_, hash)|
        hash.len() == 16 && hash.bytes().all(|c| c.is_ascii_hexdigit()))
        .map_or(key, |(source, _)| source)
}

impl Geometry {
    pub(super) fn face_plane_frame(&self, owner: &str, id: FaceId, key: &str) -> Result<Plane> {
        let sources = &self.parts[self.target(owner)?].reference_sources;
        let face = self.topo.face(id).map_err(err)?;
        let FaceSurface::Plane { normal, d } = face.surface() else {
            return Err("Choose a planar face".into());
        };
        let sign = if face.is_reversed() { -1. } else { 1. };
        let n = [normal.x() * sign, normal.y() * sign, normal.z() * sign];
        // The caller already resolved this owned face. Its deterministic patch
        // suffix distinguishes boundaries, while the source retains sketch axes.
        let source = source_face_key(key);
        let mut plane = self.anchors.iter().find_map(|(k, p, scope)| {
            (sources.contains(scope) && reference_matches(k, source)
                && plane_matches(p, *normal, *d)).then(|| p.clone())
        }).unwrap_or_else(|| {
            let seed = if n[0].abs() < 0.8 { [1., 0., 0.] } else { [0., 1., 0.] };
            let projected = sub(seed, n.map(|x| x * dot(seed, n)));
            let u = projected.map(|x| x / dot(projected, projected).sqrt());
            Plane { origin: n.map(|x| x * d * sign), u, v: cross(n, u), normal: n }
        });
        // Cuts reverse tool faces. Preserve the source U axis and origin while
        // orienting both sketch and direct-face operations out of the material.
        if dot(plane.normal, n) < 0. {
            plane.normal = plane.normal.map(|x| -x);
            plane.v = plane.v.map(|x| -x);
        }
        plane.validate()?;
        Ok(plane)
    }

    pub fn resolve_plane_reference(&self, reference: &PlaneReference) -> Result<Plane> {
        reference.validate()?;
        match reference {
            PlaneReference::Origin { plane } => Ok(Plane::named(plane, 0.)),
            PlaneReference::Datum { feature } => {
                self.planes.get(feature).cloned().ok_or_else(|| {
                    "Construction plane is missing, suppressed or later in the timeline".into()
                })
            }
            PlaneReference::Face { part, face: key } => {
                let (id, key) = self.owned_face_reference(part, key)?;
                self.face_plane_frame(part, id, &key)
            }
        }
    }

    pub(super) fn execute_plane(&mut self, feature: &Feature, p: &Value) -> Result<()> {
        let resolve = |key: &str| {
            let reference: PlaneReference = serde_json::from_value(p[key].clone())
                .map_err(|_| format!("Choose a valid {key} plane"))?;
            self.resolve_plane_reference(&reference)
        };
        let base = resolve("reference")?;
        let mut result = match text(p, "mode", "Offset") {
            "Offset" => base.shifted(num(p, "offset", 10.)),
            "Midplane" => {
                let other = resolve("second")?;
                let delta = sub(other.origin, base.origin);
                let intersection = cross(base.normal, other.normal);
                if dot(intersection, intersection) < 1e-20 {
                    base.shifted(dot(delta, base.normal) / 2.)
                } else {
                    let sign = if flag(p, "flip_alignment") { -1. } else { 1. };
                    let w = std::array::from_fn(|i| base.normal[i] + sign * other.normal[i]);
                    let square = dot(w, w);
                    let normal = w.map(|x| x / square.sqrt());
                    let distance = sign * dot(other.normal, delta) / square;
                    let origin = std::array::from_fn(|i| base.origin[i] + w[i] * distance);
                    let mut projected = sub(base.u, normal.map(|x| x * dot(base.u, normal)));
                    if dot(projected, projected) < 1e-12 {
                        projected = sub(base.v, normal.map(|x| x * dot(base.v, normal)));
                    }
                    let u = projected.map(|x| x / dot(projected, projected).sqrt());
                    Plane {
                        origin,
                        u,
                        v: cross(normal, u),
                        normal,
                    }
                }
            }
            "Angle" => {
                if p.get("axis_ref").is_none_or(Value::is_null) {
                    return Err("Choose an axis in the reference plane".into());
                }
                let (origin, axis) = self.resolve_in_plane_axis(p, &base)?;
                let angle = (num(p, "angle", 45.) % 360.).to_radians();
                let rotate = |v: Vec3| {
                    v * angle.cos()
                        + axis.cross(v) * angle.sin()
                        + axis * (axis.dot(v) * (1. - angle.cos()))
                };
                let rotated_origin = origin + rotate(p3(base.origin) - origin);
                let array = |v: Vec3| [v.x(), v.y(), v.z()];
                Plane {
                    origin: [rotated_origin.x(), rotated_origin.y(), rotated_origin.z()],
                    u: array(rotate(v3(base.u))),
                    v: array(rotate(v3(base.v))),
                    normal: array(rotate(v3(base.normal))),
                }
            }
            _ => return Err("Choose Offset, Midplane or Angle".into()),
        };
        if flag(p, "flip_normal") {
            result.normal = result.normal.map(|x| -x);
            result.v = result.v.map(|x| -x);
        }
        result.validate()?;
        self.planes.insert(feature.id.clone(), result);
        Ok(())
    }
}
