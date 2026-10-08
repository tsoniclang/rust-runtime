use std::cell::{Cell, RefCell};
use std::num::NonZeroUsize;
use std::rc::Rc;
use tsonic_rust_runtime::dispatch_queue::{TaskBudget, TaskQueue, TaskQueueError};

struct Failure(Rc<Cell<i64>>);

fn budget(limit: usize) -> TaskBudget {
    TaskBudget::new(NonZeroUsize::new(limit).unwrap())
}

#[test]
fn queued_failure_retains_exact_payload_and_uninvoked_tasks() {
    let budget = budget(4);
    let queue = TaskQueue::new(budget.clone());
    let observed = Rc::new(Cell::new(0));
    let value = Rc::new(Cell::new(9_007_199_254_740_993));
    let failure = value.clone();
    queue.enqueue(move || Err(Failure(failure))).unwrap();
    let last = observed.clone();
    queue
        .enqueue(move || {
            last.set(7);
            Ok(())
        })
        .unwrap();
    let returned = queue.poll_ready().expect_err("original queued failure");
    assert!(Rc::ptr_eq(&returned.0, &value));
    assert_eq!(returned.0.get(), 9_007_199_254_740_993);
    assert_eq!(observed.get(), 0);
    assert_eq!(budget.pending(), 1);
    assert_eq!(queue.poll_ready().ok(), Some(true));
    assert_eq!(observed.get(), 7);
    assert_eq!(budget.pending(), 0);
    assert_eq!(queue.poll_ready().ok(), Some(false));
}

#[test]
fn independent_error_domains_share_one_finite_admission_budget() {
    let budget = budget(2);
    let first = TaskQueue::<Failure>::new(budget.clone());
    let second = TaskQueue::<()>::new(budget.clone());
    let first_ticket = first.enqueue(|| Ok(())).unwrap();
    let second_ticket = second.enqueue(|| Ok(())).unwrap();
    assert!(first_ticket < second_ticket);
    assert_eq!(first.enqueue(|| Ok(())), Err(TaskQueueError::Capacity));
    assert_eq!(second.enqueue(|| Ok(())), Err(TaskQueueError::Capacity));
    assert_eq!(budget.pending(), 2);
    assert_eq!(first.poll_one().ok(), Some(true));
    let third_ticket = second.enqueue(|| Ok(())).unwrap();
    assert!(third_ticket > second_ticket);
    assert_eq!(budget.pending(), 2);
    drop(second);
    assert_eq!(budget.pending(), 0);
}

#[test]
fn in_flight_reservations_and_queued_callbacks_share_exact_capacity_and_identity() {
    let budget = budget(2);
    let queue = TaskQueue::<Failure>::new(budget.clone());
    let reservation = budget.reserve().unwrap();
    let queued = queue.enqueue(|| Ok(())).unwrap();
    assert!(reservation.ticket() < queued);
    assert_eq!(budget.pending(), 2);
    assert!(matches!(budget.reserve(), Err(TaskQueueError::Capacity)));
    assert_eq!(queue.enqueue(|| Ok(())), Err(TaskQueueError::Capacity));
    assert_eq!(budget.ready_boundary(), Some(queued));
    drop(reservation);
    assert_eq!(budget.pending(), 1);
    let later = budget.reserve().unwrap();
    assert!(later.ticket() > queued);
    drop(queue);
    assert_eq!(budget.pending(), 1);
    drop(later);
    assert_eq!(budget.pending(), 0);
}

#[test]
fn in_flight_reservation_owns_its_budget_until_released_without_a_queue() {
    let budget = budget(1);
    let reservation = budget.reserve().unwrap();
    let identity = reservation.ticket();
    assert_eq!(budget.ready_boundary(), Some(identity));
    drop(budget);
    assert_eq!(reservation.ticket(), identity);
    drop(reservation);
}

#[test]
fn one_shared_phase_frontier_defers_cross_component_reentrant_work() {
    let budget = budget(3);
    assert_eq!(budget.ready_boundary(), None);
    let first = TaskQueue::<Failure>::new(budget.clone());
    let second = TaskQueue::<()>::new(budget.clone());
    let observed = Rc::new(RefCell::new(Vec::new()));
    let target = second.handle();
    let recorded = observed.clone();
    first
        .enqueue(move || {
            recorded.borrow_mut().push(1);
            let next = recorded.clone();
            target
                .enqueue(move || {
                    next.borrow_mut().push(3);
                    Ok(())
                })
                .unwrap();
            Ok(())
        })
        .unwrap();
    let recorded = observed.clone();
    let original_last = second
        .enqueue(move || {
            recorded.borrow_mut().push(2);
            Ok(())
        })
        .unwrap();
    let frontier = budget.ready_boundary().unwrap();
    assert_eq!(frontier, original_last);
    assert_eq!(first.poll_through(frontier).ok(), Some(true));
    assert_eq!(second.poll_through(frontier), Ok(true));
    assert_eq!(*observed.borrow(), vec![1, 2]);
    assert_eq!(budget.pending(), 1);
    assert!(second.front_ticket().unwrap() > frontier);
    assert_eq!(second.poll_through(frontier), Ok(false));
    assert_eq!(second.poll_ready(), Ok(true));
    assert_eq!(*observed.borrow(), vec![1, 2, 3]);
    assert_eq!(budget.pending(), 0);
}

#[test]
fn shared_frontier_preserves_exact_first_failure_and_other_pending_domains() {
    let budget = budget(3);
    let first = TaskQueue::<Failure>::new(budget.clone());
    let second = TaskQueue::<()>::new(budget.clone());
    let value = Rc::new(Cell::new(9_007_199_254_740_993));
    let failure = value.clone();
    first.enqueue(move || Err(Failure(failure))).unwrap();
    let observed = Rc::new(Cell::new(0));
    let recorded = observed.clone();
    second
        .enqueue(move || {
            recorded.set(1);
            Ok(())
        })
        .unwrap();
    let frontier = budget.ready_boundary().unwrap();
    let returned = first
        .poll_through(frontier)
        .expect_err("exact first failure");
    assert!(Rc::ptr_eq(&returned.0, &value));
    assert_eq!(returned.0.get(), 9_007_199_254_740_993);
    assert_eq!(observed.get(), 0);
    assert_eq!(budget.pending(), 1);
    assert_eq!(first.poll_through(frontier).ok(), Some(false));
    assert_eq!(second.poll_through(frontier), Ok(true));
    assert_eq!(observed.get(), 1);
    assert_eq!(budget.pending(), 0);
}

#[test]
fn reentrant_enqueue_releases_borrows_and_capacity_but_waits_for_the_next_frontier() {
    let budget = budget(1);
    let queue = TaskQueue::<Failure>::new(budget.clone());
    let handle = queue.handle();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let recorded = observed.clone();
    queue
        .enqueue(move || {
            recorded.borrow_mut().push(1);
            let next = recorded.clone();
            handle
                .enqueue(move || {
                    next.borrow_mut().push(2);
                    Ok(())
                })
                .unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(queue.poll_ready().ok(), Some(true));
    assert_eq!(*observed.borrow(), vec![1]);
    assert_eq!(budget.pending(), 1);
    assert_eq!(queue.poll_ready().ok(), Some(true));
    assert_eq!(*observed.borrow(), vec![1, 2]);
    assert_eq!(budget.pending(), 0);
}

#[test]
fn scheduling_handles_do_not_keep_roots_or_pending_payloads_alive() {
    struct Released(Rc<Cell<usize>>);
    impl Drop for Released {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let budget = budget(2);
    let queue = TaskQueue::<Failure>::new(budget.clone());
    let handle = queue.handle();
    let retained = handle.clone();
    let releases = Rc::new(Cell::new(0));
    let payload = Released(releases.clone());
    queue
        .enqueue(move || {
            drop(payload);
            drop(retained);
            Ok(())
        })
        .unwrap();
    assert_eq!(budget.pending(), 1);
    drop(queue);
    assert_eq!(releases.get(), 1);
    assert_eq!(budget.pending(), 0);
    assert_eq!(handle.enqueue(|| Ok(())), Err(TaskQueueError::Closed));
}

#[test]
fn native_ready_tickets_preserve_interleaved_component_order_without_payload_erasure() {
    let budget = budget(4);
    let first = TaskQueue::<Failure>::new(budget.clone());
    let second = TaskQueue::<()>::new(budget);
    let observed = Rc::new(RefCell::new(Vec::new()));
    for (index, left) in [(1, true), (2, false), (3, true), (4, false)] {
        let recorded = observed.clone();
        if left {
            first
                .enqueue(move || {
                    recorded.borrow_mut().push(index);
                    Ok(())
                })
                .unwrap();
        } else {
            second
                .enqueue(move || {
                    recorded.borrow_mut().push(index);
                    Ok(())
                })
                .unwrap();
        }
    }
    loop {
        match (first.front_ticket(), second.front_ticket()) {
            (None, None) => break,
            (Some(left), Some(right)) if left < right => {
                assert_eq!(first.poll_one().ok(), Some(true));
            }
            (Some(_), None) => {
                assert_eq!(first.poll_one().ok(), Some(true));
            }
            _ => {
                assert_eq!(second.poll_one(), Ok(true));
            }
        }
    }
    assert_eq!(*observed.borrow(), vec![1, 2, 3, 4]);
}
