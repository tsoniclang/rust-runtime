use std::cell::OnceCell;

use crate::OwnedLocation;

pub struct ModuleCell<T> {
    location: OnceCell<OwnedLocation<T>>,
}

impl<T> ModuleCell<T> {
    pub const fn new() -> Self {
        Self {
            location: OnceCell::new(),
        }
    }
}

impl<T: 'static> ModuleCell<T> {
    pub fn initialized(value: T) -> Self
    where
        T: Clone,
    {
        let location = OnceCell::new();
        assert!(
            location.set(OwnedLocation::allocate(value)).is_ok(),
            "new Tsonic module binding must accept its initial value"
        );
        Self { location }
    }

    pub fn initialize(&self, value: T)
    where
        T: Clone,
    {
        assert!(
            self.location.set(OwnedLocation::allocate(value)).is_ok(),
            "Tsonic module binding initialized more than once"
        );
    }

    pub fn load(&self) -> T {
        self.require_location().load()
    }

    pub fn store(&self, value: T) {
        self.require_location().store(value);
    }

    pub fn location(&self) -> OwnedLocation<T> {
        self.require_location()
    }

    fn require_location(&self) -> OwnedLocation<T> {
        self.location
            .get()
            .cloned()
            .expect("Tsonic module binding read before initialization")
    }
}

impl<T> Default for ModuleCell<T> {
    fn default() -> Self {
        Self::new()
    }
}
