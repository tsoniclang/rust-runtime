//! Owned iteration helpers for source languages with value-bound loop variables.

pub fn iter_copied<T: Copy>(values: &[T]) -> impl Iterator<Item = T> + '_ {
    values.iter().copied()
}

pub fn iter_cloned<T: Clone>(values: &[T]) -> impl Iterator<Item = T> + '_ {
    values.iter().cloned()
}
