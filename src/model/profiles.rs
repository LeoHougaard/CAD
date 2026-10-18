//! One typed input for exact planar faces and sketch profiles.
use super::{Result, Value};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProfileReference {
    Region { sketch: String, region: String },
    Face { part: String, face: String },
    Sketch { sketch: String },
}

impl ProfileReference {
    pub fn source_key(&self) -> String {
        match self {
            Self::Face { part, face } => {
                format!("face:{part}:{}", super::canonical_reference(face))
            }
            Self::Region { sketch, .. } | Self::Sketch { sketch } => format!("sketch:{sketch}"),
        }
    }
    pub fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Region {
                    sketch: a,
                    region: x,
                },
                Self::Region {
                    sketch: b,
                    region: y,
                },
            ) => a == b && super::reference_matches(x, y),
            (Self::Face { part: a, face: x }, Self::Face { part: b, face: y }) => {
                a == b && super::reference_matches(x, y)
            }
            (Self::Sketch { sketch: a }, Self::Sketch { sketch: b }) => a == b,
            _ => false,
        }
    }
    fn canonical(&self) -> Self {
        match self {
            Self::Region { sketch, region } => Self::Region {
                sketch: sketch.clone(),
                region: super::canonical_reference(region),
            },
            Self::Face { part, face } => Self::Face {
                part: part.clone(),
                face: super::canonical_reference(face),
            },
            Self::Sketch { sketch } => Self::Sketch {
                sketch: sketch.clone(),
            },
        }
    }
    pub fn validate(&self) -> Result<()> {
        let text = |s: &str, limit| !s.is_empty() && s != "@last" && s.len() <= limit;
        let valid = match self {
            Self::Region { sketch, region } => text(sketch, 128) && text(region, 8192),
            Self::Face { part, face } => {
                text(part, 1024) && text(face, 8192) && !face.contains("untracked:")
            }
            Self::Sketch { sketch } => text(sketch, 128),
        };
        if valid {
            Ok(())
        } else {
            Err("Choose a persistent planar face or earlier sketch region".into())
        }
    }
    /// Null/missing unified inputs retain each pinned document's legacy meaning.
    pub fn from_params(params: &Value) -> Result<Vec<Self>> {
        let items = if let Some(value) = params.get("profiles").filter(|v| !v.is_null()) {
            serde_json::from_value::<Vec<Self>>(value.clone()).map_err(|_| {
                "Profiles must be a list of planar faces or sketch regions".to_owned()
            })?
        } else if let Some(face) = params.get("profile_face").filter(|v| !v.is_null()) {
            vec![Self::Face {
                part: face["part"].as_str().unwrap_or_default().into(),
                face: face["face"].as_str().unwrap_or_default().into(),
            }]
        } else if let Some(sketch) = params["sketch"].as_str().filter(|s| !s.is_empty()) {
            match params.get("regions") {
                None | Some(Value::Null) => vec![Self::Sketch {
                    sketch: sketch.into(),
                }],
                Some(Value::Array(regions)) => regions
                    .iter()
                    .map(|r| {
                        Ok(Self::Region {
                            sketch: sketch.into(),
                            region: r
                                .as_str()
                                .ok_or("Sketch region selections must be identities")?
                                .into(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
                _ => return Err("Sketch region selections must be an array or null".into()),
            }
        } else {
            vec![]
        };
        if items.len() > 4096 {
            return Err("Choose at most 4096 profiles".into());
        }
        let mut unique = std::collections::BTreeSet::new();
        for item in &items {
            item.validate()?;
            if !unique.insert(item.canonical()) {
                return Err("A profile was selected more than once".into());
            }
        }
        Ok(items)
    }
    pub fn sketch(&self) -> Option<&str> {
        match self {
            Self::Region { sketch, .. } | Self::Sketch { sketch } => Some(sketch),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn profile_aliases_keep_owners_exact_and_reject_duplicate_physical_inputs() {
        let a = ProfileReference::Region {
            sketch: "f1".into(),
            region: "region:+a2[a2:1,l1:1,xa2&l1#0>a2:2,l3:0]".into(),
        };
        let b = ProfileReference::Region {
            sketch: "f1".into(),
            region: "region:+a2[a2:1,l1:1>a2:2,l3:0]".into(),
        };
        assert!(a.matches(&b));
        assert!(ProfileReference::from_params(&json!({"profiles":[a.clone(),b.clone()]})).is_err());
        let other = ProfileReference::Region {
            sketch: "f2".into(),
            region: match &b {
                ProfileReference::Region { region, .. } => region.clone(),
                _ => unreachable!(),
            },
        };
        assert!(!a.matches(&other));
        assert_eq!(
            ProfileReference::from_params(&json!({"profiles":[a,other]}))
                .unwrap()
                .len(),
            2
        );
    }
}
