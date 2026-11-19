use alloc::boxed::Box;
use alloc::rc::Rc;
use core::cell::RefCell;
use core::future::Future;
use core::mem;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

use crate::TsonicError;

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

pub enum GeneratorResume<TNext, TReturn, TError> {
    Next(TNext),
    Return(TReturn),
    Throw(TError),
}

enum ResumeSlot<TNext, TReturn, TError> {
    Waiting,
    Ready(GeneratorResume<TNext, TReturn, TError>),
}

struct SharedState<TYield, TReturn, TNext, TError> {
    yielded: Option<TYield>,
    resume: ResumeSlot<TNext, TReturn, TError>,
}

pub struct GeneratorController<TYield, TReturn, TNext, TError> {
    shared: Rc<RefCell<SharedState<TYield, TReturn, TNext, TError>>>,
}

impl<TYield, TReturn, TNext, TError> Clone for GeneratorController<TYield, TReturn, TNext, TError> {
    fn clone(&self) -> Self {
        Self {
            shared: Rc::clone(&self.shared),
        }
    }
}

impl<TYield, TReturn, TNext, TError> GeneratorController<TYield, TReturn, TNext, TError> {
    pub fn yield_value(&self, value: TYield) -> YieldPoint<TYield, TReturn, TNext, TError> {
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
        generator: Generator<TYield, TReturn, TNext, TError>,
    ) -> GeneratorResume<TReturn, TReturn, TError>
    where
        TNext: Default,
        TReturn: Clone,
        TError: From<TsonicError>,
    {
        let mut returning = false;
        let mut result = generator.resume();
        loop {
            match result {
                Err(error) => return GeneratorResume::Throw(error),
                Ok(iterator) => match iterator.value {
                    IteratorValue::Yield(value) => {
                        result = match self.yield_value(value).await {
                            GeneratorResume::Next(next) => generator.resume_with(next),
                            GeneratorResume::Return(value) => {
                                returning = true;
                                generator.return_value(value)
                            }
                            GeneratorResume::Throw(error) => {
                                returning = false;
                                generator.throw_value(error)
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
                        return GeneratorResume::Throw(TError::from(TsonicError::unsupported(
                            "a delegated generator closed without a return value",
                        )));
                    }
                },
            }
        }
    }

    pub async fn yield_from_async(
        &self,
        generator: AsyncGenerator<TYield, TReturn, TNext, TError>,
    ) -> GeneratorResume<TReturn, TReturn, TError>
    where
        TYield: 'static,
        TNext: Default + 'static,
        TReturn: Clone + 'static,
        TError: From<TsonicError> + 'static,
    {
        let mut returning = false;
        let mut result = generator.resume().await;
        loop {
            match result {
                Err(error) => return GeneratorResume::Throw(error),
                Ok(iterator) => match iterator.value {
                    IteratorValue::Yield(value) => {
                        result = match self.yield_value(value).await {
                            GeneratorResume::Next(next) => generator.resume_with(next).await,
                            GeneratorResume::Return(value) => {
                                returning = true;
                                generator.return_value(value).await
                            }
                            GeneratorResume::Throw(error) => {
                                returning = false;
                                generator.throw_value(error).await
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
                        return GeneratorResume::Throw(TError::from(TsonicError::unsupported(
                            "a delegated async generator closed without a return value",
                        )));
                    }
                },
            }
        }
    }
}

pub struct YieldPoint<TYield, TReturn, TNext, TError> {
    shared: Rc<RefCell<SharedState<TYield, TReturn, TNext, TError>>>,
    suspended: bool,
}

impl<TYield, TReturn, TNext, TError> Future for YieldPoint<TYield, TReturn, TNext, TError> {
    type Output = GeneratorResume<TNext, TReturn, TError>;

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.suspended {
            self.suspended = true;
            return Poll::Pending;
        }
        let mut shared = self.shared.borrow_mut();
        match mem::replace(&mut shared.resume, ResumeSlot::Waiting) {
            ResumeSlot::Waiting => Poll::Pending,
            ResumeSlot::Ready(value) => Poll::Ready(value),
        }
    }
}

type GeneratorFuture<'a, TReturn, TError> =
    Pin<Box<dyn Future<Output = Result<TReturn, TError>> + 'a>>;

pub(super) struct GeneratorCore<'a, TYield, TReturn, TNext, TError> {
    future: Option<GeneratorFuture<'a, TReturn, TError>>,
    shared: Rc<RefCell<SharedState<TYield, TReturn, TNext, TError>>>,
    completed: Option<TReturn>,
    started: bool,
}

impl<'a, TYield, TReturn, TNext, TError> GeneratorCore<'a, TYield, TReturn, TNext, TError> {
    pub(super) fn new<TFactory, TFuture>(factory: TFactory) -> Self
    where
        TFactory: FnOnce(GeneratorController<TYield, TReturn, TNext, TError>) -> TFuture,
        TFuture: Future<Output = Result<TReturn, TError>> + 'a,
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

    pub(super) fn prepare_command(&mut self, command: GeneratorResume<TNext, TReturn, TError>) {
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

    pub(super) fn poll_step(&mut self, context: &mut Context<'_>) -> GeneratorPoll<TYield, TError> {
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

pub(super) enum GeneratorPoll<TYield, TError> {
    Yielded(TYield),
    Completed,
    Failed(TError),
    Pending,
}

pub struct GeneratorImpl<'a, TYield, TReturn, TNext, TError> {
    core: RefCell<GeneratorCore<'a, TYield, TReturn, TNext, TError>>,
}

pub type Generator<TYield, TReturn, TNext, TError> =
    GeneratorImpl<'static, TYield, TReturn, TNext, TError>;
pub type BorrowedGenerator<'a, TYield, TReturn, TNext, TError> =
    GeneratorImpl<'a, TYield, TReturn, TNext, TError>;

impl<'a, TYield, TReturn, TNext, TError> GeneratorImpl<'a, TYield, TReturn, TNext, TError> {
    pub fn new<TFactory, TFuture>(factory: TFactory) -> Self
    where
        TFactory: FnOnce(GeneratorController<TYield, TReturn, TNext, TError>) -> TFuture,
        TFuture: Future<Output = Result<TReturn, TError>> + 'a,
    {
        Self {
            core: RefCell::new(GeneratorCore::new(factory)),
        }
    }

    pub fn resume(&self) -> Result<IteratorResult<TYield, TReturn>, TError>
    where
        TNext: Default,
        TReturn: Clone,
    {
        self.resume_with(TNext::default())
    }

    pub fn resume_with(&self, value: TNext) -> Result<IteratorResult<TYield, TReturn>, TError>
    where
        TReturn: Clone,
    {
        self.with_core(|core| {
            if !core.is_running() {
                return Ok(core.completed_result());
            }
            core.prepare_resume(value);
            Self::poll_sync_boundary(core)
        })
    }

    pub fn return_value(&self, value: TReturn) -> Result<IteratorResult<TYield, TReturn>, TError>
    where
        TReturn: Clone,
    {
        self.with_core(|core| {
            if !core.has_started() || !core.is_running() {
                return Ok(core.force_return(value));
            }
            core.prepare_command(GeneratorResume::Return(value));
            Self::poll_sync_boundary(core)
        })
    }

    pub fn throw_value(&self, error: TError) -> Result<IteratorResult<TYield, TReturn>, TError>
    where
        TReturn: Clone,
    {
        self.with_core(|core| {
            if !core.has_started() || !core.is_running() {
                core.close();
                return Err(error);
            }
            core.prepare_command(GeneratorResume::Throw(error));
            Self::poll_sync_boundary(core)
        })
    }

    fn with_core<T>(
        &self,
        operation: impl FnOnce(
            &mut GeneratorCore<'a, TYield, TReturn, TNext, TError>,
        ) -> Result<T, TError>,
    ) -> Result<T, TError> {
        operation(&mut self.core.borrow_mut())
    }

    fn poll_sync_boundary(
        core: &mut GeneratorCore<'a, TYield, TReturn, TNext, TError>,
    ) -> Result<IteratorResult<TYield, TReturn>, TError>
    where
        TReturn: Clone,
    {
        let mut context = Context::from_waker(Waker::noop());
        match core.poll_step(&mut context) {
            GeneratorPoll::Yielded(value) => Ok(IteratorResult::yielded(value)),
            GeneratorPoll::Completed => Ok(core.completed_result()),
            GeneratorPoll::Failed(error) => Err(error),
            GeneratorPoll::Pending => {
                panic!("a synchronous generator suspended on a non-yield future")
            }
        }
    }
}

impl<'a, TYield, TReturn, TNext, TError> Iterator
    for GeneratorImpl<'a, TYield, TReturn, TNext, TError>
where
    TNext: Default,
    TReturn: Clone,
{
    type Item = Result<TYield, TError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.resume() {
            Ok(result) => result.into_yield().map(Ok),
            Err(error) => Some(Err(error)),
        }
    }
}
