use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::rc::{Rc, Weak};
use core::cell::{Cell, RefCell};
use core::fmt;
use core::num::NonZeroUsize;

use crate::dispatch::{DispatchGeneration, DispatchGenerationId, DispatchOwner};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskQueueError {
    Capacity,
    TicketExhausted,
    Closed,
}

impl fmt::Display for TaskQueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Capacity => "pending native tasks exceed the finite shared queue limit",
            Self::TicketExhausted => "native task admission ticket range is exhausted",
            Self::Closed => "native task owner has been released",
        })
    }
}

impl core::error::Error for TaskQueueError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskTicket(u64);

impl TaskTicket {
    pub const fn sequence(self) -> u64 {
        self.0
    }
}

struct BudgetState {
    limit: NonZeroUsize,
    pending: Cell<usize>,
    next_ticket: Cell<u64>,
}

#[derive(Clone)]
pub struct TaskBudget(Rc<BudgetState>);

pub struct TaskReservation {
    budget: TaskBudget,
    ticket: TaskTicket,
}

impl TaskReservation {
    pub fn ticket(&self) -> TaskTicket {
        self.ticket
    }
}

impl Drop for TaskReservation {
    fn drop(&mut self) {
        self.budget.release(1);
    }
}

impl TaskBudget {
    pub fn new(limit: NonZeroUsize) -> Self {
        Self(Rc::new(BudgetState {
            limit,
            pending: Cell::new(0),
            next_ticket: Cell::new(0),
        }))
    }

    pub fn pending(&self) -> usize {
        self.0.pending.get()
    }

    pub fn limit(&self) -> NonZeroUsize {
        self.0.limit
    }

    pub fn ready_boundary(&self) -> Option<TaskTicket> {
        self.0.next_ticket.get().checked_sub(1).map(TaskTicket)
    }

    pub fn reserve(&self) -> Result<TaskReservation, TaskQueueError> {
        let ticket = self.reserve_ticket()?;
        Ok(TaskReservation {
            budget: self.clone(),
            ticket,
        })
    }

    fn reserve_ticket(&self) -> Result<TaskTicket, TaskQueueError> {
        let pending = self.pending();
        if pending >= self.limit().get() {
            return Err(TaskQueueError::Capacity);
        }
        let ticket = self.admit()?;
        self.0.pending.set(pending + 1);
        Ok(ticket)
    }

    pub fn admit(&self) -> Result<TaskTicket, TaskQueueError> {
        let ticket = self.0.next_ticket.get();
        let next = ticket
            .checked_add(1)
            .ok_or(TaskQueueError::TicketExhausted)?;
        self.0.next_ticket.set(next);
        Ok(TaskTicket(ticket))
    }

    fn release(&self, count: usize) {
        self.0.pending.set(
            self.pending()
                .checked_sub(count)
                .expect("native task reservations belong to their shared budget"),
        );
    }
}

struct QueuedTask<TError> {
    ticket: TaskTicket,
    callback: Box<dyn FnOnce() -> Result<(), TError>>,
}

struct QueueState<TError> {
    budget: TaskBudget,
    generation: DispatchGeneration,
    tasks: RefCell<VecDeque<QueuedTask<TError>>>,
}

impl<TError> QueueState<TError> {
    fn enqueue(
        &self,
        callback: impl FnOnce() -> Result<(), TError> + 'static,
    ) -> Result<TaskTicket, TaskQueueError> {
        if !self.generation.is_open() {
            return Err(TaskQueueError::Closed);
        }
        let ticket = self.budget.reserve_ticket()?;
        self.tasks.borrow_mut().push_back(QueuedTask {
            ticket,
            callback: Box::new(callback),
        });
        Ok(ticket)
    }
}

impl<TError> Drop for QueueState<TError> {
    fn drop(&mut self) {
        self.budget.release(self.tasks.get_mut().len());
    }
}

pub struct TaskQueue<TError>(Rc<QueueState<TError>>);

pub struct TaskHandle<TError> {
    owner: Weak<QueueState<TError>>,
    generation: Option<DispatchGenerationId>,
}

impl<TError> Clone for TaskHandle<TError> {
    fn clone(&self) -> Self {
        Self {
            owner: self.owner.clone(),
            generation: self.generation,
        }
    }
}

impl<TError> TaskHandle<TError> {
    pub fn enqueue(
        &self,
        callback: impl FnOnce() -> Result<(), TError> + 'static,
    ) -> Result<TaskTicket, TaskQueueError> {
        let owner = self.owner.upgrade().ok_or(TaskQueueError::Closed)?;
        if !owner.generation.accepts(self.generation) {
            return Err(TaskQueueError::Closed);
        }
        owner.enqueue(callback)
    }
}

impl<TError> TaskQueue<TError> {
    pub fn new(budget: TaskBudget) -> Self {
        Self::with_generation(budget, DispatchGeneration::new())
    }

    pub fn new_closed(budget: TaskBudget) -> Self {
        Self::with_generation(budget, DispatchGeneration::new_closed())
    }

    fn with_generation(budget: TaskBudget, generation: DispatchGeneration) -> Self {
        Self(Rc::new(QueueState {
            budget,
            generation,
            tasks: RefCell::new(VecDeque::new()),
        }))
    }

    pub fn handle(&self) -> TaskHandle<TError> {
        TaskHandle {
            owner: Rc::downgrade(&self.0),
            generation: self.0.generation.capture(),
        }
    }

    pub fn enter(&self, owner: DispatchOwner) -> bool {
        self.0.generation.enter(owner)
    }

    pub fn close(&self, owner: DispatchOwner) {
        if !self.0.generation.close(owner) {
            return;
        }
        let pending = core::mem::take(&mut *self.0.tasks.borrow_mut());
        self.0.budget.release(pending.len());
        drop(pending);
    }

    pub fn enqueue(
        &self,
        callback: impl FnOnce() -> Result<(), TError> + 'static,
    ) -> Result<TaskTicket, TaskQueueError> {
        self.0.enqueue(callback)
    }

    pub fn front_ticket(&self) -> Option<TaskTicket> {
        self.0.tasks.borrow().front().map(|task| task.ticket)
    }

    pub fn ready_boundary(&self) -> Option<TaskTicket> {
        self.0.budget.ready_boundary()
    }

    pub fn poll_one(&self) -> Result<bool, TError> {
        let task = self.0.tasks.borrow_mut().pop_front();
        match task {
            None => Ok(false),
            Some(task) => {
                self.0.budget.release(1);
                (task.callback)()?;
                Ok(true)
            }
        }
    }

    pub fn poll_ready(&self) -> Result<bool, TError> {
        let Some(boundary) = self.ready_boundary() else {
            return Ok(false);
        };
        self.poll_through(boundary)
    }

    pub fn poll_through(&self, boundary: TaskTicket) -> Result<bool, TError> {
        let mut did_work = false;
        while self.front_ticket().is_some_and(|ticket| ticket <= boundary) {
            did_work |= self.poll_one()?;
        }
        Ok(did_work)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_initially_closed_queue_binds_precaptured_handles_to_its_exact_generation() {
        let budget = TaskBudget::new(NonZeroUsize::new(2).unwrap());
        let queue = TaskQueue::<()>::new_closed(budget.clone());
        let unbound = queue.handle();
        assert_eq!(unbound.enqueue(|| Ok(())), Err(TaskQueueError::Closed));
        let retired = DispatchOwner::new().unwrap();
        assert!(queue.enter(retired));
        unbound.enqueue(|| Ok(())).unwrap();
        assert_eq!(queue.poll_ready(), Ok(true));
        let old = queue.handle();
        queue.close(retired);
        for _ in 0..128 {
            let precaptured = queue.handle();
            assert_eq!(precaptured.enqueue(|| Ok(())), Err(TaskQueueError::Closed));
            let current = DispatchOwner::new().unwrap();
            assert!(queue.enter(current));
            assert!(!queue.enter(retired));
            precaptured.enqueue(|| Ok(())).unwrap();
            queue.close(retired);
            assert_eq!(budget.pending(), 1);
            assert_eq!(queue.poll_ready(), Ok(true));
            assert_eq!(old.enqueue(|| Ok(())), Err(TaskQueueError::Closed));
            assert_eq!(unbound.enqueue(|| Ok(())), Err(TaskQueueError::Closed));
            queue.close(current);
        }
        assert_eq!(budget.pending(), 0);
    }

    #[test]
    fn owner_close_invalidates_handles_releases_exact_budget_and_allows_new_generation() {
        let budget = TaskBudget::new(NonZeroUsize::new(2).unwrap());
        let queue = TaskQueue::<()>::new(budget.clone());
        let owner = DispatchOwner::new().unwrap();
        assert!(queue.enter(owner));
        let old = queue.handle();
        old.enqueue(|| Ok(())).unwrap();
        assert_eq!(budget.pending(), 1);
        queue.close(owner);
        assert_eq!(budget.pending(), 0);
        assert_eq!(old.enqueue(|| Ok(())), Err(TaskQueueError::Closed));
        let closed = queue.handle();
        assert!(queue.enter(owner));
        assert_eq!(old.enqueue(|| Ok(())), Err(TaskQueueError::Closed));
        closed.enqueue(|| Ok(())).unwrap();
        assert_eq!(queue.poll_ready(), Ok(true));
        assert_eq!(budget.pending(), 0);
    }

    #[test]
    fn close_drops_callbacks_outside_the_borrow_and_never_closes_another_queue() {
        struct Reenter(TaskHandle<()>, Rc<Cell<bool>>);
        impl Drop for Reenter {
            fn drop(&mut self) {
                self.1
                    .set(self.0.enqueue(|| Ok(())) == Err(TaskQueueError::Closed));
            }
        }
        let budget = TaskBudget::new(NonZeroUsize::new(2).unwrap());
        let first = TaskQueue::<()>::new(budget.clone());
        let second = TaskQueue::<()>::new(budget.clone());
        let owner = DispatchOwner::new().unwrap();
        assert!(first.enter(owner));
        assert!(second.enter(owner));
        let rejected = Rc::new(Cell::new(false));
        let capture = Reenter(first.handle(), rejected.clone());
        first
            .enqueue(move || {
                drop(capture);
                Ok(())
            })
            .unwrap();
        second.enqueue(|| Ok(())).unwrap();
        first.close(owner);
        assert!(rejected.get());
        assert_eq!(budget.pending(), 1);
        assert_eq!(second.poll_ready(), Ok(true));
        assert_eq!(budget.pending(), 0);
    }

    #[test]
    fn externally_bounded_admission_uses_the_same_exact_ticket_sequence_without_a_reservation() {
        let budget = TaskBudget::new(NonZeroUsize::new(1).unwrap());
        let reservation = budget.reserve().unwrap();
        let next = budget.admit().unwrap();
        assert_eq!(next.sequence(), reservation.ticket().sequence() + 1);
        assert_eq!(budget.pending(), 1);
        assert_eq!(budget.ready_boundary(), Some(next));
        budget.0.next_ticket.set(u64::MAX);
        assert_eq!(budget.admit(), Err(TaskQueueError::TicketExhausted));
        assert_eq!(budget.pending(), 1);
        drop(reservation);
        assert_eq!(budget.pending(), 0);
    }

    #[test]
    fn admission_ticket_overflow_is_checked_without_consuming_capacity() {
        let budget = TaskBudget::new(NonZeroUsize::new(2).unwrap());
        budget.0.next_ticket.set(u64::MAX);
        let queue = TaskQueue::<()>::new(budget.clone());
        assert_eq!(
            queue.enqueue(|| Ok(())),
            Err(TaskQueueError::TicketExhausted)
        );
        assert_eq!(budget.pending(), 0);
        assert_eq!(queue.front_ticket(), None);
        assert!(matches!(
            budget.reserve(),
            Err(TaskQueueError::TicketExhausted)
        ));
        assert_eq!(budget.pending(), 0);
    }
}
