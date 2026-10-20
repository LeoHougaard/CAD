//! Logical references belong to a placed occurrence, independently of topology IDs.
use super::*;

fn vector(matrix: &Mat4, value: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| dot([matrix.0[i][0], matrix.0[i][1], matrix.0[i][2]], value))
}

/// Only explicit instance wrappers have aliases; ordinary key suffixes never do.
fn instance_source(key: &str) -> Option<&str> {
    let (feature, rest) = key.split_once(":instance:")?;
    let (ordinal, source) = rest.split_once("::")?;
    (!feature.is_empty()
        && !feature.contains(':')
        && !ordinal.is_empty()
        && ordinal.bytes().all(|c| c.is_ascii_digit())
        && !source.is_empty())
    .then_some(source)
}

pub(super) fn reference_alias(actual: &str, requested: &str) -> bool {
    if actual.contains(" | ") || requested.contains(" | ") {
        let actual: Vec<_> = actual.split(" | ").collect();
        let requested: Vec<_> = requested.split(" | ").collect();
        return actual.len() == requested.len()
            && requested.iter().all(|requested| {
                actual
                    .iter()
                    .any(|actual| reference_alias(actual, requested))
            });
    }
    alias_depth(actual, requested).is_some()
}

fn alias_depth(actual: &str, requested: &str) -> Option<usize> {
    let mut key = actual;
    let mut depth = 0;
    loop {
        if reference_matches(key, requested) {
            return Some(depth);
        }
        key = instance_source(key)?;
        depth += 1;
    }
}

pub(super) fn reflection(plane: &Plane) -> Mat4 {
    let mut matrix = Mat4::identity();
    let d = dot(plane.origin, plane.normal);
    for i in 0..3 {
        for j in 0..3 {
            matrix.0[i][j] -= 2. * plane.normal[i] * plane.normal[j];
        }
        matrix.0[i][3] = 2. * d * plane.normal[i];
    }
    matrix
}

impl Geometry {
    /// Append immutable reference records for a rigid placement or reflection.
    /// In-place operations keep logical keys; copies introduce an occurrence.
    pub(super) fn place_references(
        &mut self,
        sources: &BTreeSet<String>,
        matrix: &Mat4,
        scope: &str,
        copy: bool,
    ) -> Result<BTreeSet<String>> {
        let placed: BTreeMap<_, _> = sources
            .iter()
            .map(|s| (s.as_str(), format!("{scope}:placement::{s}")))
            .collect();
        let key = |key: &str| {
            if copy {
                format!("{scope}::{key}")
            } else {
                key.into()
            }
        };
        let mut anchors = vec![];
        for (logical, plane, source) in self.anchors.iter().filter(|(_, _, s)| sources.contains(s))
        {
            let origin = matrix.mul_point(p3(plane.origin));
            let u = vector(matrix, plane.u);
            let normal = vector(matrix, plane.normal);
            // Reflections change handedness; keep U and the material normal.
            let frame = Plane {
                origin: [origin.x(), origin.y(), origin.z()],
                u,
                v: cross(normal, u),
                normal,
            };
            frame.validate()?;
            anchors.push((key(logical), frame, placed[source.as_str()].clone()));
        }
        let place_edge = |a: &EdgeAnchor| {
                let mut a = a.clone();
                a.source = placed[a.source.as_str()].clone();
                a.key = key(&a.key);
                a.side = a.side.as_deref().map(key);
                a.start = matrix.mul_point(a.start);
                a.end = matrix.mul_point(a.end);
                a.midpoint = matrix.mul_point(a.midpoint);
                a.circle = a.circle.map(|(center, normal, radius, full)| {
                    (
                        matrix.mul_point(center),
                        v3(vector(matrix, [normal.x(), normal.y(), normal.z()])),
                        radius,
                        full,
                    )
                });
                a
            };
        let edges = self
            .edge_anchors
            .iter()
            .filter(|a| sources.contains(&a.source))
            .map(place_edge)
            .collect::<Vec<_>>();
        let generated = self.generated_faces.iter().filter(|a| sources.contains(&a.source))
            .map(|a| Ok(generated_references::GeneratedFace {
                key: key(&a.key), source: placed[a.source.as_str()].clone(),
                surface: generated_references::transformed_surface(&a.surface, matrix)?,
                boundary: a.boundary.iter().map(place_edge).collect(),
                frame: a.frame.as_ref().map(|p| {
                    let origin = matrix.mul_point(p3(p.origin));
                    let u = vector(matrix,p.u);
                    let normal = vector(matrix,p.normal);
                    Plane { origin:[origin.x(),origin.y(),origin.z()], u, v:cross(normal,u), normal }
                }),
            })).collect::<Result<Vec<_>>>()?;
        self.anchors.extend(anchors);
        self.edge_anchors.extend(edges);
        self.generated_faces.extend(generated);
        Ok(placed.into_values().collect())
    }

    /// Prefer exact identities. Older copied-face keys may use a logical alias
    /// only when this owner contains a single matching occurrence.
    pub(super) fn owned_face_reference(&self, owner: &str, key: &str) -> Result<(FaceId, String)> {
        let index = self.target(owner)?;
        let part = &self.parts[index];
        let available = self.face_keys_scoped(part.solid, Some(&part.key))?;
        let exact: Vec<_> = available
            .iter()
            .filter(|(_, actual)| reference_matches(actual, key))
            .collect();
        if exact.len() == 1 {
            let (id, key) = exact[0];
            return Ok((*id, key.clone()));
        }
        if !exact.is_empty() {
            return Err("Face reference is ambiguous; reselect it".into());
        }
        let aliases: Vec<_> = available
            .iter()
            .filter(|(_, actual)| reference_alias(actual, key))
            .collect();
        if aliases.len() != 1 {
            return Err("Face reference is missing or ambiguous; reselect it".into());
        }
        let (id, actual) = aliases[0];
        let namespaces = self
            .anchors
            .iter()
            .filter(|(logical, plane, source)| {
                part.reference_sources.contains(source)
                    && reference_alias(logical, key)
                    && self.topo.face(*id).is_ok_and(|face| match face.surface() {
                        FaceSurface::Plane { normal, d } => plane_matches(plane, *normal, *d),
                        _ => false,
                    })
            })
            .filter_map(|(logical, _, _)| {
                alias_depth(logical, key).map(|depth| (depth, canonical_reference(logical)))
            });
        let generated_namespaces = self.generated_faces.iter().filter(|r|
            part.reference_sources.contains(&r.source) && reference_alias(&r.key, key)
                && r.frame.as_ref().is_some_and(|plane|self.topo.face(*id).is_ok_and(|face|match face.surface(){
                    FaceSurface::Plane {normal,d}=>plane_matches(plane,*normal,*d),_=>false})))
            .filter_map(|r|alias_depth(&r.key,key).map(|depth|(depth,canonical_reference(&r.key))));
        let namespaces:Vec<_> = namespaces.chain(generated_namespaces).collect();
        // A new outer occurrence preserves its prior logical namespace. Prefer
        // that exact prior namespace over aliases through additional ancestors.
        let depth = namespaces.iter().map(|(depth, _)| *depth).min();
        let namespaces: BTreeSet<_> = namespaces
            .iter()
            .filter(|(d, _)| Some(*d) == depth)
            .map(|(_, key)| key)
            .collect();
        if namespaces.len() != 1 || alias_depth(actual, key) != depth {
            return Err("Face alias has multiple placed owners; reselect it".into());
        }
        Ok((*id, actual.clone()))
    }
}
