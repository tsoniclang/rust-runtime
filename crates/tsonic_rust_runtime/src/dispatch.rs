use core::cell::Cell;
use core::marker::PhantomData;
use core::num::{NonZeroU64, NonZeroUsize};
use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;

#[cfg(all(test, feature = "std"))]
mod tests;

#[derive(Debug)]
pub struct PhaseRegistration {
    _identity: u8,
}

impl PhaseRegistration {
    pub const fn new() -> Self {
        Self { _identity: 0 }
    }

    pub const fn phase(&'static self) -> DispatchPhase {
        DispatchPhase(self)
    }
}

impl Default for PhaseRegistration {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DispatchPhase(&'static PhaseRegistration);

impl PartialEq for DispatchPhase {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.0, other.0)
    }
}

impl Eq for DispatchPhase {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchOwner(NonZeroUsize);

static NEXT_OWNER: AtomicUsize = AtomicUsize::new(0);

impl DispatchOwner {
    pub fn new() -> Option<Self> {
        Self::reserve(&NEXT_OWNER)
    }

    fn reserve(sequence: &AtomicUsize) -> Option<Self> {
        let previous = sequence
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .ok()?;
        NonZeroUsize::new(previous + 1).map(Self)
    }

    pub fn claim(self, slot: &Cell<Option<Self>>) -> bool {
        match slot.get() {
            Some(current) => current == self,
            None => {
                slot.set(Some(self));
                true
            }
        }
    }

    pub fn release(self, slot: &Cell<Option<Self>>) -> bool {
        if slot.get() == Some(self) {
            slot.set(None);
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchGenerationId {
    source: DispatchOwner,
    epoch: NonZeroU64,
}

pub struct DispatchEpoch {
    owner: Cell<Option<DispatchOwner>>,
    epoch: Cell<Option<NonZeroU64>>,
}

impl DispatchEpoch {
    pub const fn new() -> Self {
        Self {
            owner: Cell::new(None),
            epoch: Cell::new(Some(NonZeroU64::MIN)),
        }
    }

    #[inline]
    pub fn capture(&self) -> Option<NonZeroU64> {
        self.epoch.get()
    }

    #[inline]
    pub fn matches_epoch(&self, epoch: Option<NonZeroU64>) -> bool {
        epoch.is_some() && self.epoch.get() == epoch
    }

    #[inline]
    pub fn is_bound(&self) -> bool {
        self.owner.get().is_some()
    }

    #[inline]
    pub fn owns(&self, owner: DispatchOwner) -> bool {
        self.owner.get() == Some(owner)
    }

    #[inline]
    pub fn enter(&self, owner: DispatchOwner) -> bool {
        self.epoch.get().is_some() && owner.claim(&self.owner)
    }

    #[inline]
    pub fn close(&self, owner: DispatchOwner) -> bool {
        if !owner.release(&self.owner) {
            return false;
        }
        self.epoch
            .set(self.epoch.get().and_then(|epoch| epoch.checked_add(1)));
        true
    }
}

impl Default for DispatchEpoch {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DispatchGeneration {
    identity: Cell<Option<DispatchOwner>>,
    epoch: DispatchEpoch,
    unbound_open: Cell<bool>,
}

impl DispatchGeneration {
    pub const fn new() -> Self {
        Self {
            identity: Cell::new(None),
            epoch: DispatchEpoch::new(),
            unbound_open: Cell::new(true),
        }
    }

    pub const fn new_closed() -> Self {
        Self {
            identity: Cell::new(None),
            epoch: DispatchEpoch::new(),
            unbound_open: Cell::new(false),
        }
    }

    pub fn capture(&self) -> Option<DispatchGenerationId> {
        let epoch = self.epoch.capture()?;
        let source = match self.identity.get() {
            Some(identity) => identity,
            None => {
                let identity = DispatchOwner::new()?;
                self.identity.set(Some(identity));
                identity
            }
        };
        Some(DispatchGenerationId { source, epoch })
    }

    pub fn accepts(&self, generation: Option<DispatchGenerationId>) -> bool {
        self.is_open() && generation.is_some() && generation == self.capture()
    }

    pub fn is_open(&self) -> bool {
        self.epoch.capture().is_some() && (self.epoch.is_bound() || self.unbound_open.get())
    }

    pub fn owns(&self, owner: DispatchOwner) -> bool {
        self.epoch.owns(owner)
    }

    pub fn enter(&self, owner: DispatchOwner) -> bool {
        if self.capture().is_none() {
            return false;
        }
        self.epoch.enter(owner)
    }

    pub fn close(&self, owner: DispatchOwner) -> bool {
        if !self.epoch.close(owner) {
            return false;
        }
        self.unbound_open.set(false);
        true
    }
}

impl Default for DispatchGeneration {
    fn default() -> Self {
        Self::new()
    }
}

pub trait DispatchContexts {
    type Error;
    type Frontier;

    fn enter(&self, owner: DispatchOwner) -> bool;
    fn close(&self, owner: DispatchOwner);
    fn prepare(&self, phase: DispatchPhase) -> Result<Self::Frontier, Self::Error>;
    fn next_ready(&self, frontier: &Self::Frontier) -> Option<u64>;
    fn poll_next(&self, frontier: &Self::Frontier) -> Result<bool, Self::Error>;
    fn has_work(&self) -> bool;
    fn next_delay(&self) -> Option<Duration>;
}

pub struct DispatchEnd<TError>(PhantomData<fn() -> TError>);

impl<TError> DispatchEnd<TError> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<TError> Default for DispatchEnd<TError> {
    fn default() -> Self {
        Self::new()
    }
}

impl<TError> DispatchContexts for DispatchEnd<TError> {
    type Error = TError;
    type Frontier = ();

    fn enter(&self, _owner: DispatchOwner) -> bool {
        true
    }
    fn close(&self, _owner: DispatchOwner) {}

    fn prepare(&self, _phase: DispatchPhase) -> Result<(), TError> {
        Ok(())
    }

    fn next_ready(&self, _frontier: &()) -> Option<u64> {
        None
    }

    fn poll_next(&self, _frontier: &()) -> Result<bool, TError> {
        Ok(false)
    }

    fn has_work(&self) -> bool {
        false
    }

    fn next_delay(&self) -> Option<Duration> {
        None
    }
}

pub struct DispatchList<'context, TContext, TTail> {
    context: &'context TContext,
    tail: TTail,
}

pub fn prepend<TContext: DispatchContexts, TTail: DispatchContexts>(
    context: &TContext,
    tail: TTail,
) -> DispatchList<'_, TContext, TTail>
where
    TTail::Error: From<TContext::Error>,
{
    DispatchList { context, tail }
}

impl<TContext: DispatchContexts, TTail: DispatchContexts> DispatchContexts
    for DispatchList<'_, TContext, TTail>
where
    TTail::Error: From<TContext::Error>,
{
    type Error = TTail::Error;
    type Frontier = (TContext::Frontier, TTail::Frontier);

    fn enter(&self, owner: DispatchOwner) -> bool {
        if !self.context.enter(owner) {
            return false;
        }
        if self.tail.enter(owner) {
            return true;
        }
        self.context.close(owner);
        false
    }

    fn close(&self, owner: DispatchOwner) {
        self.context.close(owner);
        self.tail.close(owner);
    }

    fn prepare(&self, phase: DispatchPhase) -> Result<Self::Frontier, Self::Error> {
        let head = self.context.prepare(phase).map_err(Self::Error::from)?;
        let tail = self.tail.prepare(phase)?;
        Ok((head, tail))
    }

    fn next_ready(&self, frontier: &Self::Frontier) -> Option<u64> {
        minimum(
            self.context.next_ready(&frontier.0),
            self.tail.next_ready(&frontier.1),
        )
    }

    fn poll_next(&self, frontier: &Self::Frontier) -> Result<bool, Self::Error> {
        let head = self.context.next_ready(&frontier.0);
        let tail = self.tail.next_ready(&frontier.1);
        if head.is_some() && (tail.is_none() || head <= tail) {
            self.context
                .poll_next(&frontier.0)
                .map_err(Self::Error::from)
        } else {
            self.tail.poll_next(&frontier.1)
        }
    }

    fn has_work(&self) -> bool {
        self.context.has_work() || self.tail.has_work()
    }

    fn next_delay(&self) -> Option<Duration> {
        minimum(self.context.next_delay(), self.tail.next_delay())
    }
}

impl<TContexts: DispatchContexts> DispatchContexts for &TContexts {
    type Error = TContexts::Error;
    type Frontier = TContexts::Frontier;

    fn enter(&self, owner: DispatchOwner) -> bool {
        TContexts::enter(self, owner)
    }
    fn close(&self, owner: DispatchOwner) {
        TContexts::close(self, owner)
    }

    fn prepare(&self, phase: DispatchPhase) -> Result<Self::Frontier, Self::Error> {
        TContexts::prepare(self, phase)
    }

    fn next_ready(&self, frontier: &Self::Frontier) -> Option<u64> {
        TContexts::next_ready(self, frontier)
    }

    fn poll_next(&self, frontier: &Self::Frontier) -> Result<bool, Self::Error> {
        TContexts::poll_next(self, frontier)
    }

    fn has_work(&self) -> bool {
        TContexts::has_work(self)
    }

    fn next_delay(&self) -> Option<Duration> {
        TContexts::next_delay(self)
    }
}

pub fn poll_phase<TContexts: DispatchContexts>(
    contexts: &TContexts,
    phase: DispatchPhase,
) -> Result<bool, TContexts::Error> {
    let frontier = contexts.prepare(phase)?;
    poll_prepared(contexts, &frontier)
}

pub fn poll_prepared<TContexts: DispatchContexts>(
    contexts: &TContexts,
    frontier: &TContexts::Frontier,
) -> Result<bool, TContexts::Error> {
    let mut did_work = false;
    while contexts.next_ready(frontier).is_some() {
        did_work |= contexts.poll_next(frontier)?;
    }
    Ok(did_work)
}

fn minimum<TValue: Ord>(left: Option<TValue>, right: Option<TValue>) -> Option<TValue> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}
