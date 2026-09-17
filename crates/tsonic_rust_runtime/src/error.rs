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
    SuppressedError,
    Unsupported,
}

impl JsErrorKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            JsErrorKind::Error => "Error",
            JsErrorKind::AggregateError => "AggregateError",
            JsErrorKind::EvalError => "EvalError",
            JsErrorKind::ReferenceError => "ReferenceError",
            JsErrorKind::TypeError => "TypeError",
            JsErrorKind::RangeError => "RangeError",
            JsErrorKind::SyntaxError => "SyntaxError",
            JsErrorKind::URIError => "URIError",
            JsErrorKind::SuppressedError => "SuppressedError",
            JsErrorKind::Unsupported => "Unsupported",
        }
    }
}

impl fmt::Display for JsErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Closed error type for JS-facing APIs.
#[derive(Clone)]
pub struct JsError {
    identity: SharedIdentity<ErrorIdentity>,
}

struct ErrorIdentity {
    kind: JsErrorKind,
    message: String,
    #[cfg(feature = "std")]
    stack: std::sync::Mutex<Option<String>>,
}

pub trait ErrorStack: crate::ToSourceString {
    fn set_stack(&self, stack: Option<String>);
}

#[cfg(feature = "std")]
pub fn capture_error_stack(error: &impl ErrorStack) {
    let origin = std::backtrace::Backtrace::force_capture();
    let stack = (origin.status() == std::backtrace::BacktraceStatus::Captured)
        .then(|| alloc::format!("{}\n{}", error.to_source_string(), origin));
    error.set_stack(stack);
}

impl JsError {
    pub fn new(kind: JsErrorKind, message: impl Into<String>) -> Self {
        Self {
            identity: SharedIdentity::new(ErrorIdentity {
                kind,
                message: message.into(),
                #[cfg(feature = "std")]
                stack: std::sync::Mutex::new(None),
            }),
        }
    }

    pub fn error(message: &str) -> Self {
        Self::new(JsErrorKind::Error, message)
    }

    pub fn kind(&self) -> JsErrorKind {
        self.identity.kind
    }

    pub fn message(&self) -> &str {
        &self.identity.message
    }

    pub fn stack(&self) -> Option<String> {
        #[cfg(feature = "std")]
        {
            self.identity
                .stack
                .lock()
                .expect("error stack lock poisoned")
                .clone()
        }
        #[cfg(not(feature = "std"))]
        {
            None
        }
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

#[cfg(feature = "std")]
impl ErrorStack for JsError {
    fn set_stack(&self, stack: Option<String>) {
        *self
            .identity
            .stack
            .lock()
            .expect("error stack lock poisoned") = stack;
    }
}

impl PartialEq for JsError {
    fn eq(&self, other: &Self) -> bool {
        self.kind() == other.kind() && self.message() == other.message()
    }
}

impl Eq for JsError {}

impl fmt::Debug for JsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JsError")
            .field("kind", &self.kind())
            .field("message", &self.message())
            .finish()
    }
}

impl fmt::Display for JsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind(), self.message())
    }
}

impl core::error::Error for JsError {}

impl crate::ToSourceString for JsError {
    fn to_source_string(&self) -> String {
        if self.message().is_empty() {
            self.kind().to_string()
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
        source: JsError,
    },
    Unsupported {
        source: JsError,
    },
    Suppressed {
        source: JsError,
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
            source: JsError::new(JsErrorKind::Unsupported, message),
        }
    }

    pub fn suppressed(error: TsonicError, suppressed: TsonicError) -> Self {
        Self::Suppressed {
            source: JsError::new(
                JsErrorKind::SuppressedError,
                "An error was suppressed during disposal.",
            ),
            error: Box::new(error),
            suppressed: Box::new(suppressed),
        }
    }

    pub fn source_error(&self) -> &JsError {
        match self {
            Self::Js(source)
            | Self::Node { source, .. }
            | Self::Unsupported { source }
            | Self::Suppressed { source, .. } => source,
        }
    }

    pub fn is_error(&self) -> bool {
        true
    }

    pub fn is_error_kind(&self, kind: JsErrorKind) -> bool {
        self.source_error().kind() == kind
    }

    pub fn error_value(&self) -> JsError {
        self.source_error().clone()
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
            TsonicError::Node { code, source } => write!(f, "{code}: {}", source.message()),
            TsonicError::Unsupported { source } => write!(f, "Unsupported: {}", source.message()),
            TsonicError::Suppressed {
                error, suppressed, ..
            } => {
                write!(f, "SuppressedError: {error}; suppressed: {suppressed}")
            }
        }
    }
}

impl core::error::Error for TsonicError {}
