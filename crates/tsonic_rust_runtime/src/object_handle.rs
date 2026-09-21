use alloc::rc::Rc;
use core::cell::{OnceCell, RefCell};
use core::fmt;

use crate::{ObjectIdentity, ObjectIdentityCarrier, TsonicError};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EmptyObjectState;

pub struct ObjectState<T> {
    value: RefCell<T>,
}

impl<T> ObjectState<T> {
    pub fn new(state: T) -> Self {
        Self {
            value: RefCell::new(state),
        }
    }

    pub fn with<R>(&self, action: impl FnOnce(&T) -> R) -> R {
        action(&self.value.borrow())
    }

    pub fn with_mut<R>(&self, action: impl FnOnce(&mut T) -> R) -> R {
        action(&mut self.value.borrow_mut())
    }
}

struct ObjectHandleState<T, Context> {
    value: ObjectState<T>,
    context: Context,
    identity: OnceCell<ObjectIdentity>,
}

pub struct ObjectHandle<T, Context = ()> {
    state: Rc<ObjectHandleState<T, Context>>,
}

impl<T> ObjectHandle<T> {
    pub fn with_identity(state: T, identity: ObjectIdentity) -> Self {
        Self {
            state: Rc::new(ObjectHandleState {
                value: ObjectState::new(state),
                context: (),
                identity: OnceCell::from(identity),
            }),
        }
    }

    pub fn new(state: T) -> Self {
        Self::with_context(state, ())
    }
}

impl<T, Context> ObjectHandle<T, Context> {
    pub fn with_context(state: T, context: Context) -> Self {
        Self {
            state: Rc::new(ObjectHandleState {
                value: ObjectState::new(state),
                context,
                identity: OnceCell::new(),
            }),
        }
    }

    pub fn context(&self) -> &Context {
        &self.state.context
    }

    pub fn with<R>(&self, action: impl FnOnce(&T) -> R) -> R {
        self.state.value.with(action)
    }

    pub fn with_mut<R>(&self, action: impl FnOnce(&mut T) -> R) -> R {
        self.state.value.with_mut(action)
    }

    pub fn validate_data_write(&self) -> Result<(), TsonicError> {
        match self.state.identity.get() {
            Some(identity) => identity.validate_data_write(),
            None => Ok(()),
        }
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.state, &right.state)
            || match (left.state.identity.get(), right.state.identity.get()) {
                (Some(left), Some(right)) => ObjectIdentity::same(left, right),
                _ => false,
            }
    }

    pub fn object_identity(&self) -> &ObjectIdentity {
        self.state.identity.get_or_init(ObjectIdentity::new)
    }
}

impl<T, Context> ObjectIdentityCarrier for ObjectHandle<T, Context> {
    fn object_identity(&self) -> &ObjectIdentity {
        self.object_identity()
    }
}

impl<T, Context> Clone for ObjectHandle<T, Context> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
        }
    }
}

impl<T, Context> fmt::Debug for ObjectHandle<T, Context> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObjectHandle")
    }
}

impl<T, Context> PartialEq for ObjectHandle<T, Context> {
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl<T, Context> Eq for ObjectHandle<T, Context> {}
