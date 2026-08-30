use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

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

pub struct Location<T> {
    identity: LocationIdentity,
    load_value: Rc<dyn Fn() -> T>,
    store_value: Rc<dyn Fn(T)>,
}

impl<T> Clone for Location<T> {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            load_value: Rc::clone(&self.load_value),
            store_value: Rc::clone(&self.store_value),
        }
    }
}

impl<T> Location<T> {
    pub fn load(&self) -> T {
        (self.load_value)()
    }

    pub fn store(&self, value: T) {
        (self.store_value)(value);
    }

    pub fn same(left: Option<&Self>, right: Option<&Self>) -> bool {
        match (left, right) {
            (Some(left), Some(right)) => left.identity.same(&right.identity),
            (None, None) => true,
            _ => false,
        }
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

impl<T: Clone + 'static> Location<T> {
    pub fn allocate(initial: T) -> Self {
        let storage = Rc::new(RefCell::new(initial));
        let load_storage = Rc::clone(&storage);
        let store_storage = Rc::clone(&storage);
        Self {
            identity: LocationIdentity::root(),
            load_value: Rc::new(move || load_storage.borrow().clone()),
            store_value: Rc::new(move |value| {
                *store_storage.borrow_mut() = value;
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
