#[inline]
pub fn option_coalesce<T, R>(
    value: Option<T>,
    present: impl FnOnce(T) -> R,
    fallback: impl FnOnce() -> R,
) -> R {
    value.map_or_else(fallback, present)
}

#[cfg(test)]
mod tests {
    use super::option_coalesce;
    use std::cell::Cell;

    #[test]
    fn selects_present_value_without_evaluating_fallback() {
        let evaluated = Cell::new(false);
        let result = option_coalesce(Some(42), std::convert::identity, || {
            evaluated.set(true);
            0
        });

        assert_eq!(result, 42);
        assert!(!evaluated.get());
    }

    #[test]
    fn evaluates_fallback_once_for_absent_value() {
        let evaluations = Cell::new(0);
        let result = option_coalesce(None::<i32>, Some, || {
            evaluations.set(evaluations.get() + 1);
            Some(42)
        });

        assert_eq!(result, Some(42));
        assert_eq!(evaluations.get(), 1);
    }
}
