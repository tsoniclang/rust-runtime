use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tsonic_rust_runtime::{
    Callable, CallableImplementation, FrameCallable, FrameCallableEntry, FrameEntryCounter,
    ObjectRef, ObjectRefState,
};

#[test]
fn invocation_inputs_borrow_native_families_without_cloning_or_constraining_failures() {
    struct Failure(Rc<Cell<i64>>);
    struct FirstFrame(Rc<Cell<i64>>);
    struct SecondFrame(Rc<Cell<i64>>);
    struct CheckedEntry(Rc<Cell<usize>>);
    impl Clone for CheckedEntry {
        fn clone(&self) -> Self {
            self.0.set(self.0.get() + 1);
            Self(self.0.clone())
        }
    }
    impl PartialEq for CheckedEntry {
        fn eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.0, &other.0)
        }
    }
    impl Eq for CheckedEntry {}
    impl FrameCallableEntry<FirstFrame> for CheckedEntry {
        type Arguments = (bool,);
        type Result = Result<i64, Failure>;
        fn invoke(&self, frame: &Rc<FirstFrame>, arguments: Self::Arguments) -> Self::Result {
            assert_eq!(
                Rc::strong_count(frame),
                1,
                "input does not transiently clone its frame"
            );
            if arguments.0 {
                Err(Failure(frame.0.clone()))
            } else {
                Ok(frame.0.get())
            }
        }
    }
    impl FrameCallableEntry<SecondFrame> for CheckedEntry {
        type Arguments = (bool,);
        type Result = Result<i64, Failure>;
        fn invoke(&self, frame: &Rc<SecondFrame>, arguments: Self::Arguments) -> Self::Result {
            assert_eq!(
                Rc::strong_count(frame),
                1,
                "input does not transiently clone its frame"
            );
            if arguments.0 {
                Err(Failure(frame.0.clone()))
            } else {
                Ok(frame.0.get())
            }
        }
    }
    fn invoke(
        input: &impl CallableImplementation<(bool,), Result<i64, Failure>>,
        fail: bool,
    ) -> Result<i64, Failure> {
        input.invoke((fail,))
    }
    let value = Rc::new(Cell::new(9_007_199_254_740_993));
    let cloned = Rc::new(Cell::new(0));
    let first = FrameCallable::from_frame(
        Rc::new(FirstFrame(value.clone())),
        CheckedEntry(cloned.clone()),
    );
    let second = FrameCallable::from_frame(
        Rc::new(SecondFrame(value.clone())),
        CheckedEntry(cloned.clone()),
    );
    let ordinary = Callable::new(|_: (bool,)| -> Result<i64, Failure> { Ok(7) });
    for _ in 0..32 {
        assert_eq!(invoke(&first, false).ok(), Some(value.get()));
        assert_eq!(invoke(&second, false).ok(), Some(value.get()));
        assert_eq!(invoke(&ordinary, false).ok(), Some(7));
        let failure = invoke(&first, true).err().expect("exact typed failure");
        assert!(Rc::ptr_eq(&failure.0, &value));
    }
    assert_eq!(
        cloned.get(),
        0,
        "input does not transiently clone its entry"
    );
}
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

impl<Context> FrameCallableEntry<ObjectRefState<Frame, Context>> for Entry {
    type Arguments = i32;
    type Result = i32;

    fn invoke(&self, frame: &Rc<ObjectRefState<Frame, Context>>, count: i32) -> i32 {
        if count == 0 {
            match self {
                Self::Original => 1,
                Self::Replacement => 2,
            }
        } else {
            let selected = frame.with(|state| state.selected.get());
            selected.invoke(frame, count - 1)
        }
    }
}

#[test]
fn class_frame_entries_share_the_existing_object_owner_and_borrowed_context() {
    let drops = Rc::new(Cell::new(0));
    let context = String::from("borrowed class context");
    let instance = ObjectRef::with_context(
        Frame {
            selected: Cell::new(Entry::Original),
            drops: Rc::clone(&drops),
        },
        context.as_str(),
    );
    let address = Rc::as_ptr(instance.shared());
    assert_eq!(Rc::strong_count(instance.shared()), 1);
    let before = instance.with(|state| state.selected.get());
    for count in 0..100 {
        assert_eq!(before.invoke(instance.shared(), count), 1);
        assert_eq!(Rc::strong_count(instance.shared()), 1);
    }
    let callback = FrameCallable::from_frame(Rc::clone(instance.shared()), before);
    instance.with(|state| state.selected.set(Entry::Replacement));
    assert_eq!(callback.call(0), 1);
    assert_eq!(callback.call(8), 2);
    assert_eq!(Rc::as_ptr(callback.frame()), address);
    assert_eq!(callback.frame().context().as_ptr(), context.as_ptr());
    assert_eq!(Rc::strong_count(callback.frame()), 2);
    let alias = callback.clone();
    assert!(callback == alias);
    drop(alias);
    drop(instance);
    assert_eq!(Rc::strong_count(callback.frame()), 1);
    assert_eq!(drops.get(), 0);
    assert_eq!(callback.call(8), 2);
    drop(callback);
    assert_eq!(drops.get(), 1);
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
