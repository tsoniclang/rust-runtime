use core::hint::black_box;
use core::time::Duration;
use tsonic_rust_runtime::timer_queue::TimerQueue;

#[path = "../helpers/error_allocations.rs"]
mod allocations;

#[test]
fn timer_readiness_and_deadline_queries_allocate_no_storage_on_live_queues() {
    for length in [1, 4096] {
        let queue = TimerQueue::<()>::new();
        for index in 0..length {
            queue
                .schedule_with(
                    Duration::from_secs(86_400),
                    false,
                    index % 2 == 0,
                    false,
                    || (),
                )
                .unwrap();
        }
        assert!(queue.has_refed());
        assert!(queue.has_pending());
        assert!(queue.next_delay().unwrap() > Duration::from_secs(60));
        allocations::ALLOCATIONS.with(|count| count.set(Some(0)));
        allocations::ALLOCATION_BYTES.with(|count| count.set(Some(0)));
        for _ in 0..10_000 {
            assert!(black_box(queue.has_refed()));
            assert!(black_box(queue.has_pending()));
            assert!(black_box(queue.next_delay()).is_some());
        }
        let count = allocations::ALLOCATIONS.with(|count| count.replace(None).unwrap());
        let bytes = allocations::ALLOCATION_BYTES.with(|count| count.replace(None).unwrap());
        assert_eq!(count, 0);
        assert_eq!(bytes, 0);
    }
}
