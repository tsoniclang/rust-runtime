use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocationCounts {
    pub allocations: usize,
    pub deallocations: usize,
    pub reallocations: usize,
    pub allocation_bytes: usize,
    pub reallocation_bytes: usize,
    pub deallocation_bytes: usize,
    pub reallocation_old_bytes: usize,
    pub maximum_alignment: usize,
}

impl AllocationCounts {
    pub fn allocation_requests(self) -> usize {
        self.allocations + self.reallocations
    }

    pub fn requested_bytes(self) -> usize {
        self.allocation_bytes + self.reallocation_bytes
    }

    pub fn released_bytes(self) -> usize {
        self.deallocation_bytes + self.reallocation_old_bytes
    }
}

struct AtomicCounters {
    allocations: AtomicUsize,
    deallocations: AtomicUsize,
    reallocations: AtomicUsize,
    allocation_bytes: AtomicUsize,
    reallocation_bytes: AtomicUsize,
    deallocation_bytes: AtomicUsize,
    reallocation_old_bytes: AtomicUsize,
    maximum_alignment: AtomicUsize,
}

impl AtomicCounters {
    const fn new() -> Self {
        Self {
            allocations: AtomicUsize::new(0),
            deallocations: AtomicUsize::new(0),
            reallocations: AtomicUsize::new(0),
            allocation_bytes: AtomicUsize::new(0),
            reallocation_bytes: AtomicUsize::new(0),
            deallocation_bytes: AtomicUsize::new(0),
            reallocation_old_bytes: AtomicUsize::new(0),
            maximum_alignment: AtomicUsize::new(0),
        }
    }

    fn reset(&self) {
        self.allocations.store(0, Ordering::Relaxed);
        self.deallocations.store(0, Ordering::Relaxed);
        self.reallocations.store(0, Ordering::Relaxed);
        self.allocation_bytes.store(0, Ordering::Relaxed);
        self.reallocation_bytes.store(0, Ordering::Relaxed);
        self.deallocation_bytes.store(0, Ordering::Relaxed);
        self.reallocation_old_bytes.store(0, Ordering::Relaxed);
        self.maximum_alignment.store(0, Ordering::Relaxed);
    }

    fn snapshot(&self) -> AllocationCounts {
        AllocationCounts {
            allocations: self.allocations.load(Ordering::Relaxed),
            deallocations: self.deallocations.load(Ordering::Relaxed),
            reallocations: self.reallocations.load(Ordering::Relaxed),
            allocation_bytes: self.allocation_bytes.load(Ordering::Relaxed),
            reallocation_bytes: self.reallocation_bytes.load(Ordering::Relaxed),
            deallocation_bytes: self.deallocation_bytes.load(Ordering::Relaxed),
            reallocation_old_bytes: self.reallocation_old_bytes.load(Ordering::Relaxed),
            maximum_alignment: self.maximum_alignment.load(Ordering::Relaxed),
        }
    }

    fn record(&self, event: AllocationEvent) {
        match event {
            AllocationEvent::Allocate { bytes, alignment } => {
                self.allocations.fetch_add(1, Ordering::Relaxed);
                self.allocation_bytes.fetch_add(bytes, Ordering::Relaxed);
                self.maximum_alignment
                    .fetch_max(alignment, Ordering::Relaxed);
            }
            AllocationEvent::Deallocate { bytes } => {
                self.deallocations.fetch_add(1, Ordering::Relaxed);
                self.deallocation_bytes.fetch_add(bytes, Ordering::Relaxed);
            }
            AllocationEvent::Reallocate {
                bytes,
                old_bytes,
                alignment,
            } => {
                self.reallocations.fetch_add(1, Ordering::Relaxed);
                self.reallocation_bytes.fetch_add(bytes, Ordering::Relaxed);
                self.reallocation_old_bytes
                    .fetch_add(old_bytes, Ordering::Relaxed);
                self.maximum_alignment
                    .fetch_max(alignment, Ordering::Relaxed);
            }
        }
    }
}

struct ThreadCounters {
    depth: Cell<usize>,
    values: AtomicCounters,
}

thread_local! {
    static COUNTERS: ThreadCounters = const { ThreadCounters {
        depth: Cell::new(0),
        values: AtomicCounters::new(),
    } };
}

static PROCESS_ALLOCATION_REQUESTS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
enum AllocationEvent {
    Allocate {
        bytes: usize,
        alignment: usize,
    },
    Deallocate {
        bytes: usize,
    },
    Reallocate {
        bytes: usize,
        old_bytes: usize,
        alignment: usize,
    },
}

fn record(event: AllocationEvent) {
    if matches!(
        event,
        AllocationEvent::Allocate { .. } | AllocationEvent::Reallocate { .. }
    ) {
        PROCESS_ALLOCATION_REQUESTS.fetch_add(1, Ordering::SeqCst);
    }
    let _ = COUNTERS.try_with(|counters| {
        if counters.depth.get() != 0 {
            counters.values.record(event);
        }
    });
}

pub fn allocation_requests_all_threads() -> usize {
    PROCESS_ALLOCATION_REQUESTS.load(Ordering::SeqCst)
}

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(AllocationEvent::Allocate {
            bytes: layout.size(),
            alignment: layout.align(),
        });
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(AllocationEvent::Allocate {
            bytes: layout.size(),
            alignment: layout.align(),
        });
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record(AllocationEvent::Deallocate {
            bytes: layout.size(),
        });
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(AllocationEvent::Reallocate {
            bytes: size,
            old_bytes: layout.size(),
            alignment: layout.align(),
        });
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[must_use]
pub struct Measurement {
    before: AllocationCounts,
    depth: usize,
    open: bool,
    thread: PhantomData<Rc<()>>,
}

impl Measurement {
    pub fn start() -> Self {
        COUNTERS.with(|counters| {
            let depth = counters.depth.get() + 1;
            if depth == 1 {
                counters.values.reset();
            }
            let before = counters.values.snapshot();
            counters
                .values
                .maximum_alignment
                .store(0, Ordering::Relaxed);
            counters.depth.set(depth);
            Self {
                before,
                depth,
                open: true,
                thread: PhantomData,
            }
        })
    }

    pub fn finish(mut self) -> AllocationCounts {
        self.close()
    }

    fn close(&mut self) -> AllocationCounts {
        COUNTERS.with(|counters| {
            assert_eq!(
                counters.depth.get(),
                self.depth,
                "measurement scopes close in order"
            );
            let current = counters.values.snapshot();
            counters.depth.set(self.depth - 1);
            counters
                .values
                .maximum_alignment
                .fetch_max(self.before.maximum_alignment, Ordering::Relaxed);
            self.open = false;
            AllocationCounts {
                allocations: current.allocations - self.before.allocations,
                deallocations: current.deallocations - self.before.deallocations,
                reallocations: current.reallocations - self.before.reallocations,
                allocation_bytes: current.allocation_bytes - self.before.allocation_bytes,
                reallocation_bytes: current.reallocation_bytes - self.before.reallocation_bytes,
                deallocation_bytes: current.deallocation_bytes - self.before.deallocation_bytes,
                reallocation_old_bytes: current.reallocation_old_bytes
                    - self.before.reallocation_old_bytes,
                maximum_alignment: current.maximum_alignment,
            }
        })
    }
}

impl Drop for Measurement {
    fn drop(&mut self) {
        if self.open {
            self.close();
        }
    }
}

pub fn measure<Output>(operation: impl FnOnce() -> Output) -> (Output, AllocationCounts) {
    let measurement = Measurement::start();
    let output = operation();
    (output, measurement.finish())
}

pub fn measure_requests<Output>(operation: impl FnOnce() -> Output) -> (usize, usize) {
    let (output, requests, bytes) = measure_value_requests(operation);
    drop(output);
    (requests, bytes)
}

pub fn measure_value_requests<Output>(
    operation: impl FnOnce() -> Output,
) -> (Output, usize, usize) {
    let (output, counts) = measure(|| std::hint::black_box(operation()));
    (
        output,
        counts.allocation_requests(),
        counts.requested_bytes(),
    )
}

pub fn measure_layout<Output>(operation: impl FnOnce() -> Output) -> (Output, usize, usize, usize) {
    let (output, counts) = measure(operation);
    (
        output,
        counts.allocation_requests(),
        counts.requested_bytes(),
        counts.maximum_alignment,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_and_nested_observers_allocate_no_storage() {
        let outer = super::Measurement::start();
        for _iteration in 0..64 {
            let inner = super::Measurement::start();
            assert_eq!(inner.finish(), super::AllocationCounts::default());
        }
        assert_eq!(outer.finish(), super::AllocationCounts::default());
        assert_eq!(super::measure_requests(|| ()), (0, 0));
        assert_eq!(super::measure_value_requests(|| 7), (7, 0, 0));
    }

    #[test]
    fn requested_allocation_and_deallocation_metrics_are_exact() {
        use std::hint::black_box;

        let (value, counts) = super::measure(|| Box::new(black_box([7_u8; 37])));
        assert_eq!(counts.allocations, 1);
        assert_eq!(counts.allocation_bytes, 37);
        assert_eq!(counts.reallocations, 0);
        assert_eq!(counts.reallocation_bytes, 0);
        assert_eq!(counts.deallocations, 0);
        assert_eq!(counts.maximum_alignment, 1);
        assert_eq!(value[36], 7);
        let ((), counts) = super::measure(|| drop(value));
        assert_eq!(counts.allocations, 0);
        assert_eq!(counts.deallocations, 1);
        assert_eq!(counts.deallocation_bytes, 37);
        assert_eq!(counts.reallocation_old_bytes, 0);
        assert_eq!(counts.released_bytes(), 37);
        assert_eq!(counts.requested_bytes(), 0);
    }

    #[test]
    fn discarded_outputs_drop_after_the_requested_operation_window() {
        use std::hint::black_box;

        struct AllocatingDrop([u8; 37]);

        impl Drop for AllocatingDrop {
            fn drop(&mut self) {
                assert_eq!(self.0[36], 7);
                let extra = Box::new(black_box([9_u8; 5]));
                black_box(extra);
            }
        }

        let outer = super::Measurement::start();
        let cost = super::measure_requests(|| Box::new(AllocatingDrop(black_box([7; 37]))));
        let outer = outer.finish();
        assert_eq!(cost, (1, 37));
        assert_eq!(outer.allocations, 2);
        assert_eq!(outer.allocation_bytes, 42);
        assert_eq!(outer.deallocations, 2);
        assert_eq!(outer.deallocation_bytes, 42);
    }

    #[test]
    fn zeroed_allocation_is_counted_once_and_preserves_its_bytes() {
        use std::hint::black_box;

        let (value, counts) = super::measure(|| vec![0_u8; black_box(83)]);
        assert_eq!(counts.allocations, 1);
        assert_eq!(counts.allocation_bytes, 83);
        assert_eq!(counts.reallocations, 0);
        assert!(value.iter().all(|value| *value == 0));
    }

    #[test]
    fn reallocations_count_the_entire_requested_size_not_growth() {
        let mut value = Vec::with_capacity(16);
        value.resize(16, 91_u8);
        let ((), counts) = super::measure(|| value.reserve_exact(48));
        assert_eq!(counts.allocations, 0);
        assert_eq!(counts.allocation_bytes, 0);
        assert_eq!(counts.reallocations, 1);
        assert_eq!(counts.reallocation_bytes, 64);
        assert_eq!(counts.deallocation_bytes, 0);
        assert_eq!(counts.reallocation_old_bytes, 16);
        assert_eq!(counts.released_bytes(), 16);
        assert_eq!(counts.allocation_requests(), 1);
        assert_eq!(counts.requested_bytes(), 64);
        assert_eq!(counts.deallocations, 0);
        assert_eq!(counts.maximum_alignment, 1);
        assert_eq!(value, [91_u8; 16]);
    }

    #[test]
    fn nested_windows_preserve_outer_counts_and_alignment() {
        use std::hint::black_box;

        #[repr(align(64))]
        struct Aligned([u8; 64]);

        let outer = super::Measurement::start();
        let aligned = Box::new(Aligned(black_box([19; 64])));
        let (small, inner) = super::measure(|| Box::new(black_box([23_u8; 3])));
        let outer = outer.finish();
        assert_eq!(inner.allocations, 1);
        assert_eq!(inner.allocation_bytes, 3);
        assert_eq!(inner.maximum_alignment, 1);
        assert_eq!(outer.allocations, 2);
        assert_eq!(outer.allocation_bytes, 67);
        assert_eq!(outer.maximum_alignment, 64);
        assert_eq!(aligned.0[63], 19);
        assert_eq!(small[2], 23);
        let (aligned, allocations, bytes, alignment) =
            super::measure_layout(|| Box::new(Aligned(black_box([29; 64]))));
        assert_eq!((allocations, bytes, alignment), (1, 64, 64));
        assert_eq!(aligned.0[63], 29);
    }

    #[test]
    fn unwind_restores_outer_and_inactive_measurements() {
        use std::hint::black_box;

        let outer = super::Measurement::start();
        let result = std::panic::catch_unwind(|| {
            let _inner = super::Measurement::start();
            panic!("instrumentation unwind control");
        });
        assert!(result.is_err());
        assert_eq!(super::COUNTERS.with(|counters| counters.depth.get()), 1);
        let (_, counts) = super::measure(|| ());
        assert_eq!(counts, super::AllocationCounts::default());
        outer.finish();
        assert_eq!(super::COUNTERS.with(|counters| counters.depth.get()), 0);
        let result = std::panic::catch_unwind(|| {
            super::measure(|| panic!("inactive restoration control"));
        });
        assert!(result.is_err());
        assert_eq!(super::COUNTERS.with(|counters| counters.depth.get()), 0);
        let (value, counts) = super::measure(|| Box::new(black_box([31_u8; 43])));
        assert_eq!(counts.allocations, 1);
        assert_eq!(counts.allocation_bytes, 43);
        assert_eq!(value[42], 31);
    }

    #[test]
    fn concurrent_threads_observe_only_their_own_requests() {
        use std::hint::black_box;

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|index| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let (value, counts) = super::measure(|| vec![black_box(7_u8); 64 + index]);
                    assert_eq!(counts.allocations, 1);
                    assert_eq!(counts.allocation_bytes, 64 + index);
                    assert_eq!(counts.reallocations, 0);
                    assert_eq!(counts.maximum_alignment, 1);
                    assert!(value.iter().all(|value| *value == 7));
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
    }

    #[test]
    fn explicit_process_counter_sees_other_threads_without_observer_allocation() {
        use std::hint::{black_box, spin_loop};
        use std::sync::atomic::{AtomicUsize, Ordering};

        let phase = std::sync::Arc::new(AtomicUsize::new(0));
        let other = phase.clone();
        let thread = std::thread::spawn(move || {
            other.store(1, Ordering::SeqCst);
            while other.load(Ordering::SeqCst) != 2 {
                spin_loop();
            }
            let value = Box::new(black_box([7_u8; 97]));
            black_box(&value);
            other.store(3, Ordering::SeqCst);
            while other.load(Ordering::SeqCst) != 4 {
                spin_loop();
            }
            drop(value);
        });
        while phase.load(Ordering::SeqCst) != 1 {
            spin_loop();
        }
        let local = super::Measurement::start();
        let before = super::allocation_requests_all_threads();
        phase.store(2, Ordering::SeqCst);
        while phase.load(Ordering::SeqCst) != 3 {
            spin_loop();
        }
        let after = super::allocation_requests_all_threads();
        let local = local.finish();
        phase.store(4, Ordering::SeqCst);
        thread.join().unwrap();
        assert_eq!(local.allocation_requests(), 0);
        assert!(after.wrapping_sub(before) >= 1);
    }
}
