//! Numeric helpers that model JS numeric operators and type conversions.

const UINT32_MODULUS: f64 = 4_294_967_296.0;

/// Converts a JavaScript number to `u32` using ECMAScript's ToUint32 behavior.
pub fn to_uint32(value: f64) -> u32 {
    if !value.is_finite() || value == 0.0 {
        return 0;
    }

    let int_value = value.trunc();
    let rem = int_value.rem_euclid(UINT32_MODULUS);
    rem as u32
}

/// Converts a JavaScript number to `i32` using ECMAScript's ToInt32 behavior.
pub fn to_int32(value: f64) -> i32 {
    let uint32 = to_uint32(value);
    if uint32 < (1 << 31) {
        uint32 as i32
    } else {
        (uint32 as i64 - UINT32_MODULUS as i64) as i32
    }
}

pub fn bitwise_not(value: f64) -> i32 {
    !to_int32(value)
}

pub fn bitwise_and(left: f64, right: f64) -> i32 {
    to_int32(left) & to_int32(right)
}

pub fn bitwise_or(left: f64, right: f64) -> i32 {
    to_int32(left) | to_int32(right)
}

pub fn bitwise_xor(left: f64, right: f64) -> i32 {
    to_int32(left) ^ to_int32(right)
}

pub fn left_shift(left: f64, right: f64) -> i32 {
    let lhs = to_int32(left);
    let shift = to_uint32(right) & 0x1f;
    lhs.wrapping_shl(shift)
}

pub fn signed_right_shift(left: f64, right: f64) -> i32 {
    let lhs = to_int32(left);
    let shift = to_uint32(right) & 0x1f;
    lhs.wrapping_shr(shift)
}

pub fn unsigned_right_shift(left: f64, right: f64) -> u32 {
    let lhs = to_uint32(left);
    let shift = to_uint32(right) & 0x1f;
    lhs >> shift
}
