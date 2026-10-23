use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::mpsc;
use std::task::{Context, Poll};
use std::thread;

use tsonic_rust_runtime::{block_on, run_tasks, spawn};

#[test]
fn block_on_returns_ready_output() {
    assert_eq!(block_on(async { 42_i32 }), 42);
}

#[test]
fn block_on_parks_until_the_future_wakes_it() {
    let (sender, receiver) = mpsc::channel();
    assert_eq!(block_on(ThreadSignal::new(sender, receiver)), 7);
}

#[test]
fn native_spawn_starts_eagerly_and_drains_all_pending_tasks() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let result: Result<(), &'static str> = run_tasks(|scope| {
        let observed = Rc::clone(&events);
        spawn(&scope, async move {
            observed.borrow_mut().push("start");
            YieldOnce(false).await;
            observed.borrow_mut().push("finish");
            Err("spawned")
        });
        let observed = Rc::clone(&events);
        spawn(&scope, async move {
            YieldOnce(false).await;
            observed.borrow_mut().push("second");
            Ok(())
        });
        events.borrow_mut().push("entry");
        Ok(())
    });
    assert_eq!(result, Err("spawned"));
    assert_eq!(*events.borrow(), ["start", "entry", "finish", "second"]);
}

#[test]
fn native_main_error_wins_without_losing_spawned_work() {
    let finished = Rc::new(Cell::new(false));
    let observed = Rc::clone(&finished);
    let result: Result<(), &'static str> = run_tasks(|scope| {
        spawn(&scope, async move {
            YieldOnce(false).await;
            observed.set(true);
            Err("spawned")
        });
        Err("main")
    });
    assert_eq!(result, Err("main"));
    assert!(finished.get());
}

#[test]
fn native_task_errors_keep_identity_and_completion_order_without_clone() {
    struct Failure(Rc<Cell<i32>>);
    let first = Rc::new(Cell::new(7));
    let second = Rc::new(Cell::new(9));
    let first_task = Rc::clone(&first);
    let second_task = Rc::clone(&second);
    let result: Result<(), Failure> = run_tasks(|scope| {
        spawn(&scope, async move {
            YieldOnce(false).await;
            Err(Failure(first_task))
        });
        spawn(&scope, async move { Err(Failure(second_task)) });
        Ok(())
    });
    let Err(error) = result else {
        panic!("the first completed task failed")
    };
    assert!(Rc::ptr_eq(&error.0, &second));
    assert_eq!(first.get(), 7);
}

#[test]
fn native_tasks_can_spawn_tasks_and_progress_while_main_is_awaiting() {
    let completed = Rc::new(Cell::new(false));
    let observed = Rc::clone(&completed);
    let result: Result<(), ()> = run_tasks(|scope| {
        let nested = scope.clone();
        spawn(&scope, async move {
            YieldOnce(false).await;
            spawn(&nested, async move {
                YieldOnce(false).await;
                observed.set(true);
                Ok(())
            });
            Ok(())
        });
        scope.block_on(std::future::poll_fn(|context| {
            if completed.get() {
                Poll::Ready(Ok(()))
            } else {
                context.waker().wake_by_ref();
                Poll::Pending
            }
        }))
    });
    assert_eq!(result, Ok(()));
}

#[test]
fn scoped_driver_completes_a_signal_awaited_from_an_initially_pending_task() {
    let ready = Rc::new(Cell::new(false));
    let waiting = Rc::new(RefCell::new(None::<std::task::Waker>));
    let observed = Rc::clone(&ready);
    let task_waiting = Rc::clone(&waiting);
    let result: Result<(), ()> = run_tasks(|scope| {
        spawn(&scope, async move {
            YieldOnce(false).await;
            observed.set(true);
            if let Some(waker) = task_waiting.borrow_mut().take() {
                waker.wake();
            }
            Ok(())
        });
        scope.block_on(std::future::poll_fn(|context| {
            if ready.get() {
                Poll::Ready(Ok(()))
            } else {
                *waiting.borrow_mut() = Some(context.waker().clone());
                Poll::Pending
            }
        }))
    });
    assert_eq!(result, Ok(()));
}

#[test]
fn plain_block_on_keeps_its_no_task_polling_path_inside_a_task_scope() {
    let ready = Rc::new(Cell::new(false));
    let observed = Rc::clone(&ready);
    let result: Result<(), ()> = run_tasks(|scope| {
        spawn(&scope, async move {
            YieldOnce(false).await;
            observed.set(true);
            Ok(())
        });
        assert!(!block_on(async { ready.get() }));
        assert!(!ready.get());
        Ok(())
    });
    assert_eq!(result, Ok(()));
    assert!(ready.get());
}

#[test]
fn native_task_owner_is_cleared_on_unwinding_and_expired_scopes_are_refused() {
    let panic = std::panic::catch_unwind(|| {
        let _: Result<(), ()> = run_tasks(|_scope| panic!("entry failed"));
    });
    assert!(panic.is_err());
    let scope = run_tasks::<_, ()>(Ok).expect("a new owner can start after unwinding");
    let expired = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        spawn(&scope, async { Ok(()) });
    }));
    assert!(expired.is_err());
    assert_eq!(run_tasks::<_, ()>(|_scope| Ok(42)), Ok(42));
}

struct YieldOnce(bool);

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            context.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

struct ThreadSignal {
    sender: Option<mpsc::Sender<i32>>,
    receiver: mpsc::Receiver<i32>,
}

impl ThreadSignal {
    fn new(sender: mpsc::Sender<i32>, receiver: mpsc::Receiver<i32>) -> Self {
        Self {
            sender: Some(sender),
            receiver,
        }
    }
}

impl Future for ThreadSignal {
    type Output = i32;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Ok(value) = self.receiver.try_recv() {
            return Poll::Ready(value);
        }
        if let Some(sender) = self.sender.take() {
            let waker = context.waker().clone();
            thread::spawn(move || {
                sender
                    .send(7)
                    .expect("receiver must remain live while blocked");
                waker.wake();
            });
        }
        Poll::Pending
    }
}
