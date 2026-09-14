use alloc::boxed::Box;
#[cfg(not(target_has_atomic = "ptr"))]
use alloc::rc::Rc as SharedIdentity;
use alloc::string::{String, ToString};
#[cfg(target_has_atomic = "ptr")]
use alloc::sync::Arc as SharedIdentity;
use core::fmt;

/// Kinds of JS runtime errors supported by the closed runtime layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsErrorKind {
    Error,
    AggregateError,
    EvalError,
    ReferenceError,
    TypeError,
    RangeError,
    SyntaxError,
    URIError,
    Unsupported,
}

impl fmt::Display for JsErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            JsErrorKind::Error => "Error",
            JsErrorKind::AggregateError => "AggregateError",
            JsErrorKind::EvalError => "EvalError",
            JsErrorKind::ReferenceError => "ReferenceError",
            JsErrorKind::TypeError => "TypeError",
            JsErrorKind::RangeError => "RangeError",
            JsErrorKind::SyntaxError => "SyntaxError",
            JsErrorKind::URIError => "URIError",
            JsErrorKind::Unsupported => "Unsupported",
        };
        write!(f, "{kind}")
    }
}

/// Closed error type for JS-facing APIs.
#[derive(Clone)]
pub struct JsError {
    pub kind: JsErrorKind,
    pub message: String,
    identity: SharedIdentity<()>,
}

impl JsError {
    pub fn new(kind: JsErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            identity: SharedIdentity::new(()),
        }
    }

    pub fn error(message: &str) -> Self {
        Self::new(JsErrorKind::Error, message)
    }

    pub fn kind(&self) -> JsErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn has_same_identity(&self, other: &Self) -> bool {
        SharedIdentity::ptr_eq(&self.identity, &other.identity)
    }

    pub fn has_distinct_identity(&self, other: &Self) -> bool {
        !self.has_same_identity(other)
    }

    pub fn identity_key(&self) -> usize {
        SharedIdentity::as_ptr(&self.identity) as usize
    }
}

impl PartialEq for JsError {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.message == other.message
    }
}

impl Eq for JsError {}

impl fmt::Debug for JsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JsError")
            .field("kind", &self.kind)
            .field("message", &self.message)
            .finish()
    }
}

impl fmt::Display for JsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl core::error::Error for JsError {}

impl crate::ToSourceString for JsError {
    fn to_source_string(&self) -> String {
        if self.message.is_empty() {
            self.kind.to_string()
        } else {
            self.to_string()
        }
    }
}

/// Unified error type for generated Rust emitted by Tsonic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TsonicError {
    Js(JsError),
    Node {
        code: String,
        message: String,
    },
    Unsupported {
        message: String,
    },
    Suppressed {
        error: Box<TsonicError>,
        suppressed: Box<TsonicError>,
    },
}

pub type TsonicResult<T> = Result<T, TsonicError>;

impl crate::ToSourceString for TsonicError {
    fn to_source_string(&self) -> String {
        self.to_string()
    }
}

impl TsonicError {
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported {
            message: message.into(),
        }
    }

    pub fn suppressed(error: TsonicError, suppressed: TsonicError) -> Self {
        Self::Suppressed {
            error: Box::new(error),
            suppressed: Box::new(suppressed),
        }
    }
}

impl From<JsError> for TsonicError {
    fn from(value: JsError) -> Self {
        Self::Js(value)
    }
}

impl fmt::Display for TsonicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TsonicError::Js(error) => write!(f, "{error}"),
            TsonicError::Node { code, message } => write!(f, "{code}: {message}"),
            TsonicError::Unsupported { message } => write!(f, "Unsupported: {message}"),
            TsonicError::Suppressed { error, suppressed } => {
                write!(f, "SuppressedError: {error}; suppressed: {suppressed}")
            }
        }
    }
}

impl core::error::Error for TsonicError {}
