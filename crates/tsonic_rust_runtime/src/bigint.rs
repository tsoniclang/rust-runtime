use crate::{JsError, JsErrorKind, TsonicResult};
use std::fmt;
use std::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign};
use std::sync::Arc;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BigInt(Arc<num_bigint::BigInt>);

impl BigInt {
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

    pub fn to_signed_bytes_le(&self) -> Vec<u8> {
        self.0.to_signed_bytes_le()
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
