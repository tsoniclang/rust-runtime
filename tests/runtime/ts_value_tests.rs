use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::rc::Rc;

use tsonic_rust_runtime::ts_value::{native_values_equal, native_values_not_equal};
use tsonic_rust_runtime::{
    clone_ts_value, BigInt, EmptyObject, ObjectHandle, ObjectIdentity, ObjectRef, OptionalStorage,
    TsValue,
};

struct CountingAllocator;

thread_local! {
    static TRACKED_ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        TRACKED_ALLOCATIONS.with(|count| {
            if let Some(value) = count.get() {
                count.set(Some(value + 1));
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
