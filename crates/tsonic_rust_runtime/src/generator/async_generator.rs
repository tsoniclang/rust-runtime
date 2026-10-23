use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::rc::Rc;
use alloc::vec::Vec;
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
            let selected = {
                let mut state = self.state.borrow_mut();
                if let Some(result) = state.results.remove(&request_id) {
                    self.completed.set(true);
                    return Poll::Ready(result);
                }
                if state.core.is_none() {
                    state.queue.retain(|queued| queued.id != request_id);
                    state.wakers.remove(&request_id);
                    None
                } else {
                    state.wakers.insert(request_id, context.waker().clone());
                    let queued = state
                        .queue
                        .pop_front()
                        .expect("async generator request is neither queued nor completed");
                    let core = state
                        .core
                        .take()
                        .expect("an idle async generator owns its core");
                    Some((queued, core))
                }
            };
            let Some((mut queued, mut core)) = selected else {
                self.completed.set(true);
                return Poll::Ready(Err(Reentry::on_reentry()));
            };
            let result = poll_async_generator_operation(&mut queued.operation, &mut core, context);
            let mut wake = Vec::new();
            let step = {
                let mut state = self.state.borrow_mut();
                state.core = Some(core);
                match result {
                    Poll::Pending => {
                        state.queue.push_front(queued);
                        AsyncGeneratorRequestStep::Pending
                    }
                    Poll::Ready(result) => {
                        if let Some(waker) = state.wakers.remove(&queued.id) {
                            wake.push(waker);
                        }
                        if let Some(next) = state.queue.front() {
                            if let Some(waker) = state.wakers.get(&next.id) {
                                wake.push(waker.clone());
                            }
                        }
                        if queued.id == request_id {
                            AsyncGeneratorRequestStep::Ready(result)
                        } else {
                            if !state.abandoned.remove(&queued.id) {
                                state.results.insert(queued.id, result);
                            }
                            AsyncGeneratorRequestStep::Advanced
                        }
                    }
                }
            };
            for waker in wake {
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
        let mut state = self.state.borrow_mut();
        state.wakers.remove(&self.id);
        if state.results.remove(&self.id).is_none() {
            state.abandoned.insert(self.id);
        }
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
