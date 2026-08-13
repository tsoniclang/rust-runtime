use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EmptyObjectState;

pub struct ObjectHandle<T> {
    state: Rc<RefCell<T>>,
}

impl<T> ObjectHandle<T> {
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

impl<T> Clone for ObjectHandle<T> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
        }
    }
}

impl<T> fmt::Debug for ObjectHandle<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.state.borrow().fmt(formatter)
    }
}

impl<T> PartialEq for ObjectHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl<T> Eq for ObjectHandle<T> {}
