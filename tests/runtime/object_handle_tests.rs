use tsonic_rust_runtime::{EmptyObjectState, ObjectHandle, ObjectIdentity, ObjectRef};

struct NonDebugState;

#[test]
fn empty_object_state_is_zero_sized_and_handle_compatible() {
    assert_eq!(std::mem::size_of::<EmptyObjectState>(), 0);
    let state = ObjectHandle::new(EmptyObjectState);
    assert_eq!(state.with(|value| *value), EmptyObjectState);
}

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

#[test]
fn mutable_and_immutable_object_carriers_preserve_exact_identity_across_aliases() {
    let mutable = ObjectHandle::new(3_i32);
    let mutable_alias = mutable.clone();
    let immutable = ObjectRef::new(3_i32);
    let immutable_alias = immutable.clone();

    assert!(ObjectIdentity::same(
        mutable.object_identity(),
        mutable_alias.object_identity(),
    ));
    assert!(ObjectIdentity::same(
        immutable.object_identity(),
        immutable_alias.object_identity(),
    ));
    assert!(!ObjectIdentity::same(
        mutable.object_identity(),
        immutable.object_identity(),
    ));
}

#[test]
fn debug_represents_handle_identity_without_inspecting_state() {
    let state = ObjectHandle::new(NonDebugState);
    let alias = state.clone();

    assert_eq!(format!("{state:?}"), "ObjectHandle");
    assert_eq!(format!("{state:?}"), format!("{alias:?}"));
}
