use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::vec::Vec;

use super::*;

struct Failure(Rc<Cell<i64>>);

struct Resource {
    pending: RefCell<VecDeque<(u64, Result<bool, Failure>)>>,
    observed: Rc<RefCell<Vec<u64>>>,
}

impl DispatchContexts for Resource {
    type Error = Failure;
    type Frontier = Option<u64>;

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
    assert_eq!(poll_phase(&contexts, DispatchPhase::Ports).ok(), Some(true));
    assert_eq!(*observed.borrow(), [0, 1, 2]);
    assert_eq!(
        poll_phase(&contexts, DispatchPhase::Ports).ok(),
        Some(false)
    );
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
        poll_phase(&contexts, DispatchPhase::Workers).expect_err("original source failure");
    assert!(Rc::ptr_eq(&returned.0, &original));
    assert_eq!(returned.0.get(), 9_007_199_254_740_993);
    assert_eq!(*observed.borrow(), [0]);
    assert_eq!(
        poll_phase(&contexts, DispatchPhase::Workers).ok(),
        Some(true)
    );
    assert_eq!(*observed.borrow(), [0, 1]);
}

#[test]
fn captured_frontier_excludes_later_native_admission() {
    let resource = Resource {
        pending: RefCell::new(VecDeque::from([(0, Ok(false))])),
        observed: Rc::new(RefCell::new(Vec::new())),
    };
    let frontier = resource
        .prepare(DispatchPhase::Ports)
        .ok()
        .expect("frontier");
    resource.pending.borrow_mut().push_back((1, Ok(true)));
    assert_eq!(poll_prepared(&resource, &frontier).ok(), Some(false));
    assert_eq!(*resource.observed.borrow(), [0]);
    assert_eq!(poll_phase(&resource, DispatchPhase::Ports).ok(), Some(true));
    assert_eq!(*resource.observed.borrow(), [0, 1]);
}
