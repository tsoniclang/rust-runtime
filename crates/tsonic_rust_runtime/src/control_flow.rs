use crate::{TsonicError, TsonicResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completion<T> {
    Normal,
    Return(T),
    Break(u32),
    Continue(u32),
}

pub fn finish_resource<T>(
    body: TsonicResult<Completion<T>>,
    cleanup: TsonicResult<()>,
) -> TsonicResult<Completion<T>> {
    match (body, cleanup) {
        (Ok(completion), Ok(())) => Ok(completion),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Ok(())) => Err(error),
        (Err(suppressed), Err(error)) => Err(TsonicError::suppressed(error, suppressed)),
    }
}

pub fn finish_finally<T>(
    body: TsonicResult<Completion<T>>,
    finally: TsonicResult<Completion<T>>,
) -> TsonicResult<Completion<T>> {
    match finally {
        Ok(Completion::Normal) => body,
        Ok(completion) => Ok(completion),
        Err(error) => Err(error),
    }
}
