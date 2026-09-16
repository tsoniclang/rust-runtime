use tsonic_rust_runtime::{source_string, BigInt, JsErrorKind, TsonicError};

#[test]
fn bigint_native_conversion_and_borrow_preserve_precision_and_shared_storage() {
    let native = num_bigint::BigInt::from(u128::MAX);
    let value = BigInt::from(native.clone());
    let alias = value.clone();
    assert_eq!(value.as_ref(), &native);
    assert!(core::ptr::eq(value.as_ref(), alias.as_ref()));
    assert_eq!(
        BigInt::from(num_bigint::BigInt::from(i128::MIN)).to_string(),
        i128::MIN.to_string()
    );
}

#[test]
fn bigint_shifts_preserve_signed_counts_and_large_right_shifts() {
    for (value, shift, expected_left, expected_right) in [
        ("3", "2", "12", "0"),
        ("-7", "1", "-14", "-4"),
        ("12", "-2", "3", "48"),
        ("-7", "-1", "-4", "-14"),
    ] {
        let value = BigInt::from_decimal_literal(value);
        let shift = BigInt::from_decimal_literal(shift);
        assert_eq!(
            BigInt::checked_shift_left(value.clone(), shift.clone())
                .unwrap()
                .to_string(),
            expected_left
        );
        assert_eq!(
            BigInt::checked_shift_right(value, shift)
                .unwrap()
                .to_string(),
            expected_right
        );
    }
    let huge = BigInt::from_decimal_literal("18446744073709551616");
    for (source, result) in [("3", "0"), ("-7", "-1")] {
        assert_eq!(
            BigInt::checked_shift_right(BigInt::from_decimal_literal(source), huge.clone())
                .unwrap()
                .to_string(),
            result
        );
    }
    assert!(BigInt::checked_shift_left(BigInt::from_decimal_literal("1"), huge.clone()).is_err());
    assert_eq!(
        BigInt::checked_shift_left(BigInt::from_decimal_literal("0"), huge)
            .unwrap()
            .to_string(),
        "0"
    );
}

#[test]
fn bigint_signed_bytes_round_trip_without_number_conversion() {
    for source in [
        "0",
        "-1",
        "9007199254740993",
        "-170141183460469231731687303715884105728",
    ] {
        let value = BigInt::from_decimal_literal(source);
        assert_eq!(
            BigInt::from_signed_bytes_le(&value.to_signed_bytes_le()),
            value
        );
    }
}

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
fn bigint_exposes_canonical_signed_little_endian_bytes() {
    assert_eq!(
        BigInt::from_decimal_literal("258").to_signed_bytes_le(),
        vec![2, 1],
    );
    assert_eq!(
        BigInt::from_decimal_literal("-2").to_signed_bytes_le(),
        vec![254],
    );
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
