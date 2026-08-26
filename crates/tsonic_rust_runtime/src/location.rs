use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, Eq, PartialEq)]
enum LocationSegment {
    Member(String),
    Index(usize),
}

#[derive(Clone)]
struct LocationIdentity {
    root: Rc<()>,
    path: Rc<[LocationSegment]>,
}

impl LocationIdentity {
    fn root() -> Self {
        Self {
            root: Rc::new(()),
            path: Rc::from([]),
        }
    }

    fn child(&self, segment: LocationSegment) -> Self {
        let mut path = self.path.to_vec();
        path.push(segment);
        Self {
            root: Rc::clone(&self.root),
            path: Rc::from(path),
        }
    }

    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.root, &other.root) && self.path == other.path
    }
}

struct LocationCore<'a, T> {
    identity: LocationIdentity,
    load_value: Rc<dyn Fn() -> T + 'a>,
    store_value: Rc<dyn Fn(T) + 'a>,
}

impl<'a, T> Clone for LocationCore<'a, T> {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            load_value: Rc::clone(&self.load_value),
            store_value: Rc::clone(&self.store_value),
        }
    }
}

impl<'a, T: 'a> LocationCore<'a, T> {
    fn new(load_value: impl Fn() -> T + 'a, store_value: impl Fn(T) + 'a) -> Self {
        Self {
            identity: LocationIdentity::root(),
            load_value: Rc::new(load_value),
            store_value: Rc::new(store_value),
        }
    }

    fn load(&self) -> T {
        (self.load_value)()
    }

    fn store(&self, value: T) {
        (self.store_value)(value);
    }

    fn update(&self, change: impl FnOnce(&mut T)) {
        let mut value = self.load();
        change(&mut value);
        self.store(value);
    }

    fn update_with(&self, change: impl FnOnce(T) -> T) {
        let value = self.load();
        self.store(change(value));
    }

    fn with_mut<R>(&self, action: impl FnOnce(&mut T) -> R) -> R {
        let mut value = self.load();
        let result = action(&mut value);
        self.store(value);
        result
    }

    fn project<U>(
        &self,
        segment: LocationSegment,
        read: impl Fn(&T) -> U + 'a,
        write: impl Fn(&mut T, U) + 'a,
    ) -> LocationCore<'a, U> {
        let load_parent = self.clone();
        let store_parent = self.clone();
        LocationCore {
            identity: self.identity.child(segment),
            load_value: Rc::new(move || {
                let parent = load_parent.load();
                read(&parent)
            }),
            store_value: Rc::new(move |value| {
                let mut parent = store_parent.load();
                write(&mut parent, value);
                store_parent.store(parent);
            }),
        }
    }
}

pub struct OwnedLocation<T> {
    core: LocationCore<'static, T>,
}

impl<T> Clone for OwnedLocation<T> {
    fn clone(&self) -> Self {
        Self {
            core: self.core.clone(),
        }
    }
}

impl<T: 'static> OwnedLocation<T> {
    pub fn from_accessors(
        load_value: impl Fn() -> T + 'static,
        store_value: impl Fn(T) + 'static,
    ) -> Self {
        Self {
            core: LocationCore::new(load_value, store_value),
        }
    }

    pub fn load(&self) -> T {
        self.core.load()
    }

    pub fn store(&self, value: T) {
        self.core.store(value);
    }

    pub fn same(left: Option<&Self>, right: Option<&Self>) -> bool {
        same_location_core(
            left.map(|value| &value.core),
            right.map(|value| &value.core),
        )
    }

    pub fn update(&self, change: impl FnOnce(&mut T)) {
        self.core.update(change);
    }

    pub fn update_with(&self, change: impl FnOnce(T) -> T) {
        self.core.update_with(change);
    }

    pub fn with_mut<R>(&self, action: impl FnOnce(&mut T) -> R) -> R {
        self.core.with_mut(action)
    }

    pub fn allocate(initial: T) -> Self
    where
        T: Clone,
    {
        let storage = Rc::new(RefCell::new(initial));
        let load_storage = Rc::clone(&storage);
        let store_storage = Rc::clone(&storage);
        Self::from_accessors(
            move || load_storage.borrow().clone(),
            move |value| *store_storage.borrow_mut() = value,
        )
    }

    pub fn project_member<U: 'static>(
        &self,
        member_identity: impl Into<String>,
        read: impl Fn(&T) -> U + 'static,
        write: impl Fn(&mut T, U) + 'static,
    ) -> OwnedLocation<U> {
        OwnedLocation {
            core: self
                .core
                .project(LocationSegment::Member(member_identity.into()), read, write),
        }
    }
}

impl<T: 'static> OwnedLocation<Vec<T>> {
    pub fn project_index(&self, index: usize) -> OwnedLocation<T>
    where
        T: Clone,
    {
        OwnedLocation {
            core: self.core.project(
                LocationSegment::Index(index),
                move |values| values[index].clone(),
                move |values, value| values[index] = value,
            ),
        }
    }
}

pub struct BorrowedLocation<'a, T> {
    core: LocationCore<'a, T>,
}

impl<'a, T> Clone for BorrowedLocation<'a, T> {
    fn clone(&self) -> Self {
        Self {
            core: self.core.clone(),
        }
    }
}

impl<'a, T: 'a> BorrowedLocation<'a, T> {
    pub fn from_accessors(load_value: impl Fn() -> T + 'a, store_value: impl Fn(T) + 'a) -> Self {
        Self {
            core: LocationCore::new(load_value, store_value),
        }
    }

    pub fn load(&self) -> T {
        self.core.load()
    }

    pub fn store(&self, value: T) {
        self.core.store(value);
    }

    pub fn same(left: Option<&Self>, right: Option<&Self>) -> bool {
        same_location_core(
            left.map(|value| &value.core),
            right.map(|value| &value.core),
        )
    }

    pub fn update(&self, change: impl FnOnce(&mut T)) {
        self.core.update(change);
    }

    pub fn update_with(&self, change: impl FnOnce(T) -> T) {
        self.core.update_with(change);
    }

    pub fn with_mut<R>(&self, action: impl FnOnce(&mut T) -> R) -> R {
        self.core.with_mut(action)
    }

    pub fn project_member<U: 'a>(
        &self,
        member_identity: impl Into<String>,
        read: impl Fn(&T) -> U + 'a,
        write: impl Fn(&mut T, U) + 'a,
    ) -> BorrowedLocation<'a, U> {
        BorrowedLocation {
            core: self
                .core
                .project(LocationSegment::Member(member_identity.into()), read, write),
        }
    }
}

impl<'a, T: Clone + 'a> BorrowedLocation<'a, T> {
    pub fn from_cell(storage: &'a RefCell<T>) -> Self {
        Self::from_accessors(
            move || storage.borrow().clone(),
            move |value| *storage.borrow_mut() = value,
        )
    }
}

impl<'a, T: Clone + 'a> BorrowedLocation<'a, Vec<T>> {
    pub fn project_index(&self, index: usize) -> BorrowedLocation<'a, T> {
        BorrowedLocation {
            core: self.core.project(
                LocationSegment::Index(index),
                move |values| values[index].clone(),
                move |values, value| values[index] = value,
            ),
        }
    }
}

fn same_location_core<T>(
    left: Option<&LocationCore<'_, T>>,
    right: Option<&LocationCore<'_, T>>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.identity.same(&right.identity),
        (None, None) => true,
        _ => false,
    }
}
