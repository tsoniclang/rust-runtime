use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use crate::{JsError, TsonicError, TsonicResult};

use super::{GeneratorController, GeneratorCore, GeneratorPoll, GeneratorResume, IteratorResult};

pub struct AsyncGeneratorImpl<'a, TYield, TReturn, TNext> {
    state: Rc<RefCell<AsyncGeneratorState<'a, TYield, TReturn, TNext>>>,
}

pub type OwnedAsyncGenerator<TYield, TReturn, TNext> =
    AsyncGeneratorImpl<'static, TYield, TReturn, TNext>;
pub type BorrowedAsyncGenerator<'a, TYield, TReturn, TNext> =
    AsyncGeneratorImpl<'a, TYield, TReturn, TNext>;

impl<'a, TYield, TReturn, TNext> AsyncGeneratorImpl<'a, TYield, TReturn, TNext> {
    pub fn new<TFactory, TFuture>(factory: TFactory) -> Self
    where
        TFactory: FnOnce(GeneratorController<TYield, TReturn, TNext>) -> TFuture,
        TFuture: Future<Output = TsonicResult<TReturn>> + 'a,
    {
        Self {
            state: Rc::new(RefCell::new(AsyncGeneratorState {
                core: GeneratorCore::new(factory),
                next_request_id: 0,
                queue: VecDeque::new(),
                results: BTreeMap::new(),
                wakers: BTreeMap::new(),
                abandoned: BTreeSet::new(),
            })),
        }
    }

    pub fn resume(&self) -> impl Future<Output = IteratorResult<TYield, TReturn>> + 'a
    where
        TYield: 'a,
        TReturn: 'a,
        TNext: Default + 'a,
    {
        infallible_async_generator_request(self.resume_result())
    }

    pub fn next_yield(&self) -> impl Future<Output = Option<TYield>> + 'a
    where
        TYield: 'a,
        TReturn: 'a,
        TNext: Default + 'a,
    {
        let request = self.resume();
        async move { request.await.into_yield() }
    }

    pub fn resume_with(
        &self,
        value: TNext,
    ) -> impl Future<Output = IteratorResult<TYield, TReturn>> + 'a
    where
        TYield: 'a,
        TReturn: 'a,
        TNext: 'a,
    {
        infallible_async_generator_request(self.resume_with_result(value))
    }

    pub fn return_value(
        &self,
        value: TReturn,
    ) -> impl Future<Output = IteratorResult<TYield, TReturn>> + 'a
    where
        TYield: 'a,
        TReturn: 'a,
        TNext: 'a,
    {
        infallible_async_generator_request(self.return_value_result(value))
    }

    pub fn throw_value(
        &self,
        error: JsError,
    ) -> impl Future<Output = TsonicResult<IteratorResult<TYield, TReturn>>> + 'a
    where
        TYield: 'a,
        TReturn: 'a,
        TNext: 'a,
    {
        self.throw_error(error.into())
    }

    pub(super) fn resume_result(&self) -> AsyncGeneratorRequest<'a, TYield, TReturn, TNext>
    where
        TNext: Default,
    {
        self.enqueue(AsyncGeneratorOperation::Resume {
            value: Some(TNext::default()),
            prepared: false,
        })
    }

    pub(super) fn resume_with_result(
        &self,
        value: TNext,
    ) -> AsyncGeneratorRequest<'a, TYield, TReturn, TNext> {
        self.enqueue(AsyncGeneratorOperation::Resume {
            value: Some(value),
            prepared: false,
        })
    }

    pub(super) fn return_value_result(
        &self,
        value: TReturn,
    ) -> AsyncGeneratorRequest<'a, TYield, TReturn, TNext> {
        self.enqueue(AsyncGeneratorOperation::Return {
            value: Some(value),
            prepared: false,
        })
    }

    pub(super) fn throw_error(
        &self,
        error: TsonicError,
    ) -> AsyncGeneratorRequest<'a, TYield, TReturn, TNext> {
        self.enqueue(AsyncGeneratorOperation::Throw {
            error: Some(error),
            prepared: false,
        })
    }

    fn enqueue(
        &self,
        operation: AsyncGeneratorOperation<TReturn, TNext>,
    ) -> AsyncGeneratorRequest<'a, TYield, TReturn, TNext> {
        let mut state = self.state.borrow_mut();
        let id = state.next_request_id;
        state.next_request_id = state
            .next_request_id
            .checked_add(1)
            .expect("async generator request identity exhausted");
        state
            .queue
            .push_back(QueuedAsyncGeneratorOperation { id, operation });
        AsyncGeneratorRequest {
            state: Rc::clone(&self.state),
            id,
        }
    }
}

struct AsyncGeneratorState<'a, TYield, TReturn, TNext> {
    core: GeneratorCore<'a, TYield, TReturn, TNext>,
    next_request_id: u64,
    queue: VecDeque<QueuedAsyncGeneratorOperation<TReturn, TNext>>,
    results: BTreeMap<u64, TsonicResult<IteratorResult<TYield, TReturn>>>,
    wakers: BTreeMap<u64, Waker>,
    abandoned: BTreeSet<u64>,
}

struct QueuedAsyncGeneratorOperation<TReturn, TNext> {
    id: u64,
    operation: AsyncGeneratorOperation<TReturn, TNext>,
}

enum AsyncGeneratorOperation<TReturn, TNext> {
    Resume {
        value: Option<TNext>,
        prepared: bool,
    },
    Return {
        value: Option<TReturn>,
        prepared: bool,
    },
    Throw {
        error: Option<TsonicError>,
        prepared: bool,
    },
}

pub(super) struct AsyncGeneratorRequest<'a, TYield, TReturn, TNext> {
    state: Rc<RefCell<AsyncGeneratorState<'a, TYield, TReturn, TNext>>>,
    id: u64,
}

impl<'a, TYield, TReturn, TNext> Future for AsyncGeneratorRequest<'a, TYield, TReturn, TNext> {
    type Output = TsonicResult<IteratorResult<TYield, TReturn>>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let request_id = self.id;
        loop {
            let mut wake = Vec::new();
            let step = {
                let mut state = self.state.borrow_mut();
                if let Some(result) = state.results.remove(&request_id) {
                    return Poll::Ready(result);
                }
                state.wakers.insert(request_id, context.waker().clone());
                let Some(mut queued) = state.queue.pop_front() else {
                    panic!("async generator request is neither queued nor completed");
                };
                let result =
                    poll_async_generator_operation(&mut queued.operation, &mut state.core, context);
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
                AsyncGeneratorRequestStep::Ready(result) => return Poll::Ready(result),
                AsyncGeneratorRequestStep::Advanced => {}
            }
        }
    }
}

impl<'a, TYield, TReturn, TNext> Drop for AsyncGeneratorRequest<'a, TYield, TReturn, TNext> {
    fn drop(&mut self) {
        let mut state = self.state.borrow_mut();
        state.wakers.remove(&self.id);
        if state.results.remove(&self.id).is_none()
            && state.queue.iter().any(|request| request.id == self.id)
        {
            state.abandoned.insert(self.id);
        }
    }
}

enum AsyncGeneratorRequestStep<TYield, TReturn> {
    Pending,
    Ready(TsonicResult<IteratorResult<TYield, TReturn>>),
    Advanced,
}

fn poll_async_generator_operation<TYield, TReturn, TNext>(
    operation: &mut AsyncGeneratorOperation<TReturn, TNext>,
    core: &mut GeneratorCore<'_, TYield, TReturn, TNext>,
    context: &mut Context<'_>,
) -> Poll<TsonicResult<IteratorResult<TYield, TReturn>>> {
    match operation {
        AsyncGeneratorOperation::Resume { value, prepared } => {
            if !*prepared {
                if !core.is_running() {
                    return Poll::Ready(Ok(core.take_completed_result()));
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
        GeneratorPoll::Completed => Poll::Ready(Ok(core.take_completed_result())),
        GeneratorPoll::Failed(error) => Poll::Ready(Err(error)),
        GeneratorPoll::Pending => Poll::Pending,
    }
}

async fn infallible_async_generator_request<TYield, TReturn, TNext>(
    request: AsyncGeneratorRequest<'_, TYield, TReturn, TNext>,
) -> IteratorResult<TYield, TReturn> {
    match request.await {
        Ok(result) => result,
        Err(_) => panic!("an infallible async generator operation produced an error"),
    }
}
