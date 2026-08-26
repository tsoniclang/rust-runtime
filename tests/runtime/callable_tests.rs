use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use tsonic_rust_runtime::{
    BorrowedLocalAsyncCallable, BorrowedLocalCallable, OwnedLocalAsyncCallable, OwnedLocalCallable,
    ThreadedAsyncCallable, ThreadedCallable,
};

struct TestWake;

impl Wake for TestWake {
    fn wake(self: Arc<Self>) {}
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::from(Arc::new(TestWake));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

#[test]
fn callable_invokes_and_clones_one_identity() {
    let callable = OwnedLocalCallable::new(|(left, right): (i32, i32)| left + right);
    let alias = callable.clone();

    assert_eq!(callable.call((2, 3)), 5);
    assert!(OwnedLocalCallable::same(&callable, &alias));
    assert!(!OwnedLocalCallable::same(
        &callable,
        &OwnedLocalCallable::new(|(left, right): (i32, i32)| left + right),
    ));
}

#[test]
fn recursive_callable_receives_its_exact_identity() {
    let factorial = OwnedLocalCallable::recursive(|factorial, value: i32| {
        if value <= 1 {
            1
        } else {
            value * factorial.call(value - 1)
        }
    });

    assert_eq!(factorial.call(5), 120);
}

#[test]
fn borrowed_and_threaded_recursive_callables_preserve_their_execution_domains() {
    let offset = 1_i32;
    let local = BorrowedLocalCallable::recursive(|current, value: i32| {
        if value <= 0 {
            offset
        } else {
            current.call(value - 1) + 1
        }
    });
    assert_eq!(local.call(4), 5);

    let threaded = ThreadedCallable::recursive(|current, value: i32| {
        if value <= 1 {
            1
        } else {
            value * current.call(value - 1)
        }
    });
    let worker = threaded.clone();
    assert_eq!(
        std::thread::spawn(move || worker.call(5))
            .join()
            .expect("threaded recursive callable worker failed"),
        120
    );
}

#[test]
fn borrowed_local_callable_retains_a_non_static_borrow() {
    let prefix = String::from("local:");
    let callable = BorrowedLocalCallable::new(|value: i32| format!("{prefix}{value}"));

    assert_eq!(callable.call(7), "local:7");
}

#[test]
fn threaded_callable_has_explicit_thread_safe_identity() {
    let callable = ThreadedCallable::new(|value: i32| value + 1);
    let alias = callable.clone();
    let worker = std::thread::spawn(move || alias.call(8));

    assert_eq!(worker.join().expect("threaded callable worker failed"), 9);
    assert!(ThreadedCallable::same(&callable, &callable.clone()));
}

#[test]
fn borrowed_async_callable_retains_a_non_static_borrow() {
    let prefix = String::from("local:");
    let callable = BorrowedLocalAsyncCallable::new(|value: i32| {
        let prefix = &prefix;
        async move { format!("{prefix}{value}") }
    });
    let alias = callable.clone();

    assert_eq!(block_on(callable.call(7)), "local:7");
    assert!(BorrowedLocalAsyncCallable::same(&callable, &alias));
}

#[test]
fn owned_async_callable_supports_recursive_identity() {
    let factorial = OwnedLocalAsyncCallable::recursive(|factorial, value: i32| async move {
        if value <= 1 {
            1
        } else {
            value * factorial.call(value - 1).await
        }
    });
    let alias = factorial.clone();

    assert_eq!(block_on(factorial.call(5)), 120);
    assert!(OwnedLocalAsyncCallable::same(&factorial, &alias));
}

#[test]
fn borrowed_async_callable_supports_recursive_non_static_state() {
    let offset = 1_i32;
    let recursive = BorrowedLocalAsyncCallable::recursive(|current, value: i32| async move {
        if value <= 0 {
            offset
        } else {
            current.call(value - 1).await + 1
        }
    });

    assert_eq!(block_on(recursive.call(4)), 5);
}

#[test]
fn threaded_async_callable_has_send_static_future_and_identity() {
    let callable = ThreadedAsyncCallable::new(|value: i32| async move { value + 1 });
    let alias = callable.clone();
    let worker = std::thread::spawn(move || block_on(alias.call(8)));

    assert_eq!(
        worker
            .join()
            .expect("threaded async callable worker failed"),
        9
    );
    assert!(ThreadedAsyncCallable::same(&callable, &callable.clone()));
}

#[test]
fn threaded_async_callable_supports_recursive_send_futures() {
    let callable = ThreadedAsyncCallable::recursive(|current, value: i32| async move {
        if value <= 1 {
            1
        } else {
            value * current.call(value - 1).await
        }
    });
    let worker = callable.clone();

    assert_eq!(
        std::thread::spawn(move || block_on(worker.call(5)))
            .join()
            .expect("threaded recursive async callable worker failed"),
        120
    );
}
