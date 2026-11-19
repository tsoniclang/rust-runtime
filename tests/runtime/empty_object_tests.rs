use tsonic_rust_runtime::EmptyObject;

#[test]
fn empty_object_identity_belongs_to_the_retained_object() {
    let first = EmptyObject::new();
    let alias = first.clone();
    let second = EmptyObject::default();
    assert_eq!(first, alias);
    assert_ne!(first, second);
    assert_eq!(first, core::convert::identity(alias));
}
