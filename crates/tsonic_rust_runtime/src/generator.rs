use std::cell::RefCell;
use std::future::{poll_fn, Future};
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
        mut generator: AsyncGenerator<TYield, TReturn, TNext>,
    ) -> TReturn
    where
        TNext: Default,
        TReturn: Clone,
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
    core: GeneratorCore<TYield, TReturn, TNext>,
}

impl<TYield, TReturn, TNext> AsyncGenerator<TYield, TReturn, TNext> {
    pub fn new<TFactory, TFuture>(factory: TFactory) -> Self
    where
        TFactory: FnOnce(GeneratorController<TYield, TNext>) -> TFuture,
        TFuture: Future<Output = TReturn> + 'static,
    {
        Self {
            core: GeneratorCore::new(factory),
        }
    }

    pub async fn resume(&mut self) -> IteratorResult<TYield, TReturn>
    where
        TNext: Default,
        TReturn: Clone,
    {
        self.resume_with(TNext::default()).await
    }

    pub async fn resume_with(&mut self, value: TNext) -> IteratorResult<TYield, TReturn>
    where
        TReturn: Clone,
    {
        if self.core.future.is_none() {
            return self.core.completed_result();
        }
        self.core.prepare_resume(value);
        poll_fn(|context| match self.core.poll_step(context) {
            GeneratorPoll::Yielded(value) => Poll::Ready(IteratorResult::yielded(value)),
            GeneratorPoll::Completed => Poll::Ready(self.core.completed_result()),
            GeneratorPoll::Pending => Poll::Pending,
        })
        .await
    }

    pub async fn return_value(&mut self, value: TReturn) -> IteratorResult<TYield, TReturn>
    where
        TReturn: Clone,
    {
        self.core.return_value(value)
    }

    pub async fn throw_value(
        &mut self,
        error: JsError,
    ) -> TsonicResult<IteratorResult<TYield, TReturn>> {
        self.core.future = None;
        Err(error.into())
    }
}

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}
