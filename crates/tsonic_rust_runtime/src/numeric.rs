use core::cmp::Ordering;

use crate::BigInt;
use num_traits::ToPrimitive;

#[derive(Clone, Copy)]
pub enum NumericRef<'value> {
    Float(f64),
    Signed(i128),
    Unsigned(u128),
    BigInt(&'value BigInt),
}

impl NumericRef<'_> {
    pub fn compare(self, other: Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Float(left), Self::Float(right)) => left.partial_cmp(&right),
            (Self::Signed(left), Self::Signed(right)) => Some(left.cmp(&right)),
            (Self::Unsigned(left), Self::Unsigned(right)) => Some(left.cmp(&right)),
            (Self::Signed(left), Self::Unsigned(right)) => Some(if left < 0 {
                Ordering::Less
            } else {
                (left as u128).cmp(&right)
            }),
            (Self::Unsigned(left), Self::Signed(right)) => Self::Signed(right)
                .compare(Self::Unsigned(left))
                .map(Ordering::reverse),
            (Self::Signed(left), Self::Float(right)) => compare_signed_float(left, right),
            (Self::Unsigned(left), Self::Float(right)) => compare_unsigned_float(left, right),
            (Self::Float(left), right) => right.compare(Self::Float(left)).map(Ordering::reverse),
            (Self::BigInt(left), Self::BigInt(right)) => Some(left.cmp(right)),
            (Self::BigInt(left), Self::Signed(right)) => Some(match left.as_ref().to_i128() {
                Some(value) => value.cmp(&right),
                None if left.as_ref().sign() == num_bigint::Sign::Minus => Ordering::Less,
                None => Ordering::Greater,
            }),
            (Self::BigInt(left), Self::Unsigned(right)) => Some(match left.as_ref().to_u128() {
                Some(value) => value.cmp(&right),
                None if left.as_ref().sign() == num_bigint::Sign::Minus => Ordering::Less,
                None => Ordering::Greater,
            }),
            (Self::BigInt(left), Self::Float(right)) => compare_bigint_float(left, right),
            (left, Self::BigInt(right)) => Self::BigInt(right).compare(left).map(Ordering::reverse),
        }
    }
}

fn compare_unsigned_float(left: u128, right: f64) -> Option<Ordering> {
    if right.is_nan() {
        return None;
    }
    if right < 0.0 {
        return Some(Ordering::Greater);
    }
    if right >= 340282366920938463463374607431768211456.0 {
        return Some(Ordering::Less);
    }
    Some(match left.cmp(&(right as u128)) {
        Ordering::Equal if right != libm::trunc(right) => Ordering::Less,
        ordering => ordering,
    })
}

fn compare_signed_float(left: i128, right: f64) -> Option<Ordering> {
    if left >= 0 {
        return compare_unsigned_float(left as u128, right);
    }
    compare_unsigned_float(left.unsigned_abs(), -right).map(Ordering::reverse)
}

fn compare_bigint_float(left: &BigInt, right: f64) -> Option<Ordering> {
    if right.is_nan() {
        return None;
    }
    if right == f64::INFINITY {
        return Some(Ordering::Less);
    }
    if right == f64::NEG_INFINITY {
        return Some(Ordering::Greater);
    }
    let integer = left.as_ref();
    let negative = integer.sign() == num_bigint::Sign::Minus;
    if negative && right >= 0.0 {
        return Some(Ordering::Less);
    }
    if !negative && right < 0.0 {
        return Some(Ordering::Greater);
    }
    let magnitude = integer.magnitude();
    let absolute = right.abs();
    let bits = absolute.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023;
    let integer_bits = (exponent + 1).max(0) as u64;
    let mut ordering = magnitude.bits().cmp(&integer_bits);
    if ordering == Ordering::Equal {
        let significand = (bits & ((1_u64 << 52) - 1)) | (1_u64 << 52);
        let shift = exponent - 52;
        for (index, digit) in magnitude.iter_u64_digits().enumerate() {
            let offset = index as i32 * 64 - shift;
            let expected = if (0..64).contains(&offset) {
                significand >> offset
            } else if (-63..0).contains(&offset) {
                significand << -offset
            } else {
                0
            };
            let current = digit.cmp(&expected);
            if current != Ordering::Equal {
                ordering = current;
            }
        }
        if ordering == Ordering::Equal && absolute != libm::trunc(absolute) {
            ordering = Ordering::Less;
        }
    }
    Some(if negative {
        ordering.reverse()
    } else {
        ordering
    })
}
