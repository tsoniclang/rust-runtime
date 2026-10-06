use super::*;
use alloc::vec::Vec;

type Callback = Rc<dyn Fn()>;

fn schedule(queue: &TimerQueue<Callback>, callback: impl Fn() + 'static) -> TimerHandle<Callback> {
    queue
        .schedule_with(Duration::ZERO, false, true, false, || {
            Rc::new(callback) as Callback
        })
        .unwrap()
}

#[test]
fn cold_and_aborted_roots_do_not_initialize_storage_or_family_budget() {
    let queue = TimerQueue::<Callback>::new();
    assert!(!queue.has_pending());
    assert!(!queue.has_refed());
    assert_eq!(queue.next_delay(), None);
    assert_eq!(queue.next_ready(&queue.prepare()), None);
    let built = Cell::new(false);
    let handle = queue
        .schedule_with(Duration::ZERO, false, true, true, || {
            built.set(true);
            Rc::new(|| ()) as Callback
        })
        .unwrap();
    assert!(!built.get());
    assert!(!handle.has_ref());
    assert!(queue.entries.get().is_none());
    TIMER_BUDGET.with(|budget| assert!(budget.get().is_none()));
}

#[test]
fn roots_share_finite_admission_and_rejection_does_not_initialize_another_root() {
    TIMER_BUDGET.with(|budget| {
        assert!(budget
            .set(TaskBudget::new(NonZeroUsize::new(1).unwrap()))
            .is_ok())
    });
    let first = TimerQueue::<Callback>::new();
    let rejected = TimerQueue::<Callback>::new();
    let handle = schedule(&first, || ());
    let constructed = Cell::new(false);
    let failure = rejected.schedule_with(Duration::ZERO, false, true, false, || {
        constructed.set(true);
        Rc::new(|| ()) as Callback
    });
    assert_eq!(failure.unwrap_err(), TimerQueueError::Capacity);
    assert!(!constructed.get());
    assert!(rejected.entries.get().is_none());
    handle.close();
    TIMER_BUDGET.with(|budget| assert_eq!(budget.get().unwrap().pending(), 0));
    schedule(&rejected, || ());
    assert!(rejected.has_pending());
}

#[test]
fn selected_entry_releases_capacity_and_borrows_before_callback_reentry() {
    TIMER_BUDGET.with(|budget| {
        assert!(budget
            .set(TaskBudget::new(NonZeroUsize::new(1).unwrap()))
            .is_ok())
    });
    let queue = Rc::new(TimerQueue::<Callback>::new());
    let weak = Rc::downgrade(&queue);
    schedule(&queue, move || {
        schedule(&weak.upgrade().unwrap(), || ());
    });
    let frontier = queue.prepare();
    let callback = queue.take_ready(&frontier).unwrap();
    TIMER_BUDGET.with(|budget| assert_eq!(budget.get().unwrap().pending(), 0));
    callback();
    assert!(queue.take_ready(&frontier).is_none());
    assert!(queue.has_pending());
    assert!(queue.take_ready(&queue.prepare()).is_some());
}

#[test]
fn timer_handles_cancel_without_retaining_root_or_callback_captures() {
    let queue = TimerQueue::<Callback>::new();
    let observed = Rc::new(Cell::new(0));
    let captured = Rc::clone(&observed);
    let handle = schedule(&queue, move || captured.set(1));
    assert_eq!(Rc::strong_count(&observed), 2);
    drop(queue);
    assert_eq!(Rc::strong_count(&observed), 1);
    handle.close();
    handle.set_ref(true);
    handle.refresh().unwrap();
    assert!(!handle.has_ref());
    TIMER_BUDGET.with(|budget| assert_eq!(budget.get().unwrap().pending(), 0));
}

#[test]
fn captured_frontiers_defer_cross_root_reentrant_admission() {
    let first = TimerQueue::<Callback>::new();
    let second = Rc::new(TimerQueue::<Callback>::new());
    let observed = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&observed);
    let weak = Rc::downgrade(&second);
    schedule(&first, move || {
        recorded.borrow_mut().push(1);
        let later = Rc::clone(&recorded);
        schedule(&weak.upgrade().unwrap(), move || later.borrow_mut().push(3));
    });
    let recorded = Rc::clone(&observed);
    schedule(&second, move || recorded.borrow_mut().push(2));
    let first_frontier = first.prepare();
    let second_frontier = second.prepare();
    assert!(first.next_ready(&first_frontier) < second.next_ready(&second_frontier));
    first.take_ready(&first_frontier).unwrap()();
    second.take_ready(&second_frontier).unwrap()();
    assert!(second.take_ready(&second_frontier).is_none());
    assert_eq!(*observed.borrow(), [1, 2]);
    second.take_ready(&second.prepare()).unwrap()();
    assert_eq!(*observed.borrow(), [1, 2, 3]);
}

#[test]
fn invalid_deadline_rejects_before_callback_or_storage_initializes() {
    let queue = TimerQueue::<Callback>::new();
    let built = Cell::new(false);
    let failure = queue.schedule_with(Duration::MAX, false, true, false, || {
        built.set(true);
        Rc::new(|| ()) as Callback
    });
    assert_eq!(failure.unwrap_err(), TimerQueueError::DeadlineOutOfRange);
    assert!(!built.get());
    assert!(queue.entries.get().is_none());
    TIMER_BUDGET.with(|budget| assert!(budget.get().is_none()));
}

#[test]
fn source_callables_keep_their_exact_owner_without_an_invocation_wrapper() {
    let queue = TimerQueue::<Callable<(), Result<(), core::convert::Infallible>>>::new();
    let original = Callable::new(|()| Ok(()));
    queue
        .schedule_with(Duration::ZERO, false, true, false, || original.clone())
        .unwrap();
    let selected = queue.take_ready(&queue.prepare()).unwrap();
    assert!(Callable::same(&selected, &original));
    assert!(!queue.has_pending());
    assert!(selected.invoke().is_ok());
}

#[test]
fn frontiers_reject_other_roots_without_consuming_callbacks() {
    let first = TimerQueue::<Callback>::new();
    let second = TimerQueue::<Callback>::new();
    schedule(&first, || ());
    schedule(&second, || ());
    let wrong = second.prepare();
    assert_eq!(first.next_ready(&wrong), None);
    assert!(first.take_ready(&wrong).is_none());
    assert!(first.take_ready(&first.prepare()).is_some());
    assert!(second.take_ready(&wrong).is_some());
}
