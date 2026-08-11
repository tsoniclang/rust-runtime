use std::fmt;
use std::rc::Rc;

pub struct ObjectIdentity {
    token: Rc<()>,
}

impl ObjectIdentity {
    pub fn new() -> Self {
        Self { token: Rc::new(()) }
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.token, &right.token)
    }
}

impl Default for ObjectIdentity {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ObjectIdentity {
    fn clone(&self) -> Self {
        Self {
            token: Rc::clone(&self.token),
        }
    }
}

impl fmt::Debug for ObjectIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObjectIdentity")
    }
}

impl PartialEq for ObjectIdentity {
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl Eq for ObjectIdentity {}
