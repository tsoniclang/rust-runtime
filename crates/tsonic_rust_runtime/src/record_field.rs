use crate::location::LocationSegment;
use crate::{Location, ObjectHandle};
use alloc::string::String;

pub enum RecordField<T, E> {
    Value(T),
    Bound(Location<T, E>),
}

impl<T: Clone, E> Clone for RecordField<T, E> {
    fn clone(&self) -> Self {
        match self {
            Self::Value(value) => Self::Value(value.clone()),
            Self::Bound(location) => Self::Bound(location.clone()),
        }
    }
}

impl<T: Default, E> Default for RecordField<T, E> {
    fn default() -> Self {
        Self::Value(T::default())
    }
}

impl<T, E> RecordField<T, E> {
    pub fn bound_location(&self) -> Option<Location<T, E>> {
        match self {
            Self::Value(_) => None,
            Self::Bound(location) => Some(location.clone()),
        }
    }

    pub fn try_store(&mut self, value: T) -> Result<(), E> {
        match self {
            Self::Value(stored) => {
                *stored = value;
                Ok(())
            }
            Self::Bound(location) => location.try_store(value),
        }
    }
}

impl<T: Clone, E> RecordField<T, E> {
    pub fn try_load_object<State>(
        owner: &ObjectHandle<State>,
        select: fn(&State) -> &Self,
    ) -> Result<T, E> {
        match owner.with(|state| select(state).clone()) {
            Self::Value(value) => Ok(value),
            Self::Bound(location) => location.try_load(),
        }
    }

    pub fn try_store_object<State>(
        owner: &ObjectHandle<State>,
        select: fn(&State) -> &Self,
        select_mut: fn(&mut State) -> &mut Self,
        value: T,
    ) -> Result<(), E> {
        match owner.with(|state| select(state).bound_location()) {
            Some(location) => location.try_store(value),
            None => owner.with_mut(|state| select_mut(state).try_store(value)),
        }
    }

    pub fn try_load(&self) -> Result<T, E> {
        match self {
            Self::Value(value) => Ok(value.clone()),
            Self::Bound(location) => location.try_load(),
        }
    }
}

impl<T: Clone + 'static, E: 'static> RecordField<T, E> {
    pub fn location_from_object<State: 'static>(
        owner: &ObjectHandle<State>,
        member: String,
        select: fn(&State) -> &Self,
        select_mut: fn(&mut State) -> &mut Self,
    ) -> Location<T, E> {
        if let Some(location) = owner.with(|state| select(state).bound_location()) {
            return location;
        }
        let reader = owner.clone();
        let writer = owner.clone();
        Location::try_bind_projected(
            owner.clone(),
            LocationSegment::Member(member),
            move || Self::try_load_object(&reader, select),
            move |value| Self::try_store_object(&writer, select, select_mut, value),
        )
    }

    pub fn location_from_value<State: 'static>(
        owner: &Location<State, E>,
        member: String,
        select: fn(&State) -> &Self,
        select_mut: fn(&mut State) -> &mut Self,
    ) -> Result<Location<T, E>, E> {
        if let Some(location) = select(&owner.try_load()?).bound_location() {
            return Ok(location);
        }
        Ok(owner.try_project_member(
            member,
            move |state| select(&state).try_load(),
            move |state, value| select_mut(state).try_store(value),
        ))
    }
}
