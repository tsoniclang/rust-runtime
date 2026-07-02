use tsonic_rust_runtime::{JsError, JsErrorKind, TsonicError};

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
