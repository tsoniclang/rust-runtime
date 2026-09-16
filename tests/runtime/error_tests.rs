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
        (JsErrorKind::SuppressedError, "SuppressedError"),
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
        source: JsError::error("missing"),
    };
    assert_eq!(node.to_source_string(), "ENOENT: missing");
}

#[test]
fn unsupported_error_is_closed_and_displayable() {
    let error = TsonicError::unsupported("dynamic eval is unavailable");
    assert_eq!(
        error,
        TsonicError::Unsupported {
            source: JsError::new(JsErrorKind::Unsupported, "dynamic eval is unavailable")
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
fn transported_error_preserves_identity_and_explicit_stack() {
    let original = JsError::new(JsErrorKind::RangeError, "stored failure");
    assert_eq!(original.stack(), None);
    #[cfg(feature = "std")]
    tsonic_rust_runtime::capture_error_stack(&original);
    let stack = original.stack();
    let transport = TsonicError::from(original.clone());
    let rethrown = transport.clone();
    let TsonicError::Js(restored) = rethrown else {
        panic!("native Error changed transport variant");
    };
    assert!(restored.has_same_identity(&original));
    assert_eq!(restored.kind(), JsErrorKind::RangeError);
    assert_eq!(restored.message(), "stored failure");
    assert_eq!(restored.stack(), stack);
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
fn every_native_error_retains_one_observable_source_identity() {
    let original = JsError::new(JsErrorKind::TypeError, "failure");
    for error in [
        TsonicError::from(original.clone()),
        TsonicError::Node {
            code: "ENOENT".into(),
            source: JsError::error("missing"),
        },
        TsonicError::unsupported("unsupported"),
        TsonicError::suppressed(original.clone().into(), original.clone().into()),
    ] {
        let source = error.error_value();
        assert_eq!(source.stack(), None);
        let stack = source.stack();
        let rethrown = error.clone();
        assert!(rethrown.is_error());
        assert!(rethrown.is_error_kind(source.kind()));
        assert!(rethrown.error_value().has_same_identity(&source));
        assert_eq!(rethrown.error_value().stack(), stack);
    }
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

#[test]
fn aliases_and_error_transport_share_message_storage() {
    let original = JsError::error(&"long message".repeat(100));
    let alias = original.clone();
    let transported = TsonicError::from(original.clone()).error_value();
    assert!(core::ptr::eq(original.message(), alias.message()));
    assert!(core::ptr::eq(original.message(), transported.message()));
}

#[cfg(feature = "std")]
#[inline(never)]
fn create_error_at_origin() -> JsError {
    let error = JsError::new(JsErrorKind::TypeError, "invalid 😀 value");
    tsonic_rust_runtime::capture_error_stack(&error);
    error
}

#[cfg(feature = "std")]
#[inline(never)]
fn read_error_stack_elsewhere(error: &JsError) -> Option<String> {
    error.stack()
}

#[cfg(feature = "std")]
#[test]
fn error_stack_retains_explicit_capture_frames_across_aliases_and_later_reads() {
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
    assert_eq!(error.stack(), None);
    tsonic_rust_runtime::capture_error_stack(&error);
    assert!(error
        .stack()
        .expect("native test backtrace is available")
        .starts_with("Error\n"));
}

#[test]
fn ordinary_error_reads_and_transport_do_not_capture() {
    for kind in [
        JsErrorKind::Error,
        JsErrorKind::TypeError,
        JsErrorKind::RangeError,
        JsErrorKind::URIError,
    ] {
        let error = JsError::new(kind, "failure");
        assert_eq!(error.stack(), None);
        let alias = error.clone();
        let transported = TsonicError::from(error.clone());
        assert_eq!(alias.stack(), None);
        assert_eq!(transported.error_value().stack(), None);
        assert!(transported.error_value().has_same_identity(&error));
    }
}

#[cfg(feature = "std")]
#[test]
fn explicit_capture_after_aliasing_replaces_previous_snapshot() {
    let error = create_error_at_origin();
    let alias = error.clone();
    let previous = alias.stack();
    tsonic_rust_runtime::capture_error_stack(&alias);
    let current = error.stack().expect("native test backtrace is available");
    assert_ne!(Some(&current), previous.as_ref());
    assert!(current.contains("explicit_capture_after_aliasing_replaces_previous_snapshot"));
    assert!(!current.contains("create_error_at_origin"));
    assert_eq!(alias.stack().as_ref(), Some(&current));
}

#[cfg(not(feature = "std"))]
#[test]
fn alloc_only_errors_report_stack_unavailability() {
    let error = JsError::error("alloc only");
    assert_eq!(error.stack(), None);
    assert!(error.has_same_identity(&error.clone()));
}
