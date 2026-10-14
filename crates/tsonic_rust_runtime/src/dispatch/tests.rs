use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::vec::Vec;

use super::*;

static FIRST_PHASE: PhaseRegistration = PhaseRegistration::new();
static SECOND_PHASE: PhaseRegistration = PhaseRegistration::new();

fn owner() -> DispatchOwner {
    DispatchOwner::new().unwrap()
}

#[test]
fn owner_identity_exhaustion_is_permanent_and_never_wraps() {
    let sequence = AtomicUsize::new(usize::MAX - 1);
    let final_owner = DispatchOwner::reserve(&sequence).unwrap();
    assert_eq!(final_owner.0.get(), usize::MAX);
    assert!(DispatchOwner::reserve(&sequence).is_none());
    assert!(DispatchOwner::reserve(&sequence).is_none());
    assert_eq!(sequence.load(Ordering::Relaxed), usize::MAX);
}

#[test]
fn owner_identities_are_unique_across_threads_and_dropped_activations() {
    let owners: Vec<_> = (0..4)
        .map(|_| std::thread::spawn(|| (0..256).map(|_| owner()).collect::<Vec<_>>()))
        .collect();
    let mut identities: Vec<_> = owners
        .into_iter()
        .flat_map(|thread| thread.join().unwrap())
        .map(|owner| owner.0.get())
        .collect();
    identities.sort_unstable();
    identities.dedup();
    assert_eq!(identities.len(), 1024);
}

#[test]
fn stale_copied_owner_never_closes_or_claims_a_different_activation() {
    let source = DispatchGeneration::new();
    let stale_owner = owner();
    assert!(source.enter(stale_owner));
    let stale_generation = source.capture();
    assert!(source.close(stale_owner));
    for _ in 0..256 {
        let current = owner();
        assert_ne!(current, stale_owner);
        assert!(source.enter(current));
        assert!(!source.enter(stale_owner));
        assert!(!source.close(stale_owner));
        assert!(!source.accepts(stale_generation));
        assert!(source.owns(current));
        assert!(source.close(current));
    }
}

#[test]
fn new_sources_never_accept_a_retired_source_token_even_on_the_same_owner() {
    let identity = owner();
    let retired = {
        let source = DispatchGeneration::new();
        assert!(source.enter(identity));
        source.capture()
    };
    for _ in 0..256 {
        let replacement = DispatchGeneration::new();
        assert!(replacement.enter(identity));
        assert!(!replacement.accepts(retired));
        assert!(replacement.close(identity));
    }
}

#[test]
fn phase_registration_identity_is_stable_and_distinct_from_order() {
    assert_ne!(core::mem::size_of::<PhaseRegistration>(), 0);
    assert_eq!(FIRST_PHASE.phase(), FIRST_PHASE.phase());
    assert_ne!(FIRST_PHASE.phase(), SECOND_PHASE.phase());
    let order = [SECOND_PHASE.phase(), FIRST_PHASE.phase()];
    assert_eq!(order[0], SECOND_PHASE.phase());
    assert_eq!(order[1], FIRST_PHASE.phase());
}

#[test]
fn generation_close_retires_old_handles_and_binds_next_precaptured_handles() {
    let owner = DispatchGeneration::new();
    let identity = self::owner();
    assert!(owner.enter(identity));
    let first = owner.capture();
    assert!(owner.accepts(first));
    owner.close(identity);
    let closed = owner.capture();
    assert!(!owner.accepts(first));
    assert!(!owner.accepts(closed));
    assert!(owner.enter(identity));
    assert!(!owner.accepts(first));
    assert!(owner.accepts(closed));
    assert!(owner.accepts(owner.capture()));
}

struct OwnedResource {
    generation: DispatchGeneration,
    closes: Cell<usize>,
}

impl OwnedResource {
    fn new() -> Self {
        Self {
            generation: DispatchGeneration::new_closed(),
            closes: Cell::new(0),
        }
    }
}

impl DispatchContexts for OwnedResource {
    type Error = ();
    type Frontier = ();

    fn enter(&self, owner: DispatchOwner) -> bool {
        self.generation.enter(owner)
    }
    fn close(&self, owner: DispatchOwner) {
        if self.generation.close(owner) {
            self.closes.set(self.closes.get() + 1);
        }
    }
    fn prepare(&self, _phase: DispatchPhase) -> Result<(), ()> {
        Ok(())
    }
    fn next_ready(&self, _frontier: &()) -> Option<u64> {
        None
    }
    fn poll_next(&self, _frontier: &()) -> Result<bool, ()> {
        Ok(false)
    }
    fn has_work(&self) -> bool {
        false
    }
    fn next_delay(&self) -> Option<Duration> {
        None
    }
}

#[test]
fn composite_entry_rolls_back_only_its_own_partial_registration() {
    let free = OwnedResource::new();
    let occupied = OwnedResource::new();
    let other = owner();
    assert!(occupied.enter(other));
    let contexts = prepend(&free, prepend(&occupied, DispatchEnd::<()>::new()));
    for attempt in 1..=128 {
        let rejected = owner();
        assert!(!contexts.enter(rejected));
        contexts.close(rejected);
        assert_eq!(free.closes.get(), attempt);
        assert_eq!(occupied.closes.get(), 0);
        assert!(occupied.generation.owns(other));
    }
    occupied.close(other);
    assert_eq!(occupied.closes.get(), 1);
}

#[test]
fn exhausted_generation_never_reopens_or_accepts_a_stale_handle() {
    let owner = DispatchGeneration::new();
    owner.epoch.set(NonZeroU64::new(u64::MAX));
    let identity = self::owner();
    assert!(owner.enter(identity));
    let old = owner.capture();
    owner.close(identity);
    assert!(!owner.enter(identity));
    assert_eq!(owner.capture(), None);
    assert!(!owner.accepts(old));
}

struct Failure(Rc<Cell<i64>>);

struct Resource {
    pending: RefCell<VecDeque<(u64, Result<bool, Failure>)>>,
    observed: Rc<RefCell<Vec<u64>>>,
}

impl DispatchContexts for Resource {
    type Error = Failure;
    type Frontier = Option<u64>;

    fn enter(&self, _owner: DispatchOwner) -> bool {
        true
    }
    fn close(&self, _owner: DispatchOwner) {}

    fn prepare(&self, _phase: DispatchPhase) -> Result<Self::Frontier, Failure> {
        Ok(self.pending.borrow().back().map(|(ticket, _)| *ticket))
    }

    fn next_ready(&self, boundary: &Self::Frontier) -> Option<u64> {
        self.pending.borrow().front().and_then(|(ticket, _)| {
            boundary
                .filter(|boundary| ticket <= boundary)
                .map(|_| *ticket)
        })
    }

    fn poll_next(&self, boundary: &Self::Frontier) -> Result<bool, Failure> {
        if self.next_ready(boundary).is_none() {
            return Ok(false);
        }
        let (ticket, result) = self
            .pending
            .borrow_mut()
            .pop_front()
            .expect("selected native candidate");
        self.observed.borrow_mut().push(ticket);
        result
    }

    fn has_work(&self) -> bool {
        !self.pending.borrow().is_empty()
    }
    fn next_delay(&self) -> Option<Duration> {
        None
    }
}

#[test]
fn an_idle_native_resource_does_not_starve_later_ready_roots() {
    let observed = Rc::new(RefCell::new(Vec::new()));
    let left = Resource {
        pending: RefCell::new(VecDeque::from([(0, Ok(false)), (2, Ok(true))])),
        observed: Rc::clone(&observed),
    };
    let right = Resource {
        pending: RefCell::new(VecDeque::from([(1, Ok(true))])),
        observed: Rc::clone(&observed),
    };
    let contexts = prepend(&left, prepend(&right, DispatchEnd::<Failure>::new()));
    assert_eq!(poll_phase(&contexts, FIRST_PHASE.phase()).ok(), Some(true));
    assert_eq!(*observed.borrow(), [0, 1, 2]);
    assert_eq!(poll_phase(&contexts, FIRST_PHASE.phase()).ok(), Some(false));
    assert_eq!(*observed.borrow(), [0, 1, 2]);
}

#[test]
fn first_error_preserves_unvisited_candidates_and_exact_payload() {
    let observed = Rc::new(RefCell::new(Vec::new()));
    let original = Rc::new(Cell::new(9_007_199_254_740_993));
    let left = Resource {
        pending: RefCell::new(VecDeque::from([(0, Err(Failure(Rc::clone(&original))))])),
        observed: Rc::clone(&observed),
    };
    let right = Resource {
        pending: RefCell::new(VecDeque::from([(1, Ok(true))])),
        observed: Rc::clone(&observed),
    };
    let contexts = prepend(&left, prepend(&right, DispatchEnd::<Failure>::new()));
    let returned =
        poll_phase(&contexts, SECOND_PHASE.phase()).expect_err("original source failure");
    assert!(Rc::ptr_eq(&returned.0, &original));
    assert_eq!(returned.0.get(), 9_007_199_254_740_993);
    assert_eq!(*observed.borrow(), [0]);
    assert_eq!(poll_phase(&contexts, SECOND_PHASE.phase()).ok(), Some(true));
    assert_eq!(*observed.borrow(), [0, 1]);
}

#[test]
fn captured_frontier_excludes_later_native_admission() {
    let resource = Resource {
        pending: RefCell::new(VecDeque::from([(0, Ok(false))])),
        observed: Rc::new(RefCell::new(Vec::new())),
    };
    let frontier = resource
        .prepare(FIRST_PHASE.phase())
        .ok()
        .expect("frontier");
    resource.pending.borrow_mut().push_back((1, Ok(true)));
    assert_eq!(poll_prepared(&resource, &frontier).ok(), Some(false));
    assert_eq!(*resource.observed.borrow(), [0]);
    assert_eq!(poll_phase(&resource, FIRST_PHASE.phase()).ok(), Some(true));
    assert_eq!(*resource.observed.borrow(), [0, 1]);
}
