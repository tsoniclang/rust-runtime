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

pub fn source_number_bitwise_and(left: f64, right: f64) -> f64 {
    bitwise_and(left, right) as f64
}

pub fn source_number_bitwise_or(left: f64, right: f64) -> f64 {
    bitwise_or(left, right) as f64
}

pub fn source_number_bitwise_xor(left: f64, right: f64) -> f64 {
    bitwise_xor(left, right) as f64
}

pub fn source_number_shift_left(left: f64, right: f64) -> f64 {
    left_shift(left, right) as f64
}

pub fn source_number_shift_right(left: f64, right: f64) -> f64 {
    signed_right_shift(left, right) as f64
}

pub fn source_number_unsigned_shift_right(left: f64, right: f64) -> f64 {
    unsigned_right_shift(left, right) as f64
}

#[doc(hidden)]
pub trait NativeShiftCount {
    fn native_shift_count(self) -> u32;
}

macro_rules! impl_native_shift_count {
    ($($type:ty),+ $(,)?) => {
        $(
            impl NativeShiftCount for $type {
                fn native_shift_count(self) -> u32 {
                    self as u32
                }
            }
        )+
    };
}

impl_native_shift_count!(i8, u8, i16, u16, i32, u32, i64, u64, i128, u128, isize, usize,);

#[doc(hidden)]
pub trait NativeShift: Sized {
    fn native_shift_left(self, count: u32) -> Self;
    fn native_shift_right(self, count: u32) -> Self;
    fn native_unsigned_shift_right(self, count: u32) -> Self;
}

macro_rules! impl_native_shift_unsigned {
    ($($type:ty),+ $(,)?) => {
        $(
            impl NativeShift for $type {
                fn native_shift_left(self, count: u32) -> Self {
                    self.wrapping_shl(count)
                }

                fn native_shift_right(self, count: u32) -> Self {
                    self.wrapping_shr(count)
                }

                fn native_unsigned_shift_right(self, count: u32) -> Self {
                    self.wrapping_shr(count)
                }
            }
        )+
    };
}

macro_rules! impl_native_shift_signed {
    ($(($signed:ty, $unsigned:ty)),+ $(,)?) => {
        $(
            impl NativeShift for $signed {
                fn native_shift_left(self, count: u32) -> Self {
                    self.wrapping_shl(count)
                }

                fn native_shift_right(self, count: u32) -> Self {
                    self.wrapping_shr(count)
                }

                fn native_unsigned_shift_right(self, count: u32) -> Self {
                    (self as $unsigned).wrapping_shr(count) as Self
                }
            }
        )+
    };
}

impl_native_shift_unsigned!(u8, u16, u32, u64, u128, usize);
impl_native_shift_signed!(
    (i8, u8),
    (i16, u16),
    (i32, u32),
    (i64, u64),
    (i128, u128),
    (isize, usize),
);

pub fn native_shift_left<T: NativeShift, C: NativeShiftCount>(value: T, count: C) -> T {
    value.native_shift_left(count.native_shift_count())
}

pub fn native_shift_right<T: NativeShift, C: NativeShiftCount>(value: T, count: C) -> T {
    value.native_shift_right(count.native_shift_count())
}

pub fn native_unsigned_shift_right<T: NativeShift, C: NativeShiftCount>(value: T, count: C) -> T {
    value.native_unsigned_shift_right(count.native_shift_count())
}
