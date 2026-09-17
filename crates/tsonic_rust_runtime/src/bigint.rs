use crate::{JsError, JsErrorKind, TsonicResult};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::ops::{
    Add, AddAssign, BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Mul, MulAssign,
    Neg, Not, Sub, SubAssign,
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BigInt(Arc<num_bigint::BigInt>);

impl From<num_bigint::BigInt> for BigInt {
    fn from(value: num_bigint::BigInt) -> Self {
        Self(Arc::new(value))
    }
}

impl AsRef<num_bigint::BigInt> for BigInt {
    fn as_ref(&self) -> &num_bigint::BigInt {
        self.0.as_ref()
    }
}

impl BigInt {
    pub fn from_signed_bytes_le(bytes: &[u8]) -> Self {
        Self(Arc::new(num_bigint::BigInt::from_signed_bytes_le(bytes)))
    }

    pub fn from_decimal_literal(value: &str) -> Self {
        let parsed = num_bigint::BigInt::parse_bytes(value.as_bytes(), 10)
            .expect("compiler-emitted bigint literal must be canonical decimal text");
        Self(Arc::new(parsed))
    }

    pub fn checked_div(left: Self, right: Self) -> TsonicResult<Self> {
        Self::checked_arithmetic(left, right, |left, right| left / right)
    }

    pub fn checked_rem(left: Self, right: Self) -> TsonicResult<Self> {
        Self::checked_arithmetic(left, right, |left, right| left % right)
    }

    pub fn checked_shift_left(left: Self, right: Self) -> TsonicResult<Self> {
        Self::checked_shift(left, right, true)
    }

    pub fn checked_shift_right(left: Self, right: Self) -> TsonicResult<Self> {
        Self::checked_shift(left, right, false)
    }

    fn checked_shift(left: Self, right: Self, shift_left: bool) -> TsonicResult<Self> {
        if left.0.sign() == num_bigint::Sign::NoSign || right.0.sign() == num_bigint::Sign::NoSign {
            return Ok(left);
        }
        let shift_left = shift_left != (right.0.sign() == num_bigint::Sign::Minus);
        let mut digits = right.0.iter_u64_digits();
        let count = digits.next().unwrap_or(0);
        let count = (digits.next().is_none()).then_some(count);
        if !shift_left && count.is_none_or(|count| count >= left.0.bits()) {
            return Ok(Self::from(num_bigint::BigInt::from(
                if left.0.sign() == num_bigint::Sign::Minus {
                    -1
                } else {
                    0
                },
            )));
        }
        let count = count
            .filter(|count| {
                !shift_left
                    || left
                        .0
                        .bits()
                        .checked_add(*count)
                        .and_then(|bits| usize::try_from(bits.div_ceil(64)).ok())
                        .and_then(|words| words.checked_add(1)?.checked_mul(8))
                        .is_some_and(|bytes| bytes <= isize::MAX as usize)
            })
            .and_then(|count| usize::try_from(count).ok())
            .ok_or_else(|| {
                JsError::new(
                    JsErrorKind::RangeError,
                    "BigInt shift exceeds addressable storage",
                )
            })?;
        Ok(Self::from(if shift_left {
            left.0.as_ref() << count
        } else {
            left.0.as_ref() >> count
        }))
    }

    pub fn to_signed_bytes_le(&self) -> Vec<u8> {
        self.0.to_signed_bytes_le()
    }

    pub fn to_str_radix(&self, radix: u32) -> alloc::string::String {
        self.0.to_str_radix(radix)
    }

    fn checked_arithmetic(
        left: Self,
        right: Self,
        operation: impl FnOnce(&num_bigint::BigInt, &num_bigint::BigInt) -> num_bigint::BigInt,
    ) -> TsonicResult<Self> {
        if right.0.as_ref() == &num_bigint::BigInt::from(0_u8) {
            return Err(JsError::new(JsErrorKind::RangeError, "Division by zero").into());
        }
        Ok(Self(Arc::new(operation(left.0.as_ref(), right.0.as_ref()))))
    }
}

impl fmt::Display for BigInt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Neg for BigInt {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(Arc::new(-self.0.as_ref()))
    }
}

impl Not for BigInt {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self(Arc::new(!self.0.as_ref()))
    }
}

macro_rules! impl_binary_operation {
    ($trait:ident, $method:ident, $assign_trait:ident, $assign_method:ident, $operator:tt) => {
        impl $trait for BigInt {
            type Output = Self;

            fn $method(self, right: Self) -> Self::Output {
                Self(Arc::new(self.0.as_ref() $operator right.0.as_ref()))
            }
        }

        impl $assign_trait for BigInt {
            fn $assign_method(&mut self, right: Self) {
                *self = self.clone() $operator right;
            }
        }
    };
}

impl_binary_operation!(Add, add, AddAssign, add_assign, +);
impl_binary_operation!(Sub, sub, SubAssign, sub_assign, -);
impl_binary_operation!(Mul, mul, MulAssign, mul_assign, *);
impl_binary_operation!(BitAnd, bitand, BitAndAssign, bitand_assign, &);
impl_binary_operation!(BitOr, bitor, BitOrAssign, bitor_assign, |);
impl_binary_operation!(BitXor, bitxor, BitXorAssign, bitxor_assign, ^);
