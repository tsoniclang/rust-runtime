use tsonic_rust_runtime::ObjectIdentity;

#[test]
fn cloned_identity_preserves_reference_identity() {
    let first = ObjectIdentity::new();
    let alias = first.clone();

    assert!(ObjectIdentity::same(&first, &alias));
    assert_eq!(first, alias);
}

#[test]
fn independently_created_identities_remain_distinct() {
    let first = ObjectIdentity::new();
    let second = ObjectIdentity::new();

    assert!(!ObjectIdentity::same(&first, &second));
    assert_ne!(first, second);
}
