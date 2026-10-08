use std::cell::Cell;
use std::rc::Rc;
use tsonic_rust_runtime::{
    Callable, FrameCallableEntry, FrameCallableFamily, FrameCallableFamilyEntry,
};

struct Frame {
    entry: std::cell::RefCell<Entry>,
    drops: Rc<Cell<usize>>,
}

impl Drop for Frame {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Entry {
    Recursive,
    Independent(Callable<(usize,), usize>),
}

impl FrameCallableEntry<Frame> for Entry {
    type Arguments = (usize,);
    type Result = usize;

    fn invoke(&self, frame: &Rc<Frame>, (count,): Self::Arguments) -> Self::Result {
        match self {
            Self::Recursive if count == 0 => 1,
            Self::Recursive => {
                let entry = frame.entry.borrow().clone();
                entry.invoke(frame, (count - 1,))
            }
            Self::Independent(value) => value.call((count,)),
        }
    }
}

impl FrameCallableFamilyEntry<Frame> for Entry {
    fn from_independent(value: Callable<Self::Arguments, Self::Result>) -> Self {
        Self::Independent(value)
    }

    fn into_independent(self) -> Result<Callable<Self::Arguments, Self::Result>, Self> {
        match self {
            Self::Independent(value) => Ok(value),
            entry => Err(entry),
        }
    }
}

#[test]
fn independent_reads_reuse_identity_without_retaining_the_frame() {
    let drops = Rc::new(Cell::new(0));
    let frame = Rc::new(Frame {
        entry: std::cell::RefCell::new(Entry::Recursive),
        drops: drops.clone(),
    });
    let before = FrameCallableFamily::from_entry(Entry::Recursive, || Rc::clone(&frame));
    let weak = Rc::downgrade(&frame);
    assert_eq!(Rc::strong_count(&frame), 2);
    let replacement = Callable::new(|_: (usize,)| 99);
    *frame.entry.borrow_mut() = Entry::from_independent(replacement.clone());
    let independent =
        FrameCallableFamily::from_entry(frame.entry.borrow().clone(), || Rc::clone(&frame));
    for _ in 0..1024 {
        assert_eq!(before.call((2,)), 99);
        assert_eq!(independent.call((2,)), 99);
        let reread = FrameCallableFamily::from_entry(frame.entry.borrow().clone(), || {
            panic!("an independent entry cannot demand a frame")
        });
        assert_eq!(independent, reread);
        assert_eq!(
            Rc::strong_count(&frame),
            2,
            "independent reads never clone the frame"
        );
    }
    let repeated =
        FrameCallableFamily::<Frame, Entry>::from_independent(Callable::new(|_: (usize,)| 99));
    assert_ne!(
        independent, repeated,
        "separate creations keep separate identity"
    );
    assert_ne!(
        independent, before,
        "retained and independent alternatives differ"
    );
    let transported = independent.clone().into_entry();
    *frame.entry.borrow_mut() = transported;
    assert_eq!(before.call((2,)), 99);
    drop(frame);
    assert!(
        weak.upgrade().is_some(),
        "the recursive root still owns its frame"
    );
    drop(before);
    assert!(
        weak.upgrade().is_none(),
        "internal entries cannot form a strong cycle"
    );
    assert_eq!(drops.get(), 1);
    assert_eq!(
        independent.call((2,)),
        99,
        "independent callback outlives the unrelated frame"
    );
}

#[test]
fn retained_entry_transport_drops_only_the_transferred_root() {
    let drops = Rc::new(Cell::new(0));
    let frame = Rc::new(Frame {
        entry: std::cell::RefCell::new(Entry::Recursive),
        drops: drops.clone(),
    });
    let first = FrameCallableFamily::from_entry(Entry::Recursive, || Rc::clone(&frame));
    let alias = first.clone();
    assert_eq!(first, alias);
    assert_eq!(Rc::strong_count(&frame), 3);
    *frame.entry.borrow_mut() = alias.into_entry();
    assert_eq!(Rc::strong_count(&frame), 2);
    assert_eq!(first.call((32,)), 1);
    drop(first);
    drop(frame);
    assert_eq!(drops.get(), 1);
}

#[test]
fn an_opaque_write_cannot_retarget_a_foreign_retaining_callback() {
    let drops = Rc::new(Cell::new(0));
    let first = Rc::new(Frame {
        entry: std::cell::RefCell::new(Entry::Recursive),
        drops: drops.clone(),
    });
    let second = Rc::new(Frame {
        entry: std::cell::RefCell::new(Entry::Recursive),
        drops: drops.clone(),
    });
    let foreign = FrameCallableFamily::from_entry(Entry::Recursive, || Rc::clone(&first));
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        foreign.into_entry_for(&second)
    }));
    assert!(
        rejected.is_err(),
        "foreign ownership never becomes same-instance recursion"
    );
    assert_eq!(Rc::strong_count(&first), 1);
    assert_eq!(Rc::strong_count(&second), 1);
    assert_eq!(
        FrameCallableFamily::<Frame, Entry>::from_independent(Callable::new(|_: (usize,)| 9))
            .into_entry_for(&second)
            .invoke(&second, (1,)),
        9
    );
    drop(first);
    drop(second);
    assert_eq!(drops.get(), 2);
}

#[test]
fn an_owned_receiver_moves_into_its_published_root_once() {
    let drops = Rc::new(Cell::new(0));
    let frame = Rc::new(Frame {
        entry: std::cell::RefCell::new(Entry::Recursive),
        drops: drops.clone(),
    });
    let weak = Rc::downgrade(&frame);
    let entry = frame.entry.borrow().clone();
    let calls = Cell::new(0);
    let retained = FrameCallableFamily::from_entry(entry, || {
        calls.set(calls.get() + 1);
        assert_eq!(
            Rc::strong_count(&frame),
            1,
            "the owned receiver needs no publication clone"
        );
        frame
    });
    assert_eq!(calls.get(), 1);
    assert_eq!(weak.strong_count(), 1);
    assert_eq!(retained.call((16,)), 1);
    drop(retained);
    assert_eq!(weak.strong_count(), 0);
    assert_eq!(drops.get(), 1);
}
