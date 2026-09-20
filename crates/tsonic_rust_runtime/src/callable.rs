use alloc::rc::{Rc, Weak};
use core::cell::RefCell;

pub struct Callable<TArguments, TResult> {
    implementation: Rc<dyn Fn(TArguments) -> TResult>,
}

impl<TArguments, TResult> Clone for Callable<TArguments, TResult> {
    fn clone(&self) -> Self {
        Self {
            implementation: Rc::clone(&self.implementation),
        }
    }
}

impl<TArguments, TResult> PartialEq for Callable<TArguments, TResult> {
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl<TArguments, TResult> Eq for Callable<TArguments, TResult> {}

impl<TArguments, TResult> Callable<TArguments, TResult> {
    pub fn new(implementation: impl Fn(TArguments) -> TResult + 'static) -> Self {
        Self {
            implementation: Rc::new(implementation),
        }
    }

    pub fn recursive(implementation: impl Fn(Self, TArguments) -> TResult + 'static) -> Self
    where
        TArguments: 'static,
        TResult: 'static,
    {
        let slot = Rc::new(RefCell::new(None::<Weak<dyn Fn(TArguments) -> TResult>>));
        let callback_slot = Rc::clone(&slot);
        let callback: Rc<dyn Fn(TArguments) -> TResult> = Rc::new(move |arguments| {
            let current = callback_slot
                .borrow()
                .as_ref()
                .and_then(Weak::upgrade)
                .expect("recursive callable must be initialized before invocation");
            implementation(
                Self {
                    implementation: current,
                },
                arguments,
            )
        });
        *slot.borrow_mut() = Some(Rc::downgrade(&callback));
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

    pub fn identity_key(&self) -> usize {
        Rc::as_ptr(&self.implementation) as *const () as usize
    }
}
