use std::fmt;
use std::rc::{Rc, Weak};

pub struct ObjectIdentity {
    token: Rc<()>,
}

pub struct WeakObjectIdentity {
    token: Weak<()>,
}

pub trait ObjectIdentityCarrier {
    fn object_identity(&self) -> &ObjectIdentity;
}

impl ObjectIdentity {
    pub fn new() -> Self {
        Self { token: Rc::new(()) }
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.token, &right.token)
    }

    pub fn downgrade(&self) -> WeakObjectIdentity {
        WeakObjectIdentity {
            token: Rc::downgrade(&self.token),
        }
    }

    pub fn key(&self) -> usize {
        Rc::as_ptr(&self.token) as usize
    }
}

impl WeakObjectIdentity {
    pub fn is_alive(&self) -> bool {
        self.token.strong_count() != 0
    }

    pub fn key(&self) -> usize {
        self.token.as_ptr() as usize
    }

    pub fn matches(&self, identity: &ObjectIdentity) -> bool {
        self.token.ptr_eq(&Rc::downgrade(&identity.token))
    }
}

impl Clone for WeakObjectIdentity {
    fn clone(&self) -> Self {
        Self {
            token: self.token.clone(),
        }
    }
}

impl fmt::Debug for WeakObjectIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WeakObjectIdentity")
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
