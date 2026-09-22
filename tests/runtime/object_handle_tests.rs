use tsonic_rust_runtime::{EmptyObjectState, ObjectHandle, ObjectIdentity, ObjectRef};

#[test]
fn inline_state_lives_inside_the_existing_dispatch_owner() {
    use std::rc::Rc;
    use tsonic_rust_runtime::ObjectState;
    let root = Rc::new(ObjectState::new(String::from("first")));
    let alias = Rc::clone(&root);
    alias.with_mut(|value| value.push_str(" second"));
    assert_eq!(root.with(String::clone), "first second");
    drop(root);
    assert_eq!(Rc::strong_count(&alias), 1);
    assert_eq!(alias.with(String::len), 12);
}

struct NonDebugState;

struct CaptureContext {
    value: std::rc::Rc<std::cell::Cell<i32>>,
    drops: std::rc::Rc<std::cell::Cell<usize>>,
}

impl Drop for CaptureContext {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn mutable_object_context_is_borrowed_independently_and_retained_once() {
    use std::cell::Cell;
    use std::rc::Rc;

    let value = Rc::new(Cell::new(4));
    let drops = Rc::new(Cell::new(0));
    let object = ObjectHandle::with_context(3, CaptureContext {
        value: Rc::clone(&value),
        drops: Rc::clone(&drops),
    });
    let alias = object.clone();
    let context = object.context();
    alias.with_mut(|state| *state += context.value.get());
    assert_eq!(object.with(|state| *state), 7);
    assert_eq!(Rc::strong_count(&value), 2);
    assert!(ObjectHandle::same(&object, &alias));
    drop(object);
    assert_eq!(drops.get(), 0);
    value.set(9);
    assert_eq!(alias.context().value.get(), 9);
    drop(alias);
    assert_eq!(drops.get(), 1);
    assert_eq!(Rc::strong_count(&value), 1);
}

#[test]
fn immutable_object_context_is_not_copied_when_the_object_is_cloned() {
    use std::cell::Cell;
    use std::rc::Rc;

    let value = Rc::new(Cell::new(4));
    let drops = Rc::new(Cell::new(0));
    let object = ObjectRef::with_context(String::from("native"), CaptureContext {
        value: Rc::clone(&value),
        drops: Rc::clone(&drops),
    });
    let alias = object.clone();
    assert!(core::ptr::eq(object.context(), alias.context()));
    assert_eq!(Rc::strong_count(&value), 2);
    assert_eq!(alias.with(String::len), 6);
    assert!(ObjectRef::same(&object, &alias));
    drop(object);
    assert_eq!(drops.get(), 0);
    drop(alias);
    assert_eq!(drops.get(), 1);
}

#[test]
fn contextual_objects_keep_the_same_single_pointer_handle_size() {
    assert_eq!(core::mem::size_of::<ObjectHandle<i32, CaptureContext>>(), core::mem::size_of::<ObjectHandle<i32>>());
    assert_eq!(core::mem::size_of::<ObjectRef<i32, CaptureContext>>(), core::mem::size_of::<ObjectRef<i32>>());
}

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

#[test]
fn shared_mutable_root_round_trip_preserves_allocation_state_and_freeze_identity() {
    use std::rc::Rc;
    use tsonic_rust_runtime::{ObjectHandleState, ObjectIdentityCarrier};

    trait View: ObjectIdentityCarrier {
        fn read(&self) -> i32;
        fn add(self: Rc<Self>, value: i32);
    }

    impl View for ObjectHandleState<i32, String> {
        fn read(&self) -> i32 {
            self.with(|value| *value)
        }

        fn add(self: Rc<Self>, value: i32) {
            let instance = ObjectHandle::from_shared(self);
            instance.with_mut(|current| *current += value);
        }
    }

    let instance = ObjectHandle::with_context(4, String::from("context"));
    let retained = instance.clone().into_shared();
    let address = Rc::as_ptr(&retained);
    assert_eq!(Rc::strong_count(&retained), 2);
    let restored = ObjectHandle::from_shared(retained);
    assert!(ObjectHandle::same(&instance, &restored));
    let root = restored.into_shared();
    assert_eq!(Rc::as_ptr(&root), address);
    let view: Rc<dyn View> = root;
    assert_eq!(Rc::as_ptr(&view).cast::<()>(), address.cast::<()>());
    assert_eq!(Rc::strong_count(&view), 2);
    Rc::clone(&view).add(3);
    assert_eq!(view.read(), 7);
    assert_eq!(instance.with(|value| *value), 7);
    assert_eq!(instance.context(), "context");
    view.object_identity().freeze();
    assert!(instance.validate_data_write().is_err());
    assert!(ObjectIdentity::same(instance.object_identity(), view.object_identity()));
    drop(instance);
    assert_eq!(Rc::strong_count(&view), 1);
    assert_eq!(view.read(), 7);
}

#[test]
fn shared_immutable_root_retains_borrowed_context_without_static_bounds_or_copying() {
    use std::rc::Rc;
    use tsonic_rust_runtime::{ObjectIdentityCarrier, ObjectRefState};

    trait View: ObjectIdentityCarrier {
        fn read(&self) -> &str;
    }

    impl<'value> View for ObjectRefState<&'value str, &'value str> {
        fn read(&self) -> &str {
            self.with(|value| *value)
        }
    }

    let text = String::from("borrowed");
    let instance = ObjectRef::with_context(text.as_str(), text.as_str());
    let retained = instance.clone().into_shared();
    let address = Rc::as_ptr(&retained);
    let restored = ObjectRef::from_shared(retained);
    assert!(ObjectRef::same(&instance, &restored));
    let view: Rc<dyn View + '_> = restored.into_shared();
    assert_eq!(Rc::as_ptr(&view).cast::<()>(), address.cast::<()>());
    assert_eq!(view.read().as_ptr(), text.as_ptr());
    assert!(ObjectIdentity::same(instance.object_identity(), view.object_identity()));
    drop(instance);
    assert_eq!(Rc::strong_count(&view), 1);
    assert_eq!(view.read(), "borrowed");
}
