use tsonic_rust_runtime::ObjectHandle;

#[test]
fn cloned_handles_share_mutable_state_and_identity() {
    let first = ObjectHandle::new((3_i32, String::from("initial")));
    let alias = first.clone();

    alias.with_mut(|state| {
        state.0 += 4;
        state.1 = String::from("updated");
    });

    assert_eq!(
        first.with(|state| state.clone()),
        (7, String::from("updated"))
    );
    assert!(ObjectHandle::same(&first, &alias));
    assert_eq!(first, alias);
}

#[test]
fn independently_allocated_equal_states_have_distinct_identity() {
    let first = ObjectHandle::new((7_i32, String::from("same")));
    let second = ObjectHandle::new((7_i32, String::from("same")));

    assert_eq!(
        first.with(|state| state.clone()),
        second.with(|state| state.clone())
    );
    assert!(!ObjectHandle::same(&first, &second));
    assert_ne!(first, second);
}
