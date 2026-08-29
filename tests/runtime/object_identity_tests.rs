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

#[test]
fn weak_identity_does_not_keep_its_owner_alive() {
    let identity = ObjectIdentity::new();
    let alias = identity.clone();
    let weak = identity.downgrade();

    assert!(weak.is_alive());
    assert!(weak.matches(&alias));
    assert_eq!(weak.key(), alias.key());

    drop(identity);
    assert!(weak.is_alive());
    drop(alias);
    assert!(!weak.is_alive());
}
