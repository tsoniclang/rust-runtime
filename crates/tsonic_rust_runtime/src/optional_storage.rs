pub trait OptionalStorage<Value>: Sized {
    fn present(value: Value) -> Self;
    fn absent() -> Self;
    fn is_absent(&self) -> bool;
    fn into_present(self) -> Value;
    fn clone_present(&self) -> Value
    where
        Value: Clone;
}

impl<Value> OptionalStorage<Value> for Option<Value> {
    #[inline]
    fn present(value: Value) -> Self {
        Some(value)
    }
    #[inline]
    fn absent() -> Self {
        None
    }
    #[inline]
    fn is_absent(&self) -> bool {
        self.is_none()
    }
    #[inline]
    fn into_present(self) -> Value {
        self.expect("native optional value is absent")
    }
    #[inline]
    fn clone_present(&self) -> Value
    where
        Value: Clone,
    {
        self.as_ref()
            .expect("native optional value is absent")
            .clone()
    }
}

impl<Value> OptionalStorage<Option<Value>> for Option<Value> {
    #[inline]
    fn present(value: Option<Value>) -> Self {
        value
    }
    #[inline]
    fn absent() -> Self {
        None
    }
    #[inline]
    fn is_absent(&self) -> bool {
        self.is_none()
    }
    #[inline]
    fn into_present(self) -> Option<Value> {
        assert!(self.is_some(), "native optional value is absent");
        self
    }
    #[inline]
    fn clone_present(&self) -> Option<Value>
    where
        Option<Value>: Clone,
    {
        assert!(self.is_some(), "native optional value is absent");
        self.clone()
    }
}

impl OptionalStorage<()> for () {
    #[inline]
    fn present(_: ()) -> Self {}
    #[inline]
    fn absent() -> Self {}
    #[inline]
    fn is_absent(&self) -> bool {
        true
    }
    #[inline]
    fn into_present(self) {
        panic!("native optional value is absent")
    }
    #[inline]
    fn clone_present(&self) {
        panic!("native optional value is absent")
    }
}

#[inline]
pub fn optional_storage_coalesce<Value, Storage: OptionalStorage<Value>, Output>(
    value: Storage,
    present: impl FnOnce(Value) -> Output,
    absent: impl FnOnce() -> Output,
) -> Output {
    if value.is_absent() {
        absent()
    } else {
        present(value.into_present())
    }
}
