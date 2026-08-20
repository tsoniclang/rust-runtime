#![forbid(unsafe_code)]

//! Core runtime error definitions for closed JS/Node external crates.

pub mod async_runtime;
pub mod bigint;
pub mod callable;
pub mod control_flow;
pub mod conversions;
pub mod error;
pub mod generator;
pub mod iteration;
pub mod location;
pub mod module_cell;
pub mod null;
pub mod object_handle;
pub mod object_identity;
pub mod object_ref;
pub mod operators;
pub mod option;
pub mod source_string;
pub mod undefined;

pub use async_runtime::block_on;
pub use bigint::BigInt;
pub use callable::Callable;
pub use control_flow::{completion_region, finish_finally, finish_resource, Completion};
pub use error::{JsError, JsErrorKind, TsonicError, TsonicResult};
pub use generator::{
    AsyncGenerator, BorrowedAsyncGenerator, BorrowedGenerator, Generator, GeneratorController,
    GeneratorResume, IteratorResult, IteratorValue, YieldPoint,
};
pub use iteration::{iter_cloned, iter_copied};
pub use location::Location;
pub use module_cell::ModuleCell;
pub use null::Null;
pub use object_handle::{EmptyObjectState, ObjectHandle};
pub use object_identity::ObjectIdentity;
pub use object_ref::ObjectRef;
pub use operators::{
    bitwise_and, bitwise_not, bitwise_or, bitwise_xor, left_shift, native_shift_left,
    native_shift_right, native_unsigned_shift_right, signed_right_shift, source_number_bitwise_and,
    source_number_bitwise_or, source_number_bitwise_xor, source_number_shift_left,
    source_number_shift_right, source_number_unsigned_shift_right, to_int32, to_uint32,
    unsigned_right_shift,
};
pub use option::option_coalesce;
pub use source_string::{
    source_string, source_string_greater_than, source_string_greater_than_or_equal,
    source_string_less_than, source_string_less_than_or_equal, ToSourceString,
};
pub use undefined::Undefined;
