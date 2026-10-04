use crate::numeric::NumericRef;
use crate::{
    BigInt, EmptyObject, ErrorObject, JsErrorKind, ObjectHandle, ObjectIdentity,
    ObjectIdentityCarrier, ObjectRef, OptionalStorage, RetainedError,
};
use alloc::rc::Rc;
use alloc::string::String;
use core::cmp::Ordering;
use core::fmt;

trait ClosedTsValue {
    fn identity_key(&self) -> Option<usize> {
        None
    }
}

struct PassiveValue<Value>(Value);
impl<Value: 'static> ClosedTsValue for PassiveValue<Value> {}

struct IdentityValue<Value>(Value);
impl<Value: ObjectIdentityCarrier + 'static> ClosedTsValue for IdentityValue<Value> {
    fn identity_key(&self) -> Option<usize> {
        Some(self.0.object_identity_key())
    }
}

#[derive(Clone)]
enum Value {
    Absent,
    Bool(bool),
    Char(char),
    String(String),
    BigInt(BigInt),
    Int8(i8),
    Uint8(u8),
    Int16(i16),
    Uint16(u16),
    Int32(i32),
    Uint32(u32),
    Int64(i64),
    Uint64(u64),
    NativeInt(isize),
    NativeUint(usize),
    Float32(f32),
    Float64(f64),
    Identity(ObjectIdentity),
    SharedIdentity(Rc<dyn ObjectIdentityCarrier>),
    Closed(Rc<dyn ClosedTsValue>),
    Error(RetainedError),
}

#[derive(Clone)]
pub struct TsValue(Value);

pub enum NativeValueRef<'value> {
    Absent,
    Bool(bool),
    Char(char),
    String(&'value str),
    Numeric(NumericRef<'value>),
    Identity(usize),
    SharedIdentity(&'value Rc<dyn ObjectIdentityCarrier>),
    Passive(usize),
}

pub trait NativeValue {
    fn native_value_ref(&self) -> NativeValueRef<'_>;
}

#[inline]
pub fn native_values_equal<Left: NativeValue + ?Sized, Right: NativeValue + ?Sized>(
    left: &Left,
    right: &Right,
) -> bool {
    match (left.native_value_ref(), right.native_value_ref()) {
        (NativeValueRef::Absent, NativeValueRef::Absent) => true,
        (NativeValueRef::Bool(left), NativeValueRef::Bool(right)) => left == right,
        (NativeValueRef::Char(left), NativeValueRef::Char(right)) => left == right,
        (NativeValueRef::String(left), NativeValueRef::String(right)) => left == right,
        (NativeValueRef::Numeric(left), NativeValueRef::Numeric(right)) => {
            left.compare(right) == Some(Ordering::Equal)
        }
        (NativeValueRef::Identity(left), NativeValueRef::Identity(right))
        | (NativeValueRef::Passive(left), NativeValueRef::Passive(right)) => left == right,
        (NativeValueRef::SharedIdentity(left), NativeValueRef::SharedIdentity(right)) => {
            Rc::ptr_eq(left, right) || left.object_identity_key() == right.object_identity_key()
        }
        (NativeValueRef::SharedIdentity(left), NativeValueRef::Identity(right)) => {
            left.object_identity_key() == right
        }
        (NativeValueRef::Identity(left), NativeValueRef::SharedIdentity(right)) => {
            left == right.object_identity_key()
        }
        _ => false,
    }
}

#[inline]
pub fn native_values_not_equal<Left: NativeValue + ?Sized, Right: NativeValue + ?Sized>(
    left: &Left,
    right: &Right,
) -> bool {
    !native_values_equal(left, right)
}

impl TsValue {
    pub fn from_error(error: impl Into<RetainedError>) -> Self {
        Self(Value::Error(error.into()))
    }

    pub fn as_error(&self) -> Option<&RetainedError> {
        match &self.0 {
            Value::Error(error) => Some(error),
            _ => None,
        }
    }

    pub fn into_error(self) -> Result<RetainedError, Self> {
        match self.0 {
            Value::Error(error) => Ok(error),
            original => Err(Self(original)),
        }
    }

    pub fn is_error(&self) -> bool {
        self.as_error().is_some()
    }

    pub fn is_error_kind(&self, kind: JsErrorKind) -> bool {
        self.as_error()
            .is_some_and(|error| error.error_kind() == kind)
    }

    pub fn error_value(&self) -> RetainedError {
        self.as_error()
            .expect("checked Error projection selected a non-error payload")
            .clone()
    }

    pub fn from_closed<Payload: 'static>(value: Payload) -> Self {
        Self(Value::Closed(Rc::new(PassiveValue(value))))
    }

    pub fn from_identity<Payload: ObjectIdentityCarrier + 'static>(value: Payload) -> Self {
        Self(Value::Closed(Rc::new(IdentityValue(value))))
    }

    pub fn from_shared_identity(value: Rc<dyn ObjectIdentityCarrier>) -> Self {
        Self(Value::SharedIdentity(value))
    }

    pub fn type_of(&self) -> &'static str {
        match &self.0 {
            Value::Absent
            | Value::Closed(_)
            | Value::Identity(_)
            | Value::SharedIdentity(_)
            | Value::Error(_) => "object",
            Value::Bool(_) => "boolean",
            Value::Int64(_) | Value::Uint64(_) | Value::BigInt(_) => "bigint",
            Value::Int8(_)
            | Value::Uint8(_)
            | Value::Int16(_)
            | Value::Uint16(_)
            | Value::Int32(_)
            | Value::Uint32(_)
            | Value::NativeInt(_)
            | Value::NativeUint(_)
            | Value::Float32(_)
            | Value::Float64(_) => "number",
            Value::Char(_) | Value::String(_) => "string",
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match &self.0 {
            Value::String(value) => Some(value),
            _ => None,
        }
    }
}

impl NativeValue for TsValue {
    #[inline]
    fn native_value_ref(&self) -> NativeValueRef<'_> {
        let numeric = match &self.0 {
            Value::Int8(value) => NumericRef::Signed(i128::from(*value)),
            Value::Uint8(value) => NumericRef::Unsigned(u128::from(*value)),
            Value::Int16(value) => NumericRef::Signed(i128::from(*value)),
            Value::Uint16(value) => NumericRef::Unsigned(u128::from(*value)),
            Value::Int32(value) => NumericRef::Signed(i128::from(*value)),
            Value::Uint32(value) => NumericRef::Unsigned(u128::from(*value)),
            Value::Int64(value) => NumericRef::Signed(i128::from(*value)),
            Value::Uint64(value) => NumericRef::Unsigned(u128::from(*value)),
            Value::NativeInt(value) => NumericRef::Signed(*value as i128),
            Value::NativeUint(value) => NumericRef::Unsigned(*value as u128),
            Value::Float32(value) => NumericRef::Float(f64::from(*value)),
            Value::Float64(value) => NumericRef::Float(*value),
            Value::BigInt(value) => NumericRef::BigInt(value),
            Value::Absent => return NativeValueRef::Absent,
            Value::Bool(value) => return NativeValueRef::Bool(*value),
            Value::Char(value) => return NativeValueRef::Char(*value),
            Value::String(value) => return NativeValueRef::String(value),
            Value::Identity(value) => return NativeValueRef::Identity(value.object_identity_key()),
            Value::SharedIdentity(value) => return NativeValueRef::SharedIdentity(value),
            Value::Error(error) => return NativeValueRef::Identity(error.error_identity_key()),
            Value::Closed(value) => {
                return match value.identity_key() {
                    Some(identity) => NativeValueRef::Identity(identity),
                    None => NativeValueRef::Passive(Rc::as_ptr(value).cast::<()>().addr()),
                }
            }
        };
        NativeValueRef::Numeric(numeric)
    }
}

impl Default for TsValue {
    fn default() -> Self {
        Self(Value::Absent)
    }
}

impl From<()> for TsValue {
    fn from(_: ()) -> Self {
        Self::default()
    }
}

impl From<ObjectIdentity> for TsValue {
    fn from(value: ObjectIdentity) -> Self {
        Self(Value::Identity(value))
    }
}

impl From<EmptyObject> for TsValue {
    fn from(value: EmptyObject) -> Self {
        Self::from(value.into_identity())
    }
}

impl<Payload: 'static, Context: 'static> From<ObjectRef<Payload, Context>> for TsValue {
    fn from(value: ObjectRef<Payload, Context>) -> Self {
        Self::from_shared_identity(value.into_shared())
    }
}

impl<Payload: 'static, Context: 'static> From<ObjectHandle<Payload, Context>> for TsValue {
    fn from(value: ObjectHandle<Payload, Context>) -> Self {
        Self::from_shared_identity(value.into_shared())
    }
}

impl NativeValue for () {
    #[inline]
    fn native_value_ref(&self) -> NativeValueRef<'_> {
        NativeValueRef::Absent
    }
}

macro_rules! native_values {
    ($($native:ty => $variant:ident, $value:ident => $view:expr);* $(;)?) => {$(
        impl From<$native> for TsValue {
            #[inline]
            fn from(value: $native) -> Self { Self(Value::$variant(value)) }
        }
        impl NativeValue for $native {
            #[inline]
            fn native_value_ref(&self) -> NativeValueRef<'_> {
                let $value = self;
                $view
            }
        }
    )*};
}

native_values! {
    bool => Bool, value => NativeValueRef::Bool(*value);
    char => Char, value => NativeValueRef::Char(*value);
    String => String, value => NativeValueRef::String(value);
    BigInt => BigInt, value => NativeValueRef::Numeric(NumericRef::BigInt(value));
    i8 => Int8, value => NativeValueRef::Numeric(NumericRef::Signed(i128::from(*value)));
    u8 => Uint8, value => NativeValueRef::Numeric(NumericRef::Unsigned(u128::from(*value)));
    i16 => Int16, value => NativeValueRef::Numeric(NumericRef::Signed(i128::from(*value)));
    u16 => Uint16, value => NativeValueRef::Numeric(NumericRef::Unsigned(u128::from(*value)));
    i32 => Int32, value => NativeValueRef::Numeric(NumericRef::Signed(i128::from(*value)));
    u32 => Uint32, value => NativeValueRef::Numeric(NumericRef::Unsigned(u128::from(*value)));
    i64 => Int64, value => NativeValueRef::Numeric(NumericRef::Signed(i128::from(*value)));
    u64 => Uint64, value => NativeValueRef::Numeric(NumericRef::Unsigned(u128::from(*value)));
    isize => NativeInt, value => NativeValueRef::Numeric(NumericRef::Signed(*value as i128));
    usize => NativeUint, value => NativeValueRef::Numeric(NumericRef::Unsigned(*value as u128));
    f32 => Float32, value => NativeValueRef::Numeric(NumericRef::Float(f64::from(*value)));
    f64 => Float64, value => NativeValueRef::Numeric(NumericRef::Float(*value));
}

impl NativeValue for str {
    #[inline]
    fn native_value_ref(&self) -> NativeValueRef<'_> {
        NativeValueRef::String(self)
    }
}

impl OptionalStorage<TsValue> for TsValue {
    fn present(value: Self) -> Self {
        value
    }
    fn absent() -> Self {
        Self::default()
    }
    fn is_absent(&self) -> bool {
        matches!(self.0, Value::Absent)
    }
    fn into_present(self) -> Self {
        assert!(!self.is_absent(), "native optional value is absent");
        self
    }
    fn clone_present(&self) -> Self {
        assert!(!self.is_absent(), "native optional value is absent");
        self.clone()
    }
}

impl PartialEq for TsValue {
    fn eq(&self, other: &Self) -> bool {
        native_values_equal(self, other)
    }
}

impl fmt::Debug for TsValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TsValue")
    }
}

pub fn clone_ts_value(value: &TsValue) -> TsValue {
    value.clone()
}
