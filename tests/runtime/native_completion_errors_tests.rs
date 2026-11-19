use std::cell::Cell;
use tsonic_rust_runtime::{finish_finally, Completion};

struct BorrowedError<'scope>(&'scope mut u32);

trait Callback {
    type Error;
}

struct BorrowedCallback<'scope>(&'scope mut u32);

impl<'scope> Callback for BorrowedCallback<'scope> {
    type Error = BorrowedError<'scope>;
}

fn preserve_error<Producer: Callback>(
    error: Producer::Error,
) -> Result<Completion<(), ()>, Producer::Error> {
    finish_finally(Err(error), Ok(Completion::Normal(())))
}

#[test]
fn associated_errors_keep_non_clone_borrowed_payloads_without_error_trait_bounds() {
    let mut value = 7;
    let callback = BorrowedCallback(&mut value);
    let original = std::ptr::from_ref(&*callback.0);
    match preserve_error::<BorrowedCallback<'_>>(BorrowedError(callback.0)) {
        Err(error) => {
            assert_eq!(std::ptr::from_ref(&*error.0), original);
            *error.0 = 9;
        }
        Ok(_) => panic!("body error was lost"),
    }
    assert_eq!(value, 9);
}

struct Tracked<'scope> {
    identity: u32,
    drops: &'scope Cell<u32>,
}

impl Drop for Tracked<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn replacing_an_error_moves_the_selected_error_and_drops_each_error_once() {
    let prior_drops = Cell::new(0);
    let final_drops = Cell::new(0);
    let completed = finish_finally::<(), (), _>(
        Err(Tracked {
            identity: 1,
            drops: &prior_drops,
        }),
        Err(Tracked {
            identity: 2,
            drops: &final_drops,
        }),
    );
    assert_eq!(prior_drops.get(), 1);
    assert_eq!(final_drops.get(), 0);
    match completed {
        Err(error) => {
            assert_eq!(error.identity, 2);
            drop(error);
        }
        Ok(_) => panic!("finally error was lost"),
    }
    assert_eq!(prior_drops.get(), 1);
    assert_eq!(final_drops.get(), 1);
}

#[test]
fn abrupt_finally_replaces_and_drops_non_clone_errors_without_suppression() {
    for kind in 0..3 {
        let drops = Cell::new(0);
        let final_completion = match kind {
            0 => Completion::Return(7),
            1 => Completion::Break(3),
            _ => Completion::Continue(4),
        };
        let completed = finish_finally::<_, (), _>(
            Err(Tracked {
                identity: 1,
                drops: &drops,
            }),
            Ok(final_completion),
        );
        assert_eq!(drops.get(), 1);
        assert!(matches!(
            (kind, completed),
            (0, Ok(Completion::Return(7)))
                | (1, Ok(Completion::Break(3)))
                | (2, Ok(Completion::Continue(4)))
        ));
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn finally_error_replaces_every_prior_completion_without_copying_payloads() {
    for kind in 0..4 {
        let drops = Cell::new(0);
        let prior = match kind {
            0 => Completion::Normal(Tracked {
                identity: 1,
                drops: &drops,
            }),
            1 => Completion::Return(Tracked {
                identity: 2,
                drops: &drops,
            }),
            2 => Completion::Break(3),
            _ => Completion::Continue(4),
        };
        let mut value = 7;
        let completed = finish_finally(Ok(prior), Err(BorrowedError(&mut value)));
        assert_eq!(drops.get(), u32::from(kind < 2));
        match completed {
            Err(error) => *error.0 = 9,
            Ok(_) => panic!("finally error was lost"),
        }
        assert_eq!(value, 9);
        assert_eq!(drops.get(), u32::from(kind < 2));
    }
}
