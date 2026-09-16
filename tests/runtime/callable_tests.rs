use tsonic_rust_runtime::Callable;

#[test]
fn callable_does_not_require_borrowed_arguments_or_results_to_be_static() {
    let text = String::from("borrowed value");
    let identity = Callable::new(|(value,): (&str,)| value);
    let alias = identity.clone();
    assert_eq!(identity.call((&text,)), "borrowed value");
    assert_eq!(alias.call((&text,)), "borrowed value");
    assert!(Callable::same(&identity, &alias));
    assert_eq!(identity.identity_key(), alias.identity_key());
}

#[test]
fn callable_invokes_and_clones_one_identity() {
    let callable = Callable::new(|(left, right): (i32, i32)| left + right);
    let alias = callable.clone();
    let distinct = Callable::new(|(left, right): (i32, i32)| left + right);

    assert_eq!(callable.call((2, 3)), 5);
    assert!(Callable::same(&callable, &alias));
    assert!(!Callable::same(&callable, &distinct));
    assert!(callable == alias);
    assert!(callable != distinct);
    assert_eq!(callable.identity_key(), alias.identity_key());
    assert_ne!(callable.identity_key(), distinct.identity_key());
}

#[test]
fn recursive_callable_receives_its_exact_identity() {
    let factorial = Callable::recursive(|factorial, value: i32| {
        if value <= 1 {
            1
        } else {
            value * factorial.call(value - 1)
        }
    });

    assert_eq!(factorial.call(5), 120);
}
