use core::cell::RefCell;

use crate::Location;

pub struct ModuleCell<T> {
    state: RefCell<ModuleState<T>>,
}

enum ModuleState<T> {
    Undeclared,
    Declared,
    Assigned(Location<T>),
}

impl<T> ModuleCell<T> {
    pub const fn new() -> Self {
        Self {
            state: RefCell::new(ModuleState::Undeclared),
        }
    }

    pub fn declare(&self) {
        let mut state = self.state.borrow_mut();
        assert!(matches!(*state, ModuleState::Undeclared), "Tsonic module binding declared more than once");
        *state = ModuleState::Declared;
    }
}

impl<T: Clone + 'static> ModuleCell<T> {
    pub fn initialized(value: T) -> Self {
        Self {
            state: RefCell::new(ModuleState::Assigned(Location::allocate(value))),
        }
    }

    pub fn initialize(&self, value: T) {
        let mut state = self.state.borrow_mut();
        assert!(
            matches!(*state, ModuleState::Undeclared),
            "Tsonic module binding initialized more than once"
        );
        *state = ModuleState::Assigned(Location::allocate(value));
    }

    pub fn load(&self) -> T {
        self.require_location().load()
    }

    pub fn store(&self, value: T) {
        let location = {
            let mut state = self.state.borrow_mut();
            match &*state {
                ModuleState::Undeclared => panic!("Tsonic module binding written before declaration"),
                ModuleState::Declared => {
                    *state = ModuleState::Assigned(Location::allocate(value));
                    return;
                }
                ModuleState::Assigned(location) => location.clone(),
            }
        };
        location.store(value);
    }

    pub fn location(&self) -> Location<T> {
        self.require_location()
    }

    fn require_location(&self) -> Location<T> {
        match &*self.state.borrow() {
            ModuleState::Assigned(location) => location.clone(),
            ModuleState::Undeclared | ModuleState::Declared => {
                panic!("Tsonic module binding read before initialization")
            }
        }
    }
}

impl<T> Default for ModuleCell<T> {
    fn default() -> Self {
        Self::new()
    }
}
