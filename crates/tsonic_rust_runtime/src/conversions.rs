use crate::{JsError, JsErrorKind, TsonicError, TsonicResult};

fn range_error(source: &str, target: &str) -> TsonicError {
    JsError::new(
        JsErrorKind::RangeError,
        format!("{source} value is outside the representable {target} range"),
    )
    .into()
}

pub fn i32_to_usize(value: i32) -> TsonicResult<usize> {
    usize::try_from(value).map_err(|_| range_error("i32", "usize"))
}

pub fn i32_to_u8(value: i32) -> TsonicResult<u8> {
    u8::try_from(value).map_err(|_| range_error("i32", "u8"))
}

pub fn usize_to_i32(value: usize) -> TsonicResult<i32> {
    i32::try_from(value).map_err(|_| range_error("usize", "i32"))
}

pub fn isize_to_i32(value: isize) -> TsonicResult<i32> {
    i32::try_from(value).map_err(|_| range_error("isize", "i32"))
}

pub fn u32_to_i32(value: u32) -> TsonicResult<i32> {
    i32::try_from(value).map_err(|_| range_error("u32", "i32"))
}

pub fn u8_to_i32(value: u8) -> i32 {
    i32::from(value)
}

pub fn isize_to_f64(value: isize) -> f64 {
    value as f64
}

pub fn i32_to_f64(value: i32) -> f64 {
    f64::from(value)
}

pub fn f64_to_i32(value: f64) -> TsonicResult<i32> {
    let truncated = value.trunc();
    if !truncated.is_finite() || !(-2_147_483_648.0..2_147_483_648.0).contains(&truncated) {
        return Err(range_error("f64", "i32"));
    }
    Ok(truncated as i32)
}

pub fn usize_to_f64(value: usize) -> f64 {
    value as f64
}

pub fn u64_to_f64(value: u64) -> f64 {
    value as f64
}
