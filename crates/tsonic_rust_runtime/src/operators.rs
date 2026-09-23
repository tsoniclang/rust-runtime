use core::ops::{Shl, Shr};

pub trait NativeUnsignedShift<Count>: Sized {
    fn native_unsigned_shift_right(self, count: Count) -> Self;
}

macro_rules! impl_native_unsigned_shift {
    ($(($signed:ty, $unsigned:ty)),+ $(,)?) => {
        $(
            impl<Count> NativeUnsignedShift<Count> for $signed
            where $unsigned: Shr<Count, Output = $unsigned> {
                #[inline]
                fn native_unsigned_shift_right(self, count: Count) -> Self {
                    ((self as $unsigned) >> count) as Self
                }
            }

            impl<Count> NativeUnsignedShift<Count> for $unsigned
            where $unsigned: Shr<Count, Output = $unsigned> {
                #[inline]
                fn native_unsigned_shift_right(self, count: Count) -> Self {
                    self >> count
                }
            }
        )+
    };
}

impl_native_unsigned_shift!(
    (i8, u8), (i16, u16), (i32, u32), (i64, u64), (i128, u128), (isize, usize),
);

#[inline]
pub fn native_shift_left<Value, Count>(value: Value, count: Count) -> Value
where Value: Shl<Count, Output = Value> {
    value << count
}

#[inline]
pub fn native_shift_right<Value, Count>(value: Value, count: Count) -> Value
where Value: Shr<Count, Output = Value> {
    value >> count
}

#[inline]
pub fn native_unsigned_shift_right<Value, Count>(value: Value, count: Count) -> Value
where Value: NativeUnsignedShift<Count> {
    value.native_unsigned_shift_right(count)
}
