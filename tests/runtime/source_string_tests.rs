use tsonic_rust_runtime::{
    source_string, source_string_greater_than, source_string_greater_than_or_equal,
    source_string_less_than, source_string_less_than_or_equal, Undefined,
};

#[test]
fn source_strings_cover_closed_primitive_carriers() {
    assert_eq!(source_string(&true), "true");
    assert_eq!(source_string(&42_i32), "42");
    assert_eq!(source_string(&String::from("text")), "text");
    assert_eq!(source_string("slice"), "slice");
    assert_eq!(source_string(&()), "undefined");
    assert_eq!(source_string(&Undefined), "undefined");
}

#[test]
fn source_number_strings_follow_ecmascript_thresholds() {
    assert_eq!(source_string(&f64::NAN), "NaN");
    assert_eq!(source_string(&f64::INFINITY), "Infinity");
    assert_eq!(source_string(&f64::NEG_INFINITY), "-Infinity");
    assert_eq!(source_string(&-0.0_f64), "0");
    assert_eq!(source_string(&1e-6_f64), "0.000001");
    assert_eq!(source_string(&1e-7_f64), "1e-7");
    assert_eq!(source_string(&1e20_f64), "100000000000000000000");
    assert_eq!(source_string(&1e21_f64), "1e+21");
}

#[test]
fn source_string_ordering_uses_utf16_code_units() {
    assert!(source_string_less_than("alpha", "beta"));
    assert!(source_string_less_than_or_equal("alpha", "alpha"));
    assert!(source_string_greater_than("beta", "alpha"));
    assert!(source_string_greater_than_or_equal("alpha", "alpha"));

    let supplementary_character = "\u{10000}";
    let private_use_character = "\u{e000}";
    assert!(supplementary_character > private_use_character);
    assert!(source_string_less_than(
        supplementary_character,
        private_use_character
    ));
    assert!(!source_string_greater_than_or_equal(
        supplementary_character,
        private_use_character
    ));
}
