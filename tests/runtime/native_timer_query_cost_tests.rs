use core::hint::black_box;
use core::time::Duration;
use tsonic_rust_runtime::timer_queue::TimerQueue;

#[path = "../helpers/allocation_counts.rs"]
pub mod allocation_counts;

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
        let measurement = allocation_counts::Measurement::start();
        for _ in 0..10_000 {
            assert!(black_box(queue.has_refed()));
            assert!(black_box(queue.has_pending()));
            assert!(black_box(queue.next_delay()).is_some());
        }
        let counts = measurement.finish();
        let count = counts.allocation_requests();
        let bytes = counts.allocation_bytes;
        assert_eq!(count, 0);
        assert_eq!(bytes, 0);
    }
}
