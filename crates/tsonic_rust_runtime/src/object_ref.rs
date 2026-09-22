use alloc::rc::Rc;
use core::cell::OnceCell;
use core::fmt;

use crate::{ObjectIdentity, ObjectIdentityCarrier};

pub struct ObjectRefState<T, Context = ()> {
    value: T,
    context: Context,
    identity: OnceCell<ObjectIdentity>,
}

pub struct ObjectRef<T, Context = ()> {
    state: Rc<ObjectRefState<T, Context>>,
}

impl<T> ObjectRef<T> {
    pub fn new(state: T) -> Self {
        Self::with_context(state, ())
    }
}

impl<T, Context> ObjectRef<T, Context> {
    pub fn with_context(state: T, context: Context) -> Self {
        Self {
            state: Rc::new(ObjectRefState {
                value: state,
                context,
                identity: OnceCell::new(),
            }),
        }
    }

    pub fn context(&self) -> &Context {
        self.state.context()
    }

    pub fn with<R>(&self, action: impl FnOnce(&T) -> R) -> R {
        self.state.with(action)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.state, &right.state)
    }

    pub fn object_identity(&self) -> &ObjectIdentity {
        self.state.object_identity()
    }

    pub fn into_shared(self) -> Rc<ObjectRefState<T, Context>> {
        self.state
    }

    pub fn from_shared(state: Rc<ObjectRefState<T, Context>>) -> Self {
        Self { state }
    }
}

impl<T, Context> ObjectRefState<T, Context> {
    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn with<R>(&self, action: impl FnOnce(&T) -> R) -> R {
        action(&self.value)
    }
}

impl<T, Context> ObjectIdentityCarrier for ObjectRefState<T, Context> {
    fn object_identity(&self) -> &ObjectIdentity {
        self.identity.get_or_init(ObjectIdentity::new)
    }
}

impl<T, Context> ObjectIdentityCarrier for ObjectRef<T, Context> {
    fn object_identity(&self) -> &ObjectIdentity {
        self.object_identity()
    }
}

impl<T, Context> Clone for ObjectRef<T, Context> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
        }
    }
}

impl<T, Context> fmt::Debug for ObjectRef<T, Context> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObjectRef")
    }
}

impl<T, Context> PartialEq for ObjectRef<T, Context> {
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl<T, Context> Eq for ObjectRef<T, Context> {}
