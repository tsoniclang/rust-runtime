use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::rc::Rc;

use tsonic_rust_runtime::ts_value::{native_values_equal, native_values_not_equal};
#[cfg(feature = "std")]
use tsonic_rust_runtime::ErrorStack;
use tsonic_rust_runtime::{
    clone_ts_value, BigInt, EmptyObject, ErrorObject, JsError, JsErrorKind, Location,
    MutableJsError, NativePayload, ObjectHandle, ObjectIdentity, ObjectRef, OptionalStorage,
    TsValue, WritableErrorObject, WritableRetainedError,
};

#[test]
fn closed_native_nominal_queries_retain_the_original_shared_owner_without_allocation() {
    struct DropProbe(Rc<Cell<usize>>);
    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let original = ObjectHandle::new(DropProbe(Rc::clone(&drops)));
    let owner = original.clone().into_shared();
    let retained = TsValue::from(original.clone());
    TRACKED_ALLOCATIONS.with(|count| count.set(Some(0)));
    let recovered = retained
        .native_shared::<tsonic_rust_runtime::ObjectHandleState<DropProbe>>()
        .unwrap();
    let mismatch = retained.native_shared::<tsonic_rust_runtime::ObjectRefState<DropProbe>>();
    let allocations = TRACKED_ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert!(Rc::ptr_eq(&owner, &recovered));
    assert!(mismatch.is_none());
    let projected = ObjectHandle::from_shared(recovered);
    assert!(native_values_equal(
        &retained,
        &TsValue::from(projected.clone())
    ));
    drop(original);
    drop(owner);
    drop(retained);
    assert_eq!(drops.get(), 0);
    drop(projected);
    assert_eq!(drops.get(), 1);
}

#[test]
fn closed_native_value_queries_use_exact_native_types_without_allocating() {
    #[derive(Clone, PartialEq, Debug)]
    struct Record(u64);
    let original = Record(u64::MAX);
    let retained = TsValue::from_closed(original.clone());
    TRACKED_ALLOCATIONS.with(|count| count.set(Some(0)));
    let recovered = retained.native_value::<Record>();
    let mismatch = retained.native_value::<u64>();
    let shared = retained.native_shared::<Record>();
    let allocations = TRACKED_ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(recovered, Some(original));
    assert!(mismatch.is_none());
    assert!(shared.is_none());
}

struct CountingAllocator;

thread_local! {
    static TRACKED_ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
    static TRACKED_BYTES: Cell<Option<usize>> = const { Cell::new(None) };
    static TRACKED_ALIGNMENT: Cell<Option<usize>> = const { Cell::new(None) };
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        TRACKED_ALLOCATIONS.with(|count| {
            if let Some(value) = count.get() {
                count.set(Some(value + 1));
            }
        });
        TRACKED_BYTES.with(|count| {
            if let Some(value) = count.get() {
                count.set(Some(value + layout.size()));
            }
        });
        TRACKED_ALIGNMENT.with(|alignment| {
            if let Some(value) = alignment.get() {
                alignment.set(Some(value.max(layout.align())));
            }
        });
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn measured_allocation<Output>(
    operation: impl FnOnce() -> Output,
) -> (Output, usize, usize, usize) {
    TRACKED_ALLOCATIONS.with(|count| count.set(Some(0)));
    TRACKED_BYTES.with(|count| count.set(Some(0)));
    TRACKED_ALIGNMENT.with(|alignment| alignment.set(Some(0)));
    let output = operation();
    let allocations = TRACKED_ALLOCATIONS.with(|count| count.replace(None).unwrap());
    let bytes = TRACKED_BYTES.with(|count| count.replace(None).unwrap());
    let alignment = TRACKED_ALIGNMENT.with(|value| value.replace(None).unwrap());
    (output, allocations, bytes, alignment)
}

#[test]
fn native_payload_layout_matches_one_erased_rc_without_growing_ts_value() {
    use std::mem::{align_of, size_of};
    use tsonic_rust_runtime::ObjectIdentityCarrier;
    assert_eq!(
        size_of::<NativePayload>(),
        size_of::<Rc<dyn ObjectIdentityCarrier>>()
    );
    assert_eq!(
        align_of::<NativePayload>(),
        align_of::<Rc<dyn ObjectIdentityCarrier>>()
    );
    if usize::BITS == 64 {
        assert_eq!(size_of::<NativePayload>(), 16);
        assert_eq!(align_of::<NativePayload>(), 8);
        assert_eq!(size_of::<TsValue>(), 32);
        assert_eq!(align_of::<TsValue>(), 8);
    }
}

#[test]
fn native_payload_and_ts_value_allocate_exactly_one_native_owner() {
    #[repr(align(64))]
    struct Aligned([u8; 64]);
    fn check<Payload: 'static>(create: impl Fn() -> Payload) {
        let direct = create();
        let native = create();
        let closed = create();
        let (direct, direct_count, direct_bytes, direct_alignment) =
            measured_allocation(|| Rc::new(direct));
        let (native, native_count, native_bytes, native_alignment) =
            measured_allocation(|| NativePayload::from_closed(native));
        let (closed, closed_count, closed_bytes, closed_alignment) =
            measured_allocation(|| TsValue::from_closed(closed));
        assert_eq!(direct_count, 1);
        assert_eq!(native_count, 1);
        assert_eq!(closed_count, 1);
        assert_eq!(native_bytes, direct_bytes);
        assert_eq!(closed_bytes, direct_bytes);
        assert_eq!(native_alignment, direct_alignment);
        assert_eq!(closed_alignment, direct_alignment);
        assert_ne!(native.identity_key(), 0);
        drop((direct, native, closed));
    }
    check(|| ());
    check(|| u64::MAX);
    check(|| u128::MAX);
    check(|| [7_u8; 256]);
    check(|| String::from("owned native string"));
    check(|| Aligned([7; 64]));
    assert_eq!(Aligned([7; 64]).0[0], 7);
}

#[test]
fn native_payload_clones_only_on_exact_recovery_and_drops_after_last_owner() {
    struct Probe {
        clones: Rc<Cell<usize>>,
        drops: Rc<Cell<usize>>,
        value: u64,
    }
    impl Clone for Probe {
        fn clone(&self) -> Self {
            self.clones.set(self.clones.get() + 1);
            Self {
                clones: self.clones.clone(),
                drops: self.drops.clone(),
                value: self.value,
            }
        }
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    let clones = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let value = NativePayload::from_closed(Probe {
        clones: clones.clone(),
        drops: drops.clone(),
        value: u64::MAX,
    });
    let identity = value.identity_key();
    let (alias, count, bytes, _) = measured_allocation(|| value.clone());
    assert_eq!(count, 0);
    assert_eq!(bytes, 0);
    assert_eq!(clones.get(), 0);
    assert_eq!(alias.identity_key(), identity);
    let (wrong, count, bytes, _) = measured_allocation(|| alias.native_value::<u64>());
    assert!(wrong.is_none());
    assert_eq!(count, 0);
    assert_eq!(bytes, 0);
    assert_eq!(clones.get(), 0);
    let (recovered, count, bytes, _) = measured_allocation(|| alias.native_value::<Probe>());
    assert_eq!(count, 0);
    assert_eq!(bytes, 0);
    assert_eq!(clones.get(), 1);
    let recovered = recovered.unwrap();
    assert_eq!(recovered.value, u64::MAX);
    drop(recovered);
    assert_eq!(drops.get(), 1);
    drop(value);
    assert_eq!(drops.get(), 1);
    drop(alias);
    assert_eq!(drops.get(), 2);
}

#[test]
fn passive_payload_identity_stays_distinct_from_semantic_object_identity() {
    let identity = ObjectIdentity::new();
    let passive = TsValue::from_closed(identity.clone());
    let passive_alias = passive.clone();
    let separate_passive = TsValue::from_closed(identity.clone());
    let semantic = TsValue::from_identity(identity.clone());
    let direct = TsValue::from(identity.clone());
    let separate_semantic = TsValue::from_identity(identity.clone());
    assert!(native_values_equal(&passive, &passive_alias));
    assert!(!native_values_equal(&passive, &separate_passive));
    assert!(!native_values_equal(&passive, &semantic));
    assert!(!native_values_equal(&semantic, &passive));
    assert!(!native_values_equal(&passive, &direct));
    assert!(native_values_equal(&semantic, &direct));
    assert!(native_values_equal(&semantic, &separate_semantic));
    let recovered = semantic.native_value::<ObjectIdentity>().unwrap();
    assert_eq!(recovered.key(), identity.key());
    let (native_owner, direct_count, direct_bytes, direct_alignment) =
        measured_allocation(|| Rc::new(identity.clone()));
    let (retained, count, bytes, alignment) =
        measured_allocation(|| TsValue::from_identity(identity.clone()));
    assert_eq!(count, 1);
    assert_eq!(count, direct_count);
    assert_eq!(bytes, direct_bytes);
    assert_eq!(alignment, direct_alignment);
    assert!(native_values_equal(&retained, &direct));
    drop(native_owner);
}

#[test]
fn exact_string_recovery_has_only_the_requested_native_clone_cost() {
    let original = String::from("one explicitly requested native string clone");
    let payload = NativePayload::from_closed(original.clone());
    let retained = TsValue::from_closed(original.clone());
    let (direct, direct_count, direct_bytes, direct_alignment) =
        measured_allocation(|| original.clone());
    let (recovered, count, bytes, alignment) =
        measured_allocation(|| payload.native_value::<String>().unwrap());
    assert_eq!(count, 1);
    assert_eq!(count, direct_count);
    assert_eq!(bytes, direct_bytes);
    assert_eq!(alignment, direct_alignment);
    assert_eq!(recovered, direct);
    let (recovered, count, bytes, alignment) =
        measured_allocation(|| retained.native_value::<String>().unwrap());
    assert_eq!(count, direct_count);
    assert_eq!(bytes, direct_bytes);
    assert_eq!(alignment, direct_alignment);
    assert_eq!(recovered, direct);
}

#[test]
fn deferred_location_uses_one_activation_and_no_per_access_allocation() {
    TRACKED_ALLOCATIONS.with(|count| count.set(Some(0)));
    let location = Location::<u64>::uninitialized();
    let creation_allocations = TRACKED_ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(creation_allocations, 1);
    location.store(u64::MAX);
    TRACKED_ALLOCATIONS.with(|count| count.set(Some(0)));
    let alias = location.clone();
    for _ in 0..10_000 {
        assert_eq!(black_box(alias.load()), u64::MAX);
        location.store(u64::MAX);
    }
    let access_allocations = TRACKED_ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(access_allocations, 0);
}

#[derive(Clone)]
struct DropProbe(Rc<Cell<u32>>);

impl Drop for DropProbe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn closed_values_remain_alive_until_the_last_passive_carrier_is_dropped() {
    let drops = Rc::new(Cell::new(0));
    let value = TsValue::from_closed(DropProbe(Rc::clone(&drops)));
    assert_eq!(drops.get(), 0);

    let alias = clone_ts_value(&value);
    drop(value);
    assert_eq!(drops.get(), 0);

    drop(alias);
    assert_eq!(drops.get(), 1);
}

#[test]
fn debug_output_does_not_inspect_the_closed_value() {
    let value = TsValue::from_closed(DropProbe(Rc::new(Cell::new(0))));
    assert_eq!(format!("{value:?}"), "TsValue");
}

#[test]
fn error_admission_and_consuming_recovery_retain_live_storage_without_allocation() {
    let original = MutableJsError::error("before");
    let alias = original.clone();
    let identity = original.error_identity_key();
    TRACKED_ALLOCATIONS.with(|count| count.set(Some(0)));
    let value = TsValue::from_error(original);
    let copied = value.clone();
    let recovered = value.into_error().unwrap();
    let allocations = TRACKED_ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(recovered.error_identity_key(), identity);
    assert_eq!(copied.as_error().unwrap().error_identity_key(), identity);
    assert!(native_values_equal(
        &copied,
        &TsValue::from_error(alias.clone())
    ));
    let writable = WritableRetainedError::try_from(recovered).unwrap();
    writable.set_error_name(String::from("ChangedError"));
    writable.set_error_message(String::from("after"));
    writable.set_error_stack(Some(String::from("explicit stack")));
    assert_eq!(alias.error_name(), "ChangedError");
    assert_eq!(copied.error_value().error_message(), "after");
    assert_eq!(
        copied.error_value().error_stack().unwrap(),
        "explicit stack"
    );
}

#[test]
fn immutable_errors_preserve_kind_identity_and_explicit_stack() {
    let original = JsError::new(JsErrorKind::TypeError, "native");
    #[cfg(feature = "std")]
    original.set_stack(Some(String::from("selected stack")));
    let alias = original.clone();
    let value = TsValue::from_error(original);
    assert!(value.is_error());
    assert!(value.is_error_kind(JsErrorKind::TypeError));
    assert!(!value.is_error_kind(JsErrorKind::RangeError));
    assert_eq!(value.type_of(), "object");
    let recovered = value.into_error().unwrap();
    assert_eq!(recovered.error_identity_key(), alias.identity_key());
    #[cfg(feature = "std")]
    assert_eq!(recovered.error_stack().unwrap(), "selected stack");
    assert!(WritableRetainedError::try_from(recovered).is_err());
}

#[test]
fn non_error_recovery_returns_the_original_native_payload_and_absence() {
    for value in [
        TsValue::default(),
        TsValue::from(false),
        TsValue::from(i64::MIN),
        TsValue::from(u64::MAX),
        TsValue::from(9_007_199_254_740_993_u64),
        TsValue::from(0.0_f64),
    ] {
        assert!(!value.is_error());
        assert!(value.as_error().is_none());
        let alias = value.clone();
        TRACKED_ALLOCATIONS.with(|count| count.set(Some(0)));
        let returned = value.into_error().unwrap_err();
        let allocations = TRACKED_ALLOCATIONS.with(|count| count.replace(None).unwrap());
        assert_eq!(allocations, 0);
        assert_eq!(returned, alias);
    }
    let text = String::from("retained native allocation");
    let pointer = text.as_ptr();
    let returned = TsValue::from(text).into_error().unwrap_err();
    assert_eq!(returned.as_str().unwrap().as_ptr(), pointer);
    let drops = Rc::new(Cell::new(0));
    let value = TsValue::from_closed(DropProbe(drops.clone()));
    let returned = value.into_error().unwrap_err();
    assert_eq!(drops.get(), 0);
    drop(returned);
    assert_eq!(drops.get(), 1);
}

#[test]
fn native_scalars_compare_without_losing_integer_precision() {
    assert_eq!(TsValue::from(0_i32), TsValue::from(0.0_f64));
    assert_ne!(TsValue::from(false), TsValue::from(0_i32));
    assert_eq!(TsValue::from(u64::MAX), TsValue::from(u64::MAX));
    assert_ne!(
        TsValue::from(9_007_199_254_740_993_u64),
        TsValue::from(9_007_199_254_740_992.0_f64)
    );
    assert_ne!(TsValue::from(-1_i64), TsValue::from(u64::MAX));
    assert_ne!(TsValue::from(f64::NAN), TsValue::from(f64::NAN));
    assert_eq!(TsValue::from(-0.0_f64), TsValue::from(0.0_f64));
    assert_eq!(
        TsValue::from(String::from("native")),
        TsValue::from(String::from("native"))
    );
    assert_ne!(TsValue::from(String::from("0")), TsValue::from(0_i32));
}

#[test]
fn native_string_admission_moves_the_original_buffer() {
    let text = String::from("no extra allocation or string copy");
    let address = text.as_ptr();
    let value = TsValue::from(text);
    assert_eq!(value.as_str().unwrap().as_ptr(), address);
    assert_eq!(value.type_of(), "string");
    assert_eq!(TsValue::from(u64::MAX).type_of(), "bigint");
    assert_eq!(TsValue::from(1_i32).type_of(), "number");
}

#[test]
fn source_absence_has_one_state_and_keeps_false_zero_and_empty_present() {
    let absent = <TsValue as OptionalStorage<TsValue>>::absent();
    assert_eq!(absent, TsValue::from(()));
    assert!(absent.is_absent());
    for value in [
        TsValue::from(false),
        TsValue::from(0_i32),
        TsValue::from(String::new()),
    ] {
        assert!(!value.is_absent());
        assert_eq!(value.clone_present(), value);
        assert_eq!(value.clone().into_present(), value);
    }
}

#[test]
fn native_identity_is_preserved_across_separate_admissions() {
    let object = ObjectIdentity::new();
    let left = TsValue::from(object.clone());
    let right = TsValue::from(object);
    let different = TsValue::from(ObjectIdentity::new());
    assert_eq!(left, right);
    assert_ne!(left, different);
}

#[test]
fn identity_payload_retention_preserves_native_identity_across_passive_aliases() {
    let original = ObjectIdentity::new();
    let retained = TsValue::from_identity(original.clone());
    let direct = TsValue::from(original);
    let different = TsValue::from_identity(ObjectIdentity::new());
    assert_eq!(retained, direct);
    assert_ne!(retained, different);
    TRACKED_ALLOCATIONS.with(|count| count.set(Some(0)));
    for _iteration in 0..10_000 {
        let alias = retained.clone();
        assert!(native_values_equal(black_box(&alias), &direct));
    }
    assert_eq!(
        TRACKED_ALLOCATIONS.with(|count| count.replace(None).unwrap()),
        0
    );
}

#[test]
fn borrowed_native_comparisons_do_not_allocate_or_copy_strings_and_bigints() {
    let text = String::from("a long native UTF-8 string without a comparison copy");
    let boxed_text = TsValue::from(text.clone());
    let integer = BigInt::from_decimal_literal("18446744073709551615");
    let boxed_integer = TsValue::from(integer.clone());
    let wide = TsValue::from(u64::MAX);
    TRACKED_ALLOCATIONS.with(|count| count.set(Some(0)));
    for _ in 0..1000 {
        black_box(native_values_equal(
            black_box(&boxed_text),
            black_box(&text),
        ));
        black_box(native_values_equal(
            black_box(&boxed_integer),
            black_box(&integer),
        ));
        black_box(native_values_equal(black_box(&wide), black_box(&u64::MAX)));
        black_box(native_values_not_equal(
            black_box(&wide),
            black_box(&-1_i64),
        ));
        black_box(TsValue::from(black_box(0_i32)));
        black_box(TsValue::from(black_box(false)));
    }
    let allocations = TRACKED_ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert!(native_values_equal(&boxed_text, &text));
    assert!(native_values_equal(&boxed_integer, &integer));
    assert!(native_values_equal(&wide, &u64::MAX));
    assert!(!native_values_equal(&wide, &(u64::MAX as f64)));
    assert!(native_values_not_equal(&wide, &-1_i64));
}

#[test]
fn shared_native_admission_reuses_the_owner_and_preserves_its_lifetime() {
    let drops = Rc::new(Cell::new(0));
    let object = ObjectHandle::new(DropProbe(Rc::clone(&drops)));
    let immutable = ObjectRef::new(7_i32);
    let empty = EmptyObject::new();
    TRACKED_ALLOCATIONS.with(|count| count.set(Some(0)));
    let retained = TsValue::from(object.clone());
    let alias = TsValue::from(object.clone());
    let retained_immutable = TsValue::from(immutable);
    let retained_empty = TsValue::from(empty);
    let equals = native_values_equal(&retained, &alias);
    let allocations = TRACKED_ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert!(equals);
    assert_eq!(retained_immutable.type_of(), "object");
    assert_eq!(retained_empty.type_of(), "object");
    drop(object);
    drop(retained);
    assert_eq!(drops.get(), 0);
    drop(alias);
    assert_eq!(drops.get(), 1);
}

#[test]
fn borrowed_native_comparisons_preserve_each_supported_width_and_float_edges() {
    macro_rules! check {
        ($($native:expr),+ $(,)?) => {$({
            let value = $native;
            assert!(native_values_equal(&TsValue::from(value), &value));
        })+};
    }
    check!(
        i8::MIN,
        u8::MAX,
        i16::MIN,
        u16::MAX,
        i32::MIN,
        u32::MAX,
        i64::MIN,
        u64::MAX,
        isize::MIN,
        usize::MAX,
        1.25_f32,
        1.25_f64,
        true,
        '字'
    );
    assert!(!native_values_equal(&TsValue::from(f64::NAN), &f64::NAN));
    assert!(native_values_equal(&TsValue::from(-0.0_f64), &0.0_f64));
    assert!(!native_values_equal(&TsValue::from(1_i64), "1"));
    assert!(native_values_equal(&TsValue::default(), &()));
}
