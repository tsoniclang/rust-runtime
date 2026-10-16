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
    pub maximum_alignment: usize,
}

impl AllocationCounts {
    pub fn allocation_requests(self) -> usize {
        self.allocations + self.reallocations
    }

    pub fn requested_bytes(self) -> usize {
        self.allocation_bytes + self.reallocation_bytes
    }
}

struct ThreadCounters {
    depth: Cell<usize>,
    allocations: AtomicUsize,
    deallocations: AtomicUsize,
    reallocations: AtomicUsize,
    allocation_bytes: AtomicUsize,
    reallocation_bytes: AtomicUsize,
    maximum_alignment: AtomicUsize,
}

impl ThreadCounters {
    const fn new() -> Self {
        Self {
            depth: Cell::new(0),
            allocations: AtomicUsize::new(0),
            deallocations: AtomicUsize::new(0),
            reallocations: AtomicUsize::new(0),
            allocation_bytes: AtomicUsize::new(0),
            reallocation_bytes: AtomicUsize::new(0),
            maximum_alignment: AtomicUsize::new(0),
        }
    }

    fn reset(&self) {
        self.allocations.store(0, Ordering::Relaxed);
        self.deallocations.store(0, Ordering::Relaxed);
        self.reallocations.store(0, Ordering::Relaxed);
        self.allocation_bytes.store(0, Ordering::Relaxed);
        self.reallocation_bytes.store(0, Ordering::Relaxed);
        self.maximum_alignment.store(0, Ordering::Relaxed);
    }

    fn snapshot(&self) -> AllocationCounts {
        AllocationCounts {
            allocations: self.allocations.load(Ordering::Relaxed),
            deallocations: self.deallocations.load(Ordering::Relaxed),
            reallocations: self.reallocations.load(Ordering::Relaxed),
            allocation_bytes: self.allocation_bytes.load(Ordering::Relaxed),
            reallocation_bytes: self.reallocation_bytes.load(Ordering::Relaxed),
            maximum_alignment: self.maximum_alignment.load(Ordering::Relaxed),
        }
    }
}

thread_local! {
    static COUNTERS: ThreadCounters = const { ThreadCounters::new() };
}

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = COUNTERS.try_with(|counters| {
            if counters.depth.get() != 0 {
                counters.allocations.fetch_add(1, Ordering::Relaxed);
                counters
                    .allocation_bytes
                    .fetch_add(layout.size(), Ordering::Relaxed);
                counters
                    .maximum_alignment
                    .fetch_max(layout.align(), Ordering::Relaxed);
            }
        });
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let _ = COUNTERS.try_with(|counters| {
            if counters.depth.get() != 0 {
                counters.allocations.fetch_add(1, Ordering::Relaxed);
                counters
                    .allocation_bytes
                    .fetch_add(layout.size(), Ordering::Relaxed);
                counters
                    .maximum_alignment
                    .fetch_max(layout.align(), Ordering::Relaxed);
            }
        });
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let _ = COUNTERS.try_with(|counters| {
            if counters.depth.get() != 0 {
                counters.deallocations.fetch_add(1, Ordering::Relaxed);
            }
        });
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let _ = COUNTERS.try_with(|counters| {
            if counters.depth.get() != 0 {
                counters.reallocations.fetch_add(1, Ordering::Relaxed);
                counters
                    .reallocation_bytes
                    .fetch_add(size, Ordering::Relaxed);
                counters
                    .maximum_alignment
                    .fetch_max(layout.align(), Ordering::Relaxed);
            }
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
                counters.reset();
            }
            let before = counters.snapshot();
            counters.maximum_alignment.store(0, Ordering::Relaxed);
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
            let current = counters.snapshot();
            counters.depth.set(self.depth - 1);
            counters
                .maximum_alignment
                .fetch_max(self.before.maximum_alignment, Ordering::Relaxed);
            self.open = false;
            AllocationCounts {
                allocations: current.allocations - self.before.allocations,
                deallocations: current.deallocations - self.before.deallocations,
                reallocations: current.reallocations - self.before.reallocations,
                allocation_bytes: current.allocation_bytes - self.before.allocation_bytes,
                reallocation_bytes: current.reallocation_bytes - self.before.reallocation_bytes,
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

pub fn measure_requests(operation: impl FnOnce()) -> (usize, usize) {
    let ((), counts) = measure(operation);
    (counts.allocation_requests(), counts.requested_bytes())
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
    use super::{
        measure, measure_layout, measure_requests, AllocationCounts, Measurement, COUNTERS,
    };
    use std::hint::black_box;

    #[test]
    fn empty_and_nested_observers_allocate_no_storage() {
        let outer = Measurement::start();
        for _iteration in 0..64 {
            let inner = Measurement::start();
            assert_eq!(inner.finish(), AllocationCounts::default());
        }
        assert_eq!(outer.finish(), AllocationCounts::default());
        assert_eq!(measure_requests(|| ()), (0, 0));
    }

    #[test]
    fn requested_allocation_and_deallocation_metrics_are_exact() {
        let (value, counts) = measure(|| Box::new(black_box([7_u8; 37])));
        assert_eq!(counts.allocations, 1);
        assert_eq!(counts.allocation_bytes, 37);
        assert_eq!(counts.reallocations, 0);
        assert_eq!(counts.reallocation_bytes, 0);
        assert_eq!(counts.deallocations, 0);
        assert_eq!(counts.maximum_alignment, 1);
        assert_eq!(value[36], 7);
        let ((), counts) = measure(|| drop(value));
        assert_eq!(counts.allocations, 0);
        assert_eq!(counts.deallocations, 1);
        assert_eq!(counts.requested_bytes(), 0);
    }

    #[test]
    fn zeroed_allocation_is_counted_once_and_preserves_its_bytes() {
        let (value, counts) = measure(|| vec![0_u8; black_box(83)]);
        assert_eq!(counts.allocations, 1);
        assert_eq!(counts.allocation_bytes, 83);
        assert_eq!(counts.reallocations, 0);
        assert!(value.iter().all(|value| *value == 0));
    }

    #[test]
    fn reallocations_count_the_entire_requested_size_not_growth() {
        let mut value = Vec::with_capacity(16);
        value.resize(16, 91_u8);
        let ((), counts) = measure(|| value.reserve_exact(48));
        assert_eq!(counts.allocations, 0);
        assert_eq!(counts.allocation_bytes, 0);
        assert_eq!(counts.reallocations, 1);
        assert_eq!(counts.reallocation_bytes, 64);
        assert_eq!(counts.allocation_requests(), 1);
        assert_eq!(counts.requested_bytes(), 64);
        assert_eq!(counts.deallocations, 0);
        assert_eq!(counts.maximum_alignment, 1);
        assert_eq!(value, [91_u8; 16]);
    }

    #[repr(align(64))]
    struct Aligned([u8; 64]);

    #[test]
    fn nested_windows_preserve_outer_counts_and_alignment() {
        let outer = Measurement::start();
        let aligned = Box::new(Aligned(black_box([19; 64])));
        let (small, inner) = measure(|| Box::new(black_box([23_u8; 3])));
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
            measure_layout(|| Box::new(Aligned(black_box([29; 64]))));
        assert_eq!((allocations, bytes, alignment), (1, 64, 64));
        assert_eq!(aligned.0[63], 29);
    }

    #[test]
    fn unwind_restores_outer_and_inactive_measurements() {
        let outer = Measurement::start();
        let result = std::panic::catch_unwind(|| {
            let _inner = Measurement::start();
            panic!("instrumentation unwind control");
        });
        assert!(result.is_err());
        assert_eq!(COUNTERS.with(|counters| counters.depth.get()), 1);
        let (_, counts) = measure(|| ());
        assert_eq!(counts, AllocationCounts::default());
        outer.finish();
        assert_eq!(COUNTERS.with(|counters| counters.depth.get()), 0);
        let result = std::panic::catch_unwind(|| {
            measure(|| panic!("inactive restoration control"));
        });
        assert!(result.is_err());
        assert_eq!(COUNTERS.with(|counters| counters.depth.get()), 0);
        let (value, counts) = measure(|| Box::new(black_box([31_u8; 43])));
        assert_eq!(counts.allocations, 1);
        assert_eq!(counts.allocation_bytes, 43);
        assert_eq!(value[42], 31);
    }

    #[test]
    fn concurrent_threads_observe_only_their_own_requests() {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|index| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let (value, counts) = measure(|| vec![black_box(7_u8); 64 + index]);
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
}
