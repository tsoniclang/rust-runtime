use crate::{ObjectIdentity, ObjectIdentityCarrier};
use alloc::rc::Rc;
use core::cell::Cell;

#[derive(Clone, Debug)]
pub struct EmptyObject {
    identity: ObjectIdentity,
    frozen: Rc<Cell<bool>>,
}

impl EmptyObject {
    pub fn new() -> Self {
        Self {
            identity: ObjectIdentity::new(),
            frozen: Rc::new(Cell::new(false)),
        }
    }

    pub fn freeze(&self) -> Self {
        self.frozen.set(true);
        self.clone()
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen.get()
    }
}

impl Default for EmptyObject {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectIdentityCarrier for EmptyObject {
    fn object_identity(&self) -> &ObjectIdentity {
        &self.identity
    }
}

impl PartialEq for EmptyObject {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for EmptyObject {}
