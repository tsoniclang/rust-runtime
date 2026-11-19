#[cfg(feature = "alloc")]
use crate::{TsonicError, TsonicResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completion<T, TNormal> {
    Normal(TNormal),
    Return(T),
    Break(u32),
    Continue(u32),
}

#[cfg(feature = "alloc")]
pub fn finish_resource<T, TNormal>(
    body: TsonicResult<Completion<T, TNormal>>,
    cleanup: TsonicResult<()>,
) -> TsonicResult<Completion<T, TNormal>> {
    match (body, cleanup) {
        (Ok(completion), Ok(())) => Ok(completion),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Ok(())) => Err(error),
        (Err(suppressed), Err(error)) => Err(TsonicError::suppressed(error, suppressed)),
    }
}

pub fn finish_finally<T, TNormal, TError>(
    body: Result<Completion<T, TNormal>, TError>,
    finally: Result<Completion<T, ()>, TError>,
) -> Result<Completion<T, TNormal>, TError> {
    match finally {
        Ok(Completion::Normal(())) => body,
        Ok(Completion::Return(value)) => Ok(Completion::Return(value)),
        Ok(Completion::Break(target)) => Ok(Completion::Break(target)),
        Ok(Completion::Continue(target)) => Ok(Completion::Continue(target)),
        Err(error) => Err(error),
    }
}
