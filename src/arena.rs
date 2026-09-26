// Copyright (c) 2026 SurrealDB Ltd
// Copyright 2017 Dgraph Labs, Inc. and Contributors
// Modifications copyright (C) 2017 Andy Kimball and Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Maximum arena size (`u32::MAX` to fit in 32-bit offsets).
pub const MAX_ARENA_SIZE: usize = u32::MAX as usize;

/// A lock-free contiguous byte arena allocator.
///
/// Memory is pre-allocated upon creation and allocated sequentially via atomic bump allocation.
/// Individual allocations are not freed; instead, the entire arena is reclaimed in $O(1)$
/// when dropped, or can be recycled via [`reset`](Arena::reset) when exclusive ownership is held.
pub struct Arena {
    /// Current allocation offset (atomically incremented).
    n: AtomicU64,
    /// Pre-allocated buffer backed by `UnsafeCell<u8>` to grant write provenance
    /// under Stacked Borrows and Tree Borrows.
    buf: Box<[UnsafeCell<u8>]>,
}

// SAFETY: `Arena` memory is self-contained in a heap-allocated buffer.
unsafe impl Send for Arena {}

// SAFETY: All mutation of `buf` is coordinated through the atomic `n` cursor in `alloc`.
// Each successful `alloc` call reserves a disjoint byte range that no other call can reserve.
// Distinct threads never write or read-while-writing the same bytes concurrently.
unsafe impl Sync for Arena {}

impl Arena {
    /// Creates a new arena with the specified byte capacity.
    ///
    /// The capacity will be clamped to [`MAX_ARENA_SIZE`].
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.min(MAX_ARENA_SIZE);
        let buf: Box<[u8]> = vec![0u8; capacity].into_boxed_slice();
        // SAFETY: `UnsafeCell<u8>` is `#[repr(transparent)]` over `u8`, so it
        // has identical size, alignment, and drop characteristics.
        let buf: Box<[UnsafeCell<u8>]> =
            unsafe { Box::from_raw(Box::into_raw(buf) as *mut [UnsafeCell<u8>]) };

        Self {
            // Offset 0 is reserved as the "null" offset.
            n: AtomicU64::new(1),
            buf,
        }
    }

    /// Creates a new `Arc<Arena>` with the specified byte capacity.
    pub fn with_capacity(capacity: usize) -> Arc<Self> {
        Arc::new(Self::new(capacity))
    }

    /// Returns the number of bytes allocated in the arena so far.
    pub fn size(&self) -> usize {
        let s = self.n.load(Ordering::Relaxed);
        if s > self.buf.len() as u64 {
            self.buf.len()
        } else {
            s as usize
        }
    }

    /// Returns the total capacity in bytes of this arena.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Returns the remaining available bytes in this arena.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.capacity().saturating_sub(self.size())
    }

    /// Returns `true` if no user allocations have been made yet.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.size() <= 1
    }

    /// Resets the allocation offset to allow reusing the allocated memory buffer.
    ///
    /// Requires exclusive mutable access.
    pub fn reset(&mut self) {
        self.n.store(1, Ordering::Relaxed);
    }

    /// Atomically reserves `size` bytes with the specified `alignment`.
    ///
    /// `overflow` ensures that many additional bytes after the buffer are accessible
    /// within the arena (used for truncated node structures).
    ///
    /// Returns the allocated byte offset, or `None` if the arena is full.
    pub fn alloc(&self, size: u32, alignment: u32, overflow: u32) -> Option<u32> {
        debug_assert!(alignment.is_power_of_two());

        let orig_size = self.n.load(Ordering::Relaxed);
        if orig_size > self.buf.len() as u64 {
            return None;
        }

        let padded = size as u64 + alignment as u64 - 1;
        let new_size = self.n.fetch_add(padded, Ordering::Relaxed) + padded;
        if new_size + overflow as u64 > self.buf.len() as u64 {
            return None;
        }

        let offset = (new_size as u32 - size) & !(alignment - 1);
        Some(offset)
    }

    /// Returns a shared slice of bytes from the arena given an offset and length.
    #[inline]
    pub fn get_bytes(&self, offset: u32, size: u32) -> &[u8] {
        if offset == 0 || size == 0 {
            return &[];
        }
        let start = offset as usize;
        let end = start + size as usize;
        debug_assert!(
            end <= self.buf.len(),
            "offset {offset} + size {size} out of bounds"
        );
        let cell_slice: &[UnsafeCell<u8>] = &self.buf[start..end];
        let ptr = cell_slice.as_ptr() as *const u8;
        // SAFETY: `ptr` covers exactly `size` initialized bytes within `buf`'s allocation.
        unsafe { std::slice::from_raw_parts(ptr, size as usize) }
    }

    /// Returns an exclusive mutable slice of bytes from the arena.
    ///
    /// # Safety
    /// Caller must ensure no concurrent readers or writers access this byte range.
    #[inline]
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get_bytes_mut(&self, offset: u32, size: u32) -> &mut [u8] {
        if offset == 0 || size == 0 {
            return &mut [];
        }
        let start = offset as usize;
        let end = start + size as usize;
        debug_assert!(
            end <= self.buf.len(),
            "offset {offset} + size {size} out of bounds"
        );
        let cell_ptr: *const UnsafeCell<u8> = self.buf[start..end].as_ptr();
        let ptr: *mut u8 = UnsafeCell::raw_get(cell_ptr);
        std::slice::from_raw_parts_mut(ptr, size as usize)
    }

    /// Returns a raw pointer to the data at the specified offset.
    #[inline]
    pub fn get_pointer(&self, offset: u32) -> *const u8 {
        if offset == 0 {
            return std::ptr::null();
        }
        self.buf[offset as usize].get()
    }

    /// Returns a raw mutable pointer to the data at the specified offset.
    #[inline]
    pub fn get_pointer_mut(&self, offset: u32) -> *mut u8 {
        if offset == 0 {
            return std::ptr::null_mut();
        }
        self.buf[offset as usize].get()
    }

    /// Converts a raw pointer within the arena back to its byte offset.
    #[inline]
    pub fn get_pointer_offset(&self, ptr: *const u8) -> u32 {
        if ptr.is_null() {
            return 0;
        }
        let base = self.buf.as_ptr() as *const u8;
        let offset = (ptr as usize) - (base as usize);
        debug_assert!(offset <= self.buf.len(), "pointer not in arena buffer");
        offset as u32
    }
}
