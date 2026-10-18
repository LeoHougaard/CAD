//! Ordered, additive selection shared by the tree, viewport and feature tools.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Plane(String),
    Feature(String),
    Part(String),
    Face { part: String, key: String },
    Edge { part: String, key: String },
    Region { sketch: String, key: String },
    SketchEntity { sketch: String, entity: u64, edge: usize },
}

impl Target {
    pub fn matches(&self, other: &Self) -> bool {
        use crate::model::reference_matches;
        match (self, other) {
            (Self::Face { part: a, key: x }, Self::Face { part: b, key: y })
            | (Self::Edge { part: a, key: x }, Self::Edge { part: b, key: y }) => {
                a == b && reference_matches(x, y)
            }
            (Self::Region { sketch: a, key: x }, Self::Region { sketch: b, key: y }) => {
                a == b && reference_matches(x, y)
            }
            _ => self == other,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Faces,
    Edges,
    Parts,
    Regions,
}
impl Filter {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::Faces,
        Self::Edges,
        Self::Parts,
        Self::Regions,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Faces => "Faces",
            Self::Edges => "Edges",
            Self::Parts => "Parts",
            Self::Regions => "Sketch regions",
        }
    }
}

#[derive(Default)]
pub struct Selection {
    items: Vec<Target>,
}
impl Selection {
    pub fn items(&self) -> &[Target] {
        &self.items
    }
    pub fn contains(&self, item: &Target) -> bool {
        self.items.iter().any(|x| x.matches(item))
    }
    pub fn toggle(&mut self, item: Target) {
        if let Some(index) = self.items.iter().position(|x| x.matches(&item)) {
            self.items.remove(index);
        } else {
            self.items.push(item);
        }
    }
    pub fn add(&mut self, item: Target) {
        if !self.contains(&item) {
            self.items.push(item);
        }
    }
    pub fn remove(&mut self, item: &Target) {
        self.items.retain(|x| !x.matches(item));
    }
    pub fn clear(&mut self) {
        self.items.clear();
    }
    pub fn retain(&mut self, f: impl FnMut(&Target) -> bool) {
        self.items.retain(f);
    }
    pub fn part(&self) -> Option<&str> {
        self.items.iter().rev().find_map(|x| match x {
            Target::Part(part) | Target::Face { part, .. } | Target::Edge { part, .. } => {
                Some(part.as_str())
            }
            _ => None,
        })
    }
    pub fn faces(&self) -> impl Iterator<Item = (&str, &str)> {
        self.items.iter().filter_map(|x| match x {
            Target::Face { part, key } => Some((part.as_str(), key.as_str())),
            _ => None,
        })
    }
    pub fn edges(&self) -> impl Iterator<Item = (&str, &str)> {
        self.items.iter().filter_map(|x| match x {
            Target::Edge { part, key } => Some((part.as_str(), key.as_str())),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clicks_accumulate_and_toggle_without_modifiers() {
        let mut s = Selection::default();
        let a = Target::Face {
            part: "p".into(),
            key: "a".into(),
        };
        let b = Target::Face {
            part: "p".into(),
            key: "b".into(),
        };
        s.toggle(a.clone());
        s.toggle(b.clone());
        assert_eq!(s.items(), &[a.clone(), b.clone()]);
        s.toggle(a);
        assert_eq!(s.items(), &[b]);
        s.clear();
        assert!(s.items().is_empty());
    }
    #[test]
    fn tree_part_and_face_selection_are_distinct_and_ordered() {
        let mut s = Selection::default();
        s.add(Target::Part("one".into()));
        s.add(Target::Edge {
            part: "two".into(),
            key: "edge".into(),
        });
        assert_eq!(s.part(), Some("two"));
        assert_eq!(s.edges().collect::<Vec<_>>(), vec![("two", "edge")]);
        s.retain(|x| matches!(x, Target::Part(_)));
        assert_eq!(s.part(), Some("one"));
    }
    #[test]
    fn saved_contact_aliases_toggle_without_duplicates_or_cross_part_matches() {
        let old = "a2[a2:1,l1:1>a2:2,l3:0]";
        let new = "a2[a2:1,l1:1,xa2&l1#0>a2:2,l3:0]";
        for kind in 0..3 {
            let target = |owner: &str, key: &str| match kind {
                0 => Target::Region {
                    sketch: owner.into(),
                    key: key.into(),
                },
                1 => Target::Face {
                    part: owner.into(),
                    key: key.into(),
                },
                _ => Target::Edge {
                    part: owner.into(),
                    key: key.into(),
                },
            };
            let mut selection = Selection::default();
            selection.add(target("one", old));
            selection.add(target("one", new));
            assert_eq!(selection.items().len(), 1);
            assert!(!selection.contains(&target("two", new)));
            selection.toggle(target("one", new));
            assert!(selection.items().is_empty());
            selection.add(target("one", new));
            selection.remove(&target("one", old));
            assert!(selection.items().is_empty());
        }
    }
}
