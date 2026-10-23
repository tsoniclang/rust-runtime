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

struct ThreadWake {
    thread: Thread,
}

type NativeTask = Pin<Box<dyn Future<Output = ()>>>;

struct NativeTaskEntry {
    future: Option<NativeTask>,
    signal: Arc<NativeTaskWake>,
    waker: Waker,
}

struct NativeTaskScheduler {
    ready: Mutex<VecDeque<usize>>,
    driver: Mutex<Option<Arc<Waker>>>,
}

impl NativeTaskScheduler {
    fn register_driver(&self, waker: &Waker) {
        if self
            .driver
            .lock()
            .expect("native task driver lock poisoned")
            .as_ref()
            .is_some_and(|current| current.will_wake(waker))
        {
            return;
        }
        let replacement = Arc::new(waker.clone());
        let previous = self
            .driver
            .lock()
            .expect("native task driver lock poisoned")
            .replace(replacement);
        drop(previous);
    }

    fn wake_driver(&self) {
        let driver = self
            .driver
            .lock()
            .expect("native task driver lock poisoned")
            .as_ref()
            .map(Arc::clone);
        if let Some(driver) = driver {
            driver.wake_by_ref();
        }
    }
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
            self.scheduler.wake_driver();
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

struct NativeTaskOwnerGuard;

impl Drop for NativeTaskOwnerGuard {
    fn drop(&mut self) {
        let owner = TASK_OWNER.with(|slot| slot.borrow_mut().take());
        drop(owner);
    }
}

pub async fn run_tasks<T, E: 'static, Root: Future<Output = Result<T, E>>>(
    entry: impl FnOnce(TaskScope<E>) -> Root,
) -> Result<T, E> {
    let identity = NEXT_TASK_OWNER.with(|next| {
        let identity = next
            .get()
            .checked_add(1)
            .expect("native task owner identity exhausted");
        next.set(identity);
        identity
    });
    let scheduler = Arc::new(NativeTaskScheduler {
        ready: Mutex::new(VecDeque::new()),
        driver: Mutex::new(None),
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
            scheduler: Arc::clone(&scheduler),
        });
    });
    let guard = NativeTaskOwnerGuard;
    let failure = Rc::new(RefCell::new(None));
    let mut main = pin!(Some(entry(TaskScope {
        identity,
        failure: Rc::clone(&failure),
    })));
    let mut result = None;
    std::future::poll_fn(|context| {
        scheduler.register_driver(context.waker());
        if let Some(future) = main.as_mut().as_pin_mut() {
            if let Poll::Ready(output) = future.poll(context) {
                result = Some(output);
                main.as_mut().set(None);
            }
        }
        let completed = poll_native_tasks(&scheduler);
        let pending = TASK_OWNER.with(|slot| {
            !slot
                .borrow()
                .as_ref()
                .expect("native task driving requires its active executor owner")
                .pending
                .is_empty()
        });
        if result.is_some() && !pending {
            Poll::Ready(())
        } else {
            if completed && result.is_none() {
                context.waker().wake_by_ref();
            }
            Poll::Pending
        }
    })
    .await;
    drop(guard);
    let spawned_error = failure.borrow_mut().take();
    match (
        result.expect("a drained native task root completed its main future"),
        spawned_error,
    ) {
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

fn poll_native_tasks(scheduler: &NativeTaskScheduler) -> bool {
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

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.thread.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.thread.unpark();
    }
}

pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::from(Arc::new(ThreadWake {
        thread: thread::current(),
    }));
    let mut future = pin!(future);
    let mut context = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}
