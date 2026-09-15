use tsonic_rust_runtime::EmptyObject;

#[test]
fn empty_object_identity_and_frozen_state_belong_to_the_retained_object() {
    let first = EmptyObject::new();
    let alias = first.clone();
    let second = EmptyObject::default();
    assert_eq!(first, alias);
    assert_ne!(first, second);
    assert!(!alias.is_frozen());
    assert_eq!(first, alias.freeze());
    assert!(first.is_frozen());
    assert!(!second.is_frozen());
}
