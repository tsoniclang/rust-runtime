use tsonic_rust_runtime::{source_string, BigInt, JsErrorKind, TsonicError};

#[test]
fn bigint_preserves_arbitrary_precision_and_immutable_clone_values() {
    let original = BigInt::from_decimal_literal("1234567890123456789012345678901234567890");
    let seven = BigInt::from_decimal_literal("7");
    let sum = original.clone() + seven.clone();

    assert_eq!(sum - original.clone(), seven);
    assert_eq!(
        source_string(&original),
        "1234567890123456789012345678901234567890"
    );
}

#[test]
fn bigint_supports_arithmetic_assignment_comparison_and_negation() {
    let mut value = BigInt::from_decimal_literal("12");
    value += BigInt::from_decimal_literal("5");
    value -= BigInt::from_decimal_literal("2");
    value *= BigInt::from_decimal_literal("3");

    assert_eq!(value, BigInt::from_decimal_literal("45"));
    assert!(-value < BigInt::from_decimal_literal("0"));
}

#[test]
fn bigint_division_and_remainder_are_catchable_and_match_javascript_signs() {
    let seven = BigInt::from_decimal_literal("7");
    let negative_seven = BigInt::from_decimal_literal("-7");
    let three = BigInt::from_decimal_literal("3");
    let zero = BigInt::from_decimal_literal("0");

    assert_eq!(
        BigInt::checked_div(seven.clone(), three.clone()).expect("7n / 3n"),
        BigInt::from_decimal_literal("2"),
    );
    assert_eq!(
        BigInt::checked_div(negative_seven.clone(), three.clone()).expect("-7n / 3n"),
        BigInt::from_decimal_literal("-2"),
    );
    assert_eq!(
        BigInt::checked_rem(negative_seven, three).expect("-7n % 3n"),
        BigInt::from_decimal_literal("-1"),
    );

    for result in [
        BigInt::checked_div(seven.clone(), zero.clone()),
        BigInt::checked_rem(seven.clone(), zero),
    ] {
        match result.expect_err("division by zero must reject") {
            TsonicError::Js(error) => {
                assert_eq!(error.kind(), JsErrorKind::RangeError);
                assert_eq!(error.message(), "Division by zero");
            }
            other => panic!("unexpected bigint error: {other}"),
        }
    }
}
