use tsonic_rust_runtime::Location;

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
