//! Small helpers shared by the schema's validation code.

use std::collections::BTreeMap;

/// The keys used by more than one of `items`, with how many use each, sorted so that a validation
/// message reads the same however the items are ordered.
pub(crate) fn duplicates<'a, T: 'a, K: Ord>(
    items: impl IntoIterator<Item = &'a T>,
    key_of: impl Fn(&'a T) -> K,
) -> Vec<(K, usize)> {
    let mut counts: BTreeMap<K, usize> = BTreeMap::new();

    for item in items {
        *counts.entry(key_of(item)).or_default() += 1;
    }

    counts.into_iter().filter(|(_, count)| *count > 1).collect()
}

#[cfg(test)]
mod tests {
    use super::duplicates;

    /// Only the repeated keys are returned, each with its count, sorted by key rather than in the
    /// order they first appear.
    #[test]
    fn test_duplicates() {
        let items = vec![
            "b".to_string(),
            "a".to_string(),
            "c".to_string(),
            "b".to_string(),
            "a".to_string(),
        ];

        assert_eq!(duplicates(&items, String::as_str), vec![("a", 2), ("b", 2)]);
    }
}
