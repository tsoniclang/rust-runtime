use alloc::vec::Vec;

use super::{reinterpret_raw_location, require_abi, NativeLayout, RawPointer};
use crate::Location;

pub struct NativeArray<T> {
    storage: RawPointer,
    layout: NativeLayout<T>,
    stride: usize,
    length: usize,
}

impl<T> Clone for NativeArray<T> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            layout: self.layout,
            stride: self.stride,
            length: self.length,
        }
    }
}

impl<T: 'static> NativeArray<T> {
    pub fn new(initial: Vec<T>, layout: NativeLayout<T>, stride: usize) -> Self {
        require_abi(layout.width, layout.little_endian);
        assert!(
            stride >= layout.size && stride % layout.alignment == 0,
            "invalid native array stride"
        );
        let length = initial.len();
        let size = length
            .checked_mul(stride)
            .expect("native array extent overflow");
        let result = Self {
            storage: RawPointer::allocate(size, layout.alignment),
            layout,
            stride,
            length,
        };
        for (index, value) in initial.into_iter().enumerate() {
            (layout.write)(&result.address_at(index), value);
        }
        result
    }

    pub fn location_at(&self, index: usize) -> Location<T> {
        unsafe { reinterpret_raw_location(Some(&self.address_at(index)), self.layout) }
            .expect("native array element is non-null")
    }

    pub fn load(&self, index: usize) -> T {
        (self.layout.read)(&self.address_at(index))
    }

    fn address_at(&self, index: usize) -> RawPointer {
        assert!(index < self.length, "native array index out of bounds");
        let offset = index
            .checked_mul(self.stride)
            .expect("native array offset overflow");
        let pointer = RawPointer::offset(Some(&self.storage), offset as i128, self.layout.width)
            .expect("native array element is non-null");
        pointer.require_layout(self.layout.size, self.layout.alignment);
        pointer
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}
