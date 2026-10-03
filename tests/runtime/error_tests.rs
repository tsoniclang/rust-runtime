use core::cell::{Cell, Ref};
use std::alloc::{GlobalAlloc, Layout, System};
use tsonic_rust_runtime::error::{ErrorField, ErrorObject, MutableJsError, WritableErrorObject};
use tsonic_rust_runtime::{JsError, JsErrorKind, ToSourceString, TsonicError};

struct CountingAllocator;

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
    static ALLOCATION_BYTES: Cell<Option<usize>> = const { Cell::new(None) };
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATION_BYTES.with(|bytes| {
            if let Some(value) = bytes.get() {
                bytes.set(Some(value + layout.size()));
            }
        });
        ALLOCATIONS.with(|count| {
            if let Some(value) = count.get() {
                count.set(Some(value + 1));
            }
        });
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.with(|count| {
            if let Some(value) = count.get() {
                count.set(Some(value + 1));
            }
        });
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn source_error_borrowing_keeps_native_message_owner_and_allocates_nothing() {
    let original = JsError::error("stored native message");
    let alias = original.clone();
    let pointer = original.message().as_ptr();
    assert_eq!(
        core::mem::size_of::<JsError>(),
        core::mem::size_of::<usize>()
    );
    ALLOCATIONS.with(|count| count.set(Some(0)));
    for _ in 0..10_000 {
        let borrowed = std::hint::black_box(&alias).error_message();
        assert_eq!(borrowed, "stored native message");
        assert_eq!(borrowed.as_ptr(), pointer);
        assert_eq!(alias.error_name(), "Error");
        assert_eq!(alias.error_stack(), None);
        assert_eq!(alias.error_identity_key(), original.identity_key());
    }
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
}

#[test]
fn demanded_mutable_error_matches_one_handwritten_native_owner_allocation() {
    use std::borrow::Cow;
    use std::cell::RefCell;
    use std::rc::Rc;
    struct HandwrittenError {
        kind: JsErrorKind,
        name: Cow<'static, str>,
        message: String,
        stack: Option<String>,
    }
    let message = String::from("native moved message");
    let pointer = message.as_ptr();
    ALLOCATIONS.with(|count| count.set(Some(0)));
    ALLOCATION_BYTES.with(|bytes| bytes.set(Some(0)));
    let generated = MutableJsError::new(JsErrorKind::Error, message);
    let generated_allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    let generated_bytes = ALLOCATION_BYTES.with(|bytes| bytes.replace(None).unwrap());
    let native_message = String::from("native moved message");
    ALLOCATIONS.with(|count| count.set(Some(0)));
    ALLOCATION_BYTES.with(|bytes| bytes.set(Some(0)));
    let native = Rc::new(RefCell::new(HandwrittenError {
        kind: JsErrorKind::Error,
        name: Cow::Borrowed("Error"),
        message: native_message,
        stack: None,
    }));
    let native_allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    let native_bytes = ALLOCATION_BYTES.with(|bytes| bytes.replace(None).unwrap());
    assert_eq!(generated_allocations, 1);
    assert_eq!(generated_allocations, native_allocations);
    assert_eq!(generated_bytes, native_bytes);
    assert_eq!(
        core::mem::size_of_val(&generated),
        core::mem::size_of_val(&native)
    );
    assert_eq!(generated.error_message().as_ptr(), pointer);
    assert_eq!(generated.error_kind(), native.borrow().kind);
    assert_eq!(generated.error_name(), native.borrow().name.as_ref());
    assert_eq!(generated.error_message(), native.borrow().message.as_str());
    assert_eq!(
        generated.error_stack().as_deref(),
        native.borrow().stack.as_deref()
    );
}

#[test]
fn mutable_error_setters_keep_the_original_base_owner_and_nominal_kind_live() {
    let original = MutableJsError::error("original");
    let alias = original.clone();
    let identity = original.error_identity_key();
    let writable: &dyn WritableErrorObject = &alias;
    writable.set_error_name(String::from("ChangedName"));
    writable.set_error_message(String::from("changed"));
    writable.set_error_stack(Some(String::from("authored stack")));
    let readonly: &dyn ErrorObject = &original;
    assert_eq!(readonly.error_name(), "ChangedName");
    assert_eq!(readonly.error_message(), "changed");
    assert_eq!(readonly.error_stack().as_deref(), Some("authored stack"));
    assert_eq!(readonly.error_kind(), JsErrorKind::Error);
    assert_eq!(readonly.error_identity_key(), identity);
    assert!(original.has_same_identity(&alias));
    assert!(!original.has_same_identity(&MutableJsError::error("changed")));
    writable.set_error_stack(None);
    assert!(readonly.error_stack().is_none());
}

#[test]
fn mutable_error_pure_reads_retain_original_bytes_and_allocate_nothing() {
    let original = MutableJsError::error("original");
    let identity = original.error_identity_key();
    let pointer = original.error_message().as_ptr();
    ALLOCATIONS.with(|count| count.set(Some(0)));
    for _ in 0..10_000 {
        let error = std::hint::black_box(&original);
        assert_eq!(error.error_message().as_ptr(), pointer);
        assert_eq!(error.error_name(), "Error");
        assert_eq!(error.error_stack(), None);
        assert_eq!(error.error_identity_key(), identity);
    }
    assert_eq!(ALLOCATIONS.with(|count| count.replace(None).unwrap()), 0);
}

#[test]
fn mutable_error_borrow_consumption_precedes_callback_writes_without_changing_identity() {
    let original = MutableJsError::error("before");
    let alias = original.clone();
    let observe = |snapshot: String, callback: &dyn Fn()| {
        callback();
        snapshot
    };
    assert_eq!(
        observe(String::from(original.error_message()), &|| {
            alias.set_error_message(String::from("after"));
        }),
        "before"
    );
    assert_eq!(original.error_message(), "after");
    assert!(original.has_same_identity(&alias));
}

#[test]
fn mutable_error_stack_ownership_preserves_one_native_absence_state() {
    let original = MutableJsError::error("failure");
    ALLOCATIONS.with(|count| count.set(Some(0)));
    let absent = original.error_stack().map(String::from);
    assert_eq!(ALLOCATIONS.with(|count| count.replace(None).unwrap()), 0);
    assert_eq!(absent, None);
    original.set_error_stack(Some(String::from("stack")));
    assert_eq!(
        original.error_stack().map(String::from),
        Some(String::from("stack"))
    );
    original.set_error_stack(None);
    assert_eq!(original.error_stack().map(String::from), None);
}

#[cfg(feature = "std")]
#[test]
fn demanded_mutable_error_stack_capture_remains_explicit_and_guard_safe() {
    let original = MutableJsError::error("failure");
    let alias = original.clone();
    assert!(original.error_stack().is_none());
    original.set_error_stack(Some(String::from("original stack")));
    assert_eq!(String::from(original.error_stack().unwrap()), {
        tsonic_rust_runtime::capture_error_stack(&alias);
        String::from("original stack")
    });
    assert!(original.error_stack().is_some());
    assert!(original.has_same_identity(&alias));
}

#[test]
fn source_error_borrowed_fields_observe_the_original_mutable_owner_without_copies() {
    use tsonic_rust_runtime::ObjectHandle;
    let state = ObjectHandle::new((String::from("before"), String::from("OriginalError")));
    let alias = state.clone();
    let root = state.clone().into_shared();
    let count = std::rc::Rc::strong_count(&root);
    let pointer = state.with(|fields| fields.0.as_ptr());
    {
        let borrowed = ErrorField::Project(Ref::map(state.borrow(), |fields| fields.0.as_str()));
        assert_eq!(borrowed, "before");
        assert_eq!(borrowed.as_ptr(), pointer);
    }
    alias.with_mut(|fields| {
        fields.0 = String::from("after");
        fields.1 = String::from("ChangedError");
    });
    ALLOCATIONS.with(|count| count.set(Some(0)));
    for _ in 0..10_000 {
        let message = ErrorField::Project(Ref::map(state.borrow(), |fields| fields.0.as_str()));
        let name = ErrorField::Project(Ref::map(state.borrow(), |fields| fields.1.as_str()));
        assert_eq!(message, "after");
        assert_eq!(name, "ChangedError");
    }
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(std::rc::Rc::strong_count(&root), count);
    assert!(ObjectHandle::same(&state, &alias));
}

#[cfg(feature = "std")]
#[test]
fn borrowed_captured_stack_keeps_native_owner_and_copies_nothing() {
    use tsonic_rust_runtime::ErrorStack;
    let error = JsError::error("failure");
    error.set_stack(Some(String::from("stored native stack")));
    let alias = error.clone();
    ALLOCATIONS.with(|count| count.set(Some(0)));
    for _ in 0..10_000 {
        let stack = alias.error_stack().expect("explicitly set stack");
        assert_eq!(stack, "stored native stack");
        assert_eq!(stack.len(), 19);
    }
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
    alias.set_stack(Some(String::from("changed stack")));
    assert_eq!(error.error_stack().as_deref(), Some("changed stack"));
    alias.set_stack(None);
    assert!(error.error_stack().is_none());
}

#[test]
fn borrowed_project_stack_maps_present_original_field_without_allocating() {
    use tsonic_rust_runtime::ObjectHandle;
    let original = ObjectHandle::new(Some(String::from("authored stack")));
    let alias = original.clone();
    let read = || {
        Ref::filter_map(original.borrow(), |stack| stack.as_deref())
            .ok()
            .map(ErrorField::Project)
    };
    let before = original.with(|stack| stack.as_ref().unwrap().as_ptr());
    ALLOCATIONS.with(|count| count.set(Some(0)));
    for _ in 0..10_000 {
        let stack = read().expect("authored stack");
        assert_eq!(stack.as_ptr(), before);
        assert_eq!(stack, "authored stack");
        assert_eq!(stack.len(), 14);
    }
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
    alias.with_mut(|stack| *stack = Some(String::from("changed stack")));
    assert_eq!(read().as_deref(), Some("changed stack"));
    alias.with_mut(|stack| *stack = None);
    assert!(read().is_none());
}

#[test]
fn owned_error_field_consumes_project_guard_before_later_alias_mutation() {
    use tsonic_rust_runtime::ObjectHandle;
    let original = ObjectHandle::new(String::from("original"));
    let alias = original.clone();
    let read = || ErrorField::Project(Ref::map(original.borrow(), String::as_str));
    ALLOCATIONS.with(|count| count.set(Some(0)));
    let snapshot = String::from(read());
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 1);
    assert_eq!(snapshot, "original");
    assert_eq!(String::from(read()), {
        alias.with_mut(|message| *message = String::from("changed"));
        String::from("original")
    });
    assert_eq!(read(), "changed");
    assert!(ObjectHandle::same(&original, &alias));
}

#[cfg(feature = "std")]
#[test]
fn owned_error_stack_consumes_mutex_guard_before_later_explicit_capture() {
    use tsonic_rust_runtime::ErrorStack;
    let original = JsError::error("failure");
    let alias = original.clone();
    original.set_stack(Some(String::from("original stack")));
    assert_eq!(String::from(original.error_stack().unwrap()), {
        tsonic_rust_runtime::capture_error_stack(&alias);
        String::from("original stack")
    });
    let captured = original.error_stack().map(String::from);
    assert_eq!(original.error_stack().map(String::from), {
        alias.set_stack(None);
        captured
    });
    assert!(original.error_stack().is_none());
}

#[test]
fn absent_error_stack_ownership_allocates_nothing_and_releases_project_borrow() {
    use tsonic_rust_runtime::ObjectHandle;
    let original = ObjectHandle::new(None::<String>);
    let read = || {
        Ref::filter_map(original.borrow(), |stack| stack.as_deref())
            .ok()
            .map(ErrorField::Project)
    };
    ALLOCATIONS.with(|count| count.set(Some(0)));
    let snapshot = read().map(String::from);
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(snapshot, None);
    original.with_mut(|stack| *stack = Some(String::from("changed")));
    assert_eq!(read().as_deref(), Some("changed"));
}

#[test]
fn error_field_snapshot_survives_original_owner_release_without_retaining_it() {
    use tsonic_rust_runtime::ObjectHandle;
    let original = ObjectHandle::new(String::from("original"));
    let root = original.clone().into_shared();
    let owners = std::rc::Rc::strong_count(&root);
    let snapshot = String::from(ErrorField::Project(Ref::map(
        original.borrow(),
        String::as_str,
    )));
    assert_eq!(std::rc::Rc::strong_count(&root), owners);
    drop(original);
    drop(root);
    assert_eq!(snapshot, "original");
}

#[test]
fn error_kind_names_share_one_borrowed_and_display_contract() {
    for (kind, name) in [
        (JsErrorKind::Error, "Error"),
        (JsErrorKind::AggregateError, "AggregateError"),
        (JsErrorKind::EvalError, "EvalError"),
        (JsErrorKind::ReferenceError, "ReferenceError"),
        (JsErrorKind::TypeError, "TypeError"),
        (JsErrorKind::RangeError, "RangeError"),
        (JsErrorKind::SyntaxError, "SyntaxError"),
        (JsErrorKind::URIError, "URIError"),
        (JsErrorKind::SuppressedError, "SuppressedError"),
        (JsErrorKind::Unsupported, "Unsupported"),
    ] {
        assert_eq!(kind.as_str(), name);
        assert_eq!(kind.to_string(), name);
    }
}

#[test]
fn source_error_strings_retain_the_exact_runtime_display() {
    let error = JsError::new(JsErrorKind::TypeError, "invalid value");
    assert_eq!(error.to_source_string(), "TypeError: invalid value");
    assert_eq!(
        TsonicError::from(error).to_source_string(),
        "TypeError: invalid value"
    );
    let node = TsonicError::Node {
        code: "ENOENT".into(),
        source: JsError::error("missing"),
    };
    assert_eq!(node.to_source_string(), "ENOENT: missing");
}

#[test]
fn unsupported_error_is_closed_and_displayable() {
    let error = TsonicError::unsupported("dynamic eval is unavailable");
    assert_eq!(
        error,
        TsonicError::Unsupported {
            source: JsError::new(JsErrorKind::Unsupported, "dynamic eval is unavailable")
        }
    );
    assert_eq!(
        error.to_string(),
        "Unsupported: dynamic eval is unavailable"
    );
}

#[test]
fn js_error_accessors_and_conversion_are_closed() {
    let error = JsError::new(JsErrorKind::Unsupported, "not implemented");
    assert_eq!(error.kind(), JsErrorKind::Unsupported);
    assert_eq!(error.message(), "not implemented");
    assert_eq!(TsonicError::from(error.clone()), TsonicError::Js(error));
}

#[test]
fn transported_error_preserves_identity_and_explicit_stack() {
    let original = JsError::new(JsErrorKind::RangeError, "stored failure");
    assert_eq!(original.stack(), None);
    #[cfg(feature = "std")]
    tsonic_rust_runtime::capture_error_stack(&original);
    let stack = original.stack();
    let transport = TsonicError::from(original.clone());
    let rethrown = transport.clone();
    let TsonicError::Js(restored) = rethrown else {
        panic!("native Error changed transport variant");
    };
    assert!(restored.has_same_identity(&original));
    assert_eq!(restored.kind(), JsErrorKind::RangeError);
    assert_eq!(restored.message(), "stored failure");
    assert_eq!(restored.stack(), stack);
}

#[test]
fn base_error_kind_displays_as_error() {
    let error = JsError::new(JsErrorKind::Error, "boom");
    assert_eq!(error.kind(), JsErrorKind::Error);
    assert_eq!(format!("{error}"), "Error: boom");
    let unified: TsonicError = error.into();
    assert_eq!(format!("{unified}"), "Error: boom");
}

#[test]
fn every_native_error_retains_one_observable_source_identity() {
    let original = JsError::new(JsErrorKind::TypeError, "failure");
    for error in [
        TsonicError::from(original.clone()),
        TsonicError::Node {
            code: "ENOENT".into(),
            source: JsError::error("missing"),
        },
        TsonicError::unsupported("unsupported"),
        TsonicError::suppressed(original.clone().into(), original.clone().into()),
    ] {
        let source = error.error_value();
        assert_eq!(source.stack(), None);
        let stack = source.stack();
        let rethrown = error.clone();
        assert!(rethrown.is_error());
        assert!(rethrown.is_error_kind(source.kind()));
        assert!(rethrown.error_value().has_same_identity(&source));
        assert_eq!(rethrown.error_value().stack(), stack);
    }
}

#[test]
fn error_identity_is_distinct_from_diagnostic_equality() {
    let original = JsError::error("failure");
    let alias = original.clone();
    let independent = JsError::error("failure");
    assert!(original.has_same_identity(&alias));
    assert!(!original.has_distinct_identity(&alias));
    assert!(original.has_distinct_identity(&independent));
    assert!(!original.has_same_identity(&independent));
    assert_eq!(original, independent);
}

#[test]
fn aliases_and_error_transport_share_message_storage() {
    let original = JsError::error(&"long message".repeat(100));
    let alias = original.clone();
    let transported = TsonicError::from(original.clone()).error_value();
    assert!(core::ptr::eq(original.message(), alias.message()));
    assert!(core::ptr::eq(original.message(), transported.message()));
}

#[cfg(feature = "std")]
#[inline(never)]
fn create_error_at_origin() -> JsError {
    let error = JsError::new(JsErrorKind::TypeError, "invalid 😀 value");
    tsonic_rust_runtime::capture_error_stack(&error);
    error
}

#[cfg(feature = "std")]
#[inline(never)]
fn read_error_stack_elsewhere(error: &JsError) -> Option<String> {
    error.stack()
}

#[cfg(feature = "std")]
#[test]
fn error_stack_retains_explicit_capture_frames_across_aliases_and_later_reads() {
    let error = create_error_at_origin();
    let alias = error.clone();
    let stack = read_error_stack_elsewhere(&alias).expect("native test backtrace is available");
    assert!(stack.starts_with("TypeError: invalid 😀 value\n"));
    assert!(stack.contains("create_error_at_origin"));
    assert!(!stack.contains("read_error_stack_elsewhere"));
    assert_eq!(error.stack().as_deref(), Some(stack.as_str()));
    assert!(error.has_same_identity(&alias));
    assert!(!error.has_same_identity(&create_error_at_origin()));
}

#[cfg(feature = "std")]
#[test]
fn empty_error_stack_header_has_no_invented_colon_or_message() {
    let error = JsError::error("");
    assert_eq!(error.stack(), None);
    tsonic_rust_runtime::capture_error_stack(&error);
    assert!(error
        .stack()
        .expect("native test backtrace is available")
        .starts_with("Error\n"));
}

#[test]
fn ordinary_error_reads_and_transport_do_not_capture() {
    for kind in [
        JsErrorKind::Error,
        JsErrorKind::TypeError,
        JsErrorKind::RangeError,
        JsErrorKind::URIError,
    ] {
        let error = JsError::new(kind, "failure");
        assert_eq!(error.stack(), None);
        let alias = error.clone();
        let transported = TsonicError::from(error.clone());
        assert_eq!(alias.stack(), None);
        assert_eq!(transported.error_value().stack(), None);
        assert!(transported.error_value().has_same_identity(&error));
    }
}

#[cfg(feature = "std")]
#[test]
fn explicit_capture_after_aliasing_replaces_previous_snapshot() {
    let error = create_error_at_origin();
    let alias = error.clone();
    let previous = alias.stack();
    tsonic_rust_runtime::capture_error_stack(&alias);
    let current = error.stack().expect("native test backtrace is available");
    assert_ne!(Some(&current), previous.as_ref());
    assert!(current.contains("explicit_capture_after_aliasing_replaces_previous_snapshot"));
    assert!(!current.contains("create_error_at_origin"));
    assert_eq!(alias.stack().as_ref(), Some(&current));
}

#[cfg(not(feature = "std"))]
#[test]
fn alloc_only_errors_report_stack_unavailability() {
    let error = JsError::error("alloc only");
    assert_eq!(error.stack(), None);
    assert!(error.has_same_identity(&error.clone()));
}
