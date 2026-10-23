use std::cell::Cell;
use std::convert::Infallible;
use std::rc::Rc;

use tsonic_rust_runtime::{
    AsyncGenerator, Generator, GeneratorResume,
    TsonicError,
};

macro_rules! resume_generator {
    ($expression:expr) => {
        match $expression {
            GeneratorResume::Next(value) => value,
            GeneratorResume::Return(value) => return Ok(value),
            GeneratorResume::Throw(error) => return Err(error),
        }
    };
}

#[test]
fn synchronous_generator_yields_resumes_and_completes() {
    let generator: Generator<i32, i32, i32, Infallible> =
        Generator::new(|controller| async move {
            let resumed = resume_generator!(controller.yield_value(1_i32).await);
            resume_generator!(controller.yield_value(resumed).await);
            Ok(resumed + 1)
        });

    let first = generator.resume().unwrap();
    assert!(!first.done());
    assert_eq!(first.value(), 1);
    assert_eq!(first.yield_value(), 1);

    let second = generator.resume_with(41).unwrap();
    assert!(!second.done());
    assert_eq!(second.value(), 41);

    let completed = generator.resume().unwrap();
    assert!(completed.done());
    assert_eq!(completed.value(), 42);
    assert_eq!(completed.completed_value(), 42);
    assert_eq!(completed.into_return(), Some(42));
}

#[test]
fn first_resume_value_is_ignored_and_return_closes_the_generator() {
    let generator: Generator<i32, i32, i32, Infallible> =
        Generator::new(|controller| async move {
            let resumed = resume_generator!(controller.yield_value(7_i32).await);
            Ok(resumed)
        });

    assert_eq!(generator.resume_with(99).unwrap().value(), 7);
    assert_eq!(generator.return_value(12).unwrap().value(), 12);
    assert_eq!(generator.resume().unwrap().value(), 12);
}

#[test]
fn generator_implements_rust_iteration_without_exposing_return_values() {
    let generator =
        Generator::<i32, i32, (), Infallible>::new(|controller| async move {
            resume_generator!(controller.yield_value(2_i32).await);
            resume_generator!(controller.yield_value(3_i32).await);
            Ok(4_i32)
        });

    assert_eq!(generator.collect::<Vec<_>>(), vec![Ok(2), Ok(3)]);
}

#[test]
fn asynchronous_generator_uses_the_same_resume_protocol() {
    let generator: AsyncGenerator<i32, i32, i32, Infallible> =
        AsyncGenerator::new(|controller| async move {
            let resumed = resume_generator!(controller.yield_value(5_i32).await);
            Ok(resumed + 1)
        });

    let first = block_on(generator.resume()).unwrap();
    assert_eq!(first.value(), 5);
    let completed = block_on(generator.resume_with(8)).unwrap();
    assert!(completed.done());
    assert_eq!(completed.value(), 9);
    assert_eq!(block_on(generator.return_value(11)).unwrap().value(), 11);
}

#[test]
fn asynchronous_generator_queues_concurrent_requests_in_fifo_order() {
    let generator: AsyncGenerator<i32, i32, i32, Infallible> =
        AsyncGenerator::new(|controller| async move {
            let first = resume_generator!(controller.yield_value(1_i32).await);
            let second = resume_generator!(controller.yield_value(first).await);
            resume_generator!(controller.yield_value(second).await);
            Ok(12_i32)
        });

    let first = generator.resume();
    let second = generator.resume_with(7);
    let third = generator.resume_with(9);
    let (first, second, third) = block_on(join3(first, second, third));

    assert_eq!(first.unwrap().yield_value(), 1);
    assert_eq!(second.unwrap().yield_value(), 7);
    assert_eq!(third.unwrap().yield_value(), 9);
    assert_eq!(
        block_on(generator.resume_with(11))
            .unwrap()
            .completed_value(),
        12
    );
}

#[test]
fn asynchronous_generator_iteration_resumes_with_the_default_next_value() {
    let generator: AsyncGenerator<i32, i32, i32, Infallible> =
        AsyncGenerator::new(|controller| async move {
            let resumed = resume_generator!(controller.yield_value(1_i32).await);
            resume_generator!(controller.yield_value(resumed).await);
            Ok(9_i32)
        });

    assert_eq!(block_on(generator.next_yield()), Ok(Some(1)));
    assert_eq!(block_on(generator.next_yield()), Ok(Some(0)));
    assert_eq!(block_on(generator.next_yield()), Ok(None));
}

#[test]
fn delegated_generator_forwards_yields_next_values_and_return() {
    let outer: Generator<i32, i32, i32, TsonicError> =
        Generator::new(|controller| async move {
            let completed = resume_generator!(
                controller
                    .yield_from(Generator::<_, _, _, _>::new(
                        |inner| async move {
                            let next = resume_generator!(inner.yield_value(3_i32).await);
                            resume_generator!(inner.yield_value(next).await);
                            Ok(9_i32)
                        }
                    ))
                    .await
            );
            Ok(completed)
        });

    assert_eq!(outer.resume().unwrap().yield_value(), 3);
    assert_eq!(outer.resume_with(7).unwrap().yield_value(), 7);
    assert_eq!(outer.resume_with(0).unwrap().completed_value(), 9);
}

#[test]
fn generator_throw_closes_and_returns_the_exact_error() {
    use tsonic_rust_runtime::{JsError, JsErrorKind, TsonicError};

    let generator: Generator<i32, i32, (), TsonicError> =
        Generator::new(|controller| async move {
            resume_generator!(controller.yield_value(1_i32).await);
            Ok(2_i32)
        });
    assert_eq!(generator.resume().unwrap().yield_value(), 1);
    let error = JsError::new(JsErrorKind::Error, "stop");
    assert!(matches!(
        generator.throw_value(error.clone().into()),
        Err(TsonicError::Js(actual)) if actual == error
    ));
}

#[test]
fn return_and_throw_resume_suspended_cleanup_before_closing() {
    let return_cleanup = Rc::new(Cell::new(0));
    let return_probe = Rc::clone(&return_cleanup);
    let returned: Generator<i32, i32, (), TsonicError> =
        Generator::new(move |controller| async move {
            let command = controller.yield_value(1_i32).await;
            return_probe.set(return_probe.get() + 1);
            match command {
                GeneratorResume::Next(()) => Ok(2_i32),
                GeneratorResume::Return(value) => Ok(value),
                GeneratorResume::Throw(error) => Err(error),
            }
        });
    assert_eq!(returned.resume().unwrap().yield_value(), 1);
    assert_eq!(returned.return_value(9).unwrap().completed_value(), 9);
    assert_eq!(return_cleanup.get(), 1);

    let throw_cleanup = Rc::new(Cell::new(0));
    let throw_probe = Rc::clone(&throw_cleanup);
    let thrown: Generator<i32, i32, (), TsonicError> =
        Generator::new(move |controller| async move {
            let command = controller.yield_value(1_i32).await;
            throw_probe.set(throw_probe.get() + 1);
            match command {
                GeneratorResume::Next(()) => Ok(2_i32),
                GeneratorResume::Return(value) => Ok(value),
                GeneratorResume::Throw(error) => Err(error),
            }
        });
    assert_eq!(thrown.resume().unwrap().yield_value(), 1);
    let error = tsonic_rust_runtime::JsError::error("stop");
    assert!(thrown.throw_value(error.into()).is_err());
    assert_eq!(throw_cleanup.get(), 1);
    assert!(thrown.resume().unwrap().done());
}

#[test]
fn delegated_return_unwinds_the_inner_generator_before_outer_completion() {
    let cleanup = Rc::new(Cell::new(0));
    let probe = Rc::clone(&cleanup);
    let inner: Generator<i32, i32, (), TsonicError> =
        Generator::new(move |controller| async move {
            let command = controller.yield_value(1_i32).await;
            probe.set(probe.get() + 1);
            match command {
                GeneratorResume::Next(()) => Ok(2_i32),
                GeneratorResume::Return(value) => Ok(value),
                GeneratorResume::Throw(error) => Err(error),
            }
        });
    let outer: Generator<i32, i32, (), TsonicError> =
        Generator::new(|controller| async move {
            let value = resume_generator!(controller.yield_from(inner).await);
            Ok(value)
        });

    assert_eq!(outer.resume().unwrap().yield_value(), 1);
    assert_eq!(outer.return_value(7).unwrap().completed_value(), 7);
    assert_eq!(cleanup.get(), 1);
}

#[derive(Debug)]
struct NativeFailure {
    code: i32,
    identity: Rc<Cell<i32>>,
}

#[test]
fn generator_body_errors_keep_the_closed_native_payload_and_identity() {
    let identity = Rc::new(Cell::new(7));
    let original = Rc::clone(&identity);
    let generator: Generator<i32, i32, (), NativeFailure> =
        Generator::new(move |controller| async move {
            let controller = controller.clone();
            resume_generator!(controller.yield_value(1).await);
            Err(NativeFailure {
                code: 7,
                identity: original,
            })
        });
    assert_eq!(generator.resume().unwrap().yield_value(), 1);
    let Err(error) = generator.resume() else {
        panic!("the generator body must fail")
    };
    assert_eq!(error.code, 7);
    assert!(Rc::ptr_eq(&error.identity, &identity));
    assert!(generator.resume().unwrap().done());
}

#[test]
fn generator_iteration_propagates_native_errors_without_exposing_return_values() {
    fn consume(
        generator: Generator<i32, i32, (), NativeFailure>,
    ) -> Result<Vec<i32>, NativeFailure> {
        let mut values = Vec::new();
        for value in generator {
            values.push(value?);
        }
        Ok(values)
    }
    let identity = Rc::new(Cell::new(7));
    let original = Rc::clone(&identity);
    let generator: Generator<i32, i32, (), NativeFailure> =
        Generator::new(move |controller| async move {
            resume_generator!(controller.yield_value(1).await);
            Err(NativeFailure {
                code: 7,
                identity: original,
            })
        });
    let Err(error) = consume(generator) else {
        panic!("iteration must retain the body failure")
    };
    assert_eq!(error.code, 7);
    assert!(Rc::ptr_eq(&error.identity, &identity));
}

#[test]
fn generator_throw_in_keeps_the_same_native_error_without_clone() {
    let identity = Rc::new(Cell::new(7));
    let generator: Generator<i32, i32, (), NativeFailure> =
        Generator::new(|controller| async move {
            resume_generator!(controller.yield_value(1).await);
            Ok(2)
        });
    assert_eq!(generator.resume().unwrap().yield_value(), 1);
    let Err(error) = generator.throw_value(NativeFailure {
        code: 7,
        identity: Rc::clone(&identity),
    }) else {
        panic!("throw-in must propagate its exact native value")
    };
    assert_eq!(error.code, 7);
    assert!(Rc::ptr_eq(&error.identity, &identity));
}

#[test]
fn async_generator_keeps_native_body_and_iteration_errors_without_clone() {
    let identity = Rc::new(Cell::new(7));
    let original = Rc::clone(&identity);
    let generator: AsyncGenerator<i32, i32, (), NativeFailure> =
        AsyncGenerator::new(move |controller| async move {
            resume_generator!(controller.yield_value(1).await);
            Err(NativeFailure {
                code: 7,
                identity: original,
            })
        });
    assert_eq!(block_on(generator.next_yield()).unwrap(), Some(1));
    let Err(error) = block_on(generator.next_yield()) else {
        panic!("async iteration must propagate the native body failure")
    };
    assert_eq!(error.code, 7);
    assert!(Rc::ptr_eq(&error.identity, &identity));
    assert_eq!(block_on(generator.next_yield()).unwrap(), None);
}

#[derive(Debug)]
enum DelegationError {
    Project(NativeFailure),
    Runtime(TsonicError),
}

impl From<TsonicError> for DelegationError {
    fn from(error: TsonicError) -> Self {
        Self::Runtime(error)
    }
}

#[test]
fn delegated_generator_errors_keep_project_identity_and_admit_native_protocol_errors() {
    let identity = Rc::new(Cell::new(7));
    let original = Rc::clone(&identity);
    let inner: Generator<i32, i32, (), DelegationError> =
        Generator::new(move |_controller| async move {
            Err(DelegationError::Project(NativeFailure {
                code: 7,
                identity: original,
            }))
        });
    let outer: Generator<i32, i32, (), DelegationError> =
        Generator::new(|controller| async move {
            let value = resume_generator!(controller.yield_from(inner).await);
            Ok(value)
        });
    let Err(DelegationError::Project(error)) = outer.resume() else {
        panic!("delegation must not wrap or narrow the project payload")
    };
    assert_eq!(error.code, 7);
    assert!(Rc::ptr_eq(&error.identity, &identity));

    let closed: Generator<i32, i32, (), DelegationError> =
        Generator::new(|_controller| async { Ok(1) });
    assert!(closed
        .throw_value(DelegationError::Runtime(TsonicError::unsupported("closed")))
        .is_err());
    let outer: Generator<i32, i32, (), DelegationError> =
        Generator::new(|controller| async move {
            let value = resume_generator!(controller.yield_from(closed).await);
            Ok(value)
        });
    let Err(DelegationError::Runtime(error)) = outer.resume() else {
        panic!("the native protocol channel must enter E through From")
    };
    assert!(error
        .to_string()
        .contains("delegated generator closed without a return value"));
}

#[test]
fn reentrant_generator_uses_native_borrow_panic_without_an_error_arm() {
    type Reentrant = Generator<i32, i32, (), Infallible>;
    let slot = Rc::new(std::cell::RefCell::new(None::<Reentrant>));
    let captured = Rc::clone(&slot);
    let generator = Reentrant::new(move |_controller| async move {
        let running = captured.borrow();
        let running = running
            .as_ref()
            .expect("the captured generator was published");
        match running.resume() {
            Ok(_) => Ok(1),
            Err(error) => match error {},
        }
    });
    *slot.borrow_mut() = Some(generator);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let stored = slot.borrow();
        let _result = stored.as_ref().unwrap().resume();
    }));
    let panic = result.expect_err("native generator reentry must panic");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("the native borrow panic has a message");
    assert!(message.contains("already borrowed"));
}

#[test]
fn reentrant_async_generator_uses_native_panic_without_an_error_arm() {
    type Reentrant = AsyncGenerator<i32, i32, (), Infallible>;
    let slot = Rc::new(std::cell::RefCell::new(None::<Reentrant>));
    let captured = Rc::clone(&slot);
    let generator = Reentrant::new(move |_controller| async move {
        let request = captured
            .borrow()
            .as_ref()
            .expect("the captured generator was published")
            .resume();
        match request.await {
            Ok(_) => Ok(1),
            Err(error) => match error {},
        }
    });
    *slot.borrow_mut() = Some(generator);
    let request = slot.borrow().as_ref().unwrap().resume();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _result = block_on(request);
    }));
    let panic = result.expect_err("native async generator reentry must panic");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("the native async panic has a message");
    assert_eq!(message, "async generator is already running");
}

#[test]
fn infallible_generators_require_no_runtime_error_conversion_or_policy() {
    let generator: Generator<i32, i32, (), Infallible> =
        Generator::new(|_controller| async { Ok(7) });
    assert_eq!(generator.resume().unwrap().completed_value(), 7);
    assert!(generator.resume().unwrap().done());
    let generator: AsyncGenerator<i32, i32, (), Infallible> =
        AsyncGenerator::new(|_controller| async { Ok(9) });
    assert_eq!(block_on(generator.resume()).unwrap().completed_value(), 9);
    assert!(block_on(generator.resume()).unwrap().done());
}

#[test]
fn abandoned_async_error_payload_is_dropped_after_releasing_the_generator_state() {
    type Stored = AsyncGenerator<i32, i32, (), DropFailure>;
    struct DropFailure {
        generator: std::rc::Weak<std::cell::RefCell<Option<Stored>>>,
        armed: Rc<Cell<bool>>,
        drops: Rc<Cell<u32>>,
    }
    impl Drop for DropFailure {
        fn drop(&mut self) {
            if !self.armed.replace(false) {
                return;
            }
            self.drops.set(self.drops.get() + 1);
            let generator = self
                .generator
                .upgrade()
                .expect("the generator remains stored");
            let request = generator.borrow().as_ref().unwrap().resume();
            drop(request);
        }
    }
    let slot = Rc::new(std::cell::RefCell::new(None::<Stored>));
    let armed = Rc::new(Cell::new(true));
    let drops = Rc::new(Cell::new(0));
    let error = DropFailure {
        generator: Rc::downgrade(&slot),
        armed: Rc::clone(&armed),
        drops: Rc::clone(&drops),
    };
    *slot.borrow_mut() = Some(Stored::new(move |_controller| async move { Err(error) }));
    let first = slot.borrow().as_ref().unwrap().resume();
    let second = slot.borrow().as_ref().unwrap().resume();
    assert!(matches!(block_on(second), Ok(result) if result.done()));
    assert_eq!(drops.get(), 0);
    drop(first);
    assert_eq!(drops.get(), 1);
    let next = slot.borrow().as_ref().unwrap().resume();
    assert!(matches!(block_on(next), Ok(result) if result.done()));
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    use std::task::{Context, Poll, Waker};

    let mut context = Context::from_waker(Waker::noop());
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

async fn join3<A, B, C>(first: A, second: B, third: C) -> (A::Output, B::Output, C::Output)
where
    A: std::future::Future,
    B: std::future::Future,
    C: std::future::Future,
{
    use std::future::poll_fn;
    use std::task::Poll;

    let mut first = Box::pin(first);
    let mut second = Box::pin(second);
    let mut third = Box::pin(third);
    let mut first_result = None;
    let mut second_result = None;
    let mut third_result = None;
    poll_fn(move |context| {
        if first_result.is_none() {
            if let Poll::Ready(value) = first.as_mut().poll(context) {
                first_result = Some(value);
            }
        }
        if second_result.is_none() {
            if let Poll::Ready(value) = second.as_mut().poll(context) {
                second_result = Some(value);
            }
        }
        if third_result.is_none() {
            if let Poll::Ready(value) = third.as_mut().poll(context) {
                third_result = Some(value);
            }
        }
        match (
            first_result.take(),
            second_result.take(),
            third_result.take(),
        ) {
            (Some(first), Some(second), Some(third)) => Poll::Ready((first, second, third)),
            (first, second, third) => {
                first_result = first;
                second_result = second;
                third_result = third;
                Poll::Pending
            }
        }
    })
    .await
}
