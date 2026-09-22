use alloc::rc::{Rc, Weak};

pub trait CallableImplementation<TArguments, TResult> {
    fn invoke(&self, arguments: TArguments) -> TResult;
}

impl<TArguments, TResult, TFunction: Fn(TArguments) -> TResult>
    CallableImplementation<TArguments, TResult> for TFunction
{
    fn invoke(&self, arguments: TArguments) -> TResult {
        self(arguments)
    }
}

struct Recursive<TFunction, TArguments, TResult> {
    implementation: TFunction,
    owner: Weak<Self>,
    signature: core::marker::PhantomData<fn(TArguments) -> TResult>,
}

impl<TArguments: 'static, TResult: 'static, TFunction> CallableImplementation<TArguments, TResult>
    for Recursive<TFunction, TArguments, TResult>
where
    TFunction: Fn(Callable<TArguments, TResult>, TArguments) -> TResult + 'static,
{
    fn invoke(&self, arguments: TArguments) -> TResult {
        let owner = self
            .owner
            .upgrade()
            .expect("an invoked callable has a live owner");
        (self.implementation)(
            Callable {
                implementation: owner,
            },
            arguments,
        )
    }
}

pub struct Callable<TArguments, TResult> {
    implementation: Rc<dyn CallableImplementation<TArguments, TResult>>,
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
        Self {
            implementation: Rc::new_cyclic(|owner| Recursive {
                implementation,
                owner: owner.clone(),
                signature: core::marker::PhantomData,
            }),
        }
    }

    pub fn from_shared<TImplementation: CallableImplementation<TArguments, TResult> + 'static>(
        implementation: Rc<TImplementation>,
    ) -> Self {
        Self { implementation }
    }

    pub fn call(&self, arguments: TArguments) -> TResult {
        self.implementation.invoke(arguments)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.implementation, &right.implementation)
    }

    pub fn identity_key(&self) -> usize {
        Rc::as_ptr(&self.implementation) as *const () as usize
    }
}
