use tsonic_rust_runtime::{source_string, BigInt};

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
