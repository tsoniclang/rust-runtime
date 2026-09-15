use std::cell::Cell;
use std::rc::Rc;
use tsonic_rust_runtime::location::LocationSegment;
use tsonic_rust_runtime::{Location, ObjectIdentity, ObjectIdentityCarrier};

#[test]
fn fallible_views_keep_exact_errors_and_never_read_the_base() {
    let base = Location::bind(
        ObjectIdentity::new(),
        || -> i32 { panic!("base read") },
        |_| panic!("base write"),
    );
    let failure = Rc::new(String::from("selected failure"));
    let read_error = Rc::clone(&failure);
    let write_error = Rc::clone(&failure);
    let view = base.try_view(
        move || Err::<i64, _>(Rc::clone(&read_error)),
        move |_| Err(Rc::clone(&write_error)),
    );
    assert_eq!(Location::hash(Some(&base)), Location::hash(Some(&view)));
    assert!(Rc::ptr_eq(&view.try_load().unwrap_err(), &failure));
    assert!(Rc::ptr_eq(&view.try_store(3).unwrap_err(), &failure));
    let alias = view.clone();
    assert!(Location::same(Some(&view), Some(&alias)));
}

#[test]
fn fallible_projection_propagates_the_first_error_without_extra_effects() {
    enum Failure {
        Read,
        Convert,
        Write,
    }
    let reads = Rc::new(Cell::new(0));
    let stores = Rc::new(Cell::new(0));
    let read_count = Rc::clone(&reads);
    let store_count = Rc::clone(&stores);
    let source = Location::try_bind(
        ObjectIdentity::new(),
        move || {
            read_count.set(read_count.get() + 1);
            Err::<i32, _>(Failure::Read)
        },
        move |_| {
            store_count.set(store_count.get() + 1);
            Err(Failure::Write)
        },
    );
    let projected = source.try_map(
        |_| -> Result<i64, Failure> { panic!("read conversion") },
        |_| Err(Failure::Convert),
    );
    assert!(matches!(projected.try_load(), Err(Failure::Read)));
    assert!(matches!(projected.try_store(4), Err(Failure::Convert)));
    assert_eq!(reads.get(), 1);
    assert_eq!(stores.get(), 0);
    let writing = source.try_map(|value| Ok(i64::from(value)), |_| Ok(9));
    assert!(matches!(writing.try_store(4), Err(Failure::Write)));
    assert_eq!(stores.get(), 1);
    assert_eq!(reads.get(), 1);
}

#[test]
fn error_widening_retains_aliases_and_keeps_the_owner_alive() {
    let owner = Rc::new(Cell::new(3));
    let weak = Rc::downgrade(&owner);
    let reader = Rc::clone(&owner);
    let source = Location::bind(
        ObjectIdentity::new(),
        move || reader.get(),
        move |value| owner.set(value),
    );
    let widened: Location<i32, String> = source.into_fallible();
    assert_eq!(
        Location::hash(Some(&source)),
        Location::hash(Some(&widened))
    );
    assert_eq!(widened.try_load(), Ok(3));
    assert_eq!(widened.try_store(7), Ok(()));
    assert_eq!(source.load(), 7);
    drop(source);
    assert!(weak.upgrade().is_some());
    assert_eq!(widened.try_load(), Ok(7));
    drop(widened);
    assert!(weak.upgrade().is_none());
}

#[test]
fn fallible_optional_views_do_not_invoke_callbacks_for_absent_pointers() {
    assert!(Location::<i32, String>::try_view_optional::<i64, String>(
        &None,
        || panic!("absent read"),
        |_| panic!("absent write"),
    )
    .is_none());
    assert!(Location::<i32, String>::try_map_optional::<i64>(
        None,
        |_| panic!("absent read projection"),
        |_| panic!("absent write projection"),
    )
    .is_none());
}

#[test]
fn error_conversion_preserves_native_backing_but_views_do_not_invent_it() {
    use tsonic_rust_runtime::raw_memory::{
        allocate_native_location, location_to_raw, NativeLayout,
    };
    let layout = NativeLayout::<u32>::scalar(4, 4, usize::BITS, cfg!(target_endian = "little"));
    let source = allocate_native_location::<_, core::convert::Infallible>(7, layout);
    let original = location_to_raw(Some(&source), layout).unwrap();
    let widened: Location<u32, String> = source.into_fallible();
    let retained = location_to_raw(Some(&widened), layout).unwrap();
    assert!(original == retained);
    assert_eq!(widened.try_store(11), Ok(()));
    assert_eq!(source.load(), 11);
    let view = widened.try_view(|| Ok::<u32, String>(17), |_| Ok(()));
    assert_eq!(Location::hash(Some(&view)), Location::hash(Some(&widened)));
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        location_to_raw(Some(&view), layout)
    }))
    .is_err());
}

#[test]
fn fallible_error_conversion_runs_once_and_preserves_the_selected_error() {
    let failure = Rc::new(String::from("selected failure"));
    let read_error = Rc::clone(&failure);
    let write_error = Rc::clone(&failure);
    let source = Location::try_bind(
        ObjectIdentity::new(),
        move || Err::<i32, _>(Rc::clone(&read_error)),
        move |_| Err(Rc::clone(&write_error)),
    );
    let conversions = Rc::new(Cell::new(0));
    let observed = Rc::clone(&conversions);
    let widened = source.map_error(move |error| {
        observed.set(observed.get() + 1);
        (7, error)
    });
    let (tag, selected) = widened.try_load().unwrap_err();
    assert_eq!(tag, 7);
    assert!(Rc::ptr_eq(&selected, &failure));
    assert_eq!(conversions.get(), 1);
    let (tag, selected) = widened.try_store(4).unwrap_err();
    assert_eq!(tag, 7);
    assert!(Rc::ptr_eq(&selected, &failure));
    assert_eq!(conversions.get(), 2);
}

#[test]
fn direct_views_preserve_identity_without_reading_or_writing_the_base() {
    let source = Location::bind(
        ObjectIdentity::new(),
        || -> i32 { panic!("base reads must not occur") },
        |_| panic!("base writes must not occur"),
    );
    let values = Rc::new(Cell::new(5_i64));
    let reads = Rc::new(Cell::new(0));
    let read_values = Rc::clone(&values);
    let read_count = Rc::clone(&reads);
    let write_values = Rc::clone(&values);
    let view = source.view(
        move || {
            read_count.set(read_count.get() + 1);
            read_values.get()
        },
        move |value| write_values.set(value),
    );
    let hash = Location::hash(Some(&source));
    assert_eq!(Location::hash(Some(&view)), hash);
    assert_eq!(reads.get(), 0);
    view.store(9);
    assert_eq!(reads.get(), 0);
    assert_eq!(values.get(), 9);
    assert_eq!(view.load(), 9);
    assert_eq!(reads.get(), 1);
    assert!(Location::<i32>::view_optional::<i64>(
        &None,
        || panic!("absent read"),
        |_| panic!("absent write")
    )
    .is_none());
}

#[test]
fn direct_view_retains_base_until_its_last_alias_is_dropped() {
    let owner = Rc::new(Cell::new(3));
    let weak = Rc::downgrade(&owner);
    let source = Location::bind(ObjectIdentity::new(), move || owner.get(), |_| {});
    let view = source.view(|| 7_i32, |_| {});
    let alias = view.clone();
    drop(source);
    drop(view);
    assert!(weak.upgrade().is_some());
    assert_eq!(alias.load(), 7);
    drop(alias);
    assert!(weak.upgrade().is_none());
}

#[test]
fn projected_bindings_preserve_owner_and_distinct_member_identity() {
    let owner = ObjectIdentity::new();
    let storage = Rc::new(Cell::new(3));
    let bind = |segment| {
        let read = Rc::clone(&storage);
        let write = Rc::clone(&storage);
        Location::bind_projected(
            owner.clone(),
            segment,
            move || read.get(),
            move |value| write.set(value),
        )
    };
    let first = bind(LocationSegment::Index(0));
    let same = bind(LocationSegment::Index(0));
    let other = bind(LocationSegment::Index(1));
    let named = bind(LocationSegment::Member("0".to_string()));
    assert!(Location::same(Some(&first), Some(&same)));
    assert_eq!(Location::hash(Some(&first)), Location::hash(Some(&same)));
    assert!(!Location::same(Some(&first), Some(&other)));
    assert!(!Location::same(Some(&first), Some(&named)));
    first.store(9);
    assert_eq!(same.load(), 9);
}

#[test]
fn binding_retains_the_actual_owner_not_only_its_identity_token() {
    struct Owner {
        identity: ObjectIdentity,
        alive: Rc<Cell<bool>>,
    }
    impl ObjectIdentityCarrier for Owner {
        fn object_identity(&self) -> &ObjectIdentity {
            &self.identity
        }
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            self.alive.set(false);
        }
    }
    let alive = Rc::new(Cell::new(true));
    let pointer = Location::bind(
        Owner {
            identity: ObjectIdentity::new(),
            alive: Rc::clone(&alive),
        },
        || 3_i32,
        |_| {},
    );
    let alias = pointer.map(i64::from, |value| i32::try_from(value).unwrap());
    drop(pointer);
    assert!(alive.get());
    assert_eq!(alias.load(), 3);
    drop(alias);
    assert!(!alive.get());
}

#[test]
fn bound_and_mapped_locations_preserve_storage_identity_and_hash() {
    let value = Rc::new(Cell::new(3_i32));
    let identity = ObjectIdentity::new();
    let bind = || {
        let read = Rc::clone(&value);
        let write = Rc::clone(&value);
        Location::bind(
            identity.clone(),
            move || read.get(),
            move |next| write.set(next),
        )
    };
    let first = bind();
    let alias = bind();
    let shifted = first.map(|source| source + 1, |target| target - 1);
    let hash = Location::hash(Some(&first));

    assert_eq!(shifted.load(), 4);
    shifted.store(9);
    assert_eq!(value.get(), 8);
    assert_eq!(alias.load(), 8);
    assert!(Location::same(Some(&first), Some(&alias)));
    assert!(Location::same(Some(&first), Some(&shifted)));
    assert_eq!(hash, Location::hash(Some(&alias)));
    assert_eq!(hash, Location::hash(Some(&shifted)));
    assert_eq!(Location::<i32>::hash(None), 0.0);
    assert!(Location::<i32>::map_optional::<i32>(
        None,
        |_| panic!("read must stay lazy"),
        |_| panic!("write must stay lazy")
    )
    .is_none());
}

#[test]
fn projection_retains_its_owner_and_cross_type_hash() {
    let source = Location::allocate(5_i32);
    let pointer = source.map(i64::from, |value| i32::try_from(value).unwrap());
    let hash = Location::hash(Some(&source));
    drop(source);
    pointer.store(7);
    assert_eq!(pointer.load(), 7);
    assert_eq!(Location::hash(Some(&pointer)), hash);
    let alias = pointer.map(|value| value, |value| value);
    assert!(Location::same(Some(&pointer), Some(&alias)));
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Pair {
    left: i32,
    right: i32,
}

#[test]
fn allocated_locations_preserve_aliasing_and_isolate_roots() {
    let first = Location::allocate(10_i32);
    let alias = first.clone();
    let independent = Location::allocate(10_i32);

    alias.store(12);

    assert_eq!(first.load(), 12);
    assert!(Location::same(Some(&first), Some(&alias)));
    assert!(!Location::same(Some(&first), Some(&independent)));
    assert!(Location::<i32>::same(None, None));
    assert!(!Location::same(Some(&first), None));
}

#[test]
fn member_projection_preserves_identity_and_writes_through() {
    let pair = Location::allocate(Pair { left: 1, right: 2 });
    let first = pair.project_member(
        "Pair.left",
        |value| value.left,
        |value, next| {
            value.left = next;
        },
    );
    let alias = pair.project_member(
        "Pair.left",
        |value| value.left,
        |value, next| {
            value.left = next;
        },
    );
    let right = pair.project_member(
        "Pair.right",
        |value| value.right,
        |value, next| {
            value.right = next;
        },
    );

    first.store(7);

    assert_eq!(alias.load(), 7);
    assert_eq!(pair.load(), Pair { left: 7, right: 2 });
    assert!(Location::same(Some(&first), Some(&alias)));
    assert!(!Location::same(Some(&first), Some(&right)));
    assert_eq!(Location::hash(Some(&first)), Location::hash(Some(&alias)));
}

#[test]
fn vector_projection_evaluates_one_index_and_writes_through() {
    let values = Location::allocate(vec![3_i32, 5_i32]);
    let first = values.project_index(0);
    let alias = values.project_index(0);
    let second = values.project_index(1);

    first.store(4);

    assert_eq!(values.load(), vec![4, 5]);
    assert_eq!(alias.load(), 4);
    assert!(Location::same(Some(&first), Some(&alias)));
    assert!(!Location::same(Some(&first), Some(&second)));
    assert_eq!(Location::hash(Some(&first)), Location::hash(Some(&alias)));
}

#[test]
fn update_changes_the_canonical_root_without_replacing_its_identity() {
    let pair = Location::allocate(Pair { left: 1, right: 2 });
    let alias = pair.clone();

    pair.update(|value| value.right = 9);

    assert_eq!(alias.load(), Pair { left: 1, right: 9 });
    assert!(Location::same(Some(&pair), Some(&alias)));
}

#[test]
fn update_with_replaces_one_value_without_replacing_its_identity() {
    let value = Location::allocate(4_i32);
    let alias = value.clone();

    value.update_with(|current| current + 3);

    assert_eq!(alias.load(), 7);
    assert!(Location::same(Some(&value), Some(&alias)));
}

#[test]
fn mutable_actions_write_back_and_return_the_action_result() {
    let pair = Location::allocate(Pair { left: 1, right: 2 });
    let result = pair.with_mut(|value| {
        value.left += 4;
        value.left + value.right
    });

    assert_eq!(result, 7);
    assert_eq!(pair.load(), Pair { left: 5, right: 2 });
}
