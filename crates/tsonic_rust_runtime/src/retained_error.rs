use crate::{
    ErrorField, ErrorObject, ErrorStack, JsError, JsErrorKind, MutableJsError, TsonicError,
    WritableErrorObject,
};
use alloc::rc::Rc;
#[cfg(not(target_has_atomic = "ptr"))]
use alloc::rc::Rc as Shared;
use alloc::string::String;
#[cfg(target_has_atomic = "ptr")]
use alloc::sync::Arc as Shared;
use core::{any::Any, fmt};

pub trait RetainedErrorObject: ErrorObject + ErrorStack {
    fn project_error(self: Rc<Self>, output: &mut dyn Any)
    where
        Self: 'static;
}

pub trait WritableRetainedErrorObject: RetainedErrorObject + WritableErrorObject {}

impl<Value: RetainedErrorObject + WritableErrorObject> WritableRetainedErrorObject for Value {}

#[derive(Clone)]
pub enum RetainedError {
    Native(JsError),
    Runtime(Shared<TsonicError>),
    Created(MutableJsError),
    Project(Rc<dyn RetainedErrorObject>),
    WritableProject(Rc<dyn WritableRetainedErrorObject>),
}

#[derive(Clone)]
pub enum WritableRetainedError {
    Created(MutableJsError),
    Project(Rc<dyn WritableRetainedErrorObject>),
}

impl RetainedError {
    pub fn source_error(&self) -> Option<&dyn ErrorObject> {
        Some(match self {
            Self::Native(error) => error,
            Self::Runtime(error) => error.source_error(),
            Self::Created(error) => error,
            Self::Project(error) => error.as_ref(),
            Self::WritableProject(error) => error.as_ref(),
        })
    }

    pub fn source_error_value(&self) -> Option<Self> {
        Some(self.clone())
    }

    pub fn writable_source_error_value(&self) -> Option<WritableRetainedError> {
        match self {
            Self::Created(error) => Some(WritableRetainedError::Created(error.clone())),
            Self::WritableProject(error) => Some(WritableRetainedError::Project(error.clone())),
            Self::Native(_) | Self::Runtime(_) | Self::Project(_) => None,
        }
    }

    pub fn mutable_error_value(&self) -> Option<MutableJsError> {
        match self {
            Self::Created(error) => Some(error.clone()),
            Self::Native(_) | Self::Runtime(_) | Self::Project(_) | Self::WritableProject(_) => {
                None
            }
        }
    }

    pub fn native_error_value(&self) -> Option<JsError> {
        match self {
            Self::Native(error) => Some(error.clone()),
            Self::Runtime(error) => Some(error.source_error().clone()),
            Self::Created(_) | Self::Project(_) | Self::WritableProject(_) => None,
        }
    }

    pub fn project_error(&self, output: &mut dyn Any) {
        match self {
            Self::Project(error) => error.clone().project_error(output),
            Self::WritableProject(error) => error.clone().project_error(output),
            Self::Native(_) | Self::Runtime(_) | Self::Created(_) => {}
        }
    }

    pub fn as_error_object(&self) -> &dyn ErrorObject {
        match self {
            Self::Native(error) => error,
            Self::Runtime(error) => error.source_error(),
            Self::Created(error) => error,
            Self::Project(error) => error.as_ref(),
            Self::WritableProject(error) => error.as_ref(),
        }
    }
}

impl WritableRetainedError {
    pub fn as_error_object(&self) -> &dyn WritableErrorObject {
        match self {
            Self::Created(error) => error,
            Self::Project(error) => error.as_ref(),
        }
    }

    pub fn project_error(&self, output: &mut dyn Any) {
        if let Self::Project(error) = self {
            error.clone().project_error(output);
        }
    }

    pub fn mutable_error_value(&self) -> Option<MutableJsError> {
        match self {
            Self::Created(error) => Some(error.clone()),
            Self::Project(_) => None,
        }
    }

    pub fn native_error_value(&self) -> Option<JsError> {
        None
    }

    pub fn source_error(&self) -> Option<&dyn ErrorObject> {
        Some(self.as_error_object())
    }

    pub fn source_error_value(&self) -> Option<RetainedError> {
        Some(self.clone().into())
    }

    pub fn writable_source_error_value(&self) -> Option<Self> {
        Some(self.clone())
    }
}

impl From<JsError> for RetainedError {
    fn from(error: JsError) -> Self {
        Self::Native(error)
    }
}

impl From<TsonicError> for RetainedError {
    fn from(error: TsonicError) -> Self {
        match error {
            TsonicError::Js(error) => Self::Native(error),
            error => Self::Runtime(Shared::new(error)),
        }
    }
}

impl From<MutableJsError> for RetainedError {
    fn from(error: MutableJsError) -> Self {
        Self::Created(error)
    }
}

impl From<MutableJsError> for WritableRetainedError {
    fn from(error: MutableJsError) -> Self {
        Self::Created(error)
    }
}

impl From<WritableRetainedError> for RetainedError {
    fn from(error: WritableRetainedError) -> Self {
        match error {
            WritableRetainedError::Created(error) => Self::Created(error),
            WritableRetainedError::Project(error) => Self::WritableProject(error),
        }
    }
}

macro_rules! retained_error_field {
    ($value:expr, $field:ident) => {
        match $value {
            RetainedError::Native(error) => error.$field(),
            RetainedError::Runtime(error) => error.source_error().$field(),
            RetainedError::Created(error) => error.$field(),
            RetainedError::Project(error) => error.$field(),
            RetainedError::WritableProject(error) => error.$field(),
        }
    };
}

impl ErrorObject for RetainedError {
    fn error_name(&self) -> ErrorField<'_> {
        retained_error_field!(self, error_name)
    }
    fn error_message(&self) -> ErrorField<'_> {
        retained_error_field!(self, error_message)
    }
    fn error_stack(&self) -> Option<ErrorField<'_>> {
        retained_error_field!(self, error_stack)
    }
    fn error_kind(&self) -> JsErrorKind {
        retained_error_field!(self, error_kind)
    }
    fn error_identity_key(&self) -> usize {
        retained_error_field!(self, error_identity_key)
    }
}

macro_rules! writable_retained_error_field {
    ($value:expr, $field:ident $(, $argument:expr)*) => {
        match $value {
            WritableRetainedError::Created(error) => error.$field($($argument),*),
            WritableRetainedError::Project(error) => error.$field($($argument),*),
        }
    };
}

impl ErrorObject for WritableRetainedError {
    fn error_name(&self) -> ErrorField<'_> {
        writable_retained_error_field!(self, error_name)
    }
    fn error_message(&self) -> ErrorField<'_> {
        writable_retained_error_field!(self, error_message)
    }
    fn error_stack(&self) -> Option<ErrorField<'_>> {
        writable_retained_error_field!(self, error_stack)
    }
    fn error_kind(&self) -> JsErrorKind {
        writable_retained_error_field!(self, error_kind)
    }
    fn error_identity_key(&self) -> usize {
        writable_retained_error_field!(self, error_identity_key)
    }
}

impl WritableErrorObject for WritableRetainedError {
    fn set_error_name(&self, value: String) {
        writable_retained_error_field!(self, set_error_name, value);
    }
    fn set_error_message(&self, value: String) {
        writable_retained_error_field!(self, set_error_message, value);
    }
    fn set_error_stack(&self, value: Option<String>) {
        writable_retained_error_field!(self, set_error_stack, value);
    }
}

#[cfg(feature = "std")]
impl ErrorStack for RetainedError {
    fn set_stack(&self, value: Option<String>) {
        match self {
            Self::Native(error) => error.set_stack(value),
            Self::Runtime(error) => error.source_error().set_stack(value),
            Self::Created(error) => error.set_error_stack(value),
            Self::Project(error) => error.set_stack(value),
            Self::WritableProject(error) => error.set_stack(value),
        }
    }
}

impl ErrorStack for WritableRetainedError {
    fn set_stack(&self, value: Option<String>) {
        self.set_error_stack(value);
    }
}

impl fmt::Display for RetainedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.error_name(), self.error_message())
    }
}

impl fmt::Debug for RetainedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for WritableRetainedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.error_name(), self.error_message())
    }
}

impl fmt::Debug for WritableRetainedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl core::error::Error for RetainedError {}
impl core::error::Error for WritableRetainedError {}

impl crate::ToSourceString for RetainedError {
    fn to_source_string(&self) -> String {
        alloc::format!("{self}")
    }
}

impl crate::ToSourceString for WritableRetainedError {
    fn to_source_string(&self) -> String {
        alloc::format!("{self}")
    }
}
