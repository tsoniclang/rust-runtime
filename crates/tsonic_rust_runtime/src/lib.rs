#![no_std]
#![forbid(unsafe_code)]

//! Closed runtime support, layered over Rust's `core`, `alloc`, and `std`
//! foundations.

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
pub mod async_runtime;
#[cfg(feature = "alloc")]
pub mod bigint;
#[cfg(feature = "alloc")]
pub mod callable;
#[cfg(feature = "alloc")]
pub mod control_flow;
#[cfg(feature = "alloc")]
pub mod conversions;
#[cfg(feature = "alloc")]
pub mod error;
#[cfg(feature = "alloc")]
pub mod generator;
pub mod iteration;
#[cfg(feature = "alloc")]
pub mod location;
#[cfg(feature = "alloc")]
pub mod module_cell;
pub mod null;
#[cfg(feature = "alloc")]
pub mod object_handle;
#[cfg(feature = "alloc")]
pub mod object_identity;
#[cfg(feature = "alloc")]
pub mod object_ref;
pub mod operators;
pub mod option;
#[cfg(feature = "alloc")]
pub mod source_string;
#[cfg(feature = "alloc")]
pub mod ts_value;
pub mod undefined;

#[cfg(feature = "std")]
pub use async_runtime::block_on;
#[cfg(feature = "alloc")]
pub use bigint::BigInt;
#[cfg(feature = "alloc")]
pub use callable::Callable;
#[cfg(feature = "alloc")]
pub use control_flow::{completion_region, finish_finally, finish_resource, Completion};
#[cfg(feature = "alloc")]
pub use error::{JsError, JsErrorKind, TsonicError, TsonicResult};
#[cfg(feature = "alloc")]
pub use generator::{
    AsyncGenerator, BorrowedAsyncGenerator, BorrowedGenerator, Generator, GeneratorController,
    GeneratorResume, IteratorResult, IteratorValue, YieldPoint,
};
pub use iteration::{iter_cloned, iter_copied};
#[cfg(feature = "alloc")]
pub use location::Location;
#[cfg(feature = "alloc")]
pub use module_cell::ModuleCell;
pub use null::Null;
#[cfg(feature = "alloc")]
pub use object_handle::{EmptyObjectState, ObjectHandle};
#[cfg(feature = "alloc")]
pub use object_identity::{ObjectIdentity, ObjectIdentityCarrier, WeakObjectIdentity};
#[cfg(feature = "alloc")]
pub use object_ref::ObjectRef;
pub use operators::{
    bitwise_and, bitwise_not, bitwise_or, bitwise_xor, left_shift, native_shift_left,
    native_shift_right, native_unsigned_shift_right, signed_right_shift, source_number_bitwise_and,
    source_number_bitwise_or, source_number_bitwise_xor, source_number_shift_left,
    source_number_shift_right, source_number_unsigned_shift_right, to_int32, to_uint32,
    unsigned_right_shift,
};
pub use option::option_coalesce;
#[cfg(feature = "alloc")]
pub use source_string::{
    source_string, source_string_greater_than, source_string_greater_than_or_equal,
    source_string_less_than, source_string_less_than_or_equal, ToSourceString,
};
#[cfg(feature = "alloc")]
pub use ts_value::{clone_ts_value, TsValue};
pub use undefined::Undefined;
