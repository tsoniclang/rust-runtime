use std::cell::Cell;
use std::rc::Rc;

use tsonic_rust_runtime::{AsyncGenerator, Generator, GeneratorResume};

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
    let mut generator: Generator<i32, i32, i32> = Generator::new(|controller| async move {
        let resumed = resume_generator!(controller.yield_value(1_i32).await);
        resume_generator!(controller.yield_value(resumed).await);
        Ok(resumed + 1)
    });

    let first = generator.resume();
    assert!(!first.done());
    assert_eq!(first.value(), 1);
    assert_eq!(first.yield_value(), 1);

    let second = generator.resume_with(41);
    assert!(!second.done());
    assert_eq!(second.value(), 41);

    let completed = generator.resume();
    assert!(completed.done());
    assert_eq!(completed.value(), 42);
    assert_eq!(completed.completed_value(), 42);
    assert_eq!(completed.into_return(), Some(42));
}

#[test]
fn first_resume_value_is_ignored_and_return_closes_the_generator() {
    let mut generator = Generator::new(|controller| async move {
        let resumed = resume_generator!(controller.yield_value(7_i32).await);
        Ok(resumed)
    });

    assert_eq!(generator.resume_with(99).value(), 7);
    assert_eq!(generator.return_value(12).value(), 12);
    assert_eq!(generator.resume().value(), 12);
}

#[test]
fn generator_implements_rust_iteration_without_exposing_return_values() {
    let generator = Generator::<i32, i32, ()>::new(|controller| async move {
        resume_generator!(controller.yield_value(2_i32).await);
        resume_generator!(controller.yield_value(3_i32).await);
        Ok(4_i32)
    });

    assert_eq!(generator.collect::<Vec<_>>(), vec![2, 3]);
}

#[test]
fn asynchronous_generator_uses_the_same_resume_protocol() {
    let generator = AsyncGenerator::new(|controller| async move {
        let resumed = resume_generator!(controller.yield_value(5_i32).await);
        Ok(resumed + 1)
    });

    let first = block_on(generator.resume());
    assert_eq!(first.value(), 5);
    let completed = block_on(generator.resume_with(8));
    assert!(completed.done());
    assert_eq!(completed.value(), 9);
    assert_eq!(block_on(generator.return_value(11)).value(), 11);
}

#[test]
fn asynchronous_generator_queues_concurrent_requests_in_fifo_order() {
    let generator = AsyncGenerator::new(|controller| async move {
        let first = resume_generator!(controller.yield_value(1_i32).await);
        let second = resume_generator!(controller.yield_value(first).await);
        resume_generator!(controller.yield_value(second).await);
        Ok(12_i32)
    });

    let first = generator.resume();
    let second = generator.resume_with(7);
    let third = generator.resume_with(9);
    let (first, second, third) = block_on(join3(first, second, third));

    assert_eq!(first.yield_value(), 1);
    assert_eq!(second.yield_value(), 7);
    assert_eq!(third.yield_value(), 9);
    assert_eq!(block_on(generator.resume_with(11)).completed_value(), 12);
}

#[test]
fn asynchronous_generator_iteration_resumes_with_the_default_next_value() {
    let generator = AsyncGenerator::new(|controller| async move {
        let resumed = resume_generator!(controller.yield_value(1_i32).await);
        resume_generator!(controller.yield_value(resumed).await);
        Ok(9_i32)
    });

    assert_eq!(block_on(generator.next_yield()), Some(1));
    assert_eq!(block_on(generator.next_yield()), Some(0));
    assert_eq!(block_on(generator.next_yield()), None);
}

#[test]
fn delegated_generator_forwards_yields_next_values_and_return() {
    let mut outer = Generator::new(|controller| async move {
        let completed = resume_generator!(
            controller
                .yield_from(Generator::new(|inner| async move {
                    let next = resume_generator!(inner.yield_value(3_i32).await);
                    resume_generator!(inner.yield_value(next).await);
                    Ok(9_i32)
                }))
                .await
        );
        Ok(completed)
    });

    assert_eq!(outer.resume().yield_value(), 3);
    assert_eq!(outer.resume_with(7).yield_value(), 7);
    assert_eq!(outer.resume_with(0).completed_value(), 9);
}

#[test]
fn generator_throw_closes_and_returns_the_exact_error() {
    use tsonic_rust_runtime::{JsError, JsErrorKind, TsonicError};

    let mut generator: Generator<i32, i32, ()> = Generator::new(|controller| async move {
        resume_generator!(controller.yield_value(1_i32).await);
        Ok(2_i32)
    });
    assert_eq!(generator.resume().yield_value(), 1);
    let error = JsError::new(JsErrorKind::Error, "stop");
    assert!(matches!(
        generator.throw_value(error.clone()),
        Err(TsonicError::Js(actual)) if actual == error
    ));
}

#[test]
fn return_and_throw_resume_suspended_cleanup_before_closing() {
    let return_cleanup = Rc::new(Cell::new(0));
    let return_probe = Rc::clone(&return_cleanup);
    let mut returned: Generator<i32, i32, ()> = Generator::new(move |controller| async move {
        let command = controller.yield_value(1_i32).await;
        return_probe.set(return_probe.get() + 1);
        match command {
            GeneratorResume::Next(()) => Ok(2_i32),
            GeneratorResume::Return(value) => Ok(value),
            GeneratorResume::Throw(error) => Err(error),
        }
    });
    assert_eq!(returned.resume().yield_value(), 1);
    assert_eq!(returned.return_value(9).completed_value(), 9);
    assert_eq!(return_cleanup.get(), 1);

    let throw_cleanup = Rc::new(Cell::new(0));
    let throw_probe = Rc::clone(&throw_cleanup);
    let mut thrown: Generator<i32, i32, ()> = Generator::new(move |controller| async move {
        let command = controller.yield_value(1_i32).await;
        throw_probe.set(throw_probe.get() + 1);
        match command {
            GeneratorResume::Next(()) => Ok(2_i32),
            GeneratorResume::Return(value) => Ok(value),
            GeneratorResume::Throw(error) => Err(error),
        }
    });
    assert_eq!(thrown.resume().yield_value(), 1);
    let error = tsonic_rust_runtime::JsError::error("stop");
    assert!(thrown.throw_value(error).is_err());
    assert_eq!(throw_cleanup.get(), 1);
    assert!(thrown.resume().done());
}

#[test]
fn delegated_return_unwinds_the_inner_generator_before_outer_completion() {
    let cleanup = Rc::new(Cell::new(0));
    let probe = Rc::clone(&cleanup);
    let inner: Generator<i32, i32, ()> = Generator::new(move |controller| async move {
        let command = controller.yield_value(1_i32).await;
        probe.set(probe.get() + 1);
        match command {
            GeneratorResume::Next(()) => Ok(2_i32),
            GeneratorResume::Return(value) => Ok(value),
            GeneratorResume::Throw(error) => Err(error),
        }
    });
    let mut outer: Generator<i32, i32, ()> = Generator::new(|controller| async move {
        let value = resume_generator!(controller.yield_from(inner).await);
        Ok(value)
    });

    assert_eq!(outer.resume().yield_value(), 1);
    assert_eq!(outer.return_value(7).completed_value(), 7);
    assert_eq!(cleanup.get(), 1);
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
