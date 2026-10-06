use core::marker::PhantomData;
use core::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DispatchPhase {
    JsTimers,
    Background,
    RuntimeTasks,
    Signals,
    Timers,
    Http,
    Net,
    Tls,
    Watchers,
    Workers,
    Ports,
}

pub trait DispatchContexts {
    type Error;
    type Frontier;

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
    while contexts.poll_next(frontier)? {
        did_work = true;
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
