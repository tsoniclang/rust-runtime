use std::cell::{Cell, RefCell};

use tsonic_rust_runtime::{BorrowedLocation, OwnedLocation};

#[derive(Clone, Debug, Eq, PartialEq)]
struct Pair {
    left: i32,
    right: i32,
}

#[test]
fn allocated_locations_preserve_aliasing_and_isolate_roots() {
    let first = OwnedLocation::allocate(10_i32);
    let alias = first.clone();
    let independent = OwnedLocation::allocate(10_i32);

    alias.store(12);

    assert_eq!(first.load(), 12);
    assert!(OwnedLocation::same(Some(&first), Some(&alias)));
    assert!(!OwnedLocation::same(Some(&first), Some(&independent)));
    assert!(OwnedLocation::<i32>::same(None, None));
    assert!(!OwnedLocation::same(Some(&first), None));
}

#[test]
fn member_projection_preserves_identity_and_writes_through() {
    let pair = OwnedLocation::allocate(Pair { left: 1, right: 2 });
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
    assert!(OwnedLocation::same(Some(&first), Some(&alias)));
    assert!(!OwnedLocation::same(Some(&first), Some(&right)));
}

#[test]
fn vector_projection_evaluates_one_index_and_writes_through() {
    let values = OwnedLocation::allocate(vec![3_i32, 5_i32]);
    let first = values.project_index(0);
    let alias = values.project_index(0);
    let second = values.project_index(1);

    first.store(4);

    assert_eq!(values.load(), vec![4, 5]);
    assert_eq!(alias.load(), 4);
    assert!(OwnedLocation::same(Some(&first), Some(&alias)));
    assert!(!OwnedLocation::same(Some(&first), Some(&second)));
}

#[test]
fn update_changes_the_canonical_root_without_replacing_its_identity() {
    let pair = OwnedLocation::allocate(Pair { left: 1, right: 2 });
    let alias = pair.clone();

    pair.update(|value| value.right = 9);

    assert_eq!(alias.load(), Pair { left: 1, right: 9 });
    assert!(OwnedLocation::same(Some(&pair), Some(&alias)));
}

#[test]
fn update_with_replaces_one_value_without_replacing_its_identity() {
    let value = OwnedLocation::allocate(4_i32);
    let alias = value.clone();

    value.update_with(|current| current + 3);

    assert_eq!(alias.load(), 7);
    assert!(OwnedLocation::same(Some(&value), Some(&alias)));
}

#[test]
fn mutable_actions_write_back_and_return_the_action_result() {
    let pair = OwnedLocation::allocate(Pair { left: 1, right: 2 });
    let result = pair.with_mut(|value| {
        value.left += 4;
        value.left + value.right
    });

    assert_eq!(result, 7);
    assert_eq!(pair.load(), Pair { left: 5, right: 2 });
}

#[test]
fn borrowed_location_retains_a_non_static_storage_lifetime() {
    let storage = RefCell::new(Pair { left: 2, right: 3 });
    let pair = BorrowedLocation::from_cell(&storage);
    let left = pair.project_member(
        "Pair.left",
        |value| value.left,
        |value, next| value.left = next,
    );

    left.store(8);

    assert_eq!(storage.into_inner(), Pair { left: 8, right: 3 });
}

#[test]
fn accessor_locations_require_neither_clone_nor_static_borrowed_values() {
    struct Value<'a>(&'a str);

    let borrowed_storage = Cell::new("first");
    let borrowed = BorrowedLocation::from_accessors(
        || Value(borrowed_storage.get()),
        |value| borrowed_storage.set(value.0),
    );
    borrowed.store(Value("second"));
    assert_eq!(borrowed.load().0, "second");

    let owned_storage = std::rc::Rc::new(Cell::new(3_i32));
    let load_storage = std::rc::Rc::clone(&owned_storage);
    let store_storage = std::rc::Rc::clone(&owned_storage);
    let owned = OwnedLocation::from_accessors(
        move || {
            Value(if load_storage.get() == 3 {
                "three"
            } else {
                "four"
            })
        },
        move |value| store_storage.set(if value.0 == "three" { 3 } else { 4 }),
    );
    owned.store(Value("four"));
    assert_eq!(owned.load().0, "four");
}

#[test]
fn member_projection_does_not_add_clone_bounds_to_accessor_contracts() {
    struct PairValue<'a> {
        left: &'a str,
        right: &'a str,
    }
    struct Value<'a>(&'a str);

    let left = Cell::new("left");
    let right = Cell::new("right");
    let pair = BorrowedLocation::from_accessors(
        || PairValue {
            left: left.get(),
            right: right.get(),
        },
        |value| {
            left.set(value.left);
            right.set(value.right);
        },
    );
    let projected = pair.project_member(
        "PairValue.left",
        |value| Value(value.left),
        |value, next| value.left = next.0,
    );

    projected.store(Value("changed"));
    assert_eq!(left.get(), "changed");
    assert_eq!(right.get(), "right");
}
