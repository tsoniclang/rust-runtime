use alloc::borrow::ToOwned;
use alloc::rc::Rc;
use core::fmt;

trait ClosedTsValue: 'static {}

impl<T: 'static> ClosedTsValue for T {}

pub struct TsValue(Rc<dyn ClosedTsValue>);

impl TsValue {
    pub fn from_closed<T>(value: &T) -> Self
    where
        T: ToOwned + ?Sized,
        T::Owned: 'static,
    {
        Self(Rc::new(value.to_owned()))
    }
}

impl Clone for TsValue {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl fmt::Debug for TsValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TsValue")
    }
}

pub fn clone_ts_value(value: &TsValue) -> TsValue {
    value.clone()
}
