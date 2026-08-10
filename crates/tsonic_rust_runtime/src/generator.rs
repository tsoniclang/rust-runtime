use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use crate::{JsError, TsonicResult};

pub enum IteratorValue<TYield, TReturn> {
    Yield(TYield),
    Return(TReturn),
}

pub struct IteratorResult<TYield, TReturn> {
    value: IteratorValue<TYield, TReturn>,
}

impl<TYield, TReturn> IteratorResult<TYield, TReturn> {
    pub fn yielded(value: TYield) -> Self {
        Self {
            value: IteratorValue::Yield(value),
        }
    }

    pub fn completed(value: TReturn) -> Self {
        Self {
            value: IteratorValue::Return(value),
        }
    }

    pub fn done(&self) -> bool {
        matches!(self.value, IteratorValue::Return(_))
    }

    pub fn into_yield(self) -> Option<TYield> {
        match self.value {
            IteratorValue::Yield(value) => Some(value),
            IteratorValue::Return(_) => None,
        }
    }

    pub fn into_return(self) -> Option<TReturn> {
        match self.value {
            IteratorValue::Yield(_) => None,
            IteratorValue::Return(value) => Some(value),
        }
    }

    pub fn yield_value(&self) -> TYield
    where
        TYield: Clone,
    {
        match &self.value {
            IteratorValue::Yield(value) => value.clone(),
            IteratorValue::Return(_) => panic!("completed iterator result has no yield value"),
        }
    }

    pub fn completed_value(&self) -> TReturn
    where
        TReturn: Clone,
    {
        match &self.value {
            IteratorValue::Yield(_) => panic!("yielded iterator result has no return value"),
            IteratorValue::Return(value) => value.clone(),
        }
    }
}

impl<T: Clone> IteratorResult<T, T> {
    pub fn value(&self) -> T {
        match &self.value {
            IteratorValue::Yield(value) | IteratorValue::Return(value) => value.clone(),
        }
    }
}

enum ResumeSlot<TNext> {
    Waiting,
    Ready(TNext),
}

struct SharedState<TYield, TNext> {
    yielded: Option<TYield>,
    resume: ResumeSlot<TNext>,
}

pub struct GeneratorController<TYield, TNext> {
    shared: Rc<RefCell<SharedState<TYield, TNext>>>,
}

impl<TYield, TNext> Clone for GeneratorController<TYield, TNext> {
    fn clone(&self) -> Self {
        Self {
            shared: Rc::clone(&self.shared),
        }
    }
}

impl<TYield, TNext> GeneratorController<TYield, TNext> {
    pub fn yield_value(&self, value: TYield) -> YieldPoint<TYield, TNext> {
        let mut shared = self.shared.borrow_mut();
        assert!(
            shared.yielded.is_none(),
            "a generator cannot publish a second value before resumption"
        );
        shared.yielded = Some(value);
        drop(shared);
        YieldPoint {
            shared: Rc::clone(&self.shared),
            suspended: false,
        }
    }

    pub async fn yield_from<TReturn>(
        &self,
        mut generator: Generator<TYield, TReturn, TNext>,
    ) -> TReturn
    where
        TNext: Default,
        TReturn: Clone,
    {
        let mut result = generator.resume();
        loop {
            match result.value {
                IteratorValue::Yield(value) => {
                    result = generator.resume_with(self.yield_value(value).await);
                }
                IteratorValue::Return(value) => return value,
            }
        }
    }

    pub async fn yield_from_async<TReturn>(
        &self,
        generator: AsyncGenerator<TYield, TReturn, TNext>,
    ) -> TReturn
    where
        TYield: 'static,
        TNext: 'static,
        TReturn: Clone + 'static,
    {
        let mut result = generator.resume().await;
        loop {
            match result.value {
                IteratorValue::Yield(value) => {
                    result = generator.resume_with(self.yield_value(value).await).await;
                }
                IteratorValue::Return(value) => return value,
            }
        }
    }
}

pub struct YieldPoint<TYield, TNext> {
    shared: Rc<RefCell<SharedState<TYield, TNext>>>,
    suspended: bool,
}

impl<TYield, TNext> Future for YieldPoint<TYield, TNext> {
    type Output = TNext;

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.suspended {
            self.suspended = true;
            return Poll::Pending;
        }
        let mut shared = self.shared.borrow_mut();
        match std::mem::replace(&mut shared.resume, ResumeSlot::Waiting) {
            ResumeSlot::Waiting => Poll::Pending,
            ResumeSlot::Ready(value) => Poll::Ready(value),
        }
    }
}

type GeneratorFuture<TReturn> = Pin<Box<dyn Future<Output = TReturn>>>;

struct GeneratorCore<TYield, TReturn, TNext> {
    future: Option<GeneratorFuture<TReturn>>,
    shared: Rc<RefCell<SharedState<TYield, TNext>>>,
    completed: Option<TReturn>,
    started: bool,
}

impl<TYield, TReturn, TNext> GeneratorCore<TYield, TReturn, TNext> {
    fn new<TFactory, TFuture>(factory: TFactory) -> Self
    where
        TFactory: FnOnce(GeneratorController<TYield, TNext>) -> TFuture,
        TFuture: Future<Output = TReturn> + 'static,
    {
        let shared = Rc::new(RefCell::new(SharedState {
            yielded: None,
            resume: ResumeSlot::Waiting,
        }));
        let controller = GeneratorController {
            shared: Rc::clone(&shared),
        };
        Self {
            future: Some(Box::pin(factory(controller))),
            shared,
            completed: None,
            started: false,
        }
    }

    fn prepare_resume(&mut self, value: TNext) {
        let mut shared = self.shared.borrow_mut();
        shared.resume = if self.started {
            ResumeSlot::Ready(value)
        } else {
            ResumeSlot::Waiting
        };
        self.started = true;
    }

    fn prepare_resume_without_value(&mut self) {
        self.shared.borrow_mut().resume = ResumeSlot::Waiting;
        self.started = true;
    }

    fn poll_step(&mut self, context: &mut Context<'_>) -> GeneratorPoll<TYield> {
        let Some(future) = self.future.as_mut() else {
            return GeneratorPoll::Completed;
        };
        match future.as_mut().poll(context) {
            Poll::Ready(value) => {
                self.future = None;
                self.completed = Some(value);
                GeneratorPoll::Completed
            }
            Poll::Pending => match self.shared.borrow_mut().yielded.take() {
                Some(value) => GeneratorPoll::Yielded(value),
                None => GeneratorPoll::Pending,
            },
        }
    }

    fn completed_result(&self) -> IteratorResult<TYield, TReturn>
    where
        TReturn: Clone,
    {
        IteratorResult::completed(
            self.completed
                .as_ref()
                .expect("completed generator must retain its return value")
                .clone(),
        )
    }

    fn return_value(&mut self, value: TReturn) -> IteratorResult<TYield, TReturn>
    where
        TReturn: Clone,
    {
        self.future = None;
        self.completed = Some(value);
        self.completed_result()
    }
}

enum GeneratorPoll<TYield> {
    Yielded(TYield),
    Completed,
    Pending,
}

pub struct Generator<TYield, TReturn, TNext> {
    core: GeneratorCore<TYield, TReturn, TNext>,
}

impl<TYield, TReturn, TNext> Generator<TYield, TReturn, TNext> {
    pub fn new<TFactory, TFuture>(factory: TFactory) -> Self
    where
        TFactory: FnOnce(GeneratorController<TYield, TNext>) -> TFuture,
        TFuture: Future<Output = TReturn> + 'static,
    {
        Self {
            core: GeneratorCore::new(factory),
        }
    }

    pub fn resume(&mut self) -> IteratorResult<TYield, TReturn>
    where
        TNext: Default,
        TReturn: Clone,
    {
        self.resume_with(TNext::default())
    }

    pub fn resume_with(&mut self, value: TNext) -> IteratorResult<TYield, TReturn>
    where
        TReturn: Clone,
    {
        if self.core.future.is_none() {
            return self.core.completed_result();
        }
        self.core.prepare_resume(value);
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        match self.core.poll_step(&mut context) {
            GeneratorPoll::Yielded(value) => IteratorResult::yielded(value),
            GeneratorPoll::Completed => self.core.completed_result(),
            GeneratorPoll::Pending => {
                panic!("a synchronous generator suspended on a non-yield future")
            }
        }
    }

    pub fn return_value(&mut self, value: TReturn) -> IteratorResult<TYield, TReturn>
    where
        TReturn: Clone,
    {
        self.core.return_value(value)
    }

    pub fn throw_value(&mut self, error: JsError) -> TsonicResult<IteratorResult<TYield, TReturn>> {
        self.core.future = None;
        Err(error.into())
    }
}

impl<TYield, TReturn, TNext> Iterator for Generator<TYield, TReturn, TNext>
where
    TNext: Default,
    TReturn: Clone,
{
    type Item = TYield;

    fn next(&mut self) -> Option<Self::Item> {
        self.resume().into_yield()
    }
}

pub struct AsyncGenerator<TYield, TReturn, TNext> {
    state: Rc<RefCell<AsyncGeneratorState<TYield, TReturn, TNext>>>,
}

impl<TYield, TReturn, TNext> AsyncGenerator<TYield, TReturn, TNext> {
    pub fn new<TFactory, TFuture>(factory: TFactory) -> Self
    where
        TFactory: FnOnce(GeneratorController<TYield, TNext>) -> TFuture,
        TFuture: Future<Output = TReturn> + 'static,
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

    pub fn resume(&self) -> impl Future<Output = IteratorResult<TYield, TReturn>> + 'static
    where
        TYield: 'static,
        TReturn: Clone + 'static,
        TNext: 'static,
    {
        infallible_async_generator_request(self.enqueue(AsyncGeneratorOperation::Resume {
            value: None,
            has_value: false,
            prepared: false,
        }))
    }

    pub fn resume_with(
        &self,
        value: TNext,
    ) -> impl Future<Output = IteratorResult<TYield, TReturn>> + 'static
    where
        TYield: 'static,
        TReturn: Clone + 'static,
        TNext: 'static,
    {
        infallible_async_generator_request(self.enqueue(AsyncGeneratorOperation::Resume {
            value: Some(value),
            has_value: true,
            prepared: false,
        }))
    }

    pub fn return_value(
        &self,
        value: TReturn,
    ) -> impl Future<Output = IteratorResult<TYield, TReturn>> + 'static
    where
        TYield: 'static,
        TReturn: Clone + 'static,
        TNext: 'static,
    {
        infallible_async_generator_request(
            self.enqueue(AsyncGeneratorOperation::Return { value: Some(value) }),
        )
    }

    pub fn throw_value(
        &self,
        error: JsError,
    ) -> impl Future<Output = TsonicResult<IteratorResult<TYield, TReturn>>> + 'static
    where
        TYield: 'static,
        TReturn: Clone + 'static,
        TNext: 'static,
    {
        self.enqueue(AsyncGeneratorOperation::Throw { error: Some(error) })
    }

    fn enqueue(
        &self,
        operation: AsyncGeneratorOperation<TReturn, TNext>,
    ) -> AsyncGeneratorRequest<TYield, TReturn, TNext> {
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

struct AsyncGeneratorState<TYield, TReturn, TNext> {
    core: GeneratorCore<TYield, TReturn, TNext>,
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
        has_value: bool,
        prepared: bool,
    },
    Return {
        value: Option<TReturn>,
    },
    Throw {
        error: Option<JsError>,
    },
}

struct AsyncGeneratorRequest<TYield, TReturn, TNext> {
    state: Rc<RefCell<AsyncGeneratorState<TYield, TReturn, TNext>>>,
    id: u64,
}

impl<TYield, TReturn, TNext> Future for AsyncGeneratorRequest<TYield, TReturn, TNext>
where
    TReturn: Clone,
{
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

impl<TYield, TReturn, TNext> Drop for AsyncGeneratorRequest<TYield, TReturn, TNext> {
    fn drop(&mut self) {
        let mut state = self.state.borrow_mut();
        state.wakers.remove(&self.id);
        if state.results.remove(&self.id).is_none() {
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
    core: &mut GeneratorCore<TYield, TReturn, TNext>,
    context: &mut Context<'_>,
) -> Poll<TsonicResult<IteratorResult<TYield, TReturn>>>
where
    TReturn: Clone,
{
    match operation {
        AsyncGeneratorOperation::Resume {
            value,
            has_value,
            prepared,
        } => {
            if !*prepared {
                if core.future.is_none() {
                    return Poll::Ready(Ok(core.completed_result()));
                }
                if *has_value {
                    core.prepare_resume(
                        value
                            .take()
                            .expect("queued async generator resume must retain its value"),
                    );
                } else {
                    core.prepare_resume_without_value();
                }
                *prepared = true;
            }
            match core.poll_step(context) {
                GeneratorPoll::Yielded(value) => Poll::Ready(Ok(IteratorResult::yielded(value))),
                GeneratorPoll::Completed => Poll::Ready(Ok(core.completed_result())),
                GeneratorPoll::Pending => Poll::Pending,
            }
        }
        AsyncGeneratorOperation::Return { value } => Poll::Ready(Ok(core.return_value(
            value
                .take()
                .expect("queued async generator return must retain its value"),
        ))),
        AsyncGeneratorOperation::Throw { error } => {
            core.future = None;
            Poll::Ready(Err(error
                .take()
                .expect("queued async generator throw must retain its error")
                .into()))
        }
    }
}

fn infallible_async_generator_request<TYield, TReturn, TNext>(
    request: AsyncGeneratorRequest<TYield, TReturn, TNext>,
) -> impl Future<Output = IteratorResult<TYield, TReturn>>
where
    TReturn: Clone,
{
    async move {
        match request.await {
            Ok(result) => result,
            Err(_) => panic!("an infallible async generator request produced an error"),
        }
    }
}

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}
