#![forbid(unsafe_code)]

//! Core runtime error definitions for closed JS/Node external crates.

pub mod async_runtime;
pub mod control_flow;
pub mod conversions;
pub mod error;
pub mod generator;
pub mod location;
pub mod operators;
pub mod source_string;
pub mod undefined;

pub use async_runtime::block_on;
pub use control_flow::{finish_resource, Completion};
pub use error::{JsError, JsErrorKind, TsonicError, TsonicResult};
pub use generator::{
    AsyncGenerator, BorrowedAsyncGenerator, BorrowedGenerator, Generator, GeneratorController,
    IteratorResult, IteratorValue, YieldPoint,
};
pub use location::Location;
pub use operators::{
    bitwise_and, bitwise_not, bitwise_or, bitwise_xor, left_shift, signed_right_shift, to_int32,
    to_uint32, unsigned_right_shift,
};
pub use source_string::{source_string, ToSourceString};
pub use undefined::Undefined;
