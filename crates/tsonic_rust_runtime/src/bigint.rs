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
