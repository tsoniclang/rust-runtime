use tsonic_rust_runtime::{JsError, JsErrorKind, ToSourceString, TsonicError};

#[test]
fn error_kind_names_share_one_borrowed_and_display_contract() {
    for (kind, name) in [
        (JsErrorKind::Error, "Error"),
        (JsErrorKind::AggregateError, "AggregateError"),
        (JsErrorKind::EvalError, "EvalError"),
        (JsErrorKind::ReferenceError, "ReferenceError"),
        (JsErrorKind::TypeError, "TypeError"),
        (JsErrorKind::RangeError, "RangeError"),
        (JsErrorKind::SyntaxError, "SyntaxError"),
        (JsErrorKind::URIError, "URIError"),
        (JsErrorKind::Unsupported, "Unsupported"),
    ] {
        assert_eq!(kind.as_str(), name);
        assert_eq!(kind.to_string(), name);
    }
}

#[test]
fn source_error_strings_retain_the_exact_runtime_display() {
    let error = JsError::new(JsErrorKind::TypeError, "invalid value");
    assert_eq!(error.to_source_string(), "TypeError: invalid value");
    assert_eq!(
        TsonicError::from(error).to_source_string(),
        "TypeError: invalid value"
    );
    let node = TsonicError::Node {
        code: "ENOENT".into(),
        message: "missing".into(),
    };
    assert_eq!(node.to_source_string(), "ENOENT: missing");
}

#[test]
fn unsupported_error_is_closed_and_displayable() {
    let error = TsonicError::unsupported("dynamic eval is unavailable");
    assert_eq!(
        error,
        TsonicError::Unsupported {
            message: "dynamic eval is unavailable".to_string()
        }
    );
    assert_eq!(
        error.to_string(),
        "Unsupported: dynamic eval is unavailable"
    );
}

#[test]
fn js_error_accessors_and_conversion_are_closed() {
    let error = JsError::new(JsErrorKind::Unsupported, "not implemented");
    assert_eq!(error.kind(), JsErrorKind::Unsupported);
    assert_eq!(error.message(), "not implemented");
    assert_eq!(TsonicError::from(error.clone()), TsonicError::Js(error));
}

#[test]
fn base_error_kind_displays_as_error() {
    let error = JsError::new(JsErrorKind::Error, "boom");
    assert_eq!(error.kind(), JsErrorKind::Error);
    assert_eq!(format!("{error}"), "Error: boom");
    let unified: TsonicError = error.into();
    assert_eq!(format!("{unified}"), "Error: boom");
}

#[test]
fn error_identity_is_distinct_from_diagnostic_equality() {
    let original = JsError::error("failure");
    let alias = original.clone();
    let independent = JsError::error("failure");
    assert!(original.has_same_identity(&alias));
    assert!(!original.has_distinct_identity(&alias));
    assert!(original.has_distinct_identity(&independent));
    assert!(!original.has_same_identity(&independent));
    assert_eq!(original, independent);
}

#[cfg(feature = "std")]
#[inline(never)]
fn create_error_at_origin() -> JsError {
    JsError::new(JsErrorKind::TypeError, "invalid 😀 value")
}

#[cfg(feature = "std")]
#[inline(never)]
fn read_error_stack_elsewhere(error: &JsError) -> Option<String> {
    error.stack()
}

#[cfg(feature = "std")]
#[test]
fn error_stack_retains_creation_frames_across_aliases_and_later_reads() {
    let error = create_error_at_origin();
    let alias = error.clone();
    let stack = read_error_stack_elsewhere(&alias).expect("native test backtrace is available");
    assert!(stack.starts_with("TypeError: invalid 😀 value\n"));
    assert!(stack.contains("create_error_at_origin"));
    assert!(!stack.contains("read_error_stack_elsewhere"));
    assert_eq!(error.stack().as_deref(), Some(stack.as_str()));
    assert!(error.has_same_identity(&alias));
    assert!(!error.has_same_identity(&create_error_at_origin()));
}

#[cfg(feature = "std")]
#[test]
fn empty_error_stack_header_has_no_invented_colon_or_message() {
    let error = JsError::error("");
    assert!(error
        .stack()
        .expect("native test backtrace is available")
        .starts_with("Error\n"));
}

#[cfg(not(feature = "std"))]
#[test]
fn alloc_only_errors_report_stack_unavailability() {
    let error = JsError::error("alloc only");
    assert_eq!(error.stack(), None);
    assert!(error.has_same_identity(&error.clone()));
}
