use tsonic_rust_runtime::{finish_resource, Completion, TsonicError, TsonicResult};

fn failure(message: &str) -> TsonicError {
    TsonicError::unsupported(message)
}

#[test]
fn resource_completion_preserves_normal_and_abrupt_control_flow() {
    assert_eq!(
        finish_resource::<i32>(Ok(Completion::Normal), Ok(())),
        Ok(Completion::Normal),
    );
    assert_eq!(
        finish_resource(Ok(Completion::Return(7)), Ok(())),
        Ok(Completion::Return(7)),
    );
    assert_eq!(
        finish_resource::<i32>(Ok(Completion::Break(3)), Ok(())),
        Ok(Completion::Break(3)),
    );
    assert_eq!(
        finish_resource::<i32>(Ok(Completion::Continue(4)), Ok(())),
        Ok(Completion::Continue(4)),
    );
}

#[test]
fn resource_completion_uses_cleanup_failure_and_suppresses_body_failure() {
    let cleanup_only: TsonicResult<Completion<()>> =
        finish_resource(Ok(Completion::Normal), Err(failure("cleanup")));
    assert_eq!(cleanup_only, Err(failure("cleanup")));

    let body_only = finish_resource::<()>(Err(failure("body")), Ok(()));
    assert_eq!(body_only, Err(failure("body")));

    let both = finish_resource::<()>(Err(failure("body")), Err(failure("cleanup")));
    assert_eq!(
        both,
        Err(TsonicError::suppressed(failure("cleanup"), failure("body"),)),
    );
}
