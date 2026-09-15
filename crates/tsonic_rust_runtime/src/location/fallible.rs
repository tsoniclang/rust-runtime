use super::{Location, LocationIdentity, LocationRoot};
use crate::ObjectIdentityCarrier;
use alloc::rc::Rc;
use core::convert::Infallible;

impl<T, E> Location<T, E> {
    pub(crate) fn raw_backing(&self) -> Option<&crate::raw_memory::RawPointer> {
        self.raw.as_ref()
    }

    pub fn try_load(&self) -> Result<T, E> {
        (self.load_value)()
    }

    pub fn try_store(&self, value: T) -> Result<(), E> {
        (self.store_value)(value)
    }

    pub fn same(left: Option<&Self>, right: Option<&Self>) -> bool {
        match (left, right) {
            (Some(left), Some(right)) => left.identity.same(&right.identity),
            (None, None) => true,
            _ => false,
        }
    }

    pub fn hash(pointer: Option<&Self>) -> f64 {
        pointer.map_or(0.0, |pointer| f64::from(pointer.identity.hash()))
    }

    pub fn try_bind<Owner: ObjectIdentityCarrier + 'static>(
        owner: Owner,
        read: impl Fn() -> Result<T, E> + 'static,
        write: impl Fn(T) -> Result<(), E> + 'static,
    ) -> Self {
        Self {
            identity: LocationIdentity {
                root: LocationRoot::Logical(owner.object_identity().clone()),
                path: Rc::from([]),
            },
            load_value: Rc::new(move || {
                let value = read();
                crate::keep_alive(&owner);
                value
            }),
            store_value: Rc::new(write),
            raw: None,
        }
    }

    pub fn try_view<U: 'static, F>(
        &self,
        read: impl Fn() -> Result<U, F> + 'static,
        write: impl Fn(U) -> Result<(), F> + 'static,
    ) -> Location<U, F>
    where
        T: 'static,
        E: 'static,
    {
        let source = self.clone();
        Location {
            identity: self.identity.clone(),
            load_value: Rc::new(move || {
                let value = read();
                crate::keep_alive(&source);
                value
            }),
            store_value: Rc::new(write),
            raw: None,
        }
    }

    pub fn try_map<U: 'static>(
        &self,
        read: impl Fn(T) -> Result<U, E> + 'static,
        write: impl Fn(U) -> Result<T, E> + 'static,
    ) -> Location<U, E>
    where
        T: 'static,
        E: 'static,
    {
        let source_read = self.clone();
        let source_write = self.clone();
        Location {
            identity: self.identity.clone(),
            load_value: Rc::new(move || read(source_read.try_load()?)),
            store_value: Rc::new(move |value| source_write.try_store(write(value)?)),
            raw: None,
        }
    }

    pub fn try_view_optional<U: 'static, F>(
        source: &Option<Self>,
        read: impl Fn() -> Result<U, F> + 'static,
        write: impl Fn(U) -> Result<(), F> + 'static,
    ) -> Option<Location<U, F>>
    where
        T: 'static,
        E: 'static,
    {
        source.as_ref().map(|source| source.try_view(read, write))
    }

    pub fn try_map_optional<U: 'static>(
        source: Option<&Self>,
        read: impl Fn(T) -> Result<U, E> + 'static,
        write: impl Fn(U) -> Result<T, E> + 'static,
    ) -> Option<Location<U, E>>
    where
        T: 'static,
        E: 'static,
    {
        source.map(|source| source.try_map(read, write))
    }

    pub fn map_error<F>(&self, convert: impl Fn(E) -> F + 'static) -> Location<T, F>
    where
        T: 'static,
        E: 'static,
    {
        let source_read = self.clone();
        let source_write = self.clone();
        let read_error = Rc::new(convert);
        let write_error = Rc::clone(&read_error);
        Location {
            identity: self.identity.clone(),
            raw: self.raw.clone(),
            load_value: Rc::new(move || source_read.try_load().map_err(|error| read_error(error))),
            store_value: Rc::new(move |value| {
                source_write
                    .try_store(value)
                    .map_err(|error| write_error(error))
            }),
        }
    }
}

impl<T: 'static> Location<T> {
    pub fn into_fallible<E>(&self) -> Location<T, E> {
        self.map_error(|error: Infallible| match error {})
    }
}
