use tsonic_rust_runtime::{iter_cloned, iter_copied};

#[test]
fn copied_iteration_yields_owned_scalar_values_without_consuming_the_source() {
    let values = vec![1_i32, 2, 3];
    let collected = iter_copied(values.as_slice()).collect::<Vec<_>>();

    assert_eq!(collected, values);
    assert_eq!(values.len(), 3);
}

#[test]
fn cloned_iteration_yields_owned_reference_values_without_consuming_the_source() {
    let values = vec![String::from("first"), String::from("second")];
    let mut collected = iter_cloned(values.as_slice()).collect::<Vec<_>>();
    collected[0].push('!');

    assert_eq!(collected[0], "first!");
    assert_eq!(values[0], "first");
}
