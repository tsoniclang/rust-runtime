use alloc::rc::Rc;
use core::cell::{OnceCell, RefCell};
use core::fmt;

use crate::{ObjectIdentity, ObjectIdentityCarrier};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EmptyObjectState;

struct ObjectHandleState<T> {
    value: RefCell<T>,
    identity: OnceCell<ObjectIdentity>,
}

pub struct ObjectHandle<T> {
    state: Rc<ObjectHandleState<T>>,
}

impl<T> ObjectHandle<T> {
    pub fn new(state: T) -> Self {
        Self {
            state: Rc::new(ObjectHandleState {
                value: RefCell::new(state),
                identity: OnceCell::new(),
            }),
        }
    }

    pub fn with<R>(&self, action: impl FnOnce(&T) -> R) -> R {
        action(&self.state.value.borrow())
    }

    pub fn with_mut<R>(&self, action: impl FnOnce(&mut T) -> R) -> R {
        action(&mut self.state.value.borrow_mut())
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.state, &right.state)
    }

    pub fn object_identity(&self) -> &ObjectIdentity {
        self.state.identity.get_or_init(ObjectIdentity::new)
    }
}

impl<T> ObjectIdentityCarrier for ObjectHandle<T> {
    fn object_identity(&self) -> &ObjectIdentity {
        self.object_identity()
    }
}

impl<T> Clone for ObjectHandle<T> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
        }
    }
}

impl<T> fmt::Debug for ObjectHandle<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObjectHandle")
    }
}

impl<T> PartialEq for ObjectHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl<T> Eq for ObjectHandle<T> {}
