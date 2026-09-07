use alloc::alloc::{alloc_zeroed, dealloc, handle_alloc_error};
use alloc::boxed::Box;
use alloc::rc::Rc;
use core::alloc::Layout;
use core::hash::{Hash, Hasher};
use core::mem::size_of;
use core::ptr::{self, NonNull};

use crate::Location;

mod array;
pub use array::NativeArray;

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

trait LeaseOwner {}

impl<Owner> LeaseOwner for Owner {}

enum Backing {
    Allocation(Allocation),
    External {
        address: NonNull<u8>,
        size: usize,
        _owner: Box<dyn LeaseOwner>,
    },
}

impl Backing {
    fn bounds(&self) -> (NonNull<u8>, usize) {
        match self {
            Self::Allocation(allocation) => (allocation.address, allocation.size),
            Self::External { address, size, .. } => (*address, *size),
        }
    }
}

#[derive(Clone)]
pub struct RawPointer {
    address: NonNull<u8>,
    backing: Option<Rc<Backing>>,
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
            backing: Some(Rc::new(Backing::Allocation(Allocation {
                address,
                layout,
                size,
            }))),
        }
    }

    pub fn from_address(address: u64, width: u32) -> Option<Self> {
        require_width(width);
        let address = usize::try_from(address).expect("address exceeds the selected ABI");
        NonNull::new(ptr::with_exposed_provenance_mut(address)).map(|address| Self {
            address,
            backing: None,
        })
    }

    /// Retains a provider's lease for a bounded native memory region.
    ///
    /// # Safety
    /// The region must remain initialized, writable, and at the same address
    /// until the owner is dropped. The owner must prevent early release and
    /// relocation. All accesses through the region and its aliases must obey
    /// Rust's validity, aliasing, and concurrency rules.
    pub unsafe fn from_external<Owner: 'static>(
        address: *mut u8,
        size: usize,
        owner: Owner,
    ) -> Option<Self> {
        let Some(address) = NonNull::new(address) else {
            assert_eq!(size, 0, "a null external region must be empty");
            return None;
        };
        assert!(
            size <= isize::MAX as usize,
            "external region exceeds native object size"
        );
        address
            .as_ptr()
            .addr()
            .checked_add(size)
            .expect("external region address overflow");
        Some(Self {
            address,
            backing: Some(Rc::new(Backing::External {
                address,
                size,
                _owner: Box::new(owner),
            })),
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
            backing: pointer.and_then(|pointer| pointer.backing.clone()),
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

    fn require_layout(&self, size: usize, alignment: usize) {
        assert!(
            alignment.is_power_of_two(),
            "invalid selected memory alignment"
        );
        assert_eq!(
            self.address.as_ptr().addr() % alignment,
            0,
            "unaligned raw access"
        );
        if let Some(backing) = &self.backing {
            let (start, extent) = backing.bounds();
            let offset = self
                .address
                .as_ptr()
                .addr()
                .checked_sub(start.as_ptr().addr())
                .expect("raw access precedes its allocation");
            assert!(
                offset <= extent && size <= extent - offset,
                "raw access exceeds its retained allocation"
            );
        }
    }

    /// Reads one initialized scalar at an explicit field placement.
    ///
    /// # Safety
    /// The address and its aliases must satisfy the external-region contract.
    pub unsafe fn read_at<T: MemoryValue>(&self, offset: usize, alignment: usize) -> T {
        let field = Self::offset(Some(self), offset as i128, usize::BITS).expect("non-null field");
        field.require_layout(size_of::<T>(), alignment);
        unsafe { field.address.cast::<T>().as_ptr().read_unaligned() }
    }

    /// Writes one scalar without reading padding from an aggregate.
    ///
    /// # Safety
    /// The address and its aliases must satisfy the external-region contract.
    pub unsafe fn write_at<T: MemoryValue>(&self, offset: usize, alignment: usize, value: T) {
        let field = Self::offset(Some(self), offset as i128, usize::BITS).expect("non-null field");
        field.require_layout(size_of::<T>(), alignment);
        unsafe { field.address.cast::<T>().as_ptr().write_unaligned(value) };
    }
}

pub struct NativeLayout<T> {
    size: usize,
    alignment: usize,
    width: u32,
    little_endian: bool,
    read: fn(&RawPointer) -> T,
    write: fn(&RawPointer, T),
}

impl<T> Copy for NativeLayout<T> {}

impl<T> Clone for NativeLayout<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> NativeLayout<T> {
    pub fn new(
        size: usize,
        alignment: usize,
        width: u32,
        little_endian: bool,
        read: fn(&RawPointer) -> T,
        write: fn(&RawPointer, T),
    ) -> Self {
        assert!(
            alignment.is_power_of_two(),
            "invalid selected memory alignment"
        );
        Self {
            size,
            alignment,
            width,
            little_endian,
            read,
            write,
        }
    }
}

impl<T: MemoryValue> NativeLayout<T> {
    pub fn scalar(size: usize, alignment: usize, width: u32, little_endian: bool) -> Self {
        assert_eq!(
            size,
            size_of::<T>(),
            "layout size differs from the closed native scalar type"
        );
        Self::new(
            size,
            alignment,
            width,
            little_endian,
            |pointer| unsafe { pointer.read_at::<T>(0, 1) },
            |pointer, value| unsafe { pointer.write_at::<T>(0, 1, value) },
        )
    }
}

pub fn allocate_native_location<T: 'static>(initial: T, layout: NativeLayout<T>) -> Location<T> {
    require_abi(layout.width, layout.little_endian);
    let pointer = RawPointer::allocate(layout.size, layout.alignment);
    pointer.require_layout(layout.size, layout.alignment);
    (layout.write)(&pointer, initial);
    location_from_raw(pointer, layout)
}

pub fn location_to_raw<T>(
    pointer: Option<&Location<T>>,
    layout: NativeLayout<T>,
) -> Option<RawPointer> {
    require_abi(layout.width, layout.little_endian);
    pointer.map(|pointer| {
        let raw = pointer
            .raw_backing()
            .expect("location has no proven physical backing");
        raw.require_layout(layout.size, layout.alignment);
        raw.clone()
    })
}

/// Creates a typed alias without changing storage or obtaining an owner from address bits.
///
/// # Safety
/// The address must remain valid for reads and writes of T until every resulting
/// location and its aliases are dropped. External storage must be initialized,
/// writable, and free from conflicting references or concurrent access.
pub unsafe fn reinterpret_raw_location<T: 'static>(
    pointer: Option<&RawPointer>,
    layout: NativeLayout<T>,
) -> Option<Location<T>> {
    require_abi(layout.width, layout.little_endian);
    pointer.map(|pointer| location_from_raw(pointer.clone(), layout))
}

fn location_from_raw<T: 'static>(pointer: RawPointer, layout: NativeLayout<T>) -> Location<T> {
    pointer.require_layout(layout.size, layout.alignment);
    let read = pointer.clone();
    let write = pointer.clone();
    Location::from_raw(
        pointer,
        move || (layout.read)(&read),
        move |value| (layout.write)(&write, value),
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
