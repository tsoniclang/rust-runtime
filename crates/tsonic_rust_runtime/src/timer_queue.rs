use alloc::collections::BTreeMap;
use alloc::rc::{Rc, Weak};
use core::cell::{Cell, OnceCell, RefCell};
use core::fmt;
use core::num::NonZeroUsize;
use core::sync::atomic::{AtomicU64, Ordering};
use core::time::Duration;
use std::time::Instant;

use crate::dispatch::{DispatchContexts, DispatchPhase};
use crate::dispatch_queue::{TaskBudget, TaskQueueError, TaskReservation};
use crate::ordered_dispatch::next_ordered_key;
use crate::Callable;

pub trait TimerCallback: Clone {
    type Error;
    fn invoke(self) -> Result<(), Self::Error>;
}

impl<TError> TimerCallback for Callable<(), Result<(), TError>> {
    type Error = TError;
    fn invoke(self) -> Result<(), TError> {
        self.call(())
    }
}

impl<TError> TimerCallback for Rc<RefCell<dyn FnMut() -> Result<(), TError>>> {
    type Error = TError;
    fn invoke(self) -> Result<(), TError> {
        self.borrow_mut()()
    }
}

pub struct TimerContext<TCallback> {
    queue: TimerQueue<TCallback>,
    phase: DispatchPhase,
}

impl<TCallback> TimerContext<TCallback> {
    pub const fn new(phase: DispatchPhase) -> Self {
        Self {
            queue: TimerQueue::new(),
            phase,
        }
    }

    pub fn schedule_with(
        &self,
        delay: Duration,
        interval: bool,
        refed: bool,
        aborted: bool,
        callback: impl FnOnce() -> TCallback,
    ) -> Result<TimerHandle<TCallback>, TimerQueueError> {
        self.queue
            .schedule_with(delay, interval, refed, aborted, callback)
    }

    pub fn cancel(&self, id: u64) {
        self.queue.cancel(id);
    }

    pub fn has_pending_work(&self) -> bool {
        self.queue.has_refed()
    }
}

impl<TCallback: TimerCallback> TimerContext<TCallback> {
    pub fn poll(&self) -> Result<bool, TCallback::Error> {
        crate::dispatch::poll_phase(self, self.phase)
    }
}

impl<TCallback: TimerCallback> DispatchContexts for TimerContext<TCallback> {
    type Error = TCallback::Error;
    type Frontier = Option<TimerFrontier>;

    fn prepare(&self, phase: DispatchPhase) -> Result<Self::Frontier, Self::Error> {
        Ok((phase == self.phase).then(|| self.queue.prepare()))
    }

    fn next_ready(&self, frontier: &Self::Frontier) -> Option<u64> {
        self.queue.next_ready(frontier.as_ref()?)
    }

    fn poll_next(&self, frontier: &Self::Frontier) -> Result<bool, Self::Error> {
        match frontier
            .as_ref()
            .and_then(|frontier| self.queue.take_ready(frontier))
        {
            Some(callback) => callback.invoke().map(|()| true),
            None => Ok(false),
        }
    }

    fn has_work(&self) -> bool {
        self.has_pending_work()
    }

    fn next_delay(&self) -> Option<Duration> {
        self.queue.next_delay()
    }
}

const MAX_PENDING_TIMERS: usize = 1 << 20;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

std::thread_local! {
    static TIMER_BUDGET: OnceCell<TaskBudget> = const { OnceCell::new() };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerQueueError {
    Capacity,
    IdentityExhausted,
    DeadlineOutOfRange,
}

impl fmt::Display for TimerQueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Capacity => "pending native timers exceed the finite shared limit",
            Self::IdentityExhausted => "native timer identity range is exhausted",
            Self::DeadlineOutOfRange => "native timer deadline is outside the platform range",
        })
    }
}

impl core::error::Error for TimerQueueError {}

impl From<TaskQueueError> for TimerQueueError {
    fn from(value: TaskQueueError) -> Self {
        match value {
            TaskQueueError::Capacity => Self::Capacity,
            TaskQueueError::TicketExhausted => Self::IdentityExhausted,
            TaskQueueError::Closed => unreachable!("a live timer budget is not a weak task handle"),
        }
    }
}

struct TimerEntry<TCallback> {
    callback: TCallback,
    delay: Duration,
    due: Instant,
    interval: bool,
    refed: bool,
    reservation: TaskReservation,
}

type TimerEntries<TCallback> = RefCell<BTreeMap<u64, TimerEntry<TCallback>>>;

pub struct TimerQueue<TCallback> {
    entries: OnceCell<Rc<TimerEntries<TCallback>>>,
}

pub struct TimerHandle<TCallback> {
    entries: Weak<TimerEntries<TCallback>>,
    id: u64,
    delay: Duration,
}

pub struct TimerFrontier {
    owner: Option<usize>,
    boundary: Option<u64>,
    cursor: Cell<Option<u64>>,
    now: Instant,
}

impl<TCallback> Clone for TimerHandle<TCallback> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            id: self.id,
            delay: self.delay,
        }
    }
}

impl<TCallback> fmt::Debug for TimerHandle<TCallback> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TimerHandle")
            .field("id", &self.id)
            .field("delay", &self.delay)
            .finish()
    }
}

impl<TCallback> PartialEq for TimerHandle<TCallback> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<TCallback> Eq for TimerHandle<TCallback> {}

impl<TCallback> TimerHandle<TCallback> {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn delay(&self) -> Duration {
        self.delay
    }

    pub fn has_ref(&self) -> bool {
        self.entries.upgrade().is_some_and(|entries| {
            entries
                .borrow()
                .get(&self.id)
                .is_some_and(|entry| entry.refed)
        })
    }

    pub fn set_ref(&self, refed: bool) {
        if let Some(entries) = self.entries.upgrade() {
            if let Some(entry) = entries.borrow_mut().get_mut(&self.id) {
                entry.refed = refed;
            }
        }
    }

    pub fn refresh(&self) -> Result<(), TimerQueueError> {
        if let Some(entries) = self.entries.upgrade() {
            if let Some(entry) = entries.borrow_mut().get_mut(&self.id) {
                entry.due = Instant::now()
                    .checked_add(entry.delay)
                    .ok_or(TimerQueueError::DeadlineOutOfRange)?;
            }
        }
        Ok(())
    }

    pub fn close(&self) {
        if let Some(entries) = self.entries.upgrade() {
            let removed = entries.borrow_mut().remove(&self.id);
            drop(removed);
        }
    }
}

impl<TCallback> Default for TimerQueue<TCallback> {
    fn default() -> Self {
        Self::new()
    }
}

impl<TCallback> TimerQueue<TCallback> {
    pub const fn new() -> Self {
        Self {
            entries: OnceCell::new(),
        }
    }

    pub fn schedule_with(
        &self,
        delay: Duration,
        interval: bool,
        refed: bool,
        aborted: bool,
        callback: impl FnOnce() -> TCallback,
    ) -> Result<TimerHandle<TCallback>, TimerQueueError> {
        let id = NEXT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| TimerQueueError::IdentityExhausted)?;
        if aborted {
            return Ok(TimerHandle {
                entries: Weak::new(),
                id,
                delay,
            });
        }
        let due = Instant::now()
            .checked_add(delay)
            .ok_or(TimerQueueError::DeadlineOutOfRange)?;
        let reservation = TIMER_BUDGET.with(|budget| {
            budget
                .get_or_init(|| {
                    TaskBudget::new(
                        NonZeroUsize::new(MAX_PENDING_TIMERS).expect("finite native timer limit"),
                    )
                })
                .reserve()
        })?;
        let callback = callback();
        let entries = self
            .entries
            .get_or_init(|| Rc::new(RefCell::new(BTreeMap::new())));
        entries.borrow_mut().insert(
            id,
            TimerEntry {
                callback,
                delay,
                due,
                interval,
                refed,
                reservation,
            },
        );
        Ok(TimerHandle {
            entries: Rc::downgrade(entries),
            id,
            delay,
        })
    }

    pub fn cancel(&self, id: u64) {
        if let Some(entries) = self.entries.get() {
            let removed = entries.borrow_mut().remove(&id);
            drop(removed);
        }
    }

    pub fn has_refed(&self) -> bool {
        self.entries
            .get()
            .is_some_and(|entries| entries.borrow().values().any(|entry| entry.refed))
    }

    pub fn has_pending(&self) -> bool {
        self.entries
            .get()
            .is_some_and(|entries| !entries.borrow().is_empty())
    }

    pub fn next_delay(&self) -> Option<Duration> {
        let now = Instant::now();
        self.entries.get().and_then(|entries| {
            entries
                .borrow()
                .values()
                .map(|entry| entry.due.saturating_duration_since(now))
                .min()
        })
    }

    pub fn prepare(&self) -> TimerFrontier {
        TimerFrontier {
            owner: self
                .entries
                .get()
                .map(|entries| Rc::as_ptr(entries) as usize),
            boundary: self
                .entries
                .get()
                .and_then(|entries| entries.borrow().keys().next_back().copied()),
            cursor: Cell::new(None),
            now: Instant::now(),
        }
    }

    pub fn next_ready(&self, frontier: &TimerFrontier) -> Option<u64> {
        if frontier.owner
            != self
                .entries
                .get()
                .map(|entries| Rc::as_ptr(entries) as usize)
        {
            return None;
        }
        let boundary = frontier.boundary?;
        next_ordered_key(
            &self.entries.get()?.borrow(),
            frontier.cursor.get(),
            boundary,
            |entry| entry.due <= frontier.now,
        )
    }
}

impl<TCallback: Clone> TimerQueue<TCallback> {
    pub fn take_ready(&self, frontier: &TimerFrontier) -> Option<TCallback> {
        if frontier.owner
            != self
                .entries
                .get()
                .map(|entries| Rc::as_ptr(entries) as usize)
        {
            return None;
        }
        let boundary = frontier.boundary?;
        let entries = self.entries.get()?;
        let selected = {
            let mut entries = entries.borrow_mut();
            let id = next_ordered_key(&entries, frontier.cursor.get(), boundary, |entry| {
                entry.due <= frontier.now
            })?;
            frontier.cursor.set(Some(id));
            let entry = entries.get_mut(&id).expect("selected native timer");
            if entry.interval {
                entry.due = Instant::now()
                    .checked_add(entry.delay)
                    .expect("native timer repeat deadline exceeds platform range");
                (entry.callback.clone(), None)
            } else {
                let entry = entries.remove(&id).expect("selected native timer");
                (entry.callback, Some(entry.reservation))
            }
        };
        drop(selected.1);
        Some(selected.0)
    }
}

#[cfg(test)]
mod tests;
