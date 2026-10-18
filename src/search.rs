//! Small local search shared by command and document lists.
pub fn ranked<'a>(query: &str, entries: impl IntoIterator<Item = &'a str>) -> Vec<usize> {
    let words: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut results: Vec<_> = entries
        .into_iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let label = entry.to_lowercase();
            let positions: Option<Vec<_>> = words.iter().map(|word| label.find(word)).collect();
            positions.map(|positions| (positions.iter().sum::<usize>(), index))
        })
        .collect();
    results.sort_by_key(|&(score, index)| (score, index));
    results.into_iter().map(|(_, index)| index).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_and_history_support_multiple_words_and_stable_ranking() {
        let entries = [
            "Feature: Mount plate",
            "Extrude",
            "Settings: Mouse sensitivity",
            "Feature: Plate mounting holes",
        ];
        assert_eq!(ranked("PLATE mount", entries), vec![0, 3]);
        assert_eq!(ranked("mouse sens", entries), vec![2]);
        assert!(ranked("plate extrude", entries).is_empty());
        assert_eq!(ranked("", entries), vec![0, 1, 2, 3]);
    }
}
