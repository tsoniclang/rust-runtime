use std::boxed::Box;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::{pin, Pin};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

struct ThreadWake<Notification> {
    thread: Thread,
    notification: Notification,
}

type NativeTask = Pin<Box<dyn Future<Output = ()>>>;

struct NativeTaskEntry {
    future: Option<NativeTask>,
    signal: Arc<NativeTaskWake>,
    waker: Waker,
}

struct NativeTaskScheduler {
    ready: Mutex<VecDeque<usize>>,
    wake: Arc<ThreadWake<AtomicBool>>,
}

struct NativeTaskWake {
    identity: usize,
    ready: AtomicBool,
    queued: AtomicBool,
    scheduler: Arc<NativeTaskScheduler>,
}

impl NativeTaskWake {
    fn enqueue_if_ready(&self) {
        if self.ready.load(Ordering::Acquire) && !self.queued.swap(true, Ordering::AcqRel) {
            self.scheduler
                .ready
                .lock()
                .expect("native task readiness lock poisoned")
                .push_back(self.identity);
            self.scheduler
                .wake
                .notification
                .store(true, Ordering::Release);
            self.scheduler.wake.thread.unpark();
        }
    }

    fn notify(&self) {
        self.ready.store(true, Ordering::Release);
        self.enqueue_if_ready();
    }
}

impl Wake for NativeTaskWake {
    fn wake(self: Arc<Self>) {
        self.notify();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.notify();
    }
}

struct NativeTaskOwner {
    identity: usize,
    next_task: usize,
    pending: HashMap<usize, NativeTaskEntry>,
    scheduler: Arc<NativeTaskScheduler>,
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
        drive_future(future, &waker, |_context| {
            let progressed = poll_native_tasks();
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
    let scheduler = Arc::new(NativeTaskScheduler {
        ready: Mutex::new(VecDeque::new()),
        wake: Arc::clone(&wake),
    });
    TASK_OWNER.with(|slot| {
        let mut owner = slot.borrow_mut();
        assert!(
            owner.is_none(),
            "native tasks require one active executor owner"
        );
        *owner = Some(NativeTaskOwner {
            identity,
            next_task: 0,
            pending: HashMap::new(),
            scheduler,
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
        |_context| {
            let progressed = poll_native_tasks();
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
    let (identity, scheduler) = TASK_OWNER.with(|slot| {
        let mut owner = slot.borrow_mut();
        let owner = owner
            .as_mut()
            .expect("native spawn requires an active executor owner");
        assert_eq!(
            owner.identity, scope.identity,
            "native spawn requires its own active task scope"
        );
        let identity = owner.next_task;
        owner.next_task = identity
            .checked_add(1)
            .expect("native task identity exhausted");
        (identity, Arc::clone(&owner.scheduler))
    });
    let failure = Rc::clone(&scope.failure);
    let task: NativeTask = Box::pin(async move {
        if let Err(error) = future.await {
            let mut recorded = failure.borrow_mut();
            if recorded.is_none() {
                *recorded = Some(error);
            }
        }
    });
    let signal = Arc::new(NativeTaskWake {
        identity,
        ready: AtomicBool::new(false),
        queued: AtomicBool::new(false),
        scheduler,
    });
    let entry = NativeTaskEntry {
        future: Some(task),
        waker: Waker::from(Arc::clone(&signal)),
        signal,
    };
    TASK_OWNER.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .expect("a native task requires its active executor owner")
            .pending
            .insert(identity, entry);
    });
    poll_native_task(identity);
}

fn poll_native_task(identity: usize) -> bool {
    let selected = TASK_OWNER.with(|slot| {
        let mut owner = slot.borrow_mut();
        let entry = owner.as_mut()?.pending.get_mut(&identity)?;
        entry.signal.queued.store(false, Ordering::Release);
        let future = entry.future.take()?;
        entry.signal.ready.store(false, Ordering::Release);
        Some((future, entry.waker.clone(), Arc::clone(&entry.signal)))
    });
    let Some((mut task, waker, signal)) = selected else {
        return false;
    };
    let mut context = Context::from_waker(&waker);
    if task.as_mut().poll(&mut context).is_ready() {
        let completed = TASK_OWNER.with(|slot| {
            slot.borrow_mut()
                .as_mut()
                .and_then(|owner| owner.pending.remove(&identity))
        });
        drop(completed);
        true
    } else {
        TASK_OWNER.with(|slot| {
            slot.borrow_mut()
                .as_mut()
                .expect("a pending native task requires its active executor owner")
                .pending
                .get_mut(&identity)
                .expect("a running native task retains its identity")
                .future = Some(task);
        });
        signal.enqueue_if_ready();
        false
    }
}

fn poll_native_tasks() -> bool {
    let scheduler = TASK_OWNER.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|owner| Arc::clone(&owner.scheduler))
    });
    let Some(scheduler) = scheduler else {
        return false;
    };
    let count = scheduler
        .ready
        .lock()
        .expect("native task readiness lock poisoned")
        .len();
    let mut completed = false;
    for _index in 0..count {
        let identity = scheduler
            .ready
            .lock()
            .expect("native task readiness lock poisoned")
            .pop_front();
        let Some(identity) = identity else { break };
        completed |= poll_native_task(identity);
    }
    completed
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
