use tsonic_rust_runtime::{
    EmptyObjectState, LocalObjectHandle, LocalObjectRef, ThreadedObjectHandle, ThreadedObjectRef,
};

struct NonDebugState;

#[test]
fn empty_object_state_is_zero_sized_and_handle_compatible() {
    assert_eq!(std::mem::size_of::<EmptyObjectState>(), 0);
    let state = LocalObjectHandle::new(EmptyObjectState);
    assert_eq!(state.with(|value| *value), EmptyObjectState);
}

#[test]
fn cloned_handles_share_mutable_state_and_identity() {
    let first = LocalObjectHandle::new((3_i32, String::from("initial")));
    let alias = first.clone();

    alias.with_mut(|state| {
        state.0 += 4;
        state.1 = String::from("updated");
    });

    assert_eq!(
        first.with(|state| state.clone()),
        (7, String::from("updated"))
    );
    assert!(LocalObjectHandle::same(&first, &alias));
    assert_eq!(first, alias);
}

#[test]
fn independently_allocated_equal_states_have_distinct_identity() {
    let first = LocalObjectHandle::new((7_i32, String::from("same")));
    let second = LocalObjectHandle::new((7_i32, String::from("same")));

    assert_eq!(
        first.with(|state| state.clone()),
        second.with(|state| state.clone())
    );
    assert!(!LocalObjectHandle::same(&first, &second));
    assert_ne!(first, second);
}

#[test]
fn debug_represents_handle_identity_without_inspecting_state() {
    let state = LocalObjectHandle::new(NonDebugState);
    let alias = state.clone();

    assert_eq!(format!("{state:?}"), "LocalObjectHandle");
    assert_eq!(format!("{state:?}"), format!("{alias:?}"));
}

#[test]
fn threaded_object_handle_serializes_explicit_shared_mutation() {
    let state = ThreadedObjectHandle::new(1_i32);
    let worker_state = state.clone();
    std::thread::spawn(move || worker_state.with_mut(|value| *value += 2))
        .join()
        .expect("threaded object worker failed");

    assert_eq!(state.with(|value| *value), 3);
}

#[test]
fn local_and_threaded_read_only_carriers_preserve_exact_identity_domains() {
    let local = LocalObjectRef::new(String::from("local"));
    let local_alias = local.clone();
    assert!(LocalObjectRef::same(&local, &local_alias));
    assert_eq!(local.with(String::clone), "local");

    let threaded = ThreadedObjectRef::new(String::from("threaded"));
    let worker_value = threaded.clone();
    let observed = std::thread::spawn(move || worker_value.with(String::clone))
        .join()
        .expect("threaded object reference worker failed");
    assert_eq!(observed, "threaded");
    assert!(ThreadedObjectRef::same(&threaded, &threaded.clone()));
}
