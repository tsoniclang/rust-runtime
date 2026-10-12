use core::convert::Infallible;
use core::future::Future;
use core::hint::black_box;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use tsonic_rust_runtime::CallableImplementation;

#[path = "../helpers/error_allocations.rs"]
mod allocations;

fn completed<Output>(future: impl Future<Output = Output>) -> Output {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the selected native future must complete synchronously"),
    }
}

fn make<'input>(
    text: &'input str,
) -> impl CallableImplementation<
    (usize,),
    Output = impl Future<Output = Result<usize, Infallible>> + 'input,
> + Clone
       + 'input {
    move |(value,)| async move { Ok(value + text.len()) }
}

fn wrap<Input, Output, Implementation>(
    callback: Implementation,
) -> impl CallableImplementation<(Input,), Output = impl Future<Output = Output>>
where
    Implementation: CallableImplementation<(Input,)>,
    Implementation::Output: Future<Output = Output>,
{
    move |(value,)| {
        let completion = callback.invoke((value,));
        async move { completion.await }
    }
}

struct Holder<Implementation> {
    callback: Implementation,
}

impl<Implementation> Holder<Implementation> {
    async fn invoke<Input, Output>(&self, value: Input) -> Output
    where
        Implementation: CallableImplementation<(Input,)>,
        Implementation::Output: Future<Output = Output>,
    {
        self.callback.invoke((value,)).await
    }
}

#[test]
fn anonymous_outputs_survive_return_forward_alias_fields_and_arrays_without_allocation() {
    let text = String::from("borrowed");
    allocations::ALLOCATIONS.with(|count| count.set(Some(0)));
    allocations::ALLOCATION_BYTES.with(|count| count.set(Some(0)));
    let callback = make(black_box(&text));
    let alias = callback.clone();
    let holder = Holder {
        callback: wrap(callback),
    };
    let stored = [wrap(alias)];
    for value in 0..10_000 {
        assert_eq!(completed(holder.invoke(black_box(value))), Ok(value + 8));
        assert_eq!(
            completed(stored[0].invoke((black_box(value),))),
            Ok(value + 8)
        );
    }
    let count = allocations::ALLOCATIONS.with(|count| count.replace(None).unwrap());
    let bytes = allocations::ALLOCATION_BYTES.with(|count| count.replace(None).unwrap());
    assert_eq!(count, 0);
    assert_eq!(bytes, 0);
    assert_eq!(
        core::mem::size_of_val(&holder),
        core::mem::size_of::<&str>()
    );
    assert_eq!(
        core::mem::size_of_val(&stored),
        core::mem::size_of::<&str>()
    );
}

fn invoke<Arguments, Implementation: CallableImplementation<Arguments>>(
    callback: &Implementation,
    arguments: Arguments,
) -> Implementation::Output {
    callback.invoke(arguments)
}

#[test]
fn associated_outputs_preserve_distinct_errors_and_pure_native_results() {
    #[derive(Debug, PartialEq)]
    struct FirstError(u64);
    #[derive(Debug, PartialEq)]
    struct SecondError(&'static str);
    let first = |(): ()| async { Err::<u64, _>(FirstError(u64::MAX)) };
    let second = |(): ()| async { Err::<u64, _>(SecondError("second")) };
    assert_eq!(completed(invoke(&first, ())), Err(FirstError(u64::MAX)));
    assert_eq!(completed(invoke(&second, ())), Err(SecondError("second")));
    let pure = |value: u64| value;
    assert_eq!(invoke(&pure, u64::MAX), u64::MAX);
    assert_eq!(core::mem::size_of_val(&pure), 0);
    assert_eq!(
        core::mem::size_of::<Result<u64, Infallible>>(),
        core::mem::size_of::<u64>()
    );
}

#[test]
fn universally_generic_and_borrowed_outputs_do_not_become_static_or_concrete() {
    struct Identity;
    impl<Value> CallableImplementation<Value> for Identity {
        type Output = Value;

        fn invoke(&self, value: Value) -> Value {
            value
        }
    }
    fn borrowed<'input>(
        callback: &impl for<'value> CallableImplementation<&'value str, Output = &'value str>,
        value: &'input str,
    ) -> &'input str {
        callback.invoke(value)
    }
    let text = String::from("borrowed");
    let result = borrowed(&Identity, &text);
    assert!(core::ptr::eq(result.as_ptr(), text.as_ptr()));
    assert_eq!(invoke(&Identity, u64::MAX), u64::MAX);
    assert_eq!(invoke(&Identity, false), false);
    assert_eq!(core::mem::size_of_val(&Identity), 0);
}

#[test]
fn cancelled_native_completion_keeps_and_releases_its_actual_owner() {
    use std::cell::Cell;
    use std::rc::Rc;

    struct Payload {
        drops: Rc<Cell<usize>>,
        text: String,
    }
    impl Drop for Payload {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let owner = Rc::new(Payload {
        drops: Rc::clone(&drops),
        text: String::from("one allocation for the native payload"),
    });
    let callback = move |(): ()| {
        let retained = Rc::clone(&owner);
        async move { retained.text.len() }
    };
    allocations::ALLOCATIONS.with(|count| count.set(Some(0)));
    let first = invoke(&callback, ());
    let second = invoke(&callback, ());
    drop(callback);
    assert_eq!(drops.get(), 0);
    drop(first);
    assert_eq!(drops.get(), 0);
    assert_eq!(completed(second), 37);
    assert_eq!(drops.get(), 1);
    let count = allocations::ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(count, 0);
}
