use std::cell::Cell;
use std::rc::Rc;
use tsonic_rust_runtime::{FrameCallable, FrameCallableEntry};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Entry {
    Original,
    Replacement,
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

    fn invoke(self, frame: &Rc<Frame>, count: i32) -> i32 {
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

        fn invoke(self, _frame: &Rc<BorrowedFrame>, value: &'value str) -> &'value str {
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
