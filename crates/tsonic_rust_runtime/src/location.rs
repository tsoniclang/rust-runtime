use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::convert::Infallible;
use core::hash::{Hash, Hasher};

use crate::raw_memory::RawPointer;
use crate::{ObjectIdentity, ObjectIdentityCarrier};

mod fallible;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum LocationSegment {
    Member(String),
    Index(usize),
}

#[derive(Clone)]
struct LocationIdentity {
    root: LocationRoot,
    path: Rc<[LocationSegment]>,
}

#[derive(Clone)]
enum LocationRoot {
    Logical(ObjectIdentity),
    Native(RawPointer),
}

impl LocationIdentity {
    fn root() -> Self {
        Self {
            root: LocationRoot::Logical(ObjectIdentity::new()),
            path: Rc::from([]),
        }
    }

    fn child(&self, segment: LocationSegment) -> Self {
        let mut path = self.path.to_vec();
        path.push(segment);
        Self {
            root: self.root.clone(),
            path: Rc::from(path),
        }
    }

    fn same(&self, other: &Self) -> bool {
        let same_root = match (&self.root, &other.root) {
            (LocationRoot::Logical(left), LocationRoot::Logical(right)) => {
                ObjectIdentity::same(left, right)
            }
            (LocationRoot::Native(left), LocationRoot::Native(right)) => left == right,
            _ => false,
        };
        same_root && self.path == other.path
    }

    fn hash(&self) -> u32 {
        let mut hash = LocationHasher(2166136261);
        match &self.root {
            LocationRoot::Logical(identity) => identity.key().hash(&mut hash),
            LocationRoot::Native(pointer) => Hash::hash(pointer, &mut hash),
        }
        self.path.hash(&mut hash);
        hash.0
    }
}

struct LocationHasher(u32);

impl Hasher for LocationHasher {
    fn finish(&self) -> u64 {
        u64::from(self.0)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u32::from(*byte)).wrapping_mul(16777619);
        }
    }
}

pub struct Location<T, E = Infallible> {
    identity: LocationIdentity,
    load_value: Rc<dyn Fn() -> Result<T, E>>,
    store_value: Rc<dyn Fn(T) -> Result<(), E>>,
    raw: Option<RawPointer>,
}

impl<T, E> Clone for Location<T, E> {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            load_value: Rc::clone(&self.load_value),
            store_value: Rc::clone(&self.store_value),
            raw: self.raw.clone(),
        }
    }
}

impl<T> Location<T> {
    pub(crate) fn from_raw(
        pointer: RawPointer,
        read: impl Fn() -> T + 'static,
        write: impl Fn(T) + 'static,
    ) -> Self {
        Self {
            identity: LocationIdentity {
                root: LocationRoot::Native(pointer.clone()),
                path: Rc::from([]),
            },
            load_value: Rc::new(move || Ok(read())),
            store_value: Rc::new(move |value| {
                write(value);
                Ok(())
            }),
            raw: Some(pointer),
        }
    }

    pub fn load(&self) -> T {
        match self.try_load() {
            Ok(value) => value,
            Err(error) => match error {},
        }
    }

    pub fn store(&self, value: T) {
        match self.try_store(value) {
            Ok(()) => (),
            Err(error) => match error {},
        }
    }

    pub fn bind<Owner: ObjectIdentityCarrier + 'static>(
        owner: Owner,
        read: impl Fn() -> T + 'static,
        write: impl Fn(T) + 'static,
    ) -> Self {
        Self {
            identity: LocationIdentity {
                root: LocationRoot::Logical(owner.object_identity().clone()),
                path: Rc::from([]),
            },
            load_value: Rc::new(move || {
                let value = read();
                crate::keep_alive(&owner);
                Ok(value)
            }),
            store_value: Rc::new(move |value| {
                write(value);
                Ok(())
            }),
            raw: None,
        }
    }

    pub fn bind_projected<Owner: ObjectIdentityCarrier + 'static>(
        owner: Owner,
        segment: LocationSegment,
        read: impl Fn() -> T + 'static,
        write: impl Fn(T) + 'static,
    ) -> Self {
        let mut location = Self::bind(owner, read, write);
        location.identity = location.identity.child(segment);
        location
    }

    pub fn map<U: 'static>(
        &self,
        read: impl Fn(T) -> U + 'static,
        write: impl Fn(U) -> T + 'static,
    ) -> Location<U>
    where
        T: 'static,
    {
        let load_source = self.clone();
        let store_source = self.clone();
        Location {
            identity: self.identity.clone(),
            load_value: Rc::new(move || Ok(read(load_source.load()))),
            store_value: Rc::new(move |value| {
                store_source.store(write(value));
                Ok(())
            }),
            raw: None,
        }
    }

    pub fn view<U: 'static>(
        &self,
        read: impl Fn() -> U + 'static,
        write: impl Fn(U) + 'static,
    ) -> Location<U>
    where
        T: 'static,
    {
        let source = self.clone();
        Location {
            identity: self.identity.clone(),
            load_value: Rc::new(move || {
                let value = read();
                crate::keep_alive(&source);
                Ok(value)
            }),
            store_value: Rc::new(move |value| {
                write(value);
                Ok(())
            }),
            raw: None,
        }
    }

    pub fn view_optional<U: 'static>(
        source: &Option<Self>,
        read: impl Fn() -> U + 'static,
        write: impl Fn(U) + 'static,
    ) -> Option<Location<U>>
    where
        T: 'static,
    {
        source.as_ref().map(|source| source.view(read, write))
    }

    pub fn map_optional<U: 'static>(
        source: Option<&Self>,
        read: impl Fn(T) -> U + 'static,
        write: impl Fn(U) -> T + 'static,
    ) -> Option<Location<U>>
    where
        T: 'static,
    {
        source.map(|source| source.map(read, write))
    }

    pub fn update(&self, change: impl FnOnce(&mut T)) {
        let mut value = self.load();
        change(&mut value);
        self.store(value);
    }

    pub fn update_with(&self, change: impl FnOnce(T) -> T) {
        let value = self.load();
        self.store(change(value));
    }

    pub fn with_mut<R>(&self, action: impl FnOnce(&mut T) -> R) -> R {
        let mut value = self.load();
        let result = action(&mut value);
        self.store(value);
        result
    }

    pub fn project_member<U: Clone + 'static>(
        &self,
        member_identity: impl Into<String>,
        read: impl Fn(&T) -> U + 'static,
        write: impl Fn(&mut T, U) + 'static,
    ) -> Location<U>
    where
        T: Clone + 'static,
    {
        self.project(LocationSegment::Member(member_identity.into()), read, write)
    }

    fn project<U: Clone + 'static>(
        &self,
        segment: LocationSegment,
        read: impl Fn(&T) -> U + 'static,
        write: impl Fn(&mut T, U) + 'static,
    ) -> Location<U>
    where
        T: Clone + 'static,
    {
        let load_parent = self.clone();
        let store_parent = self.clone();
        Location {
            identity: self.identity.child(segment),
            raw: None,
            load_value: Rc::new(move || {
                let parent = load_parent.load();
                Ok(read(&parent))
            }),
            store_value: Rc::new(move |value| {
                let mut parent = store_parent.load();
                write(&mut parent, value);
                store_parent.store(parent);
                Ok(())
            }),
        }
    }
}

impl<T: Clone + 'static> Location<T> {
    pub fn allocate(initial: T) -> Self {
        let storage = Rc::new(RefCell::new(initial));
        let load_storage = Rc::clone(&storage);
        let store_storage = Rc::clone(&storage);
        Self {
            identity: LocationIdentity::root(),
            raw: None,
            load_value: Rc::new(move || Ok(load_storage.borrow().clone())),
            store_value: Rc::new(move |value| {
                *store_storage.borrow_mut() = value;
                Ok(())
            }),
        }
    }
}

impl<T: Clone + 'static> Location<Vec<T>> {
    pub fn project_index(&self, index: usize) -> Location<T> {
        self.project(
            LocationSegment::Index(index),
            move |values| values[index].clone(),
            move |values, value| values[index] = value,
        )
    }
}
