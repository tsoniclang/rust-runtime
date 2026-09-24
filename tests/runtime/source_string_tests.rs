use tsonic_rust_runtime::{
    source_string, source_string_greater_than, source_string_greater_than_or_equal,
    source_string_less_than, source_string_less_than_or_equal,
};

#[test]
fn source_strings_cover_closed_primitive_carriers() {
    assert_eq!(source_string(&true), "true");
    assert_eq!(source_string(&42_i32), "42");
    assert_eq!(source_string(&String::from("text")), "text");
    assert_eq!(source_string("slice"), "slice");
    assert_eq!(source_string(&()), "null");
}

#[test]
fn source_number_strings_follow_native_formatting() {
    for value in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        1e-6,
        1e-7,
        1e20,
        1e21,
    ] {
        assert_eq!(source_string(&value), value.to_string());
    }
    for value in [0.1_f32, -0.0, f32::MIN_POSITIVE, f32::MAX] {
        assert_eq!(source_string(&value), value.to_string());
    }
}

#[test]
fn source_string_ordering_uses_native_utf8() {
    assert!(source_string_less_than("alpha", "beta"));
    assert!(source_string_less_than_or_equal("alpha", "alpha"));
    assert!(source_string_greater_than("beta", "alpha"));
    assert!(source_string_greater_than_or_equal("alpha", "alpha"));

    let supplementary_character = "\u{10000}";
    let private_use_character = "\u{e000}";
    assert!(supplementary_character > private_use_character);
    assert!(!source_string_less_than(
        supplementary_character,
        private_use_character
    ));
    assert!(source_string_greater_than_or_equal(
        supplementary_character,
        private_use_character
    ));
}
