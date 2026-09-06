use alloc::alloc::{alloc_zeroed, dealloc, handle_alloc_error};
use alloc::rc::Rc;
use core::alloc::Layout;
use core::hash::{Hash, Hasher};
use core::mem::size_of;
use core::ptr::{self, NonNull};

use crate::Location;

/// A closed, addressable value with no references, ownership, or invalid bit patterns.
///
/// # Safety
/// Every bit pattern must be valid. The value must have no padding whose bytes are
/// observed by a raw read, and copying its bytes must preserve its complete value.
pub unsafe trait MemoryValue: Copy + 'static {}

macro_rules! memory_values {
    ($($value:ty),* $(,)?) => { $(unsafe impl MemoryValue for $value {})* };
}

memory_values!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, usize, isize, f32, f64);
unsafe impl<T: MemoryValue, const LENGTH: usize> MemoryValue for [T; LENGTH] {}

struct Allocation {
    address: NonNull<u8>,
    layout: Layout,
    size: usize,
}

impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe { dealloc(self.address.as_ptr(), self.layout) };
    }
}

#[derive(Clone)]
pub struct RawPointer {
    address: NonNull<u8>,
    allocation: Option<Rc<Allocation>>,
}

impl PartialEq for RawPointer {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

impl Eq for RawPointer {}

impl Hash for RawPointer {
    fn hash<HasherType: Hasher>(&self, state: &mut HasherType) {
        self.address.hash(state);
    }
}

impl RawPointer {
    fn allocate(size: usize, alignment: usize) -> Self {
        let layout = Layout::from_size_align(size.max(1), alignment)
            .expect("invalid native allocation layout");
        let address = NonNull::new(unsafe { alloc_zeroed(layout) })
            .unwrap_or_else(|| handle_alloc_error(layout));
        Self {
            address,
            allocation: Some(Rc::new(Allocation {
                address,
                layout,
                size,
            })),
        }
    }

    pub fn from_address(address: u64, width: u32) -> Option<Self> {
        require_width(width);
        let address = usize::try_from(address).expect("address exceeds the selected ABI");
        NonNull::new(ptr::with_exposed_provenance_mut(address)).map(|address| Self {
            address,
            allocation: None,
        })
    }

    pub fn address(pointer: Option<&Self>, width: u32) -> u64 {
        require_width(width);
        pointer.map_or(0, |pointer| {
            pointer.address.as_ptr().expose_provenance() as u64
        })
    }

    pub fn offset(pointer: Option<&Self>, offset: i128, width: u32) -> Option<Self> {
        require_width(width);
        let current = pointer.map_or(0, |pointer| pointer.address.as_ptr().addr());
        let address = usize::try_from(
            (current as i128)
                .checked_add(offset)
                .expect("raw byte offset overflow"),
        )
        .expect("raw byte offset exceeds the selected ABI");
        let original = pointer.map_or(ptr::null_mut(), |pointer| pointer.address.as_ptr());
        NonNull::new(original.with_addr(address)).map(|address| Self {
            address,
            allocation: pointer.and_then(|pointer| pointer.allocation.clone()),
        })
    }

    pub fn same(left: &Option<Self>, right: &Option<Self>) -> bool {
        left == right
    }

    pub fn offset_unsigned(pointer: Option<&Self>, offset: u128, width: u32) -> Option<Self> {
        Self::offset(
            pointer,
            i128::try_from(offset).expect("raw byte offset exceeds the selected ABI"),
            width,
        )
    }

    pub fn hash(pointer: &Option<Self>) -> f64 {
        let address = pointer
            .as_ref()
            .map_or(0, |pointer| pointer.address.as_ptr().addr() as u64);
        f64::from((address ^ (address >> 32)) as u32)
    }

    fn require_layout<T: MemoryValue>(&self, size: usize, alignment: usize) {
        assert_eq!(
            size,
            size_of::<T>(),
            "layout size differs from the closed native type"
        );
        assert!(
            alignment.is_power_of_two(),
            "invalid selected memory alignment"
        );
        assert_eq!(
            self.address.as_ptr().addr() % alignment,
            0,
            "unaligned raw access"
        );
        if let Some(allocation) = &self.allocation {
            let offset = self
                .address
                .as_ptr()
                .addr()
                .checked_sub(allocation.address.as_ptr().addr())
                .expect("raw access precedes its allocation");
            assert!(
                offset <= allocation.size && size <= allocation.size - offset,
                "raw access exceeds its retained allocation"
            );
        }
    }
}

pub fn allocate_native_location<T: MemoryValue>(
    initial: T,
    size: usize,
    alignment: usize,
    width: u32,
    little_endian: bool,
) -> Location<T> {
    require_abi(width, little_endian);
    let pointer = RawPointer::allocate(size, alignment);
    pointer.require_layout::<T>(size, alignment);
    unsafe {
        pointer
            .address
            .cast::<T>()
            .as_ptr()
            .write_unaligned(initial)
    };
    unsafe { location_from_raw(pointer, size, alignment) }
}

pub fn location_to_raw<T: MemoryValue>(
    pointer: Option<&Location<T>>,
    size: usize,
    alignment: usize,
    width: u32,
    little_endian: bool,
) -> Option<RawPointer> {
    require_abi(width, little_endian);
    assert_eq!(
        size,
        size_of::<T>(),
        "layout size differs from the closed native type"
    );
    pointer.map(|pointer| {
        let raw = pointer
            .raw_backing()
            .expect("location has no proven physical backing");
        raw.require_layout::<T>(size, alignment);
        raw.clone()
    })
}

/// Creates a typed alias without changing storage or obtaining an owner from address bits.
///
/// # Safety
/// The address must remain valid for reads and writes of T until every resulting
/// location and its aliases are dropped. External storage must be initialized,
/// writable, and free from conflicting references or concurrent access.
pub unsafe fn reinterpret_raw_location<T: MemoryValue>(
    pointer: Option<&RawPointer>,
    size: usize,
    alignment: usize,
    width: u32,
    little_endian: bool,
) -> Option<Location<T>> {
    require_abi(width, little_endian);
    assert_eq!(
        size,
        size_of::<T>(),
        "layout size differs from the closed native type"
    );
    pointer.map(|pointer| unsafe { location_from_raw(pointer.clone(), size, alignment) })
}

unsafe fn location_from_raw<T: MemoryValue>(
    pointer: RawPointer,
    size: usize,
    alignment: usize,
) -> Location<T> {
    pointer.require_layout::<T>(size, alignment);
    let read = pointer.clone();
    let write = pointer.clone();
    Location::from_raw(
        pointer,
        move || unsafe { read.address.cast::<T>().as_ptr().read_unaligned() },
        move |value| unsafe { write.address.cast::<T>().as_ptr().write_unaligned(value) },
    )
}

fn require_width(width: u32) {
    assert_eq!(
        width,
        usize::BITS,
        "selected address ABI differs from the native process"
    );
}

fn require_abi(width: u32, little_endian: bool) {
    require_width(width);
    assert_eq!(
        little_endian,
        cfg!(target_endian = "little"),
        "selected memory byte order differs from the native process"
    );
}
