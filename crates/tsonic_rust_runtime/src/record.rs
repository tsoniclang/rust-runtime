use alloc::vec::Vec;
use core::borrow::Borrow;
use core::fmt;
use core::hash::Hash;
use std::collections::HashMap;

use crate::ObjectHandle;

pub struct Record<Key, Value> {
    entries: ObjectHandle<HashMap<Key, Value>>,
}

impl<Key, Value> Record<Key, Value> {
    pub fn storage_identity_key(&self) -> usize {
        self.entries.storage_identity_key()
    }

    pub fn with_entries<Output>(
        &self,
        read: impl FnOnce(&HashMap<Key, Value>) -> Output,
    ) -> Output {
        self.entries.with(read)
    }
}

impl<Key, Value> Default for Record<Key, Value> {
    fn default() -> Self {
        Self {
            entries: ObjectHandle::new(HashMap::new()),
        }
    }
}

impl<Key, Value> Record<Key, Value>
where
    Key: Eq + Hash,
{
    pub fn from_entries(entries: impl IntoIterator<Item = (Key, Value)>) -> Self {
        Self::from_map(entries.into_iter().collect())
    }

    pub fn from_map(entries: HashMap<Key, Value>) -> Self {
        Self {
            entries: ObjectHandle::new(entries),
        }
    }

    pub fn set(&self, key: Key, value: Value) {
        self.entries.with_mut(|entries| {
            entries.insert(key, value);
        });
    }

    pub fn contains_key<Query: Eq + Hash + ?Sized>(&self, key: &Query) -> bool
    where
        Key: Borrow<Query>,
    {
        self.entries.with(|entries| entries.contains_key(key))
    }

    pub fn remove<Query: Eq + Hash + ?Sized>(&self, key: &Query) -> bool
    where
        Key: Borrow<Query>,
    {
        self.entries
            .with_mut(|entries| entries.remove(key).is_some())
    }

    pub fn get<Query: Eq + Hash + ?Sized>(&self, key: &Query) -> Value
    where
        Key: Borrow<Query>,
        Value: Clone,
    {
        self.entries
            .with(|entries| entries.get(key).expect("record key is absent").clone())
    }

    pub fn get_or_default<Query: Eq + Hash + ?Sized>(&self, key: &Query) -> Value
    where
        Key: Borrow<Query>,
        Value: Clone + Default,
    {
        self.entries
            .with(|entries| entries.get(key).cloned().unwrap_or_default())
    }

    pub fn keys(&self) -> Vec<Key>
    where
        Key: Clone,
    {
        self.entries
            .with(|entries| entries.keys().cloned().collect())
    }

    pub fn values(&self) -> Vec<Value>
    where
        Value: Clone,
    {
        self.entries
            .with(|entries| entries.values().cloned().collect())
    }

    pub fn entries(&self) -> Vec<(Key, Value)>
    where
        Key: Clone,
        Value: Clone,
    {
        self.entries.with(|entries| {
            entries
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
    }

    pub fn extend(&self, source: &Self)
    where
        Key: Clone,
        Value: Clone,
    {
        if self == source {
            return;
        }
        self.entries
            .with_mut(|entries| source.copy_entries_to(entries));
    }

    pub fn copy_entries_to(&self, target: &mut HashMap<Key, Value>)
    where
        Key: Clone,
        Value: Clone,
    {
        self.entries.with(|entries| extend_entries(target, entries));
    }
}

pub fn extend_entries<Key: Eq + Hash + Clone, Value: Clone>(
    target: &mut HashMap<Key, Value>,
    source: &HashMap<Key, Value>,
) {
    target.extend(
        source
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
}

impl<Key, Value> Clone for Record<Key, Value> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
        }
    }
}

impl<Key, Value> PartialEq for Record<Key, Value> {
    fn eq(&self, other: &Self) -> bool {
        ObjectHandle::same(&self.entries, &other.entries)
    }
}

impl<Key, Value> Eq for Record<Key, Value> {}

impl<Key, Value> fmt::Debug for Record<Key, Value> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Record")
    }
}
