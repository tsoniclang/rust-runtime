use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use crate::{JsError, TsonicError, TsonicResult};

mod async_generator;

pub use async_generator::{AsyncGenerator, AsyncGeneratorImpl, BorrowedAsyncGenerator};

pub enum IteratorValue<TYield, TReturn> {
    Yield(TYield),
    Return(TReturn),
    Closed,
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

    pub fn closed() -> Self {
        Self {
            value: IteratorValue::Closed,
        }
    }

    pub fn done(&self) -> bool {
        !matches!(self.value, IteratorValue::Yield(_))
    }

    pub fn into_yield(self) -> Option<TYield> {
        match self.value {
            IteratorValue::Yield(value) => Some(value),
            IteratorValue::Return(_) | IteratorValue::Closed => None,
        }
    }

    pub fn into_return(self) -> Option<TReturn> {
        match self.value {
            IteratorValue::Return(value) => Some(value),
            IteratorValue::Yield(_) | IteratorValue::Closed => None,
        }
    }

    pub fn yield_value(&self) -> TYield
    where
        TYield: Clone,
    {
        match &self.value {
            IteratorValue::Yield(value) => value.clone(),
            IteratorValue::Return(_) | IteratorValue::Closed => {
                panic!("completed iterator result has no yield value")
            }
        }
    }

    pub fn completed_value(&self) -> TReturn
    where
        TReturn: Clone,
    {
        match &self.value {
            IteratorValue::Return(value) => value.clone(),
            IteratorValue::Yield(_) | IteratorValue::Closed => {
                panic!("iterator result has no completed return value")
            }
        }
    }
}

impl<T: Clone> IteratorResult<T, T> {
    pub fn value(&self) -> T {
        match &self.value {
            IteratorValue::Yield(value) | IteratorValue::Return(value) => value.clone(),
            IteratorValue::Closed => panic!("closed iterator result has no value"),
        }
    }
}

pub enum GeneratorResume<TNext, TReturn> {
    Next(TNext),
    Return(TReturn),
    Throw(TsonicError),
}

enum ResumeSlot<TNext, TReturn> {
    Waiting,
    Ready(GeneratorResume<TNext, TReturn>),
}

struct SharedState<TYield, TReturn, TNext> {
    yielded: Option<TYield>,
    resume: ResumeSlot<TNext, TReturn>,
}

pub struct GeneratorController<TYield, TReturn, TNext> {
    shared: Rc<RefCell<SharedState<TYield, TReturn, TNext>>>,
}

impl<TYield, TReturn, TNext> Clone for GeneratorController<TYield, TReturn, TNext> {
    fn clone(&self) -> Self {
        Self {
            shared: Rc::clone(&self.shared),
        }
    }
}

impl<TYield, TReturn, TNext> GeneratorController<TYield, TReturn, TNext> {
    pub fn yield_value(&self, value: TYield) -> YieldPoint<TYield, TReturn, TNext> {
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

    pub async fn yield_from(
        &self,
        mut generator: Generator<TYield, TReturn, TNext>,
    ) -> GeneratorResume<TReturn, TReturn>
    where
        TNext: Default,
        TReturn: Clone,
    {
        let mut returning = false;
        let mut result = generator.resume_result();
        loop {
            match result {
                Err(error) => return GeneratorResume::Throw(error),
                Ok(iterator) => match iterator.value {
                    IteratorValue::Yield(value) => {
                        result = match self.yield_value(value).await {
                            GeneratorResume::Next(next) => generator.resume_with_result(next),
                            GeneratorResume::Return(value) => {
                                returning = true;
                                generator.return_value_result(value)
                            }
                            GeneratorResume::Throw(error) => {
                                returning = false;
                                generator.throw_error(error)
                            }
                        };
                    }
                    IteratorValue::Return(value) => {
                        return if returning {
                            GeneratorResume::Return(value)
                        } else {
                            GeneratorResume::Next(value)
                        };
                    }
                    IteratorValue::Closed => {
                        return GeneratorResume::Throw(TsonicError::unsupported(
                            "a delegated generator closed without a return value",
                        ));
                    }
                },
            }
        }
    }

    pub async fn yield_from_async(
        &self,
        generator: AsyncGenerator<TYield, TReturn, TNext>,
    ) -> GeneratorResume<TReturn, TReturn>
    where
        TYield: 'static,
        TNext: Default + 'static,
        TReturn: Clone + 'static,
    {
        let mut returning = false;
        let mut result = generator.resume_result().await;
        loop {
            match result {
                Err(error) => return GeneratorResume::Throw(error),
                Ok(iterator) => match iterator.value {
                    IteratorValue::Yield(value) => {
                        result = match self.yield_value(value).await {
                            GeneratorResume::Next(next) => generator.resume_with_result(next).await,
                            GeneratorResume::Return(value) => {
                                returning = true;
                                generator.return_value_result(value).await
                            }
                            GeneratorResume::Throw(error) => {
                                returning = false;
                                generator.throw_error(error).await
                            }
                        };
                    }
                    IteratorValue::Return(value) => {
                        return if returning {
                            GeneratorResume::Return(value)
                        } else {
                            GeneratorResume::Next(value)
                        };
                    }
                    IteratorValue::Closed => {
                        return GeneratorResume::Throw(TsonicError::unsupported(
                            "a delegated async generator closed without a return value",
                        ));
                    }
                },
            }
        }
    }
}

pub struct YieldPoint<TYield, TReturn, TNext> {
    shared: Rc<RefCell<SharedState<TYield, TReturn, TNext>>>,
    suspended: bool,
}

impl<TYield, TReturn, TNext> Future for YieldPoint<TYield, TReturn, TNext> {
    type Output = GeneratorResume<TNext, TReturn>;

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

type GeneratorFuture<'a, TReturn> = Pin<Box<dyn Future<Output = TsonicResult<TReturn>> + 'a>>;

pub(super) struct GeneratorCore<'a, TYield, TReturn, TNext> {
    future: Option<GeneratorFuture<'a, TReturn>>,
    shared: Rc<RefCell<SharedState<TYield, TReturn, TNext>>>,
    completed: Option<TReturn>,
    started: bool,
}

impl<'a, TYield, TReturn, TNext> GeneratorCore<'a, TYield, TReturn, TNext> {
    pub(super) fn new<TFactory, TFuture>(factory: TFactory) -> Self
    where
        TFactory: FnOnce(GeneratorController<TYield, TReturn, TNext>) -> TFuture,
        TFuture: Future<Output = TsonicResult<TReturn>> + 'a,
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

    pub(super) fn is_running(&self) -> bool {
        self.future.is_some()
    }

    pub(super) fn has_started(&self) -> bool {
        self.started
    }

    pub(super) fn prepare_resume(&mut self, value: TNext) {
        if self.started {
            self.prepare_command(GeneratorResume::Next(value));
        } else {
            self.started = true;
        }
    }

    pub(super) fn prepare_command(&mut self, command: GeneratorResume<TNext, TReturn>) {
        assert!(
            self.started,
            "only a started generator can receive an injected command"
        );
        let mut shared = self.shared.borrow_mut();
        assert!(
            matches!(shared.resume, ResumeSlot::Waiting),
            "a generator cannot receive a second command before polling"
        );
        shared.resume = ResumeSlot::Ready(command);
    }

    pub(super) fn poll_step(&mut self, context: &mut Context<'_>) -> GeneratorPoll<TYield> {
        let Some(future) = self.future.as_mut() else {
            return GeneratorPoll::Completed;
        };
        match future.as_mut().poll(context) {
            Poll::Ready(Ok(value)) => {
                self.future = None;
                self.completed = Some(value);
                GeneratorPoll::Completed
            }
            Poll::Ready(Err(error)) => {
                self.future = None;
                self.completed = None;
                GeneratorPoll::Failed(error)
            }
            Poll::Pending => match self.shared.borrow_mut().yielded.take() {
                Some(value) => GeneratorPoll::Yielded(value),
                None => GeneratorPoll::Pending,
            },
        }
    }

    pub(super) fn completed_result(&self) -> IteratorResult<TYield, TReturn>
    where
        TReturn: Clone,
    {
        self.completed
            .as_ref()
            .map(|value| IteratorResult::completed(value.clone()))
            .unwrap_or_else(IteratorResult::closed)
    }

    pub(super) fn force_return(&mut self, value: TReturn) -> IteratorResult<TYield, TReturn>
    where
        TReturn: Clone,
    {
        self.future = None;
        self.completed = Some(value);
        self.started = true;
        self.completed_result()
    }

    pub(super) fn close(&mut self) {
        self.future = None;
        self.completed = None;
        self.started = true;
    }
}

pub(super) enum GeneratorPoll<TYield> {
    Yielded(TYield),
    Completed,
    Failed(TsonicError),
    Pending,
}

pub struct GeneratorImpl<'a, TYield, TReturn, TNext> {
    core: GeneratorCore<'a, TYield, TReturn, TNext>,
}

pub type Generator<TYield, TReturn, TNext> = GeneratorImpl<'static, TYield, TReturn, TNext>;
pub type BorrowedGenerator<'a, TYield, TReturn, TNext> = GeneratorImpl<'a, TYield, TReturn, TNext>;

impl<'a, TYield, TReturn, TNext> GeneratorImpl<'a, TYield, TReturn, TNext> {
    pub fn new<TFactory, TFuture>(factory: TFactory) -> Self
    where
        TFactory: FnOnce(GeneratorController<TYield, TReturn, TNext>) -> TFuture,
        TFuture: Future<Output = TsonicResult<TReturn>> + 'a,
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
        infallible_generator_result(self.resume_result())
    }

    pub fn resume_with(&mut self, value: TNext) -> IteratorResult<TYield, TReturn>
    where
        TReturn: Clone,
    {
        infallible_generator_result(self.resume_with_result(value))
    }

    pub fn return_value(&mut self, value: TReturn) -> IteratorResult<TYield, TReturn>
    where
        TReturn: Clone,
    {
        infallible_generator_result(self.return_value_result(value))
    }

    pub fn throw_value(&mut self, error: JsError) -> TsonicResult<IteratorResult<TYield, TReturn>>
    where
        TReturn: Clone,
    {
        self.throw_error(error.into())
    }

    fn resume_result(&mut self) -> TsonicResult<IteratorResult<TYield, TReturn>>
    where
        TNext: Default,
        TReturn: Clone,
    {
        self.resume_with_result(TNext::default())
    }

    fn resume_with_result(&mut self, value: TNext) -> TsonicResult<IteratorResult<TYield, TReturn>>
    where
        TReturn: Clone,
    {
        if !self.core.is_running() {
            return Ok(self.core.completed_result());
        }
        self.core.prepare_resume(value);
        self.poll_sync_boundary()
    }

    fn return_value_result(
        &mut self,
        value: TReturn,
    ) -> TsonicResult<IteratorResult<TYield, TReturn>>
    where
        TReturn: Clone,
    {
        if !self.core.has_started() || !self.core.is_running() {
            return Ok(self.core.force_return(value));
        }
        self.core.prepare_command(GeneratorResume::Return(value));
        self.poll_sync_boundary()
    }

    fn throw_error(&mut self, error: TsonicError) -> TsonicResult<IteratorResult<TYield, TReturn>>
    where
        TReturn: Clone,
    {
        if !self.core.has_started() || !self.core.is_running() {
            self.core.close();
            return Err(error);
        }
        self.core.prepare_command(GeneratorResume::Throw(error));
        self.poll_sync_boundary()
    }

    fn poll_sync_boundary(&mut self) -> TsonicResult<IteratorResult<TYield, TReturn>>
    where
        TReturn: Clone,
    {
        let mut context = Context::from_waker(Waker::noop());
        match self.core.poll_step(&mut context) {
            GeneratorPoll::Yielded(value) => Ok(IteratorResult::yielded(value)),
            GeneratorPoll::Completed => Ok(self.core.completed_result()),
            GeneratorPoll::Failed(error) => Err(error),
            GeneratorPoll::Pending => {
                panic!("a synchronous generator suspended on a non-yield future")
            }
        }
    }
}

impl<'a, TYield, TReturn, TNext> Iterator for GeneratorImpl<'a, TYield, TReturn, TNext>
where
    TNext: Default,
    TReturn: Clone,
{
    type Item = TYield;

    fn next(&mut self) -> Option<Self::Item> {
        self.resume().into_yield()
    }
}

fn infallible_generator_result<TYield, TReturn>(
    result: TsonicResult<IteratorResult<TYield, TReturn>>,
) -> IteratorResult<TYield, TReturn> {
    match result {
        Ok(result) => result,
        Err(_) => panic!("an infallible generator operation produced an error"),
    }
}
