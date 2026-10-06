use alloc::rc::Rc;
use core::cell::Cell;
use core::fmt;

#[derive(Default, Debug)]
pub struct FrameEntryCounter {
    next: Cell<usize>,
}

impl FrameEntryCounter {
    pub const fn new() -> Self {
        Self { next: Cell::new(0) }
    }

    pub fn allocate(&self) -> usize {
        let identity = self.next.get();
        self.next.set(
            identity
                .checked_add(1)
                .expect("callable frame identity exhausted"),
        );
        identity
    }
}

pub trait FrameCallableEntry<TFrame>: Clone + Eq {
    type Arguments;
    type Result;

    fn invoke(&self, frame: &Rc<TFrame>, arguments: Self::Arguments) -> Self::Result;
}

pub struct FrameCallable<TFrame, TEntry: FrameCallableEntry<TFrame>> {
    frame: Rc<TFrame>,
    entry: TEntry,
}

impl<TFrame, TEntry: FrameCallableEntry<TFrame>> FrameCallable<TFrame, TEntry> {
    pub fn from_frame(frame: Rc<TFrame>, entry: TEntry) -> Self {
        Self { frame, entry }
    }

    pub fn call(&self, arguments: TEntry::Arguments) -> TEntry::Result {
        self.entry.invoke(&self.frame, arguments)
    }

    pub fn entry(&self) -> &TEntry {
        &self.entry
    }

    pub fn frame(&self) -> &Rc<TFrame> {
        &self.frame
    }

    pub fn same(left: &Self, right: &Self) -> bool {
        Rc::ptr_eq(&left.frame, &right.frame) && left.entry == right.entry
    }
}

impl<TFrame, TEntry: FrameCallableEntry<TFrame>> Clone for FrameCallable<TFrame, TEntry> {
    fn clone(&self) -> Self {
        Self {
            frame: Rc::clone(&self.frame),
            entry: self.entry.clone(),
        }
    }
}

impl<TFrame, TEntry: FrameCallableEntry<TFrame>> PartialEq for FrameCallable<TFrame, TEntry> {
    fn eq(&self, other: &Self) -> bool {
        Self::same(self, other)
    }
}

impl<TFrame, TEntry: FrameCallableEntry<TFrame>> Eq for FrameCallable<TFrame, TEntry> {}

impl<TFrame, TEntry: FrameCallableEntry<TFrame>> fmt::Debug for FrameCallable<TFrame, TEntry> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FrameCallable")
    }
}

#[cfg(test)]
mod identity_tests {
    use super::FrameEntryCounter;
    use core::cell::Cell;

    #[test]
    #[should_panic(expected = "callable frame identity exhausted")]
    fn native_identity_overflow_cannot_reuse_an_existing_identity() {
        FrameEntryCounter {
            next: Cell::new(usize::MAX),
        }
        .allocate();
    }
}
