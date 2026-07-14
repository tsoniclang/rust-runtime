use tsonic_rust_runtime::conversions::{
    f64_to_i32, i32_to_f64, i32_to_u8, i32_to_usize, isize_to_f64, isize_to_i32, u32_to_i32,
    u64_to_f64, u8_to_i32, usize_to_f64, usize_to_i32,
};
use tsonic_rust_runtime::{JsErrorKind, TsonicError};

fn assert_range_error(error: TsonicError) {
    match error {
        TsonicError::Js(error) => assert_eq!(error.kind(), JsErrorKind::RangeError),
        other => panic!("expected range error, got {other:?}"),
    }
}

#[test]
fn checked_integer_conversions_accept_boundaries() {
    assert_eq!(i32_to_usize(0).unwrap(), 0);
    assert_eq!(i32_to_u8(255).unwrap(), 255);
    assert_eq!(usize_to_i32(i32::MAX as usize).unwrap(), i32::MAX);
    assert_eq!(isize_to_i32(i32::MIN as isize).unwrap(), i32::MIN);
    assert_eq!(u32_to_i32(i32::MAX as u32).unwrap(), i32::MAX);
    assert_eq!(u8_to_i32(u8::MAX), 255);
}

#[test]
fn checked_integer_conversions_reject_out_of_range_values() {
    assert_range_error(i32_to_usize(-1).unwrap_err());
    assert_range_error(i32_to_u8(256).unwrap_err());
    if usize::BITS > i32::BITS {
        assert_range_error(usize_to_i32(i32::MAX as usize + 1).unwrap_err());
    }
    if isize::BITS > i32::BITS {
        assert_range_error(isize_to_i32(i32::MAX as isize + 1).unwrap_err());
    }
    assert_range_error(u32_to_i32(i32::MAX as u32 + 1).unwrap_err());
}

#[test]
fn js_number_conversions_are_explicit_and_deterministic() {
    assert_eq!(isize_to_f64(-1), -1.0);
    assert_eq!(i32_to_f64(-1), -1.0);
    assert_eq!(usize_to_f64(42), 42.0);
    assert_eq!(u64_to_f64(9_007_199_254_740_992), 9_007_199_254_740_992.0);
}

#[test]
fn float64_to_int32_is_truncating_and_checked() {
    assert_eq!(f64_to_i32(42.9).unwrap(), 42);
    assert_eq!(f64_to_i32(-42.9).unwrap(), -42);
    assert_eq!(f64_to_i32(-2_147_483_648.0).unwrap(), i32::MIN);
    assert_range_error(f64_to_i32(2_147_483_648.0).unwrap_err());
    assert_range_error(f64_to_i32(f64::NAN).unwrap_err());
    assert_range_error(f64_to_i32(f64::INFINITY).unwrap_err());
}
