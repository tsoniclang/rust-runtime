use std::cell::Cell;
use std::rc::Rc;

use tsonic_rust_runtime::{clone_ts_value, TsValue};

#[derive(Clone)]
struct DropProbe(Rc<Cell<u32>>);

impl Drop for DropProbe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn closed_values_remain_alive_until_the_last_passive_carrier_is_dropped() {
    let drops = Rc::new(Cell::new(0));
    let value = TsValue::from_closed(&DropProbe(Rc::clone(&drops)));
    assert_eq!(drops.get(), 1);

    let alias = clone_ts_value(&value);
    drop(value);
    assert_eq!(drops.get(), 1);

    drop(alias);
    assert_eq!(drops.get(), 2);
}

#[test]
fn debug_output_does_not_inspect_the_closed_value() {
    let value = TsValue::from_closed(&DropProbe(Rc::new(Cell::new(0))));
    assert_eq!(format!("{value:?}"), "TsValue");
}
