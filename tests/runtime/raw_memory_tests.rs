use core::mem::{align_of, size_of};
use tsonic_rust_runtime::raw_memory::{
    allocate_native_location, location_to_raw, reinterpret_raw_location, RawPointer,
};
use tsonic_rust_runtime::Location;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Header {
    tag: u8,
    count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Envelope {
    prefix: u8,
    header: Header,
}

#[test]
fn nested_packed_records_replace_values_but_retain_physical_aliases() {
    use tsonic_rust_runtime::raw_memory::NativeLayout;
    let layout = NativeLayout::new(
        9,
        1,
        usize::BITS,
        cfg!(target_endian = "little"),
        |pointer| Envelope {
            prefix: unsafe { pointer.read_at(0, 1) },
            header: Header {
                tag: unsafe { pointer.read_at(1, 1) },
                count: unsafe { pointer.read_at(5, 1) },
            },
        },
        |pointer, value: Envelope| unsafe {
            pointer.write_at(0, 1, value.prefix);
            pointer.write_at(1, 1, value.header.tag);
            pointer.write_at(5, 1, value.header.count);
        },
    );
    let original = allocate_native_location(
        Envelope {
            prefix: 1,
            header: Header { tag: 2, count: 7 },
        },
        layout,
    );
    let saved = original.load();
    let raw = location_to_raw(Some(&original), layout).unwrap();
    let alias = unsafe { reinterpret_raw_location(Some(&raw), layout) }.unwrap();
    let field = RawPointer::offset(Some(&raw), 5, usize::BITS).unwrap();
    let word = unsafe {
        reinterpret_raw_location(
            Some(&field),
            NativeLayout::scalar(4, 1, usize::BITS, cfg!(target_endian = "little")),
        )
    }
    .unwrap();
    word.store(9_u32);
    assert_eq!(original.load().header.count, 9);
    alias.store(Envelope {
        prefix: 3,
        header: Header { tag: 4, count: 11 },
    });
    assert_eq!(word.load(), 11);
    assert_eq!(original.load().prefix, 3);
    assert_eq!(saved.header.count, 7);
    assert!(Location::same(Some(&original), Some(&alias)));
    for offset in 2..5 {
        assert_eq!(unsafe { raw.read_at::<u8>(offset, 1) }, 0);
    }
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            raw.read_at::<u32>(6, 1)
        }))
        .is_err()
    );
}

struct ProviderAllocation {
    storage: Box<[u32; 2]>,
    released: std::rc::Rc<std::cell::Cell<usize>>,
}

struct DescriptorAllocation {
    descriptor: std::cell::UnsafeCell<[u64; 3]>,
    data: std::cell::UnsafeCell<[u32; 2]>,
    released: std::rc::Rc<std::cell::Cell<usize>>,
}

impl Drop for DescriptorAllocation {
    fn drop(&mut self) {
        self.released.set(self.released.get() + 1);
    }
}

unsafe fn descriptor_view(pointer: &RawPointer) -> RawPointer {
    let address = unsafe { pointer.read_at::<u64>(0, 8) };
    let length = unsafe { pointer.read_at::<u64>(8, 8) };
    let capacity = unsafe { pointer.read_at::<u64>(16, 8) };
    assert!(length <= capacity);
    let extent = usize::try_from(length).unwrap().checked_mul(4).unwrap();
    let data = core::ptr::with_exposed_provenance_mut::<u8>(usize::try_from(address).unwrap());
    unsafe { RawPointer::from_external(data, extent, std::rc::Rc::new(pointer.clone())) }.unwrap()
}

#[test]
fn physical_descriptor_views_retain_composite_lease_without_recovering_it_from_bits() {
    use std::cell::{Cell, UnsafeCell};
    use std::rc::Rc;
    use tsonic_rust_runtime::raw_memory::NativeLayout;
    let released = Rc::new(Cell::new(0));
    let owner = Rc::new(DescriptorAllocation {
        descriptor: UnsafeCell::new([0, 2, 2]),
        data: UnsafeCell::new([7, 11]),
        released: released.clone(),
    });
    let weak = Rc::downgrade(&owner);
    unsafe {
        (*owner.descriptor.get())[0] = owner.data.get().cast::<u8>().expose_provenance() as u64;
    }
    let raw =
        unsafe { RawPointer::from_external(owner.descriptor.get().cast(), 24, owner.clone()) }
            .unwrap();
    let first = unsafe { descriptor_view(&raw) };
    let second = unsafe { descriptor_view(&raw) };
    let word = NativeLayout::<u32>::scalar(4, 4, usize::BITS, cfg!(target_endian = "little"));
    let alias = unsafe { reinterpret_raw_location(Some(&first), word) }.unwrap();
    alias.store(19);
    assert_eq!(unsafe { (*owner.data.get())[0] }, 19);
    assert_eq!(unsafe { second.read_at::<u32>(0, 4) }, 19);
    unsafe {
        (*owner.descriptor.get())[0] += 4;
        (*owner.descriptor.get())[1] = 1;
        (*owner.descriptor.get())[2] = 1;
    }
    let replacement = unsafe { descriptor_view(&raw) };
    assert_eq!(unsafe { replacement.read_at::<u32>(0, 4) }, 11);
    assert_eq!(alias.load(), 19);
    drop(owner);
    drop(raw);
    assert!(weak.upgrade().is_some());
    drop(first);
    drop(second);
    drop(replacement);
    assert_eq!(alias.load(), 19);
    assert_eq!(released.get(), 0);
    drop(alias);
    assert_eq!(released.get(), 1);
    assert!(weak.upgrade().is_none());
}

impl Drop for ProviderAllocation {
    fn drop(&mut self) {
        self.released.set(self.released.get() + 1);
    }
}

fn external_region(released: std::rc::Rc<std::cell::Cell<usize>>) -> RawPointer {
    let mut owner = ProviderAllocation {
        storage: Box::new([0, 0]),
        released,
    };
    let address = owner.storage.as_mut_ptr().cast::<u8>();
    unsafe { RawPointer::from_external(address, 8, owner) }.unwrap()
}

#[test]
fn external_aliases_retain_and_release_the_actual_provider_owner() {
    let released = std::rc::Rc::new(std::cell::Cell::new(0));
    let original = external_region(released.clone());
    let raw = RawPointer::offset(Some(&original), 4, usize::BITS).unwrap();
    let typed = unsafe {
        reinterpret_raw_location::<u32>(
            Some(&raw),
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                4,
                4,
                usize::BITS,
                cfg!(target_endian = "little"),
            ),
        )
    }
    .unwrap();
    let duplicate = unsafe {
        reinterpret_raw_location::<u32>(
            Some(&raw),
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                4,
                4,
                usize::BITS,
                cfg!(target_endian = "little"),
            ),
        )
    }
    .unwrap();
    drop(original);
    drop(raw);
    assert_eq!(released.get(), 0);
    typed.store(17);
    assert_eq!(duplicate.load(), 17);
    assert!(Location::same(Some(&typed), Some(&duplicate)));
    assert_eq!(
        Location::hash(Some(&typed)),
        Location::hash(Some(&duplicate))
    );
    drop(typed);
    assert_eq!(released.get(), 0);
    duplicate.store(23);
    assert_eq!(duplicate.load(), 23);
    drop(duplicate);
    assert_eq!(released.get(), 1);
}

#[test]
fn extracted_address_bits_do_not_retain_or_recover_a_provider_owner() {
    let released = std::rc::Rc::new(std::cell::Cell::new(0));
    let raw = external_region(released.clone());
    let bits = RawPointer::address(Some(&raw), usize::BITS);
    let unowned = RawPointer::from_address(bits, usize::BITS);
    drop(raw);
    assert_eq!(released.get(), 1);
    assert_eq!(RawPointer::address(unowned.as_ref(), usize::BITS), bits);
}

#[test]
fn provider_views_mutate_the_original_native_storage_without_a_copy() {
    let mut storage = Box::new([3_u32, 5]);
    let address = storage.as_mut_ptr();
    let raw = unsafe { RawPointer::from_external(address.cast(), 8, storage) }.unwrap();
    let alias = RawPointer::offset(Some(&raw), 4, usize::BITS).unwrap();
    let second = unsafe {
        reinterpret_raw_location::<u32>(
            Some(&alias),
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                4,
                4,
                usize::BITS,
                cfg!(target_endian = "little"),
            ),
        )
    }
    .unwrap();
    second.store(17);
    assert_eq!(unsafe { address.add(1).read() }, 17);
    unsafe { address.add(1).write(23) };
    assert_eq!(second.load(), 23);
    assert_eq!(unsafe { address.read() }, 3);
}

#[test]
fn external_views_retain_their_original_extent_and_alignment() {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let raw = external_region(std::rc::Rc::new(std::cell::Cell::new(0)));
    for offset in [-1, 1, 8] {
        let alias = RawPointer::offset(Some(&raw), offset, usize::BITS);
        assert!(catch_unwind(AssertUnwindSafe(|| unsafe {
            reinterpret_raw_location::<u32>(
                alias.as_ref(),
                tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                    4,
                    4,
                    usize::BITS,
                    cfg!(target_endian = "little"),
                ),
            )
        }))
        .is_err());
    }
    assert!(
        catch_unwind(|| unsafe { RawPointer::from_external(core::ptr::null_mut(), 1, ()) })
            .is_err()
    );
    assert!(catch_unwind(|| unsafe {
        RawPointer::from_external(core::ptr::without_provenance_mut(usize::MAX), 1, ())
    })
    .is_err());
    assert!(unsafe { RawPointer::from_external(core::ptr::null_mut(), 0, ()) }.is_none());
}

#[test]
fn descriptor_copies_keep_their_own_backing_when_the_container_is_replaced() {
    let first_released = std::rc::Rc::new(std::cell::Cell::new(0));
    let second_released = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut descriptors = [(external_region(first_released.clone()), 2)];
    let copied = descriptors[0].clone();
    descriptors[0] = (external_region(second_released.clone()), 1);
    let old_view = unsafe {
        reinterpret_raw_location::<u32>(
            Some(&copied.0),
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                4,
                4,
                usize::BITS,
                cfg!(target_endian = "little"),
            ),
        )
    }
    .unwrap();
    let new_view = unsafe {
        reinterpret_raw_location::<u32>(
            Some(&descriptors[0].0),
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                4,
                4,
                usize::BITS,
                cfg!(target_endian = "little"),
            ),
        )
    }
    .unwrap();
    old_view.store(37);
    assert_eq!(new_view.load(), 0);
    new_view.store(41);
    assert_eq!(old_view.load(), 37);
    assert_eq!(copied.1, 2);
    assert!(!Location::same(Some(&old_view), Some(&new_view)));
    drop(copied);
    drop(descriptors);
    assert_eq!(first_released.get(), 0);
    assert_eq!(second_released.get(), 0);
    drop(old_view);
    assert_eq!(first_released.get(), 1);
    assert_eq!(second_released.get(), 0);
    drop(new_view);
    assert_eq!(second_released.get(), 1);
}

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
    let original = allocate_native_location(
        7_u32,
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            4,
            4,
            usize::BITS,
            cfg!(target_endian = "little"),
        ),
    );
    let raw = location_to_raw(
        Some(&original),
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            4,
            4,
            usize::BITS,
            cfg!(target_endian = "little"),
        ),
    )
    .unwrap();
    let restored = unsafe {
        reinterpret_raw_location::<u32>(
            Some(&raw),
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                4,
                4,
                usize::BITS,
                cfg!(target_endian = "little"),
            ),
        )
    }
    .unwrap();
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
    let original = allocate_native_location(
        0_u32,
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            4,
            4,
            usize::BITS,
            cfg!(target_endian = "little"),
        ),
    );
    let raw = location_to_raw(
        Some(&original),
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            4,
            4,
            usize::BITS,
            cfg!(target_endian = "little"),
        ),
    )
    .unwrap();
    let next = RawPointer::offset(Some(&raw), 1, usize::BITS).unwrap();
    let byte = unsafe {
        reinterpret_raw_location::<u8>(
            Some(&next),
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                1,
                1,
                usize::BITS,
                cfg!(target_endian = "little"),
            ),
        )
    }
    .unwrap();
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
    assert!(location_to_raw::<u32>(
        None,
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            4,
            4,
            usize::BITS,
            cfg!(target_endian = "little")
        )
    )
    .is_none());
    assert!(unsafe {
        reinterpret_raw_location::<u32>(
            None,
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                4,
                4,
                usize::BITS,
                cfg!(target_endian = "little"),
            ),
        )
    }
    .is_none());
}

#[test]
fn native_arrays_and_zero_sized_values_keep_closed_storage() {
    let original = allocate_native_location(
        [1_u32, 2],
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            size_of::<[u32; 2]>(),
            align_of::<u32>(),
            usize::BITS,
            cfg!(target_endian = "little"),
        ),
    );
    let raw = location_to_raw(
        Some(&original),
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            8,
            4,
            usize::BITS,
            cfg!(target_endian = "little"),
        ),
    )
    .unwrap();
    let second = RawPointer::offset(Some(&raw), 4, usize::BITS).unwrap();
    let alias = unsafe {
        reinterpret_raw_location::<u32>(
            Some(&second),
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                4,
                4,
                usize::BITS,
                cfg!(target_endian = "little"),
            ),
        )
    }
    .unwrap();
    alias.store(9);
    assert_eq!(original.load(), [1, 9]);
    let empty = allocate_native_location(
        [0_u8; 0],
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            0,
            1,
            usize::BITS,
            cfg!(target_endian = "little"),
        ),
    );
    let raw_empty = location_to_raw(
        Some(&empty),
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            0,
            1,
            usize::BITS,
            cfg!(target_endian = "little"),
        ),
    );
    assert!(raw_empty.is_some());
    assert_eq!(empty.load(), []);
}

#[test]
fn invalid_ranges_alignment_abi_and_nonphysical_views_are_rejected() {
    let original = allocate_native_location(
        1_u32,
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            4,
            4,
            usize::BITS,
            cfg!(target_endian = "little"),
        ),
    );
    let raw = location_to_raw(
        Some(&original),
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            4,
            4,
            usize::BITS,
            cfg!(target_endian = "little"),
        ),
    )
    .unwrap();
    let outside = RawPointer::offset(Some(&raw), 4, usize::BITS).unwrap();
    let unaligned = RawPointer::offset(Some(&raw), 1, usize::BITS).unwrap();
    let invalid = std::panic::AssertUnwindSafe(|| unsafe {
        reinterpret_raw_location::<u32>(
            Some(&outside),
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                4,
                4,
                usize::BITS,
                cfg!(target_endian = "little"),
            ),
        )
    });
    assert!(std::panic::catch_unwind(invalid).is_err());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            reinterpret_raw_location::<u32>(
                Some(&unaligned),
                tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                    4,
                    4,
                    usize::BITS,
                    cfg!(target_endian = "little"),
                ),
            )
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
        location_to_raw(
            Some(&Location::allocate(1_u32)),
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                4,
                4,
                usize::BITS,
                cfg!(target_endian = "little"),
            ),
        )
    }))
    .is_err());
    let shifted = original.map(|value| value + 1, |value| value - 1);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| location_to_raw(
            Some(&shifted),
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                4,
                4,
                usize::BITS,
                cfg!(target_endian = "little")
            )
        )))
        .is_err()
    );
    assert_eq!(shifted.load(), 2);
    assert_eq!(original.load(), 1);
}

#[test]
fn physical_operations_validate_selected_process_abi_even_for_nil() {
    let opposite_width = if usize::BITS == 64 { 32 } else { 64 };
    let little = cfg!(target_endian = "little");
    assert!(std::panic::catch_unwind(|| allocate_native_location(
        1_u32,
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(4, 4, opposite_width, little)
    ))
    .is_err());
    assert!(std::panic::catch_unwind(|| allocate_native_location(
        1_u32,
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(4, 4, usize::BITS, !little)
    ))
    .is_err());
    assert!(std::panic::catch_unwind(|| location_to_raw::<u32>(
        None,
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(4, 4, opposite_width, little)
    ))
    .is_err());
    assert!(std::panic::catch_unwind(|| location_to_raw::<u32>(
        None,
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(4, 4, usize::BITS, !little)
    ))
    .is_err());
    assert!(std::panic::catch_unwind(|| unsafe {
        reinterpret_raw_location::<u32>(
            None,
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(4, 4, opposite_width, little),
        )
    })
    .is_err());
    assert!(std::panic::catch_unwind(|| unsafe {
        reinterpret_raw_location::<u32>(
            None,
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(4, 4, usize::BITS, !little),
        )
    })
    .is_err());
}

#[test]
fn selected_byte_alignment_preserves_unaligned_native_aliasing() {
    let initial = allocate_native_location(
        [0_u8; 8],
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            8,
            1,
            usize::BITS,
            cfg!(target_endian = "little"),
        ),
    );
    let raw = location_to_raw(
        Some(&initial),
        tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
            8,
            1,
            usize::BITS,
            cfg!(target_endian = "little"),
        ),
    )
    .unwrap();
    let offset = RawPointer::offset(Some(&raw), 1, usize::BITS).unwrap();
    let alias = unsafe {
        reinterpret_raw_location::<u32>(
            Some(&offset),
            tsonic_rust_runtime::raw_memory::NativeLayout::scalar(
                4,
                1,
                usize::BITS,
                cfg!(target_endian = "little"),
            ),
        )
    }
    .unwrap();
    alias.store(0x01020304);
    let bytes = initial.load();
    assert_eq!(&bytes[1..5], &0x01020304_u32.to_ne_bytes());
    assert_eq!(alias.load(), 0x01020304);
}
