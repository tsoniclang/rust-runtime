pub trait IntegerInput<Output>: Copy {
    fn checked_integer(self) -> Option<Output>;
    fn truncated_integer(self) -> Option<Output>;
}

macro_rules! integral_inputs {
    ($($input:ty),+ $(,)?) => {$(
        impl<Output: TryFrom<$input>> IntegerInput<Output> for $input {
            #[inline]
            fn checked_integer(self) -> Option<Output> {
                Output::try_from(self).ok()
            }

            #[inline]
            fn truncated_integer(self) -> Option<Output> {
                self.checked_integer()
            }
        }
    )+};
}

integral_inputs!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, usize, isize);

macro_rules! floating_inputs {
    ($($output:ty),+ $(,)?) => {$(
        impl IntegerInput<$output> for f64 {
            #[inline]
            fn checked_integer(self) -> Option<$output> {
                if !self.is_finite() || libm::trunc(self) != self { return None; }
                self.truncated_integer()
            }

            #[inline]
            fn truncated_integer(self) -> Option<$output> {
                if self.is_nan() { return Some(0); }
                let upper = <$output>::MAX as f64;
                let too_large = if <$output>::BITS > 53 { self >= upper } else { self > upper };
                if !self.is_finite() || self < <$output>::MIN as f64 || too_large {
                    return None;
                }
                Some(self as $output)
            }
        }

        impl IntegerInput<$output> for f32 {
            #[inline]
            fn checked_integer(self) -> Option<$output> {
                f64::from(self).checked_integer()
            }

            #[inline]
            fn truncated_integer(self) -> Option<$output> {
                f64::from(self).truncated_integer()
            }
        }
    )+};
}

floating_inputs!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, usize, isize);

#[inline]
pub fn checked_integer<Output>(value: impl IntegerInput<Output>) -> crate::TsonicResult<Output> {
    value.checked_integer().ok_or_else(|| {
        super::range_error(
            core::any::type_name_of_val(&value),
            core::any::type_name::<Output>(),
        )
    })
}

#[inline]
pub fn checked_optional_integer<Output>(
    value: Option<impl IntegerInput<Output>>,
) -> crate::TsonicResult<Option<Output>> {
    value.map(checked_integer).transpose()
}

#[cfg(test)]
mod tests {
    use super::IntegerInput;
    #[test]
    fn optional_integer_conversion_preserves_absence_and_rejects_inexact_values() {
        assert_eq!(
            super::checked_optional_integer::<u64>(None::<f64>).unwrap(),
            None
        );
        assert_eq!(
            super::checked_optional_integer::<u64>(Some(9_007_199_254_740_993_u64)).unwrap(),
            Some(9_007_199_254_740_993)
        );
        assert!(super::checked_optional_integer::<u32>(Some(0.5)).is_err());
        assert!(super::checked_optional_integer::<u32>(Some(f64::NAN)).is_err());
        assert!(super::checked_optional_integer::<u32>(Some(u64::MAX)).is_err());
    }
    #[test]
    fn native_arguments_do_not_round_through_float() {
        assert_eq!(
            IntegerInput::<u64>::checked_integer(9_007_199_254_740_993_u64),
            Some(9_007_199_254_740_993)
        );
        assert_eq!(
            IntegerInput::<u64>::checked_integer(u128::from(u64::MAX)),
            Some(u64::MAX)
        );
        assert_eq!(
            IntegerInput::<u64>::checked_integer(u128::from(u64::MAX) + 1),
            None
        );
        assert_eq!(IntegerInput::<u8>::checked_integer(-1_i64), None);
        assert_eq!(IntegerInput::<i8>::checked_integer(128_u32), None);
        assert_eq!(
            IntegerInput::<u32>::truncated_integer(u64::from(u32::MAX)),
            Some(u32::MAX)
        );
    }

    #[test]
    fn floating_arguments_keep_exact_index_and_truncating_value_rules() {
        for value in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1.0,
            0.5,
            18_446_744_073_709_551_616.0,
        ] {
            assert_eq!(IntegerInput::<u64>::checked_integer(value), None);
        }
        assert_eq!(IntegerInput::<u8>::truncated_integer(f64::NAN), Some(0));
        assert_eq!(IntegerInput::<u8>::truncated_integer(254.9), Some(254));
        assert_eq!(IntegerInput::<u8>::truncated_integer(255.1), None);
        assert_eq!(IntegerInput::<i8>::truncated_integer(-127.9), Some(-127));
        assert_eq!(
            IntegerInput::<i64>::checked_integer(-9_223_372_036_854_775_808.0),
            Some(i64::MIN)
        );
        assert_eq!(
            IntegerInput::<i64>::checked_integer(9_223_372_036_854_775_808.0),
            None
        );
        assert_eq!(IntegerInput::<usize>::checked_integer(16_f32), Some(16));
        assert_eq!(IntegerInput::<usize>::checked_integer(16.5_f32), None);
    }
}
