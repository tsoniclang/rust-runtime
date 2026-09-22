use crate::{JsError, JsErrorKind, TsonicError};
use alloc::rc::{Rc, Weak};
use core::cell::Cell;
use core::fmt;

struct IdentityState {
    frozen: Cell<bool>,
}

pub struct ObjectIdentity {
    token: Rc<IdentityState>,
}

pub struct WeakObjectIdentity {
    token: Weak<IdentityState>,
}

pub trait ObjectIdentityCarrier {
    fn object_identity(&self) -> &ObjectIdentity;

    fn object_identity_key(&self) -> usize {
        self.object_identity().key()
    }
}

pub fn source_objects_equal<
    Left: ObjectIdentityCarrier + ?Sized,
    Right: ObjectIdentityCarrier + ?Sized,
>(
    left: &Left,
    right: &Right,
) -> bool {
    left.object_identity_key() == right.object_identity_key()
}

pub fn source_objects_not_equal<
    Left: ObjectIdentityCarrier + ?Sized,
    Right: ObjectIdentityCarrier + ?Sized,
>(
    left: &Left,
    right: &Right,
) -> bool {
    !source_objects_equal(left, right)
}

impl ObjectIdentityCarrier for ObjectIdentity {
    fn object_identity(&self) -> &ObjectIdentity {
        self
    }
}

impl ObjectIdentity {
    pub fn new() -> Self {
        Self {
            token: Rc::new(IdentityState {
                frozen: Cell::new(false),
            }),
        }
    }

    pub fn freeze(&self) {
        self.token.frozen.set(true);
    }

    pub fn is_frozen(&self) -> bool {
        self.token.frozen.get()
    }

    pub fn validate_data_write(&self) -> Result<(), TsonicError> {
        if self.is_frozen() {
            Err(JsError::new(
                JsErrorKind::TypeError,
                "Cannot assign to a frozen object's data property",
            )
            .into())
        } else {
            Ok(())
        }
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

pub fn freeze_object<T: ObjectIdentityCarrier + Clone>(value: &T) -> T {
    value.object_identity().freeze();
    value.clone()
}

pub fn object_is_frozen<T: ObjectIdentityCarrier + ?Sized>(value: &T) -> bool {
    value.object_identity().is_frozen()
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
