use tsonic_rust_runtime::object_identity::{source_objects_equal, source_objects_not_equal};
use tsonic_rust_runtime::ObjectIdentity;
use tsonic_rust_runtime::{ObjectHandle, ObjectRef};

#[test]
fn structural_comparisons_preserve_identity_not_field_equality() {
    let original = ObjectHandle::new(7_u32);
    let alias = original.clone();
    let distinct = ObjectHandle::new(7_u32);
    let other_payload = ObjectHandle::new(7_u64);
    let immutable = ObjectRef::new(7_u32);

    assert!(source_objects_equal(&original, &alias));
    assert!(!source_objects_not_equal(&original, &alias));
    assert!(!source_objects_equal(&original, &distinct));
    assert!(source_objects_not_equal(&original, &distinct));
    assert!(!source_objects_equal(&original, &other_payload));
    assert!(!source_objects_equal(&original, &immutable));
    original.with_mut(|value| *value = 9);
    assert!(source_objects_equal(&original, &alias));
    assert_eq!(alias.with(|value| *value), 9);
}

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
#[test]
fn reachability_borrows_the_owner_without_shortening_its_drop_scope() {
    let owner = tsonic_rust_runtime::ObjectIdentity::new();
    let weak = owner.downgrade();
    assert!(weak.is_alive());
    tsonic_rust_runtime::keep_alive(&owner);
    assert!(weak.is_alive());
    assert!(weak.matches(&owner));
    drop(owner);
    assert!(!weak.is_alive());
}
