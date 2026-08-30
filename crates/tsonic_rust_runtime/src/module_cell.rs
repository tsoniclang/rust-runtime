use core::cell::RefCell;

use crate::Location;

pub struct ModuleCell<T> {
    location: RefCell<Option<Location<T>>>,
}

impl<T> ModuleCell<T> {
    pub const fn new() -> Self {
        Self {
            location: RefCell::new(None),
        }
    }
}

impl<T: Clone + 'static> ModuleCell<T> {
    pub fn initialized(value: T) -> Self {
        Self {
            location: RefCell::new(Some(Location::allocate(value))),
        }
    }

    pub fn initialize(&self, value: T) {
        let mut location = self.location.borrow_mut();
        assert!(
            location.is_none(),
            "Tsonic module binding initialized more than once"
        );
        *location = Some(Location::allocate(value));
    }

    pub fn load(&self) -> T {
        self.require_location().load()
    }

    pub fn store(&self, value: T) {
        self.require_location().store(value);
    }

    pub fn location(&self) -> Location<T> {
        self.require_location()
    }

    fn require_location(&self) -> Location<T> {
        self.location
            .borrow()
            .as_ref()
            .cloned()
            .expect("Tsonic module binding read before initialization")
    }
}

impl<T> Default for ModuleCell<T> {
    fn default() -> Self {
        Self::new()
    }
}
