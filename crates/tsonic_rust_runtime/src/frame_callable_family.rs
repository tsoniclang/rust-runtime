use crate::{Callable, CallableImplementation, FrameCallable, FrameCallableEntry};
use alloc::rc::Rc;
use core::fmt;

pub trait FrameCallableFamilyEntry<TFrame>: FrameCallableEntry<TFrame> {
    fn from_independent(value: Callable<Self::Arguments, Self::Result>) -> Self;
    fn into_independent(self) -> Result<Callable<Self::Arguments, Self::Result>, Self>;
}

pub enum FrameCallableFamily<TFrame, TEntry: FrameCallableFamilyEntry<TFrame>> {
    Retained(FrameCallable<TFrame, TEntry>),
    Independent(Callable<TEntry::Arguments, TEntry::Result>),
}

impl<TFrame, TEntry: FrameCallableFamilyEntry<TFrame>> FrameCallableFamily<TFrame, TEntry> {
    pub fn from_entry(entry: TEntry, retained_frame: impl FnOnce() -> Rc<TFrame>) -> Self {
        match entry.into_independent() {
            Ok(value) => Self::Independent(value),
            Err(entry) => Self::Retained(FrameCallable::from_frame(retained_frame(), entry)),
        }
    }

    pub fn from_independent(value: Callable<TEntry::Arguments, TEntry::Result>) -> Self {
        Self::Independent(value)
    }

    pub fn into_entry(self) -> TEntry {
        match self {
            Self::Retained(value) => value.into_entry(),
            Self::Independent(value) => TEntry::from_independent(value),
        }
    }

    pub fn into_entry_for(self, frame: &Rc<TFrame>) -> TEntry {
        match self {
            Self::Retained(value) => value.into_entry_for(frame),
            Self::Independent(value) => TEntry::from_independent(value),
        }
    }

    pub fn call(&self, arguments: TEntry::Arguments) -> TEntry::Result {
        match self {
            Self::Retained(value) => value.call(arguments),
            Self::Independent(value) => value.call(arguments),
        }
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        match (left, right) {
            (Self::Retained(left), Self::Retained(right)) => FrameCallable::same(left, right),
            (Self::Independent(left), Self::Independent(right)) => Callable::same(left, right),
            _ => false,
        }
    }
}

impl<TFrame, TEntry: FrameCallableFamilyEntry<TFrame>>
    CallableImplementation<TEntry::Arguments, TEntry::Result>
    for FrameCallableFamily<TFrame, TEntry>
{
    fn invoke(&self, arguments: TEntry::Arguments) -> TEntry::Result {
        self.call(arguments)
    }
}

impl<TFrame, TEntry: FrameCallableFamilyEntry<TFrame>> Clone
    for FrameCallableFamily<TFrame, TEntry>
{
    fn clone(&self) -> Self {
        match self {
            Self::Retained(value) => Self::Retained(value.clone()),
            Self::Independent(value) => Self::Independent(value.clone()),
        }
    }
}

impl<TFrame, TEntry: FrameCallableFamilyEntry<TFrame>> PartialEq
    for FrameCallableFamily<TFrame, TEntry>
{
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl<TFrame, TEntry: FrameCallableFamilyEntry<TFrame>> Eq for FrameCallableFamily<TFrame, TEntry> {}

impl<TFrame, TEntry: FrameCallableFamilyEntry<TFrame>> fmt::Debug
    for FrameCallableFamily<TFrame, TEntry>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FrameCallableFamily")
    }
}
