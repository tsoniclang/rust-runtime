#![forbid(unsafe_code)]

//! Core runtime error definitions for closed JS/Node external crates.

pub mod async_runtime;
pub mod control_flow;
pub mod conversions;
pub mod error;
pub mod generator;
pub mod location;
pub mod operators;

pub use async_runtime::block_on;
pub use control_flow::{finish_resource, Completion};
pub use error::{JsError, JsErrorKind, TsonicError, TsonicResult};
pub use generator::{
    AsyncGenerator, Generator, GeneratorController, IteratorResult, IteratorValue, YieldPoint,
};
pub use location::Location;
pub use operators::{
    bitwise_and, bitwise_not, bitwise_or, bitwise_xor, left_shift, signed_right_shift, to_int32,
    to_uint32, unsigned_right_shift,
};
