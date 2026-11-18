//! Pure, deterministic collection query helpers over in-memory slices.

use std::cmp::Ordering;

/// Filters `items` using `pred`, preserving order.
pub fn filter_items<T: Clone, E>(
    items: &[T],
    mut pred: impl FnMut(&T) -> Result<bool, E>,
) -> Result<Vec<T>, E> {
    let mut kept = Vec::new();
    for item in items {
        if pred(item)? {
            kept.push(item.clone());
        }
    }
    Ok(kept)
}

/// Sorts `items` by an extracted optional key, placing items with `None` keys
/// last (preserving their relative order) regardless of `descending`.
pub fn sort_items_by_key<T: Clone, K, E>(
    items: &[T],
    descending: bool,
    mut extract: impl FnMut(&T) -> Result<Option<K>, E>,
    mut cmp: impl FnMut(&K, &K) -> Ordering,
) -> Result<Vec<T>, E> {
    let mut keyed = Vec::with_capacity(items.len());
    let mut absent = Vec::new();
    for item in items {
        match extract(item)? {
            Some(key) => keyed.push((key, item.clone())),
            None => absent.push(item.clone()),
        }
    }
    keyed.sort_by(|(a, _), (b, _)| {
        let order = cmp(a, b);
        if descending { order.reverse() } else { order }
    });
    let mut sorted: Vec<T> = keyed.into_iter().map(|(_, item)| item).collect();
    sorted.extend(absent);
    Ok(sorted)
}

#[cfg(test)]
mod tests {
    use super::{filter_items, sort_items_by_key};

    #[test]
    fn filter_and_sort_preserve_stability_and_none_last() {
        let items = [3, 1, 4, 2];
        let kept = filter_items(&items, |&x| Ok::<_, ()>(x > 1)).unwrap();
        assert_eq!(kept, vec![3, 4, 2]);

        let keyed = [("a", Some(2)), ("b", None), ("c", Some(1)), ("d", None)];
        let asc =
            sort_items_by_key(&keyed, false, |&(_, k)| Ok::<_, ()>(k), |a, b| a.cmp(b)).unwrap();
        assert_eq!(
            asc,
            vec![("c", Some(1)), ("a", Some(2)), ("b", None), ("d", None)]
        );

        let desc =
            sort_items_by_key(&keyed, true, |&(_, k)| Ok::<_, ()>(k), |a, b| a.cmp(b)).unwrap();
        assert_eq!(
            desc,
            vec![("a", Some(2)), ("c", Some(1)), ("b", None), ("d", None)]
        );
    }
}
