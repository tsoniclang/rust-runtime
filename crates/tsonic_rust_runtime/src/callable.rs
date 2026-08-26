use std::cell::OnceCell as LocalOnceCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak as LocalWeak};
use std::sync::{Arc, OnceLock as ThreadedOnceCell, Weak as ThreadedWeak};

pub type LocalAsyncFuture<'a, TResult> = Pin<Box<dyn Future<Output = TResult> + 'a>>;
pub type ThreadedAsyncFuture<TResult> = Pin<Box<dyn Future<Output = TResult> + Send + 'static>>;

pub struct BorrowedLocalCallable<'a, TArguments, TResult> {
    implementation: Rc<dyn Fn(TArguments) -> TResult + 'a>,
}

impl<'a, TArguments, TResult> Clone for BorrowedLocalCallable<'a, TArguments, TResult> {
    fn clone(&self) -> Self {
        Self {
            implementation: Rc::clone(&self.implementation),
        }
    }
}

impl<'a, TArguments: 'a, TResult: 'a> BorrowedLocalCallable<'a, TArguments, TResult> {
    pub fn new(implementation: impl Fn(TArguments) -> TResult + 'a) -> Self {
        Self {
            implementation: Rc::new(implementation),
        }
    }

    pub fn recursive(implementation: impl Fn(Self, TArguments) -> TResult + 'a) -> Self {
        let slot = Rc::new(LocalOnceCell::<LocalWeak<dyn Fn(TArguments) -> TResult + 'a>>::new());
        let callback_slot = Rc::clone(&slot);
        let callback: Rc<dyn Fn(TArguments) -> TResult + 'a> = Rc::new(move |arguments| {
            let current = callback_slot
                .get()
                .and_then(LocalWeak::upgrade)
                .expect("recursive local callable must be initialized before invocation");
            implementation(
                Self {
                    implementation: current,
                },
                arguments,
            )
        });
        slot.set(Rc::downgrade(&callback))
            .unwrap_or_else(|_| panic!("recursive local callable initialized more than once"));
        Self {
            implementation: callback,
        }
    }

    pub fn call(&self, arguments: TArguments) -> TResult {
        (self.implementation)(arguments)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.implementation, &right.implementation)
    }
}

pub struct OwnedLocalCallable<TArguments, TResult> {
    implementation: BorrowedLocalCallable<'static, TArguments, TResult>,
}

impl<TArguments, TResult> Clone for OwnedLocalCallable<TArguments, TResult> {
    fn clone(&self) -> Self {
        Self {
            implementation: self.implementation.clone(),
        }
    }
}

impl<TArguments: 'static, TResult: 'static> OwnedLocalCallable<TArguments, TResult> {
    pub fn new(implementation: impl Fn(TArguments) -> TResult + 'static) -> Self {
        Self {
            implementation: BorrowedLocalCallable::new(implementation),
        }
    }

    pub fn recursive(implementation: impl Fn(Self, TArguments) -> TResult + 'static) -> Self {
        Self {
            implementation: BorrowedLocalCallable::recursive(move |current, arguments| {
                implementation(
                    Self {
                        implementation: current,
                    },
                    arguments,
                )
            }),
        }
    }

    pub fn call(&self, arguments: TArguments) -> TResult {
        self.implementation.call(arguments)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        BorrowedLocalCallable::same(&left.implementation, &right.implementation)
    }
}

pub struct ThreadedCallable<TArguments, TResult> {
    implementation: Arc<dyn Fn(TArguments) -> TResult + Send + Sync + 'static>,
}

impl<TArguments, TResult> Clone for ThreadedCallable<TArguments, TResult> {
    fn clone(&self) -> Self {
        Self {
            implementation: Arc::clone(&self.implementation),
        }
    }
}

impl<TArguments: 'static, TResult: 'static> ThreadedCallable<TArguments, TResult> {
    pub fn new(implementation: impl Fn(TArguments) -> TResult + Send + Sync + 'static) -> Self {
        Self {
            implementation: Arc::new(implementation),
        }
    }

    pub fn recursive(
        implementation: impl Fn(Self, TArguments) -> TResult + Send + Sync + 'static,
    ) -> Self {
        type ThreadedImplementation<TArguments, TResult> =
            dyn Fn(TArguments) -> TResult + Send + Sync + 'static;
        let slot = Arc::new(ThreadedOnceCell::<
            ThreadedWeak<ThreadedImplementation<TArguments, TResult>>,
        >::new());
        let callback_slot = Arc::clone(&slot);
        let callback: Arc<ThreadedImplementation<TArguments, TResult>> =
            Arc::new(move |arguments| {
                let current = callback_slot
                    .get()
                    .and_then(ThreadedWeak::upgrade)
                    .expect("recursive threaded callable must be initialized before invocation");
                implementation(
                    Self {
                        implementation: current,
                    },
                    arguments,
                )
            });
        slot.set(Arc::downgrade(&callback))
            .unwrap_or_else(|_| panic!("recursive threaded callable initialized more than once"));
        Self {
            implementation: callback,
        }
    }

    pub fn call(&self, arguments: TArguments) -> TResult {
        (self.implementation)(arguments)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.implementation, &right.implementation)
    }
}

pub struct BorrowedLocalAsyncCallable<'a, TArguments, TResult> {
    implementation: Rc<dyn Fn(TArguments) -> LocalAsyncFuture<'a, TResult> + 'a>,
}

impl<'a, TArguments, TResult> Clone for BorrowedLocalAsyncCallable<'a, TArguments, TResult> {
    fn clone(&self) -> Self {
        Self {
            implementation: Rc::clone(&self.implementation),
        }
    }
}

impl<'a, TArguments: 'a, TResult: 'a> BorrowedLocalAsyncCallable<'a, TArguments, TResult> {
    pub fn new<TImplementation, TFuture>(implementation: TImplementation) -> Self
    where
        TImplementation: Fn(TArguments) -> TFuture + 'a,
        TFuture: Future<Output = TResult> + 'a,
    {
        Self {
            implementation: Rc::new(move |arguments| Box::pin(implementation(arguments))),
        }
    }

    pub fn recursive<TImplementation, TFuture>(implementation: TImplementation) -> Self
    where
        TImplementation: Fn(Self, TArguments) -> TFuture + 'a,
        TFuture: Future<Output = TResult> + 'a,
    {
        type LocalAsyncImplementation<'a, TArguments, TResult> =
            dyn Fn(TArguments) -> LocalAsyncFuture<'a, TResult> + 'a;
        let slot = Rc::new(LocalOnceCell::<
            LocalWeak<LocalAsyncImplementation<'a, TArguments, TResult>>,
        >::new());
        let callback_slot = Rc::clone(&slot);
        let callback: Rc<LocalAsyncImplementation<'a, TArguments, TResult>> =
            Rc::new(move |arguments| {
                let current = callback_slot
                    .get()
                    .and_then(LocalWeak::upgrade)
                    .expect("recursive local async callable must be initialized before invocation");
                Box::pin(implementation(
                    Self {
                        implementation: current,
                    },
                    arguments,
                ))
            });
        slot.set(Rc::downgrade(&callback)).unwrap_or_else(|_| {
            panic!("recursive local async callable initialized more than once")
        });
        Self {
            implementation: callback,
        }
    }

    pub fn call(&self, arguments: TArguments) -> LocalAsyncFuture<'a, TResult> {
        (self.implementation)(arguments)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.implementation, &right.implementation)
    }
}

pub struct OwnedLocalAsyncCallable<TArguments, TResult> {
    implementation: BorrowedLocalAsyncCallable<'static, TArguments, TResult>,
}

impl<TArguments, TResult> Clone for OwnedLocalAsyncCallable<TArguments, TResult> {
    fn clone(&self) -> Self {
        Self {
            implementation: self.implementation.clone(),
        }
    }
}

impl<TArguments: 'static, TResult: 'static> OwnedLocalAsyncCallable<TArguments, TResult> {
    pub fn new<TImplementation, TFuture>(implementation: TImplementation) -> Self
    where
        TImplementation: Fn(TArguments) -> TFuture + 'static,
        TFuture: Future<Output = TResult> + 'static,
    {
        Self {
            implementation: BorrowedLocalAsyncCallable::new(implementation),
        }
    }

    pub fn recursive<TImplementation, TFuture>(implementation: TImplementation) -> Self
    where
        TImplementation: Fn(Self, TArguments) -> TFuture + 'static,
        TFuture: Future<Output = TResult> + 'static,
    {
        Self {
            implementation: BorrowedLocalAsyncCallable::recursive(move |current, arguments| {
                implementation(
                    Self {
                        implementation: current,
                    },
                    arguments,
                )
            }),
        }
    }

    pub fn call(&self, arguments: TArguments) -> LocalAsyncFuture<'static, TResult> {
        self.implementation.call(arguments)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        BorrowedLocalAsyncCallable::same(&left.implementation, &right.implementation)
    }
}

pub struct ThreadedAsyncCallable<TArguments, TResult> {
    implementation: Arc<dyn Fn(TArguments) -> ThreadedAsyncFuture<TResult> + Send + Sync + 'static>,
}

impl<TArguments, TResult> Clone for ThreadedAsyncCallable<TArguments, TResult> {
    fn clone(&self) -> Self {
        Self {
            implementation: Arc::clone(&self.implementation),
        }
    }
}

impl<TArguments: 'static, TResult: 'static> ThreadedAsyncCallable<TArguments, TResult> {
    pub fn new<TImplementation, TFuture>(implementation: TImplementation) -> Self
    where
        TImplementation: Fn(TArguments) -> TFuture + Send + Sync + 'static,
        TFuture: Future<Output = TResult> + Send + 'static,
    {
        Self {
            implementation: Arc::new(move |arguments| Box::pin(implementation(arguments))),
        }
    }

    pub fn recursive<TImplementation, TFuture>(implementation: TImplementation) -> Self
    where
        TImplementation: Fn(Self, TArguments) -> TFuture + Send + Sync + 'static,
        TFuture: Future<Output = TResult> + Send + 'static,
    {
        type ThreadedAsyncImplementation<TArguments, TResult> =
            dyn Fn(TArguments) -> ThreadedAsyncFuture<TResult> + Send + Sync + 'static;
        let slot = Arc::new(ThreadedOnceCell::<
            ThreadedWeak<ThreadedAsyncImplementation<TArguments, TResult>>,
        >::new());
        let callback_slot = Arc::clone(&slot);
        let callback: Arc<ThreadedAsyncImplementation<TArguments, TResult>> =
            Arc::new(move |arguments| {
                let current = callback_slot.get().and_then(ThreadedWeak::upgrade).expect(
                    "recursive threaded async callable must be initialized before invocation",
                );
                Box::pin(implementation(
                    Self {
                        implementation: current,
                    },
                    arguments,
                ))
            });
        slot.set(Arc::downgrade(&callback)).unwrap_or_else(|_| {
            panic!("recursive threaded async callable initialized more than once")
        });
        Self {
            implementation: callback,
        }
    }

    pub fn call(&self, arguments: TArguments) -> ThreadedAsyncFuture<TResult> {
        (self.implementation)(arguments)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.implementation, &right.implementation)
    }
}
