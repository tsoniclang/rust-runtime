use tsonic_rust_runtime::{AsyncGenerator, Generator};

#[test]
fn synchronous_generator_yields_resumes_and_completes() {
    let mut generator: Generator<i32, i32, i32> = Generator::new(|controller| async move {
        let resumed = controller.yield_value(1_i32).await;
        controller.yield_value(resumed).await;
        resumed + 1
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
}

#[test]
fn first_resume_value_is_ignored_and_return_closes_the_generator() {
    let mut generator = Generator::new(|controller| async move {
        let resumed = controller.yield_value(7_i32).await;
        resumed
    });

    assert_eq!(generator.resume_with(99).value(), 7);
    assert_eq!(generator.return_value(12).value(), 12);
    assert_eq!(generator.resume().value(), 12);
}

#[test]
fn generator_implements_rust_iteration_without_exposing_return_values() {
    let generator = Generator::<i32, i32, ()>::new(|controller| async move {
        controller.yield_value(2_i32).await;
        controller.yield_value(3_i32).await;
        4_i32
    });

    assert_eq!(generator.collect::<Vec<_>>(), vec![2, 3]);
}

#[test]
fn asynchronous_generator_uses_the_same_resume_protocol() {
    let mut generator = AsyncGenerator::new(|controller| async move {
        let resumed = controller.yield_value(5_i32).await;
        resumed + 1
    });

    let first = block_on(generator.resume());
    assert_eq!(first.value(), 5);
    let completed = block_on(generator.resume_with(8));
    assert!(completed.done());
    assert_eq!(completed.value(), 9);
    assert_eq!(block_on(generator.return_value(11)).value(), 11);
}

#[test]
fn delegated_generator_forwards_yields_next_values_and_return() {
    let mut outer = Generator::new(|controller| async move {
        controller
            .yield_from(Generator::new(|inner| async move {
                let next = inner.yield_value(3_i32).await;
                inner.yield_value(next).await;
                9_i32
            }))
            .await
    });

    assert_eq!(outer.resume().yield_value(), 3);
    assert_eq!(outer.resume_with(7).yield_value(), 7);
    assert_eq!(outer.resume_with(0).completed_value(), 9);
}

#[test]
fn generator_throw_closes_and_returns_the_exact_error() {
    use tsonic_rust_runtime::{JsError, JsErrorKind, TsonicError};

    let mut generator: Generator<i32, i32, ()> = Generator::new(|controller| async move {
        controller.yield_value(1_i32).await;
        2_i32
    });
    assert_eq!(generator.resume().yield_value(), 1);
    let error = JsError::new(JsErrorKind::Error, "stop");
    assert!(matches!(
        generator.throw_value(error.clone()),
        Err(TsonicError::Js(actual)) if actual == error
    ));
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct TestWake;
    impl Wake for TestWake {
        fn wake(self: Arc<Self>) {}
    }

    let waker = Waker::from(Arc::new(TestWake));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}
