use tsonic_rust_runtime::object_identity::{source_objects_equal, source_objects_not_equal};
use tsonic_rust_runtime::ObjectIdentity;
use tsonic_rust_runtime::{ObjectHandle, ObjectRef};

#[test]
fn freezing_is_shared_by_every_view_of_one_identity() {
    let value = ObjectHandle::new(7_u32);
    let alias = value.clone();
    let view = ObjectHandle::with_identity(3_u64, value.object_identity().clone());
    let other = ObjectHandle::new(7_u32);
    assert!(value.validate_data_write().is_ok());
    let frozen = tsonic_rust_runtime::freeze_object(&value);
    assert!(source_objects_equal(&value, &frozen));
    assert!(tsonic_rust_runtime::object_is_frozen(&alias));
    assert!(tsonic_rust_runtime::object_is_frozen(&view));
    assert!(!tsonic_rust_runtime::object_is_frozen(&other));
    for error in [
        value.validate_data_write(),
        alias.validate_data_write(),
        view.validate_data_write(),
    ] {
        assert_eq!(
            error.unwrap_err().source_error().kind(),
            tsonic_rust_runtime::JsErrorKind::TypeError
        );
    }
    assert!(other.validate_data_write().is_ok());
}

#[test]
fn freezing_an_object_does_not_freeze_its_nested_objects() {
    let nested = ObjectHandle::new(1_i32);
    let parent = ObjectHandle::new(nested.clone());
    tsonic_rust_runtime::freeze_object(&parent);
    assert!(parent.validate_data_write().is_err());
    assert!(nested.validate_data_write().is_ok());
    nested.with_mut(|value| *value = 2);
    assert_eq!(parent.with(|child| child.with(|value| *value)), 2);
}

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
fn constructor_views_share_only_their_explicit_owner_identity() {
    let identity = ObjectIdentity::new();
    let first = ObjectHandle::with_identity(1_u32, identity.clone());
    let second = ObjectHandle::with_identity(2_u32, identity.clone());
    let other_shape = ObjectHandle::with_identity(3_u64, identity);
    let different = ObjectHandle::with_identity(1_u32, ObjectIdentity::new());
    assert_eq!(first, second);
    assert!(source_objects_equal(&first, &other_shape));
    assert_ne!(first, different);
    assert_ne!(first, ObjectHandle::new(1_u32));
}

#[test]
fn stable_owner_views_compare_without_materializing_identity_state() {
    use core::cell::OnceCell;
    use std::rc::Rc;
    use tsonic_rust_runtime::ObjectIdentityCarrier;

    struct Owner(OnceCell<ObjectIdentity>);

    impl ObjectIdentityCarrier for Owner {
        fn object_identity(&self) -> &ObjectIdentity {
            self.0.get_or_init(ObjectIdentity::new)
        }

        fn object_identity_key(&self) -> usize {
            core::ptr::from_ref(self).addr()
        }
    }

    struct View(Rc<Owner>);

    impl ObjectIdentityCarrier for View {
        fn object_identity(&self) -> &ObjectIdentity {
            self.0.object_identity()
        }

        fn object_identity_key(&self) -> usize {
            self.0.object_identity_key()
        }
    }

    let owner = Rc::new(Owner(OnceCell::new()));
    let view = View(owner.clone());
    let other = Rc::new(Owner(OnceCell::new()));
    assert!(source_objects_equal(owner.as_ref(), &view));
    assert!(source_objects_not_equal(other.as_ref(), &view));
    assert!(owner.0.get().is_none());
    assert!(other.0.get().is_none());
    owner.object_identity().freeze();
    assert!(view.object_identity().is_frozen());
    assert!(source_objects_equal(owner.as_ref(), &view));
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
