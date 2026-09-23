use tsonic_rust_runtime::{Location, ModuleCell};

#[test]
fn module_cell_initializes_reads_writes_and_exposes_one_location() {
    let cell = ModuleCell::new();
    cell.initialize(3_i32);

    assert_eq!(cell.load(), 3);
    cell.store(5);
    assert_eq!(cell.load(), 5);

    let first = cell.location();
    let second = cell.location();
    assert!(Location::same(Some(&first), Some(&second)));
    first.store(8);
    assert_eq!(cell.load(), 8);
}

#[test]
#[should_panic(expected = "Tsonic module binding read before initialization")]
fn module_cell_rejects_reads_before_initialization() {
    let cell = ModuleCell::<i32>::new();
    let _ = cell.load();
}

#[test]
#[should_panic(expected = "Tsonic module binding initialized more than once")]
fn module_cell_rejects_duplicate_initialization() {
    let cell = ModuleCell::new();
    cell.initialize(1_i32);
    cell.initialize(2_i32);
}

#[test]
fn declared_module_cell_assigns_once_then_preserves_its_location() {
    let cell = ModuleCell::new();
    cell.declare();
    cell.store(3_i32);
    let first = cell.location();
    cell.store(5);
    assert_eq!(first.load(), 5);
    assert!(Location::same(Some(&first), Some(&cell.location())));
    first.store(8);
    assert_eq!(cell.load(), 8);
}

#[test]
#[should_panic(expected = "Tsonic module binding read before initialization")]
fn declared_module_cell_never_fabricates_an_initial_value() {
    let cell = ModuleCell::<i32>::new();
    cell.declare();
    let _ = cell.load();
}

#[test]
#[should_panic(expected = "Tsonic module binding read before initialization")]
fn declared_module_cell_does_not_expose_uninitialized_storage() {
    let cell = ModuleCell::<i32>::new();
    cell.declare();
    let _ = cell.location();
}

#[test]
#[should_panic(expected = "Tsonic module binding written before declaration")]
fn module_cell_rejects_assignment_in_the_temporal_dead_zone() {
    ModuleCell::new().store(3_i32);
}

#[test]
#[should_panic(expected = "Tsonic module binding declared more than once")]
fn module_cell_rejects_duplicate_declaration() {
    let cell = ModuleCell::<i32>::new();
    cell.declare();
    cell.declare();
}

#[test]
#[should_panic(expected = "Tsonic module binding initialized more than once")]
fn declaration_cannot_be_replaced_with_a_second_binding_initialization() {
    let cell = ModuleCell::new();
    cell.declare();
    cell.initialize(3_i32);
}

#[test]
fn module_cell_releases_its_state_borrow_before_destroying_the_old_value() {
    use std::rc::{Rc, Weak};

    #[derive(Clone)]
    struct Reentrant(Weak<ModuleCell<Reentrant>>);

    impl Drop for Reentrant {
        fn drop(&mut self) {
            if let Some(cell) = self.0.upgrade() {
                drop(cell.location());
            }
        }
    }

    let cell = Rc::new(ModuleCell::new());
    cell.declare();
    cell.store(Reentrant(Rc::downgrade(&cell)));
    cell.store(Reentrant(Rc::downgrade(&cell)));
}

#[test]
fn declaration_state_does_not_enlarge_the_module_storage() {
    assert_eq!(
        std::mem::size_of::<ModuleCell<i32>>(),
        std::mem::size_of::<std::cell::RefCell<Option<Location<i32>>>>(),
    );
}
