use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EmptyObjectState;

pub struct LocalObjectHandle<T> {
    state: Rc<RefCell<T>>,
}

impl<T> LocalObjectHandle<T> {
    pub fn new(state: T) -> Self {
        Self {
            state: Rc::new(RefCell::new(state)),
        }
    }

    pub fn with<R>(&self, action: impl FnOnce(&T) -> R) -> R {
        action(&self.state.borrow())
    }

    pub fn with_mut<R>(&self, action: impl FnOnce(&mut T) -> R) -> R {
        action(&mut self.state.borrow_mut())
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.state, &right.state)
    }
}

impl<T> Clone for LocalObjectHandle<T> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
        }
    }
}

impl<T> fmt::Debug for LocalObjectHandle<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LocalObjectHandle")
    }
}

impl<T> PartialEq for LocalObjectHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl<T> Eq for LocalObjectHandle<T> {}

pub struct ThreadedObjectHandle<T> {
    state: Arc<RwLock<T>>,
}

impl<T> ThreadedObjectHandle<T> {
    pub fn new(state: T) -> Self {
        Self {
            state: Arc::new(RwLock::new(state)),
        }
    }

    pub fn with<R>(&self, action: impl FnOnce(&T) -> R) -> R {
        let state = self
            .state
            .read()
            .expect("threaded object read lock was poisoned");
        action(&state)
    }

    pub fn with_mut<R>(&self, action: impl FnOnce(&mut T) -> R) -> R {
        let mut state = self
            .state
            .write()
            .expect("threaded object write lock was poisoned");
        action(&mut state)
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.state, &right.state)
    }
}

impl<T> Clone for ThreadedObjectHandle<T> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<T> fmt::Debug for ThreadedObjectHandle<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ThreadedObjectHandle")
    }
}

impl<T> PartialEq for ThreadedObjectHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl<T> Eq for ThreadedObjectHandle<T> {}
