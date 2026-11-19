use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use tsonic_rust_runtime::ordered_dispatch::poll_ordered_entries;

struct Failure(Rc<Cell<i64>>);

#[test]
fn exact_first_failure_retains_uninvoked_entries() {
    let entries = RefCell::new(BTreeMap::from([(1_u64, 1), (2, 2)]));
    let identity = Rc::new(Cell::new(9_007_199_254_740_993));
    let error = poll_ordered_entries(
        &entries,
        |_| true,
        |entries, key| entries.remove(&key).unwrap(),
        |_| Err::<(), _>(Failure(identity.clone())),
    )
    .err()
    .unwrap();
    assert!(Rc::ptr_eq(&error.0, &identity));
    assert_eq!(error.0.get(), 9_007_199_254_740_993);
    assert_eq!(*entries.borrow(), BTreeMap::from([(2, 2)]));
    let mut observed = Vec::new();
    assert_eq!(
        poll_ordered_entries(
            &entries,
            |_| true,
            |entries, key| entries.remove(&key).unwrap(),
            |value| {
                observed.push(value);
                Ok::<(), Failure>(())
            }
        )
        .ok(),
        Some(true)
    );
    assert_eq!(observed, vec![2]);
    assert!(entries.borrow().is_empty());
}

#[test]
fn reentrant_admission_waits_and_cancellation_removes_uninvoked_entries() {
    let entries = RefCell::new(BTreeMap::from([(1_u64, 1), (2, 2)]));
    let mut observed = Vec::new();
    assert_eq!(
        poll_ordered_entries(
            &entries,
            |_| true,
            |entries, key| entries.remove(&key).unwrap(),
            |value| {
                observed.push(value);
                entries.borrow_mut().remove(&2);
                entries.borrow_mut().insert(3, 3);
                Ok::<(), Failure>(())
            }
        )
        .ok(),
        Some(true)
    );
    assert_eq!(observed, vec![1]);
    assert_eq!(*entries.borrow(), BTreeMap::from([(3, 3)]));
    assert_eq!(
        poll_ordered_entries(
            &entries,
            |_| true,
            |entries, key| entries.remove(&key).unwrap(),
            |value| {
                observed.push(value);
                Ok::<(), Failure>(())
            }
        )
        .ok(),
        Some(true)
    );
    assert_eq!(observed, vec![1, 3]);
}

#[test]
fn retained_ready_entries_execute_once_per_ordered_frontier() {
    let entries = RefCell::new(BTreeMap::from([(1_u64, false), (2, true)]));
    let mut observed = Vec::new();
    for _ in 0..2 {
        assert_eq!(
            poll_ordered_entries(
                &entries,
                |ready| *ready,
                |_entries, key| key,
                |value| {
                    observed.push(value);
                    Ok::<(), Failure>(())
                }
            )
            .ok(),
            Some(true)
        );
    }
    assert_eq!(observed, vec![2, 2]);
    assert_eq!(entries.borrow().len(), 2);
    entries.borrow_mut().clear();
    assert_eq!(
        poll_ordered_entries(
            &entries,
            |_| panic!("empty store"),
            |_entries, _key| (),
            |_value| Ok::<(), Failure>(())
        )
        .ok(),
        Some(false)
    );
}
