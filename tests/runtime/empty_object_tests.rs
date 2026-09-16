use tsonic_rust_runtime::{source_object_identity, EmptyObject, ObjectIdentityCarrier};

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

#[test]
fn erased_identity_keeps_the_original_token_and_frozen_state() {
    let original = EmptyObject::new();
    let identity = source_object_identity(&original);
    let other = source_object_identity(&EmptyObject::new());
    assert_eq!(identity.key(), original.object_identity().key());
    assert_ne!(identity, other);
    identity.freeze();
    assert!(original.is_frozen());
    assert!(!other.is_frozen());
}
