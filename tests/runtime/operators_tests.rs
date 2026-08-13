use tsonic_rust_runtime::operators;

#[test]
fn to_uint32_zero_and_infinity() {
    assert_eq!(operators::to_uint32(0.0), 0);
    assert_eq!(operators::to_uint32(f64::INFINITY), 0);
    assert_eq!(operators::to_uint32(f64::NEG_INFINITY), 0);
    assert_eq!(operators::to_uint32(f64::NAN), 0);
}

#[test]
fn to_int32_wraps_negatives() {
    assert_eq!(operators::to_int32(1.0), 1);
    assert_eq!(operators::to_int32(-1.0), -1);
    assert_eq!(operators::to_int32(4_294_967_296.0), 0);
    assert_eq!(operators::to_int32(4_294_967_295.0), -1);
    assert_eq!(operators::to_int32(-4_294_967_296.0), 0);
    assert_eq!(operators::to_int32(2_147_483_648.0), -2_147_483_648);
}

#[test]
fn bitwise_operators_match_js_style() {
    assert_eq!(operators::bitwise_and(1.0, 3.0), 1);
    assert_eq!(operators::bitwise_or(1.0, 2.0), 3);
    assert_eq!(operators::bitwise_xor(5.0, 3.0), 6);
    assert_eq!(operators::bitwise_not(0.0), -1);
}

#[test]
fn shifts_mask_rhs_to_five_bits() {
    assert_eq!(operators::left_shift(1.0, 32.0), 1);
    assert_eq!(
        operators::signed_right_shift(2147483648.0, 1.0),
        0xC000_0000_u32 as i32
    );
    assert_eq!(
        operators::unsigned_right_shift(2_147_483_648_f64, 1.0),
        0x4000_0000
    );
    assert_eq!(operators::left_shift(-1.0, -1.0), -1 << 31);
    assert_eq!(operators::unsigned_right_shift(-1.0, 0.0), 0xFFFF_FFFF);
}

#[test]
fn source_number_wrappers_preserve_number_carriers() {
    assert_eq!(operators::source_number_bitwise_and(5.0, 3.0), 1.0);
    assert_eq!(operators::source_number_bitwise_or(5.0, 2.0), 7.0);
    assert_eq!(operators::source_number_bitwise_xor(5.0, 3.0), 6.0);
    assert_eq!(operators::source_number_shift_left(1.0, 32.0), 1.0);
    assert_eq!(operators::source_number_shift_right(-2.0, 1.0), -1.0);
    assert_eq!(
        operators::source_number_unsigned_shift_right(-1.0, 1.0),
        2_147_483_647.0
    );
}

#[test]
fn native_shifts_mask_counts_and_preserve_the_left_carrier() {
    assert_eq!(operators::native_shift_left(1_i32, 32_i32), 1_i32);
    assert_eq!(operators::native_shift_left(1_i32, -1_i32), i32::MIN);
    assert_eq!(operators::native_shift_right(-2_i32, 1_u8), -1_i32);
    assert_eq!(
        operators::native_unsigned_shift_right(-1_i32, 1_i64),
        i32::MAX
    );
    assert_eq!(
        operators::native_unsigned_shift_right(0x80_u8, 7_usize),
        1_u8
    );
}
