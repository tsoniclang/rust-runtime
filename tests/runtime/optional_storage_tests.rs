use std::cell::Cell;
use std::rc::Rc;
use tsonic_rust_runtime::{optional_storage_coalesce, OptionalStorage};

#[test]
fn native_values_and_absence_are_distinct() {
    for value in [i64::MIN, -9007199254740993, 0, 9007199254740993, i64::MAX] {
        let stored = <Option<i64> as OptionalStorage<i64>>::present(value);
        assert!(!<Option<i64> as OptionalStorage<i64>>::is_absent(&stored));
        assert_eq!(
            <Option<i64> as OptionalStorage<i64>>::into_present(stored),
            value
        );
    }
    assert!(<Option<i64> as OptionalStorage<i64>>::is_absent(&None));
    assert_eq!(<Option<i64> as OptionalStorage<i64>>::absent(), None);
    assert!(!<Option<bool> as OptionalStorage<bool>>::is_absent(&Some(
        false
    )));
    assert!(!<Option<&str> as OptionalStorage<&str>>::is_absent(&Some(
        ""
    )));
}

#[test]
fn source_nullable_storage_is_idempotent_but_native_options_retain_membership() {
    assert_eq!(
        <Option<i64> as OptionalStorage<Option<i64>>>::present(None),
        None
    );
    assert_eq!(
        <Option<i64> as OptionalStorage<Option<i64>>>::present(Some(0)),
        Some(0)
    );
    assert_eq!(
        <Option<i64> as OptionalStorage<Option<i64>>>::clone_present(&Some(7)),
        Some(7)
    );
    assert_eq!(
        <Option<i64> as OptionalStorage<Option<i64>>>::into_present(Some(9)),
        Some(9)
    );
    assert!(<() as OptionalStorage<()>>::is_absent(&()));
    <() as OptionalStorage<()>>::present(());
    <() as OptionalStorage<()>>::absent();
    let native = <Option<Option<i64>> as OptionalStorage<Option<i64>>>::present(None);
    assert_eq!(native, Some(None));
    assert!(!<Option<Option<i64>> as OptionalStorage<Option<i64>>>::is_absent(&native));
    assert_eq!(
        <Option<Option<i64>> as OptionalStorage<Option<i64>>>::into_present(native),
        None
    );
}

#[test]
fn values_copy_and_reference_values_retain_identity() {
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Value {
        count: i64,
    }
    let original = Value {
        count: 9007199254740993,
    };
    let stored = <Option<Value> as OptionalStorage<Value>>::present(original);
    let mut copy = <Option<Value> as OptionalStorage<Value>>::clone_present(&stored);
    copy.count = 7;
    assert_eq!(original.count, 9007199254740993);
    assert_eq!(stored, Some(original));
    assert_eq!(copy.count, 7);
    let reference = Rc::new(Cell::new(0));
    let optional =
        <Option<Rc<Cell<i64>>> as OptionalStorage<Rc<Cell<i64>>>>::present(reference.clone());
    let alias = <Option<Rc<Cell<i64>>> as OptionalStorage<Rc<Cell<i64>>>>::clone_present(&optional);
    alias.set(11);
    assert!(Rc::ptr_eq(&reference, &alias));
    assert_eq!(reference.get(), 11);
}

#[test]
fn coalescing_evaluates_only_the_selected_branch_without_another_storage_layer() {
    let visits = Cell::new(0);
    let absent = optional_storage_coalesce::<Option<i64>, Option<i64>, Option<i64>>(
        None,
        |_| panic!("absent storage selected present branch"),
        || {
            visits.set(visits.get() + 1);
            Some(9007199254740993)
        },
    );
    assert_eq!(absent, Some(9007199254740993));
    let present = optional_storage_coalesce::<Option<i64>, Option<i64>, _>(
        Some(0),
        |value| {
            visits.set(visits.get() + 10);
            value
        },
        || panic!("present storage selected absent branch"),
    );
    assert_eq!(present, Some(0));
    assert_eq!(visits.get(), 11);
    assert_eq!(
        std::mem::size_of_val(&present),
        std::mem::size_of::<Option<i64>>()
    );
}

#[test]
fn absent_projection_panics_instead_of_inventing_a_value() {
    assert!(
        std::panic::catch_unwind(|| <Option<i64> as OptionalStorage<i64>>::into_present(None))
            .is_err()
    );
    assert!(std::panic::catch_unwind(|| {
        <Option<i64> as OptionalStorage<Option<i64>>>::into_present(None)
    })
    .is_err());
    assert!(std::panic::catch_unwind(|| <() as OptionalStorage<()>>::into_present(())).is_err());
}
