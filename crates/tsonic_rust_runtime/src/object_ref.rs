use alloc::rc::Rc;
use core::cell::OnceCell;
use core::fmt;

use crate::{ObjectIdentity, ObjectIdentityCarrier};

struct ObjectRefState<T> {
    value: T,
    identity: OnceCell<ObjectIdentity>,
}

pub struct ObjectRef<T> {
    state: Rc<ObjectRefState<T>>,
}

impl<T> ObjectRef<T> {
    pub fn new(state: T) -> Self {
        Self {
            state: Rc::new(ObjectRefState {
                value: state,
                identity: OnceCell::new(),
            }),
        }
    }

    pub fn with<R>(&self, action: impl FnOnce(&T) -> R) -> R {
        action(&self.state.value)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.state, &right.state)
    }

    pub fn object_identity(&self) -> &ObjectIdentity {
        self.state.identity.get_or_init(ObjectIdentity::new)
    }
}

impl<T> ObjectIdentityCarrier for ObjectRef<T> {
    fn object_identity(&self) -> &ObjectIdentity {
        self.object_identity()
    }
}

impl<T> Clone for ObjectRef<T> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
        }
    }
}

impl<T> fmt::Debug for ObjectRef<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObjectRef")
    }
}

impl<T> PartialEq for ObjectRef<T> {
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl<T> Eq for ObjectRef<T> {}
