use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::rc::Rc;
use core::cell::{Cell, RefCell};
use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

use super::{
    GeneratorController, GeneratorCore, GeneratorPoll, GeneratorReentryPolicy, GeneratorResume,
    IteratorResult,
};

pub struct AsyncGeneratorImpl<'a, TYield, TReturn, TNext, TError, Reentry> {
    state: Rc<RefCell<AsyncGeneratorState<'a, TYield, TReturn, TNext, TError>>>,
    reentry: PhantomData<fn() -> Reentry>,
}

pub type AsyncGenerator<TYield, TReturn, TNext, TError, Reentry> =
    AsyncGeneratorImpl<'static, TYield, TReturn, TNext, TError, Reentry>;
pub type BorrowedAsyncGenerator<'a, TYield, TReturn, TNext, TError, Reentry> =
    AsyncGeneratorImpl<'a, TYield, TReturn, TNext, TError, Reentry>;

impl<'a, TYield, TReturn, TNext, TError, Reentry: GeneratorReentryPolicy<TError>>
    AsyncGeneratorImpl<'a, TYield, TReturn, TNext, TError, Reentry>
{
    pub fn new<TFactory, TFuture>(factory: TFactory) -> Self
    where
        TFactory: FnOnce(GeneratorController<TYield, TReturn, TNext, TError>) -> TFuture,
        TFuture: Future<Output = Result<TReturn, TError>> + 'a,
    {
        Self {
            state: Rc::new(RefCell::new(AsyncGeneratorState {
                core: Some(GeneratorCore::new(factory)),
                next_request_id: 0,
                queue: VecDeque::new(),
                results: BTreeMap::new(),
                wakers: BTreeMap::new(),
                abandoned: BTreeSet::new(),
            })),
            reentry: PhantomData,
        }
    }

    pub fn resume(
        &self,
    ) -> impl Future<Output = Result<IteratorResult<TYield, TReturn>, TError>> + 'a
    where
        TYield: 'a,
        TReturn: Clone + 'a,
        TNext: Default + 'a,
        TError: 'a,
    {
        self.enqueue(AsyncGeneratorOperation::Resume {
            value: Some(TNext::default()),
            prepared: false,
        })
    }

    pub fn next_yield(&self) -> impl Future<Output = Result<Option<TYield>, TError>> + 'a
    where
        TYield: 'a,
        TReturn: Clone + 'a,
        TNext: Default + 'a,
        TError: 'a,
    {
        let request = self.resume();
        async move { request.await.map(IteratorResult::into_yield) }
    }

    pub fn resume_with(
        &self,
        value: TNext,
    ) -> impl Future<Output = Result<IteratorResult<TYield, TReturn>, TError>> + 'a
    where
        TYield: 'a,
        TReturn: Clone + 'a,
        TNext: 'a,
        TError: 'a,
    {
        self.enqueue(AsyncGeneratorOperation::Resume {
            value: Some(value),
            prepared: false,
        })
    }

    pub fn return_value(
        &self,
        value: TReturn,
    ) -> impl Future<Output = Result<IteratorResult<TYield, TReturn>, TError>> + 'a
    where
        TYield: 'a,
        TReturn: Clone + 'a,
        TNext: 'a,
        TError: 'a,
    {
        self.enqueue(AsyncGeneratorOperation::Return {
            value: Some(value),
            prepared: false,
        })
    }

    pub fn throw_value(
        &self,
        error: TError,
    ) -> impl Future<Output = Result<IteratorResult<TYield, TReturn>, TError>> + 'a
    where
        TYield: 'a,
        TReturn: Clone + 'a,
        TNext: 'a,
        TError: 'a,
    {
        self.enqueue(AsyncGeneratorOperation::Throw {
            error: Some(error),
            prepared: false,
        })
    }

    fn enqueue(
        &self,
        operation: AsyncGeneratorOperation<TReturn, TNext, TError>,
    ) -> AsyncGeneratorRequest<'a, TYield, TReturn, TNext, TError, Reentry> {
        let (id, reentrant) = {
            let mut state = self.state.borrow_mut();
            let id = state.next_request_id;
            state.next_request_id = state
                .next_request_id
                .checked_add(1)
                .expect("async generator request identity exhausted");
            let reentrant = state.core.is_none();
            if !reentrant {
                state
                    .queue
                    .push_back(QueuedAsyncGeneratorOperation { id, operation });
            }
            (id, reentrant)
        };
        if reentrant {
            let error = Reentry::on_reentry();
            self.state.borrow_mut().results.insert(id, Err(error));
        }
        AsyncGeneratorRequest {
            state: Rc::clone(&self.state),
            id,
            completed: Cell::new(false),
            reentry: PhantomData,
        }
    }
}

struct AsyncGeneratorState<'a, TYield, TReturn, TNext, TError> {
    core: Option<GeneratorCore<'a, TYield, TReturn, TNext, TError>>,
    next_request_id: u64,
    queue: VecDeque<QueuedAsyncGeneratorOperation<TReturn, TNext, TError>>,
    results: BTreeMap<u64, Result<IteratorResult<TYield, TReturn>, TError>>,
    wakers: BTreeMap<u64, Waker>,
    abandoned: BTreeSet<u64>,
}

struct QueuedAsyncGeneratorOperation<TReturn, TNext, TError> {
    id: u64,
    operation: AsyncGeneratorOperation<TReturn, TNext, TError>,
}

enum AsyncGeneratorOperation<TReturn, TNext, TError> {
    Resume {
        value: Option<TNext>,
        prepared: bool,
    },
    Return {
        value: Option<TReturn>,
        prepared: bool,
    },
    Throw {
        error: Option<TError>,
        prepared: bool,
    },
}

struct AsyncGeneratorRequest<'a, TYield, TReturn, TNext, TError, Reentry> {
    state: Rc<RefCell<AsyncGeneratorState<'a, TYield, TReturn, TNext, TError>>>,
    id: u64,
    completed: Cell<bool>,
    reentry: PhantomData<fn() -> Reentry>,
}

impl<'a, TYield, TReturn, TNext, TError, Reentry: GeneratorReentryPolicy<TError>> Future
    for AsyncGeneratorRequest<'a, TYield, TReturn, TNext, TError, Reentry>
where
    TReturn: Clone,
{
    type Output = Result<IteratorResult<TYield, TReturn>, TError>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let request_id = self.id;
        loop {
            let incoming_waker = context.waker().clone();
            let (selected, replaced_waker, discarded_operation) = {
                let mut state = self.state.borrow_mut();
                if let Some(result) = state.results.remove(&request_id) {
                    self.completed.set(true);
                    return Poll::Ready(result);
                }
                if state.core.is_none() {
                    let position = state
                        .queue
                        .iter()
                        .position(|queued| queued.id == request_id);
                    let discarded = position.and_then(|index| state.queue.remove(index));
                    (None, state.wakers.remove(&request_id), discarded)
                } else {
                    let replaced = state.wakers.insert(request_id, incoming_waker);
                    (
                        Some((state.queue.pop_front(), state.core.take())),
                        replaced,
                        None,
                    )
                }
            };
            drop(replaced_waker);
            drop(discarded_operation);
            let Some((queued, core)) = selected else {
                self.completed.set(true);
                return Poll::Ready(Err(Reentry::on_reentry()));
            };
            let mut queued =
                queued.expect("async generator request is neither queued nor completed");
            let mut core = core.expect("an idle async generator owns its core");
            let result = poll_async_generator_operation(&mut queued.operation, &mut core, context);
            let (step, wake, discarded_result) = {
                let mut state = self.state.borrow_mut();
                state.core = Some(core);
                match result {
                    Poll::Pending => {
                        state.queue.push_front(queued);
                        (AsyncGeneratorRequestStep::Pending, [None, None], None)
                    }
                    Poll::Ready(result) => {
                        let completed_waker = state.wakers.remove(&queued.id);
                        let next_id = state.queue.front().map(|next| next.id);
                        let next_waker = next_id.and_then(|id| state.wakers.remove(&id));
                        let (step, discarded) = if queued.id == request_id {
                            (AsyncGeneratorRequestStep::Ready(result), None)
                        } else if state.abandoned.remove(&queued.id) {
                            (AsyncGeneratorRequestStep::Advanced, Some(result))
                        } else {
                            (
                                AsyncGeneratorRequestStep::Advanced,
                                state.results.insert(queued.id, result),
                            )
                        };
                        (step, [completed_waker, next_waker], discarded)
                    }
                }
            };
            drop(discarded_result);
            for waker in wake.into_iter().flatten() {
                waker.wake();
            }
            match step {
                AsyncGeneratorRequestStep::Pending => return Poll::Pending,
                AsyncGeneratorRequestStep::Ready(result) => {
                    self.completed.set(true);
                    return Poll::Ready(result);
                }
                AsyncGeneratorRequestStep::Advanced => {}
            }
        }
    }
}

impl<'a, TYield, TReturn, TNext, TError, Reentry> Drop
    for AsyncGeneratorRequest<'a, TYield, TReturn, TNext, TError, Reentry>
{
    fn drop(&mut self) {
        if self.completed.get() {
            return;
        }
        let released = {
            let mut state = self.state.borrow_mut();
            let waker = state.wakers.remove(&self.id);
            let result = state.results.remove(&self.id);
            if result.is_none() {
                state.abandoned.insert(self.id);
            }
            (waker, result)
        };
        drop(released);
    }
}

enum AsyncGeneratorRequestStep<TYield, TReturn, TError> {
    Pending,
    Ready(Result<IteratorResult<TYield, TReturn>, TError>),
    Advanced,
}

fn poll_async_generator_operation<TYield, TReturn, TNext, TError>(
    operation: &mut AsyncGeneratorOperation<TReturn, TNext, TError>,
    core: &mut GeneratorCore<'_, TYield, TReturn, TNext, TError>,
    context: &mut Context<'_>,
) -> Poll<Result<IteratorResult<TYield, TReturn>, TError>>
where
    TReturn: Clone,
{
    match operation {
        AsyncGeneratorOperation::Resume { value, prepared } => {
            if !*prepared {
                if !core.is_running() {
                    return Poll::Ready(Ok(core.completed_result()));
                }
                core.prepare_resume(
                    value
                        .take()
                        .expect("queued async generator resume must retain its value"),
                );
                *prepared = true;
            }
        }
        AsyncGeneratorOperation::Return { value, prepared } => {
            if !*prepared {
                let value = value
                    .take()
                    .expect("queued async generator return must retain its value");
                if !core.has_started() || !core.is_running() {
                    return Poll::Ready(Ok(core.force_return(value)));
                }
                core.prepare_command(GeneratorResume::Return(value));
                *prepared = true;
            }
        }
        AsyncGeneratorOperation::Throw { error, prepared } => {
            if !*prepared {
                let error = error
                    .take()
                    .expect("queued async generator throw must retain its error");
                if !core.has_started() || !core.is_running() {
                    core.close();
                    return Poll::Ready(Err(error));
                }
                core.prepare_command(GeneratorResume::Throw(error));
                *prepared = true;
            }
        }
    }
    match core.poll_step(context) {
        GeneratorPoll::Yielded(value) => Poll::Ready(Ok(IteratorResult::yielded(value))),
        GeneratorPoll::Completed => Poll::Ready(Ok(core.completed_result())),
        GeneratorPoll::Failed(error) => Poll::Ready(Err(error)),
        GeneratorPoll::Pending => Poll::Pending,
    }
}
