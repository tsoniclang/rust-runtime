use alloc::rc::{Rc, Weak};
use core::cell::RefCell;

pub struct Callable<TArguments, TResult> {
    implementation: Rc<dyn Fn(TArguments) -> TResult>,
    identity: Rc<()>,
}

impl<TArguments, TResult> Clone for Callable<TArguments, TResult> {
    fn clone(&self) -> Self {
        Self {
            implementation: Rc::clone(&self.implementation),
            identity: Rc::clone(&self.identity),
        }
    }
}

impl<TArguments: 'static, TResult: 'static> Callable<TArguments, TResult> {
    pub fn new(implementation: impl Fn(TArguments) -> TResult + 'static) -> Self {
        Self {
            implementation: Rc::new(implementation),
            identity: Rc::new(()),
        }
    }

    pub fn recursive(implementation: impl Fn(Self, TArguments) -> TResult + 'static) -> Self {
        let slot = Rc::new(RefCell::new(None::<Weak<dyn Fn(TArguments) -> TResult>>));
        let identity = Rc::new(());
        let callback_slot = Rc::clone(&slot);
        let callback_identity = Rc::clone(&identity);
        let callback: Rc<dyn Fn(TArguments) -> TResult> = Rc::new(move |arguments| {
            let current = callback_slot
                .borrow()
                .as_ref()
                .and_then(Weak::upgrade)
                .expect("recursive callable must be initialized before invocation");
            implementation(
                Self {
                    implementation: current,
                    identity: Rc::clone(&callback_identity),
                },
                arguments,
            )
        });
        *slot.borrow_mut() = Some(Rc::downgrade(&callback));
        Self {
            implementation: callback,
            identity,
        }
    }

    pub fn call(&self, arguments: TArguments) -> TResult {
        (self.implementation)(arguments)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.identity, &right.identity)
    }

    pub fn identity_key(&self) -> usize {
        Rc::as_ptr(&self.identity) as usize
    }
}
