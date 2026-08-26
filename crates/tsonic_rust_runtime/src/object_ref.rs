use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

pub struct LocalObjectRef<T> {
    state: Rc<T>,
}

impl<T> LocalObjectRef<T> {
    pub fn new(state: T) -> Self {
        Self {
            state: Rc::new(state),
        }
    }

    pub fn with<R>(&self, action: impl FnOnce(&T) -> R) -> R {
        action(&self.state)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.state, &right.state)
    }
}

impl<T> Clone for LocalObjectRef<T> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
        }
    }
}

impl<T> fmt::Debug for LocalObjectRef<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LocalObjectRef")
    }
}

impl<T> PartialEq for LocalObjectRef<T> {
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl<T> Eq for LocalObjectRef<T> {}

pub struct ThreadedObjectRef<T> {
    state: Arc<T>,
}

impl<T> ThreadedObjectRef<T> {
    pub fn new(state: T) -> Self {
        Self {
            state: Arc::new(state),
        }
    }

    pub fn with<R>(&self, action: impl FnOnce(&T) -> R) -> R {
        action(&self.state)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.state, &right.state)
    }
}

impl<T> Clone for ThreadedObjectRef<T> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<T> fmt::Debug for ThreadedObjectRef<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ThreadedObjectRef")
    }
}

impl<T> PartialEq for ThreadedObjectRef<T> {
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl<T> Eq for ThreadedObjectRef<T> {}
