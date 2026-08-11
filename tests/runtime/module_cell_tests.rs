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
