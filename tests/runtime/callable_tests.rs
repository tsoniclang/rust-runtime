use tsonic_rust_runtime::Callable;

#[test]
fn callable_invokes_and_clones_one_identity() {
    let callable = Callable::new(|(left, right): (i32, i32)| left + right);
    let alias = callable.clone();

    assert_eq!(callable.call((2, 3)), 5);
    assert!(Callable::same(&callable, &alias));
    assert!(!Callable::same(
        &callable,
        &Callable::new(|(left, right): (i32, i32)| left + right),
    ));
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
