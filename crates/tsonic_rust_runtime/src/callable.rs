use std::rc::{Rc, Weak};

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

impl<TArguments: 'static, TResult: 'static> Callable<TArguments, TResult> {
    pub fn new(implementation: impl Fn(TArguments) -> TResult + 'static) -> Self {
        Self {
            implementation: Rc::new(implementation),
        }
    }

    pub fn recursive(implementation: impl Fn(Self, TArguments) -> TResult + 'static) -> Self {
        let slot = Rc::new(std::cell::RefCell::new(
            None::<Weak<dyn Fn(TArguments) -> TResult>>,
        ));
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
}
