use std::fmt;

/// Kinds of JS runtime errors supported by the closed Packet A runtime layer.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsError {
    pub kind: JsErrorKind,
    pub message: String,
}

impl JsError {
    pub fn new(kind: JsErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> JsErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for JsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for JsError {}

/// Unified error type for generated Rust emitted by Tsonic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TsonicError {
    Js(JsError),
    Node { code: String, message: String },
    Unsupported { message: String },
}

pub type TsonicResult<T> = Result<T, TsonicError>;

impl TsonicError {
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported {
            message: message.into(),
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
        }
    }
}

impl std::error::Error for TsonicError {}
