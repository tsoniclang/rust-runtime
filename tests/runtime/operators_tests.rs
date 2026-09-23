use tsonic_rust_runtime::operators;

#[test]
fn shifts_preserve_native_widths_and_counts() {
    assert_eq!(operators::native_shift_left(1_i64, 53_u64), 9_007_199_254_740_992);
    assert_eq!(operators::native_shift_left(1_u128, 127_u128), 1_u128 << 127);
    assert_eq!(operators::native_shift_right(-2_i64, 1_u8), -1);
    assert_eq!(operators::native_unsigned_shift_right(-1_i64, 1_i128), i64::MAX);
    assert_eq!(operators::native_unsigned_shift_right(0x80_u8, 7_usize), 1);
    assert_eq!(operators::native_unsigned_shift_right(-1_i128, 127_u64), 1);
}

#[test]
fn generic_shift_counts_are_not_narrowed() {
    struct ExactShift;
    impl core::ops::Shl<u64> for ExactShift {
        type Output = Self;
        fn shl(self, count: u64) -> Self {
            assert_eq!(count, 9_007_199_254_740_993);
            self
        }
    }
    operators::native_shift_left(ExactShift, 9_007_199_254_740_993_u64);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn excessive_shift_obeys_native_overflow_checks() {
    let count = std::hint::black_box(64_u64);
    let _ = operators::native_shift_left(1_u64, count);
}
