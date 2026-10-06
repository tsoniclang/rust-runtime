use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tsonic_rust_runtime::{FrameCallable, FrameCallableEntry, FrameEntryCounter};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Entry {
    Original,
    Replacement,
}

#[test]
fn repeated_creation_has_native_identity_without_an_entry_allocation() {
    struct IdentityFrame {
        counter: FrameEntryCounter,
    }
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct IdentityEntry(usize);
    impl FrameCallableEntry<IdentityFrame> for IdentityEntry {
        type Arguments = ();
        type Result = usize;

        fn invoke(&self, _frame: &Rc<IdentityFrame>, (): ()) -> usize {
            self.0
        }
    }
    let frame = Rc::new(IdentityFrame {
        counter: FrameEntryCounter::new(),
    });
    let first = FrameCallable::from_frame(frame.clone(), IdentityEntry(frame.counter.allocate()));
    let second = FrameCallable::from_frame(frame.clone(), IdentityEntry(frame.counter.allocate()));
    let alias = first.clone();
    assert!(first == alias);
    assert!(first != second);
    assert_eq!(first.call(()), 0);
    assert_eq!(second.call(()), 1);
    assert!(Rc::ptr_eq(first.frame(), &frame));
    assert_eq!(first.entry().0, alias.entry().0);
    assert!(!std::ptr::eq(first.entry(), alias.entry()));
    assert_eq!(
        core::mem::size_of::<FrameEntryCounter>(),
        core::mem::size_of::<usize>()
    );
    let borrowed = first.entry();
    for expected in 2..10_000 {
        assert_eq!(frame.counter.allocate(), expected);
        assert_eq!(borrowed.invoke(first.frame(), ()), 0);
    }
    assert_eq!(Rc::strong_count(&frame), 4);
    let independent = Rc::new(IdentityFrame {
        counter: FrameEntryCounter::new(),
    });
    let other = FrameCallable::from_frame(
        independent.clone(),
        IdentityEntry(independent.counter.allocate()),
    );
    assert!(first != other);
}

struct Frame {
    selected: Cell<Entry>,
    drops: Rc<Cell<usize>>,
}

impl Drop for Frame {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

impl FrameCallableEntry<Frame> for Entry {
    type Arguments = i32;
    type Result = i32;

    fn invoke(&self, frame: &Rc<Frame>, count: i32) -> i32 {
        if count == 0 {
            match self {
                Self::Original => 1,
                Self::Replacement => 2,
            }
        } else {
            frame.selected.get().invoke(frame, count - 1)
        }
    }
}

#[test]
fn escaped_entry_retains_one_frame_and_observes_its_live_binding() {
    let drops = Rc::new(Cell::new(0));
    let frame = Rc::new(Frame {
        selected: Cell::new(Entry::Original),
        drops: drops.clone(),
    });
    let callback = FrameCallable::from_frame(frame.clone(), Entry::Original);
    let alias = callback.clone();
    let replacement = FrameCallable::from_frame(frame.clone(), Entry::Replacement);
    assert!(FrameCallable::same(&callback, &alias));
    assert!(callback != replacement);
    frame.selected.set(Entry::Replacement);
    drop(frame);
    assert_eq!(callback.call(0), 1);
    assert_eq!(callback.call(8), 2);
    assert_eq!(replacement.call(0), 2);
    drop(callback);
    drop(replacement);
    assert_eq!(drops.get(), 0);
    drop(alias);
    assert_eq!(drops.get(), 1);
}

#[test]
fn frame_identity_distinguishes_independent_activations_and_never_invokes_for_debug() {
    let drops = Rc::new(Cell::new(0));
    let first = FrameCallable::from_frame(
        Rc::new(Frame {
            selected: Cell::new(Entry::Original),
            drops: drops.clone(),
        }),
        Entry::Original,
    );
    let second = FrameCallable::from_frame(
        Rc::new(Frame {
            selected: Cell::new(Entry::Original),
            drops: drops.clone(),
        }),
        Entry::Original,
    );
    assert!(first != second);
    assert_eq!(format!("{first:?}"), "FrameCallable");
    drop(first);
    drop(second);
    assert_eq!(drops.get(), 2);
}

#[test]
fn frame_entries_do_not_require_borrowed_values_to_be_static() {
    struct BorrowedFrame;
    struct BorrowedEntry<'value>(std::marker::PhantomData<&'value str>);

    impl<'value> Copy for BorrowedEntry<'value> {}
    impl<'value> Clone for BorrowedEntry<'value> {
        fn clone(&self) -> Self {
            *self
        }
    }
    impl<'value> PartialEq for BorrowedEntry<'value> {
        fn eq(&self, _other: &Self) -> bool {
            true
        }
    }
    impl<'value> Eq for BorrowedEntry<'value> {}
    impl<'value> FrameCallableEntry<BorrowedFrame> for BorrowedEntry<'value> {
        type Arguments = &'value str;
        type Result = &'value str;

        fn invoke(&self, _frame: &Rc<BorrowedFrame>, value: &'value str) -> &'value str {
            value
        }
    }

    let text = String::from("borrowed");
    let callback = FrameCallable::from_frame(
        Rc::new(BorrowedFrame),
        BorrowedEntry(std::marker::PhantomData),
    );
    assert_eq!(callback.call(&text), "borrowed");
}

#[test]
fn per_creation_entry_state_is_borrowed_on_invocation_and_released_without_an_owner_cycle() {
    struct State {
        value: i32,
        drops: Rc<Cell<usize>>,
    }
    impl Drop for State {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    struct OwningEntry {
        state: Rc<State>,
        clones: Rc<Cell<usize>>,
    }
    impl Clone for OwningEntry {
        fn clone(&self) -> Self {
            self.clones.set(self.clones.get() + 1);
            Self {
                state: self.state.clone(),
                clones: self.clones.clone(),
            }
        }
    }
    impl PartialEq for OwningEntry {
        fn eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.state, &other.state)
        }
    }
    impl Eq for OwningEntry {}
    struct OwningFrame {
        selected: RefCell<Option<OwningEntry>>,
        drops: Rc<Cell<usize>>,
    }
    impl Drop for OwningFrame {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    impl FrameCallableEntry<OwningFrame> for OwningEntry {
        type Arguments = i32;
        type Result = i32;

        fn invoke(&self, frame: &Rc<OwningFrame>, count: i32) -> i32 {
            if count == 0 {
                self.state.value
            } else {
                let entry = frame.selected.borrow().as_ref().unwrap().clone();
                entry.invoke(frame, count - 1)
            }
        }
    }
    let frame_drops = Rc::new(Cell::new(0));
    let state_drops = Rc::new(Cell::new(0));
    let clones = Rc::new(Cell::new(0));
    let frame = Rc::new(OwningFrame {
        selected: RefCell::new(None),
        drops: frame_drops.clone(),
    });
    let original = FrameCallable::from_frame(
        frame.clone(),
        OwningEntry {
            state: Rc::new(State {
                value: 1,
                drops: state_drops.clone(),
            }),
            clones: clones.clone(),
        },
    );
    let replacement_entry = OwningEntry {
        state: Rc::new(State {
            value: 2,
            drops: state_drops.clone(),
        }),
        clones: clones.clone(),
    };
    *frame.selected.borrow_mut() = Some(replacement_entry.clone());
    let replacement = FrameCallable::from_frame(frame.clone(), replacement_entry);
    let alias = original.clone();
    assert!(original == alias);
    assert!(original != replacement);
    let before = clones.get();
    for _index in 0..10_000 {
        assert_eq!(original.call(0), 1);
        assert_eq!(replacement.call(0), 2);
    }
    assert_eq!(clones.get(), before);
    assert_eq!(original.call(8), 2);
    drop(frame);
    drop(original);
    assert_eq!(state_drops.get(), 0);
    drop(alias);
    assert_eq!(state_drops.get(), 1);
    assert_eq!(frame_drops.get(), 0);
    drop(replacement);
    assert_eq!(state_drops.get(), 2);
    assert_eq!(frame_drops.get(), 1);
}
