use core::cell::RefCell;
use std::any::Any;
use std::rc::Rc;
use tsonic_rust_runtime::{
    ErrorField, ErrorObject, ErrorStack, JsError, JsErrorKind, MutableJsError, RetainedError,
    RetainedErrorObject, ToSourceString, TsonicError, WritableErrorObject, WritableRetainedError,
};

#[path = "../helpers/error_allocations.rs"]
mod error_allocations;
use error_allocations::{ALLOCATIONS, ALLOCATION_BYTES};

#[test]
fn consuming_projection_moves_the_original_owner_without_cloning_or_allocating() {
    for writable in [false, true] {
        let original = Rc::new(DetailedError {
            message: RefCell::new(String::from("owned")),
            code: u64::MAX,
        });
        let weak = Rc::downgrade(&original);
        let identity = original.error_identity_key();
        let mut selected: Option<Rc<DetailedError>> = None;
        let (_, allocations, bytes) = measured(|| {
            if writable {
                WritableRetainedError::Project(original).into_project_error(&mut selected);
            } else {
                RetainedError::Project(original).into_project_error(&mut selected);
            }
        });
        assert_eq!((allocations, bytes), (0, 0));
        let selected = selected.unwrap();
        assert_eq!(Rc::strong_count(&selected), 1);
        assert_eq!(selected.error_identity_key(), identity);
        assert_eq!(selected.code, u64::MAX);
        drop(selected);
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn writable_admission_moves_original_handles_and_rejects_readonly_without_loss() {
    let created = MutableJsError::error("created");
    let original = RetainedError::from(created.clone());
    let (writable, allocations, bytes) =
        measured(|| WritableRetainedError::try_from(original.clone()).unwrap());
    assert_eq!((allocations, bytes), (0, 0));
    assert_eq!(RetainedError::from(writable.clone()), original);
    writable.set_error_message(String::from("changed"));
    assert_eq!(created.error_message(), "changed");
    let native = RetainedError::from(TsonicError::Node {
        code: String::from("EIO"),
        source: JsError::error("context"),
    });
    let (rejected, allocations, bytes) =
        measured(|| WritableRetainedError::try_from(native.clone()).unwrap_err());
    assert_eq!((allocations, bytes), (0, 0));
    assert_eq!(rejected, native);
    let RetainedError::Runtime(context) = rejected else {
        panic!("native context was narrowed");
    };
    let TsonicError::Node { code, .. } = context.as_ref() else {
        panic!("native kind was lost");
    };
    assert_eq!(code, "EIO");
    assert_ne!(native, RetainedError::from(JsError::error("context")));
}

fn measured<Value>(operation: impl FnOnce() -> Value) -> (Value, usize, usize) {
    ALLOCATIONS.with(|count| count.set(Some(0)));
    ALLOCATION_BYTES.with(|bytes| bytes.set(Some(0)));
    let value = operation();
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    let bytes = ALLOCATION_BYTES.with(|bytes| bytes.replace(None).unwrap());
    (value, allocations, bytes)
}

struct DetailedError {
    message: RefCell<String>,
    code: u64,
}

impl ErrorObject for DetailedError {
    fn error_name(&self) -> ErrorField<'_> {
        ErrorField::Native("DetailedError")
    }
    fn error_message(&self) -> ErrorField<'_> {
        ErrorField::Project(core::cell::Ref::map(self.message.borrow(), String::as_str))
    }
    fn error_stack(&self) -> Option<ErrorField<'_>> {
        None
    }
    fn error_kind(&self) -> JsErrorKind {
        JsErrorKind::Error
    }
    fn error_identity_key(&self) -> usize {
        std::ptr::from_ref(self).addr()
    }
}

impl WritableErrorObject for DetailedError {
    fn set_error_name(&self, value: String) {
        assert_eq!(value, "DetailedError");
    }
    fn set_error_message(&self, value: String) {
        *self.message.borrow_mut() = value;
    }
    fn set_error_stack(&self, value: Option<String>) {
        assert!(value.is_none());
    }
}

impl RetainedErrorObject for DetailedError {
    fn project_error(self: Rc<Self>, output: &mut dyn Any) {
        if let Some(slot) = output.downcast_mut::<Option<Rc<Self>>>() {
            *slot = Some(self);
        }
    }
}

impl ErrorStack for DetailedError {
    fn set_stack(&self, value: Option<String>) {
        self.set_error_stack(value);
    }
}

impl ToSourceString for DetailedError {
    fn to_source_string(&self) -> String {
        format!("DetailedError: {}", self.error_message())
    }
}

#[test]
fn existing_native_and_created_handles_retain_identity_without_allocating() {
    let native = JsError::error("native");
    let native_identity = native.identity_key();
    let created = MutableJsError::error("created");
    let alias = created.clone();
    let created_identity = created.identity_key();
    let ((native, created), allocations, bytes) =
        measured(|| (RetainedError::from(native), RetainedError::from(created)));
    assert_eq!((allocations, bytes), (0, 0));
    let (_, allocations, bytes) = measured(|| {
        for _iteration in 0..10_000 {
            let native = native.clone();
            assert_eq!(native.error_identity_key(), native_identity);
            assert_eq!(native.error_message(), "native");
            assert!(native.writable_source_error_value().is_none());
            assert_eq!(
                native.native_error_value().unwrap().identity_key(),
                native_identity
            );
            let created = created.source_error_value().unwrap();
            assert_eq!(created.error_identity_key(), created_identity);
            assert_eq!(created.error_name(), "Error");
            assert_eq!(created.error_kind(), JsErrorKind::Error);
            assert!(created.error_stack().is_none());
            assert_eq!(
                created.mutable_error_value().unwrap().identity_key(),
                created_identity
            );
            assert_eq!(
                created
                    .writable_source_error_value()
                    .unwrap()
                    .error_identity_key(),
                created_identity
            );
            assert!(created.native_error_value().is_none());
        }
    });
    assert_eq!((allocations, bytes), (0, 0));
    created
        .writable_source_error_value()
        .unwrap()
        .set_error_message(String::from("updated"));
    assert_eq!(alias.error_message(), "updated");
    let writable = created.writable_source_error_value().unwrap();
    assert_eq!(
        writable.source_error().unwrap().error_identity_key(),
        created_identity
    );
    assert_eq!(
        writable.source_error_value().unwrap().error_identity_key(),
        created_identity
    );
    assert_eq!(
        writable
            .writable_source_error_value()
            .unwrap()
            .error_identity_key(),
        created_identity
    );
    assert_eq!(
        writable.mutable_error_value().unwrap().identity_key(),
        created_identity
    );
    assert!(writable.native_error_value().is_none());
    assert_eq!(writable.to_string(), "Error: updated");
}

#[test]
fn custom_objects_retain_exact_fields_mutation_and_checked_projection() {
    let original = Rc::new(DetailedError {
        message: RefCell::new(String::from("before")),
        code: 9007199254740993,
    });
    let weak = Rc::downgrade(&original);
    let identity = original.error_identity_key();
    let (retained, allocations, bytes) = measured(|| RetainedError::WritableProject(original));
    assert_eq!((allocations, bytes), (0, 0));
    let writable = retained.writable_source_error_value().unwrap();
    writable.set_error_message(String::from("after"));
    let (_, allocations, bytes) = measured(|| {
        for _iteration in 0..10_000 {
            let mut selected: Option<Rc<DetailedError>> = None;
            retained.project_error(&mut selected);
            let selected = selected.unwrap();
            assert_eq!(selected.code, 9007199254740993);
            assert_eq!(selected.error_identity_key(), identity);
            assert_eq!(selected.error_message(), "after");
            let mut wrong: Option<Rc<JsError>> = None;
            retained.project_error(&mut wrong);
            assert!(wrong.is_none());
            let mut writable_selected: Option<Rc<DetailedError>> = None;
            writable.project_error(&mut writable_selected);
            assert!(Rc::ptr_eq(&selected, &writable_selected.unwrap()));
            assert_eq!(
                retained
                    .clone()
                    .source_error()
                    .unwrap()
                    .error_identity_key(),
                identity
            );
        }
    });
    assert_eq!((allocations, bytes), (0, 0));
    assert!(writable.mutable_error_value().is_none());
    assert!(retained.mutable_error_value().is_none());
    assert!(writable.native_error_value().is_none());
    assert_eq!(writable.to_string(), "DetailedError: after");
    drop(writable);
    assert!(weak.upgrade().is_some());
    drop(retained);
    assert!(weak.upgrade().is_none());
}

#[test]
fn readonly_project_admission_preserves_payload_without_fabricating_writable_evidence() {
    let original = Rc::new(DetailedError {
        message: RefCell::new(String::from("readonly")),
        code: u64::MAX,
    });
    let identity = original.error_identity_key();
    let weak = Rc::downgrade(&original);
    let (retained, allocations, bytes) = measured(|| RetainedError::Project(original));
    assert_eq!((allocations, bytes), (0, 0));
    assert!(retained.writable_source_error_value().is_none());
    let mut selected: Option<Rc<DetailedError>> = None;
    retained.project_error(&mut selected);
    let selected = selected.unwrap();
    assert_eq!(selected.code, u64::MAX);
    assert_eq!(selected.error_identity_key(), identity);
    drop(selected);
    drop(retained);
    assert!(weak.upgrade().is_none());
}

#[test]
fn native_context_owns_all_payload_once_and_repeated_retention_is_allocation_free() {
    let source = JsError::error("native context");
    let identity = source.identity_key();
    let native = TsonicError::Node {
        code: String::from("NATIVE_CODE"),
        source,
    };
    let (retained, allocations, bytes) = measured(|| RetainedError::from(native));
    let (_, native_allocations, native_bytes) = measured(|| {
        std::sync::Arc::new(TsonicError::Node {
            code: String::new(),
            source: retained.native_error_value().unwrap(),
        })
    });
    assert_eq!((allocations, bytes), (native_allocations, native_bytes));
    let RetainedError::Runtime(context) = &retained else {
        panic!("native context was dropped");
    };
    let TsonicError::Node { code, .. } = context.as_ref() else {
        panic!("native Node context was changed");
    };
    assert_eq!(code, "NATIVE_CODE");
    let pointer = code.as_ptr();
    let (_, allocations, bytes) = measured(|| {
        for _iteration in 0..10_000 {
            let alias = retained.clone();
            assert_eq!(alias.error_identity_key(), identity);
            assert_eq!(alias.error_message(), "native context");
            assert_eq!(alias.native_error_value().unwrap().identity_key(), identity);
            let RetainedError::Runtime(context) = alias else {
                panic!("context variant changed");
            };
            let TsonicError::Node { code, .. } = context.as_ref() else {
                panic!("Node variant changed");
            };
            assert_eq!(code.as_ptr(), pointer);
            assert!(retained.writable_source_error_value().is_none());
        }
    });
    assert_eq!((allocations, bytes), (0, 0));
}

#[cfg(feature = "std")]
#[test]
fn explicit_retained_stack_assignment_preserves_original_handles() {
    use tsonic_rust_runtime::ErrorStack;
    let native = JsError::error("native");
    let retained = RetainedError::from(native.clone());
    retained.set_stack(Some(String::from("native stack")));
    assert_eq!(native.borrowed_stack().as_deref(), Some("native stack"));
    let created = MutableJsError::error("created");
    let retained = RetainedError::from(created.clone());
    retained.set_stack(Some(String::from("created stack")));
    assert_eq!(created.error_stack().as_deref(), Some("created stack"));
    retained
        .writable_source_error_value()
        .unwrap()
        .set_stack(None);
    assert!(created.error_stack().is_none());
}
