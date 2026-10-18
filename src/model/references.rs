//! Persistent identity ignores an intersection tag already identified by both endpoints.
/// Keep genuine crossings, split branches and curve identities distinct. This
/// accepts older keys whose redundant endpoint intersection came and went with
/// floating-point roundoff, without repairing references by spatial proximity.
pub fn reference_matches(a: &str, b: &str) -> bool {
    a == b
        || (a.split_once('[').map(|p| p.0) == b.split_once('[').map(|p| p.0)
            && a.contains('[')
            && b.contains('[')
            && canonical_reference(a) == canonical_reference(b))
}
pub fn canonical_reference(key: &str) -> String {
    let mut result = String::with_capacity(key.len());
    let mut junction = false;
    for part in key.split_inclusive(['[', '>', ']']) {
        let delimiter = part
            .as_bytes()
            .last()
            .copied()
            .filter(|c| matches!(c, b'[' | b'>' | b']'));
        let body = if delimiter.is_some() {
            &part[..part.len() - 1]
        } else {
            part
        };
        if junction {
            let labels: Vec<_> = body.split(',').collect();
            let endpoint = |source: &str| {
                labels.iter().any(|label| {
                    label
                        .strip_prefix(source)
                        .and_then(|s| s.strip_prefix(':'))
                        .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                })
            };
            let redundant = |label: &str| {
                label
                    .strip_prefix('x')
                    .and_then(|s| s.rsplit_once('#'))
                    .filter(|(_, n)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                    .and_then(|(sources, _)| sources.split_once('&'))
                    .is_some_and(|(a, b)| endpoint(a) && endpoint(b))
            };
            result.push_str(
                &labels
                    .iter()
                    .copied()
                    .filter(|s| !redundant(s))
                    .collect::<Vec<_>>()
                    .join(","),
            );
        } else {
            result.push_str(body);
        }
        if let Some(c) = delimiter {
            result.push(char::from(c));
            if c == b'[' {
                junction = true;
            } else if c == b']' {
                junction = false;
            }
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redundant_contact_labels_do_not_change_the_same_reference() {
        for prefix in ["region:+a2", "f2:region-side:+a2"] {
            let a = format!("{prefix}[a2:1,l1:1,xa2&l1#0>a2:2,l3:0] | f2:end");
            let b = format!("{prefix}[a2:1,l1:1>a2:2,l3:0] | f2:end");
            assert!(reference_matches(&a, &b));
            assert!(reference_matches(&b, &a));
        }
    }
    #[test]
    fn real_crossings_branches_other_curves_and_separate_junctions_remain_distinct() {
        for (a, b) in [
            ("l1[l1:0,xl1&l2#0>l1:1]", "l1[l1:0>l1:1]"),
            ("l1[xl1&l2#0>l1:1]", "l1[xl1&l2#1>l1:1]"),
            ("l1[l1:0,l10:0,xl1&l2#0>l1:1]", "l1[l1:0,l10:0>l1:1]"),
            ("l1[l1:0,xl1&l2#0>l2:0]", "l1[l1:0>l2:0]"),
            ("a2[a2:1,l1:1>a2:2,l3:0]", "a3[a3:1,l1:1>a3:2,l3:0]"),
        ] {
            assert!(!reference_matches(a, b), "{a} != {b}");
        }
    }
}
