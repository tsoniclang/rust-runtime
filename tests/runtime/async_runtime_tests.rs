use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
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
    let entry_events = Rc::clone(&events);
    let result: Result<(), &'static str> = block_on(run_tasks(|scope| async move {
        let observed = Rc::clone(&entry_events);
        spawn(&scope, async move {
            observed.borrow_mut().push("start");
            YieldOnce(false).await;
            observed.borrow_mut().push("finish");
            Err("spawned")
        });
        let observed = Rc::clone(&entry_events);
        spawn(&scope, async move {
            YieldOnce(false).await;
            observed.borrow_mut().push("second");
            Ok(())
        });
        entry_events.borrow_mut().push("entry");
        Ok(())
    }));
    assert_eq!(result, Err("spawned"));
    assert_eq!(*events.borrow(), ["start", "entry", "finish", "second"]);
}

#[test]
fn native_main_error_wins_without_losing_spawned_work() {
    let finished = Rc::new(Cell::new(false));
    let observed = Rc::clone(&finished);
    let result: Result<(), &'static str> = block_on(run_tasks(|scope| async move {
        spawn(&scope, async move {
            YieldOnce(false).await;
            observed.set(true);
            Err("spawned")
        });
        Err("main")
    }));
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
    let result: Result<(), Failure> = block_on(run_tasks(|scope| async move {
        spawn(&scope, async move {
            YieldOnce(false).await;
            Err(Failure(first_task))
        });
        spawn(&scope, async move { Err(Failure(second_task)) });
        Ok(())
    }));
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
    let result: Result<(), ()> = block_on(run_tasks(|scope| async move {
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
        std::future::poll_fn(|context| {
            if completed.get() {
                Poll::Ready(Ok(()))
            } else {
                context.waker().wake_by_ref();
                Poll::Pending
            }
        })
        .await
    }));
    assert_eq!(result, Ok(()));
}

#[test]
fn scoped_driver_completes_a_signal_awaited_from_an_initially_pending_task() {
    let ready = Rc::new(Cell::new(false));
    let waiting = Rc::new(RefCell::new(None::<std::task::Waker>));
    let observed = Rc::clone(&ready);
    let task_waiting = Rc::clone(&waiting);
    let result: Result<(), ()> = block_on(run_tasks(|scope| async move {
        spawn(&scope, async move {
            YieldOnce(false).await;
            observed.set(true);
            if let Some(waker) = task_waiting.borrow_mut().take() {
                waker.wake();
            }
            Ok(())
        });
        std::future::poll_fn(|context| {
            if ready.get() {
                Poll::Ready(Ok(()))
            } else {
                *waiting.borrow_mut() = Some(context.waker().clone());
                Poll::Pending
            }
        })
        .await
    }));
    assert_eq!(result, Ok(()));
}

#[test]
fn plain_block_on_keeps_its_no_task_polling_path_inside_a_task_scope() {
    let ready = Rc::new(Cell::new(false));
    let observed = Rc::clone(&ready);
    let entry_ready = Rc::clone(&ready);
    let result: Result<(), ()> = block_on(run_tasks(|scope| async move {
        spawn(&scope, async move {
            YieldOnce(false).await;
            observed.set(true);
            Ok(())
        });
        assert!(!block_on(async { entry_ready.get() }));
        assert!(!entry_ready.get());
        Ok(())
    }));
    assert_eq!(result, Ok(()));
    assert!(ready.get());
}

#[test]
fn native_task_owner_is_cleared_on_unwinding_and_expired_scopes_are_refused() {
    let panic = std::panic::catch_unwind(|| {
        let _: Result<(), ()> = block_on(run_tasks(|_scope| async { panic!("entry failed") }));
    });
    assert!(panic.is_err());
    let scope = block_on(run_tasks::<_, (), _>(|scope| async move { Ok(scope) }))
        .expect("a new owner can start after unwinding");
    let expired = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        spawn(&scope, async { Ok(()) });
    }));
    assert!(expired.is_err());
    assert_eq!(
        block_on(run_tasks::<_, (), _>(|_scope| async { Ok(42) })),
        Ok(42)
    );
}

#[test]
fn dormant_tasks_are_not_repolled_by_main_or_another_tasks_wakes() {
    for hot_task in [false, true] {
        let polls: Vec<_> = (0..4).map(|_| Rc::new(Cell::new(0))).collect();
        let ready: Vec<_> = (0..4).map(|_| Rc::new(Cell::new(false))).collect();
        let wakers: Vec<_> = (0..4).map(|_| Rc::new(RefCell::new(None))).collect();
        let task_polls = polls.clone();
        let result: Result<Vec<usize>, ()> = block_on(run_tasks(|scope| async move {
            for index in 0..task_polls.len() {
                spawn(
                    &scope,
                    DormantNativeTask {
                        polls: Rc::clone(&task_polls[index]),
                        ready: Rc::clone(&ready[index]),
                        waker: Rc::clone(&wakers[index]),
                    },
                );
            }
            let finished = Rc::new(Cell::new(false));
            if hot_task {
                let finished = Rc::clone(&finished);
                spawn(&scope, async move {
                    for _index in 0..12 {
                        YieldOnce(false).await;
                    }
                    finished.set(true);
                    Ok(())
                });
            }
            let mut remaining = 12;
            std::future::poll_fn(|context| {
                if hot_task {
                    if finished.get() {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                } else if remaining == 0 {
                    Poll::Ready(())
                } else {
                    remaining -= 1;
                    context.waker().wake_by_ref();
                    Poll::Pending
                }
            })
            .await;
            let before_release = task_polls.iter().map(|count| count.get()).collect();
            for index in 0..ready.len() {
                ready[index].set(true);
                let waker = wakers[index]
                    .borrow_mut()
                    .take()
                    .expect("the eager task registered its wake");
                waker.wake();
            }
            Ok(before_release)
        }));
        assert_eq!(
            result,
            Ok(vec![1; 4]),
            "unrelated wakes must not poll dormant futures"
        );
        assert!(
            polls.iter().all(|count| count.get() == 2),
            "each dormant future is polled once when explicitly released"
        );
    }
}

#[test]
fn aggregate_tasks_wake_the_selected_outer_driver_and_drain_after_main() {
    let polls = Rc::new(Cell::new(0));
    let ready = Rc::new(Cell::new(false));
    let child_waker = Rc::new(RefCell::new(None));
    let task = DormantNativeTask {
        polls: Rc::clone(&polls),
        ready: Rc::clone(&ready),
        waker: Rc::clone(&child_waker),
    };
    let mut root = std::pin::pin!(run_tasks(|scope| async move {
        spawn(&scope, task);
        Ok(7)
    }));
    let driver = Arc::new(NativeDriverWake(Arc::new(AtomicUsize::new(0))));
    let waker = Waker::from(Arc::clone(&driver));
    let mut context = Context::from_waker(&waker);
    assert_eq!(root.as_mut().poll(&mut context), Poll::Pending);
    assert_eq!(polls.get(), 1);
    assert_eq!(driver.0.load(Ordering::SeqCst), 0);
    ready.set(true);
    child_waker
        .borrow_mut()
        .take()
        .expect("the eager child registered its readiness waker")
        .wake();
    assert_eq!(driver.0.load(Ordering::SeqCst), 1);
    assert_eq!(polls.get(), 1);
    assert_eq!(root.as_mut().poll(&mut context), Poll::Ready(Ok(7)));
    assert_eq!(polls.get(), 2);
}

#[test]
fn aggregate_tasks_do_not_start_an_owner_until_the_future_is_driven() {
    let entered = Rc::new(Cell::new(false));
    let observed = Rc::clone(&entered);
    let future = run_tasks::<_, (), _>(|_scope| async move {
        observed.set(true);
        Ok(())
    });
    assert!(!entered.get());
    drop(future);
    assert!(!entered.get());
    assert_eq!(
        block_on(run_tasks::<_, (), _>(|_scope| async { Ok(42) })),
        Ok(42)
    );
}

#[test]
fn aggregate_retirement_releases_the_driver_despite_saved_child_wakers() {
    for complete in [false, true] {
        let saved = Rc::new(RefCell::new(None::<Waker>));
        let ready = Rc::new(Cell::new(false));
        let child_saved = Rc::clone(&saved);
        let child_ready = Rc::clone(&ready);
        let notifications = Arc::new(AtomicUsize::new(0));
        let driver = Arc::new(NativeDriverWake(Arc::clone(&notifications)));
        let observed = Arc::downgrade(&driver);
        let waker = Waker::from(Arc::clone(&driver));
        {
            let mut root = std::pin::pin!(run_tasks(|scope| async move {
                spawn(&scope, std::future::poll_fn(move |context| {
                    *child_saved.borrow_mut() = Some(context.waker().clone());
                    if child_ready.get() {
                        Poll::Ready(Ok::<(), ()>(()))
                    } else {
                        Poll::Pending
                    }
                }));
                Ok(())
            }));
            let mut context = Context::from_waker(&waker);
            assert!(root.as_mut().poll(&mut context).is_pending());
            if complete {
                ready.set(true);
                saved
                    .borrow()
                    .as_ref()
                    .expect("the child registered its native waker")
                    .wake_by_ref();
                assert!(root.as_mut().poll(&mut context).is_ready());
            }
        }
        drop(waker);
        drop(driver);
        assert!(observed.upgrade().is_none(), "retired roots release the outer driver");
        let before = notifications.load(Ordering::SeqCst);
        saved
            .borrow()
            .as_ref()
            .expect("the child waker remains independently owned")
            .wake_by_ref();
        assert_eq!(notifications.load(Ordering::SeqCst), before, "retired children do not notify the old driver");
    }
}

#[test]
fn aggregate_registration_replaces_the_driver_and_coalesces_ready_children() {
    let polls: Vec<_> = (0..2).map(|_| Rc::new(Cell::new(0))).collect();
    let ready: Vec<_> = (0..2).map(|_| Rc::new(Cell::new(false))).collect();
    let saved: Vec<_> = (0..2).map(|_| Rc::new(RefCell::new(None::<Waker>))).collect();
    let task_polls = polls.clone();
    let task_ready = ready.clone();
    let task_saved = saved.clone();
    let mut root = std::pin::pin!(run_tasks(|scope| async move {
        for index in 0..task_polls.len() {
            spawn(&scope, DormantNativeTask {
                polls: Rc::clone(&task_polls[index]),
                ready: Rc::clone(&task_ready[index]),
                waker: Rc::clone(&task_saved[index]),
            });
        }
        Ok(7)
    }));
    let first = Arc::new(NativeDriverWake(Arc::new(AtomicUsize::new(0))));
    let second = Arc::new(NativeDriverWake(Arc::new(AtomicUsize::new(0))));
    let first_waker = Waker::from(Arc::clone(&first));
    let second_waker = Waker::from(Arc::clone(&second));
    assert!(root.as_mut().poll(&mut Context::from_waker(&first_waker)).is_pending());
    assert!(root.as_mut().poll(&mut Context::from_waker(&second_waker)).is_pending());
    assert_eq!(Arc::strong_count(&first), 2, "the previous registration is released");
    for index in 0..ready.len() {
        ready[index].set(true);
        saved[index]
            .borrow()
            .as_ref()
            .expect("the eager child registered its native waker")
            .wake_by_ref();
    }
    assert_eq!(first.0.load(Ordering::SeqCst), 0);
    assert_eq!(second.0.load(Ordering::SeqCst), 1, "ready children share one outstanding parent notification");
    assert!(polls.iter().all(|count| count.get() == 1), "wake only schedules the selected root");
    assert_eq!(root.as_mut().poll(&mut Context::from_waker(&second_waker)), Poll::Ready(Ok(7)));
    assert!(polls.iter().all(|count| count.get() == 2), "each ready child is polled once");
}

struct NativeDriverWake(Arc<AtomicUsize>);

impl Wake for NativeDriverWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

struct DormantNativeTask {
    polls: Rc<Cell<usize>>,
    ready: Rc<Cell<bool>>,
    waker: Rc<RefCell<Option<std::task::Waker>>>,
}

impl Future for DormantNativeTask {
    type Output = Result<(), ()>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        self.polls.set(self.polls.get() + 1);
        if self.ready.get() {
            Poll::Ready(Ok(()))
        } else {
            let waker = context.waker().clone();
            *self.waker.borrow_mut() = Some(waker);
            Poll::Pending
        }
    }
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
