use tsonic_rust_runtime::Callable;
use std::cell::Cell;
use std::rc::Rc;
use tsonic_rust_runtime::CallableImplementation;

struct RetainedState {
    text: String,
    calls: Cell<usize>,
    drops: Rc<Cell<usize>>,
}

impl Drop for RetainedState {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

struct RetainedFrame {
    state: RetainedState,
    owner: std::rc::Weak<Self>,
}

impl CallableImplementation<(), Box<dyn FnOnce() -> String>> for RetainedFrame {
    fn invoke(&self, (): ()) -> Box<dyn FnOnce() -> String> {
        let owner = self.owner.upgrade().expect("live invocation");
        self.state.calls.set(self.state.calls.get() + 1);
        Box::new(move || format!("{}:{}", owner.state.text, owner.state.calls.get()))
    }
}

#[test]
fn suspended_invocations_retain_one_state_without_cloning_its_payload() {
    let drops = Rc::new(Cell::new(0));
    let callback = Callable::from_shared(Rc::new_cyclic(|owner| RetainedFrame {
        state: RetainedState {
            text: String::from("payload"),
            calls: Cell::new(0),
            drops: drops.clone(),
        },
        owner: owner.clone(),
    }));
    let first = callback.call(());
    let second = callback.call(());
    drop(callback);
    assert_eq!(drops.get(), 0);
    assert_eq!(first(), "payload:2");
    assert_eq!(drops.get(), 0);
    assert_eq!(second(), "payload:2");
    assert_eq!(drops.get(), 1);
}

#[test]
fn retained_state_keeps_the_exact_callable_identity() {
    let implementation = Rc::new(|value: i32| value + 1);
    let callback = Callable::from_shared(implementation.clone());
    let alias = Callable::from_shared(implementation.clone());
    assert_eq!(callback.identity_key(), Rc::as_ptr(&implementation) as usize);
    assert!(callback == alias);
    assert_eq!(callback.call(3), 4);
}

#[test]
fn retained_state_does_not_widen_borrowed_argument_or_result_lifetimes() {
    struct BorrowedFrame;
    impl<'value> CallableImplementation<&'value str, &'value str> for BorrowedFrame {
        fn invoke(&self, value: &'value str) -> &'value str { value }
    }
    let text = String::from("borrowed");
    let callback = Callable::from_shared(Rc::new(BorrowedFrame));
    assert_eq!(callback.call(&text), "borrowed");
}

#[test]
fn callable_does_not_require_borrowed_arguments_or_results_to_be_static() {
    let text = String::from("borrowed value");
    let identity = Callable::new(|(value,): (&str,)| value);
    let alias = identity.clone();
    assert_eq!(identity.call((&text,)), "borrowed value");
    assert_eq!(alias.call((&text,)), "borrowed value");
    assert!(Callable::same(&identity, &alias));
    assert_eq!(identity.identity_key(), alias.identity_key());
}

#[test]
fn callable_invokes_and_clones_one_identity() {
    let callable = Callable::new(|(left, right): (i32, i32)| left + right);
    let alias = callable.clone();
    let distinct = Callable::new(|(left, right): (i32, i32)| left + right);

    assert_eq!(callable.call((2, 3)), 5);
    assert!(Callable::same(&callable, &alias));
    assert!(!Callable::same(&callable, &distinct));
    assert!(callable == alias);
    assert!(callable != distinct);
    assert_eq!(callable.identity_key(), alias.identity_key());
    assert_ne!(callable.identity_key(), distinct.identity_key());
}

#[test]
fn recursive_callable_receives_its_exact_identity() {
    let factorial = Callable::recursive(|factorial, value: i32| {
        if value <= 1 {
            1
        } else {
            value * factorial.call(value - 1)
        }
    });

    assert_eq!(factorial.call(5), 120);
}
