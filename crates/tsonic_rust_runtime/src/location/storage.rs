use alloc::rc::Rc;
use core::cell::RefCell;

pub(super) trait LocationAccess<T, E> {
    fn load(&self) -> Result<T, E>;
    fn store(&self, value: T) -> Result<(), E>;
}

struct Accessors<Read, Write> {
    read: Read,
    write: Write,
}

impl<T, E, Read, Write> LocationAccess<T, E> for Accessors<Read, Write>
where
    Read: Fn() -> Result<T, E>,
    Write: Fn(T) -> Result<(), E>,
{
    fn load(&self) -> Result<T, E> {
        (self.read)()
    }
    fn store(&self, value: T) -> Result<(), E> {
        (self.write)(value)
    }
}

pub(super) fn location_access<T, E>(
    read: impl Fn() -> Result<T, E> + 'static,
    write: impl Fn(T) -> Result<(), E> + 'static,
) -> Rc<dyn LocationAccess<T, E>> {
    Rc::new(Accessors { read, write })
}

pub(super) struct OwnedLocation<T>(pub RefCell<T>);

impl<T: Clone, E> LocationAccess<T, E> for OwnedLocation<T> {
    fn load(&self) -> Result<T, E> {
        Ok(self.0.borrow().clone())
    }
    fn store(&self, value: T) -> Result<(), E> {
        *self.0.borrow_mut() = value;
        Ok(())
    }
}
