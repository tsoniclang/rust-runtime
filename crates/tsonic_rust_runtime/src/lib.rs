#![forbid(unsafe_code)]

//! Core runtime error definitions for closed JS/Node external crates.

pub mod conversions;
pub mod error;
pub mod operators;

pub use error::{JsError, JsErrorKind, TsonicError, TsonicResult};
pub use operators::{
    bitwise_and, bitwise_not, bitwise_or, bitwise_xor, left_shift, signed_right_shift, to_int32,
    to_uint32, unsigned_right_shift,
};
