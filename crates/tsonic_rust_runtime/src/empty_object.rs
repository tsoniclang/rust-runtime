use crate::{ObjectIdentity, ObjectIdentityCarrier};

#[derive(Clone, Debug)]
pub struct EmptyObject {
    identity: ObjectIdentity,
}

impl EmptyObject {
    pub fn new() -> Self {
        Self {
            identity: ObjectIdentity::new(),
        }
    }

    pub fn freeze(&self) -> Self {
        self.identity.freeze();
        self.clone()
    }

    pub fn is_frozen(&self) -> bool {
        self.identity.is_frozen()
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
