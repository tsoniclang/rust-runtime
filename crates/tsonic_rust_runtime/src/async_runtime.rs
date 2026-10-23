use std::boxed::Box;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::{pin, Pin};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

struct ThreadWake<Notification> {
    thread: Thread,
    notification: Notification,
}

type NativeTask = Pin<Box<dyn Future<Output = ()>>>;

struct NativeTaskOwner {
    identity: usize,
    pending: VecDeque<NativeTask>,
    waker: Waker,
    wake: Arc<ThreadWake<AtomicBool>>,
}

std::thread_local! {
    static TASK_OWNER: RefCell<Option<NativeTaskOwner>> = const { RefCell::new(None) };
    static NEXT_TASK_OWNER: Cell<usize> = const { Cell::new(0) };
}

pub struct TaskScope<E> {
    identity: usize,
    failure: Rc<RefCell<Option<E>>>,
}

impl<E> Clone for TaskScope<E> {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity,
            failure: Rc::clone(&self.failure),
        }
    }
}

impl<E> TaskScope<E> {
    pub fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        let (waker, wake) = TASK_OWNER.with(|slot| {
            let owner = slot.borrow();
            let owner = owner
                .as_ref()
                .expect("native task driving requires an active executor owner");
            assert_eq!(
                owner.identity, self.identity,
                "native task driving requires its own active task scope"
            );
            (owner.waker.clone(), Arc::clone(&owner.wake))
        });
        drive_future(future, &waker, |context| {
            let progressed = poll_native_tasks(context);
            wake.notification.swap(false, Ordering::AcqRel) || progressed
        })
    }
}

struct NativeTaskOwnerGuard;

impl Drop for NativeTaskOwnerGuard {
    fn drop(&mut self) {
        let owner = TASK_OWNER.with(|slot| slot.borrow_mut().take());
        drop(owner);
    }
}

pub fn run_tasks<T, E: 'static>(entry: impl FnOnce(TaskScope<E>) -> Result<T, E>) -> Result<T, E> {
    let identity = NEXT_TASK_OWNER.with(|next| {
        let identity = next
            .get()
            .checked_add(1)
            .expect("native task owner identity exhausted");
        next.set(identity);
        identity
    });
    let wake = Arc::new(ThreadWake {
        thread: thread::current(),
        notification: AtomicBool::new(false),
    });
    let waker = Waker::from(Arc::clone(&wake));
    TASK_OWNER.with(|slot| {
        let mut owner = slot.borrow_mut();
        assert!(
            owner.is_none(),
            "native tasks require one active executor owner"
        );
        *owner = Some(NativeTaskOwner {
            identity,
            pending: VecDeque::new(),
            waker: waker.clone(),
            wake: Arc::clone(&wake),
        });
    });
    let guard = NativeTaskOwnerGuard;
    let failure = Rc::new(RefCell::new(None));
    let result = entry(TaskScope {
        identity,
        failure: Rc::clone(&failure),
    });
    drive_future(
        std::future::poll_fn(|_context| {
            if TASK_OWNER.with(|slot| {
                slot.borrow()
                    .as_ref()
                    .is_some_and(|owner| !owner.pending.is_empty())
            }) {
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        }),
        &waker,
        |context| {
            let progressed = poll_native_tasks(context);
            wake.notification.swap(false, Ordering::AcqRel) || progressed
        },
    );
    drop(guard);
    let spawned_error = failure.borrow_mut().take();
    match (result, spawned_error) {
        (Err(error), _) | (Ok(_), Some(error)) => Err(error),
        (Ok(value), None) => Ok(value),
    }
}

pub fn spawn<E: 'static>(
    scope: &TaskScope<E>,
    future: impl Future<Output = Result<(), E>> + 'static,
) {
    let waker = TASK_OWNER.with(|slot| {
        let owner = slot.borrow();
        let owner = owner
            .as_ref()
            .expect("native spawn requires an active executor owner");
        assert_eq!(
            owner.identity, scope.identity,
            "native spawn requires its own active task scope"
        );
        owner.waker.clone()
    });
    let failure = Rc::clone(&scope.failure);
    let mut task: NativeTask = Box::pin(async move {
        if let Err(error) = future.await {
            let mut recorded = failure.borrow_mut();
            if recorded.is_none() {
                *recorded = Some(error);
            }
        }
    });
    let mut context = Context::from_waker(&waker);
    if task.as_mut().poll(&mut context).is_pending() {
        TASK_OWNER.with(|slot| {
            slot.borrow_mut()
                .as_mut()
                .expect("a pending native task requires its active executor owner")
                .pending
                .push_back(task);
        });
    }
}

fn poll_native_tasks(context: &mut Context<'_>) -> bool {
    let count = TASK_OWNER.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(0, |owner| owner.pending.len())
    });
    let mut completed = false;
    for _index in 0..count {
        let task = TASK_OWNER.with(|slot| {
            slot.borrow_mut()
                .as_mut()
                .and_then(|owner| owner.pending.pop_front())
        });
        let Some(mut task) = task else { break };
        if task.as_mut().poll(context).is_ready() {
            completed = true;
        } else {
            TASK_OWNER.with(|slot| {
                slot.borrow_mut()
                    .as_mut()
                    .expect("a pending native task requires its active executor owner")
                    .pending
                    .push_back(task);
            });
        }
    }
    completed
        || TASK_OWNER.with(|slot| {
            slot.borrow()
                .as_ref()
                .is_some_and(|owner| owner.pending.len() > count)
        })
}

trait WakeNotification: Send + Sync {
    fn notify(&self);
}

impl WakeNotification for () {
    fn notify(&self) {}
}

impl WakeNotification for AtomicBool {
    fn notify(&self) {
        self.store(true, Ordering::Release);
    }
}

impl<Notification: WakeNotification> Wake for ThreadWake<Notification> {
    fn wake(self: Arc<Self>) {
        self.notification.notify();
        self.thread.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.notification.notify();
        self.thread.unpark();
    }
}

pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::from(Arc::new(ThreadWake {
        thread: thread::current(),
        notification: (),
    }));
    drive_future(future, &waker, |_context| false)
}

fn drive_future<T>(
    future: impl Future<Output = T>,
    waker: &Waker,
    mut poll_tasks: impl FnMut(&mut Context<'_>) -> bool,
) -> T {
    let mut future = pin!(future);
    let mut context = Context::from_waker(waker);
    loop {
        let result = future.as_mut().poll(&mut context);
        let progressed = poll_tasks(&mut context);
        match result {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                if !progressed {
                    thread::park();
                }
            }
        }
    }
}
