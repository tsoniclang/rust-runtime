use core::mem::{align_of, size_of};
use tsonic_rust_runtime::raw_memory::{
    allocate_native_location, location_to_raw, reinterpret_raw_location, RawPointer,
};
use tsonic_rust_runtime::Location;

#[test]
fn native_collections_use_address_identity_not_owner_or_wrapper_identity() {
    let first = RawPointer::from_address(4096, usize::BITS).unwrap();
    let same = RawPointer::from_address(4096, usize::BITS).unwrap();
    let other = RawPointer::from_address(8192, usize::BITS).unwrap();
    let values: std::collections::HashSet<_> = [first, same, other].into_iter().collect();
    assert_eq!(values.len(), 2);
}

#[test]
fn native_round_trip_mutates_original_storage_and_retains_its_owner() {
    let original = allocate_native_location(7_u32, 4, 4);
    let raw = location_to_raw(Some(&original), 4, 4).unwrap();
    let restored = unsafe { reinterpret_raw_location::<u32>(Some(&raw), 4, 4) }.unwrap();
    assert!(Location::same(Some(&original), Some(&restored)));
    assert_eq!(
        Location::hash(Some(&original)),
        Location::hash(Some(&restored))
    );
    restored.store(11);
    assert_eq!(original.load(), 11);
    drop(original);
    drop(raw);
    restored.store(13);
    assert_eq!(restored.load(), 13);
}

#[test]
fn byte_offsets_do_not_scale_by_the_original_pointee() {
    let original = allocate_native_location(0_u32, 4, 4);
    let raw = location_to_raw(Some(&original), 4, 4).unwrap();
    let next = RawPointer::offset(Some(&raw), 1, usize::BITS).unwrap();
    let byte = unsafe { reinterpret_raw_location::<u8>(Some(&next), 1, 1) }.unwrap();
    byte.store(7);
    assert_eq!(original.load(), u32::from_ne_bytes([0, 7, 0, 0]));
    assert_eq!(
        RawPointer::address(Some(&next), usize::BITS),
        RawPointer::address(Some(&raw), usize::BITS) + 1
    );
    assert!(RawPointer::same(
        &Some(raw),
        &RawPointer::offset(Some(&next), -1, usize::BITS)
    ));
}

#[test]
fn exact_address_bits_round_trip_without_dereferencing_or_fabricating_owners() {
    let address = if usize::BITS == 64 {
        9_007_199_254_740_993
    } else {
        u64::from(u32::MAX)
    };
    let pointer = RawPointer::from_address(address, usize::BITS).unwrap();
    assert_eq!(RawPointer::address(Some(&pointer), usize::BITS), address);
    assert_eq!(
        RawPointer::hash(&Some(pointer)),
        RawPointer::hash(&RawPointer::from_address(address, usize::BITS))
    );
    assert_eq!(
        RawPointer::address(
            RawPointer::from_address(usize::MAX as u64, usize::BITS).as_ref(),
            usize::BITS
        ),
        usize::MAX as u64
    );
    assert!(RawPointer::from_address(0, usize::BITS).is_none());
    assert!(RawPointer::offset(None, 0, usize::BITS).is_none());
    assert!(RawPointer::same(&None, &None));
    assert_eq!(RawPointer::hash(&None), 0.0);
    assert_eq!(RawPointer::address(None, usize::BITS), 0);
    assert!(location_to_raw::<u32>(None, 4, 4).is_none());
    assert!(unsafe { reinterpret_raw_location::<u32>(None, 4, 4) }.is_none());
}

#[test]
fn native_arrays_and_zero_sized_values_keep_closed_storage() {
    let original = allocate_native_location([1_u32, 2], size_of::<[u32; 2]>(), align_of::<u32>());
    let raw = location_to_raw(Some(&original), 8, 4).unwrap();
    let second = RawPointer::offset(Some(&raw), 4, usize::BITS).unwrap();
    let alias = unsafe { reinterpret_raw_location::<u32>(Some(&second), 4, 4) }.unwrap();
    alias.store(9);
    assert_eq!(original.load(), [1, 9]);
    let empty = allocate_native_location([0_u8; 0], 0, 1);
    let raw_empty = location_to_raw(Some(&empty), 0, 1);
    assert!(raw_empty.is_some());
    assert_eq!(empty.load(), []);
}

#[test]
fn invalid_ranges_alignment_abi_and_nonphysical_views_are_rejected() {
    let original = allocate_native_location(1_u32, 4, 4);
    let raw = location_to_raw(Some(&original), 4, 4).unwrap();
    let outside = RawPointer::offset(Some(&raw), 4, usize::BITS).unwrap();
    let unaligned = RawPointer::offset(Some(&raw), 1, usize::BITS).unwrap();
    let invalid = std::panic::AssertUnwindSafe(|| unsafe {
        reinterpret_raw_location::<u32>(Some(&outside), 4, 4)
    });
    assert!(std::panic::catch_unwind(invalid).is_err());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            reinterpret_raw_location::<u32>(Some(&unaligned), 4, 4)
        }))
        .is_err()
    );
    assert!(std::panic::catch_unwind(|| RawPointer::from_address(
        1,
        if usize::BITS == 64 { 32 } else { 64 }
    ))
    .is_err());
    assert!(std::panic::catch_unwind(|| RawPointer::offset(None, -1, usize::BITS)).is_err());
    assert!(
        std::panic::catch_unwind(|| RawPointer::offset_unsigned(None, u128::MAX, usize::BITS))
            .is_err()
    );
    assert!(std::panic::catch_unwind(|| RawPointer::offset(None, i128::MAX, usize::BITS)).is_err());
    assert!(std::panic::catch_unwind(|| RawPointer::offset(None, i128::MIN, usize::BITS)).is_err());
    assert_eq!(
        RawPointer::address(
            RawPointer::offset_unsigned(None, 4, usize::BITS).as_ref(),
            usize::BITS
        ),
        4
    );
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        location_to_raw(Some(&Location::allocate(1_u32)), 4, 4)
    }))
    .is_err());
    let shifted = original.map(|value| value + 1, |value| value - 1);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| location_to_raw(
            Some(&shifted),
            4,
            4
        )))
        .is_err()
    );
    assert_eq!(shifted.load(), 2);
    assert_eq!(original.load(), 1);
}
