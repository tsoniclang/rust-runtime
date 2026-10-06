use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::rc::{Rc, Weak};
use core::cell::{Cell, RefCell};
use core::fmt;
use core::num::NonZeroUsize;

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
    tasks: RefCell<VecDeque<QueuedTask<TError>>>,
}

impl<TError> QueueState<TError> {
    fn enqueue(
        &self,
        callback: impl FnOnce() -> Result<(), TError> + 'static,
    ) -> Result<TaskTicket, TaskQueueError> {
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

pub struct TaskHandle<TError>(Weak<QueueState<TError>>);

impl<TError> Clone for TaskHandle<TError> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<TError> TaskHandle<TError> {
    pub fn enqueue(
        &self,
        callback: impl FnOnce() -> Result<(), TError> + 'static,
    ) -> Result<TaskTicket, TaskQueueError> {
        self.0
            .upgrade()
            .ok_or(TaskQueueError::Closed)?
            .enqueue(callback)
    }
}

impl<TError> TaskQueue<TError> {
    pub fn new(budget: TaskBudget) -> Self {
        Self(Rc::new(QueueState {
            budget,
            tasks: RefCell::new(VecDeque::new()),
        }))
    }

    pub fn handle(&self) -> TaskHandle<TError> {
        TaskHandle(Rc::downgrade(&self.0))
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
