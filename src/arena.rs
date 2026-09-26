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

use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Maximum arena size (`u32::MAX` to fit in 32-bit offsets).
pub const MAX_ARENA_SIZE: usize = u32::MAX as usize;

/// Default chunk size for dynamic chunked arenas (16 MB).
pub const DEFAULT_CHUNK_SIZE: usize = 16 * 1024 * 1024;

/// Maximum number of chunks addressable in a 32-bit offset space with 16MB chunks.
pub const MAX_CHUNKS: usize = 256;

/// A lock-free contiguous or dynamically growing chunked byte arena allocator.
///
/// Memory is allocated sequentially via atomic bump allocation. Individual allocations
/// are not freed; instead, the entire arena is reclaimed in $O(1)$ when dropped, or can
/// be recycled via [`reset`](Arena::reset) when exclusive ownership is held.
pub struct Arena {
    /// Current allocation offset (atomically incremented), isolated on its own cache line.
    n: AtomicU64,
    _pad: [u8; 56],
    /// Bit shift for chunk indexing (32 for single-chunk fixed arena, e.g. 24 for 16MB chunks).
    chunk_shift: u32,
    chunk_mask: u32,
    chunk_size: usize,
    max_chunks: usize,
    allocated_chunks: AtomicUsize,
    chunks: Box<[AtomicPtr<u8>]>,
    chunk_lock: Mutex<()>,
}

// SAFETY: `Arena` memory is self-contained in heap-allocated chunk buffers.
unsafe impl Send for Arena {}

// SAFETY: All mutation is coordinated through atomic counters and mutex-guarded chunk expansion.
// Distinct threads never write or read-while-writing the same bytes concurrently.
unsafe impl Sync for Arena {}

impl Arena {
    /// Creates a new fixed-size arena with the specified byte capacity.
    ///
    /// The capacity will be clamped to [`MAX_ARENA_SIZE`].
    #[cfg_attr(target_pointer_width = "32", allow(clippy::unnecessary_min_or_max))]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.min(MAX_ARENA_SIZE);
        let buf: Box<[u8]> = vec![0u8; capacity].into_boxed_slice();
        let ptr = Box::into_raw(buf) as *mut u8;

        let chunk_ptrs = vec![AtomicPtr::new(ptr)];

        Self {
            // Offset 0 is reserved as the "null" offset.
            n: AtomicU64::new(1),
            _pad: [0u8; 56],
            chunk_shift: 32, // Single chunk mode: offset >> 32 == 0
            chunk_mask: u32::MAX,
            chunk_size: capacity,
            max_chunks: 1,
            allocated_chunks: AtomicUsize::new(1),
            chunks: chunk_ptrs.into_boxed_slice(),
            chunk_lock: Mutex::new(()),
        }
    }

    /// Creates a new dynamic chunked arena with default 16MB chunks that grows on demand up to 4GB.
    pub fn chunked() -> Self {
        Self::with_chunk_config(DEFAULT_CHUNK_SIZE, MAX_CHUNKS)
    }

    /// Creates a new dynamic chunked arena with a custom chunk size.
    pub fn with_chunk_size(chunk_size: usize) -> Self {
        let chunk_size = chunk_size
            .next_power_of_two()
            .clamp(1024 * 1024, 256 * 1024 * 1024);
        let max_chunks = MAX_ARENA_SIZE / chunk_size;
        Self::with_chunk_config(chunk_size, max_chunks)
    }

    /// Creates a new dynamic chunked arena with explicit chunk size and maximum chunks.
    pub fn with_chunk_config(chunk_size: usize, max_chunks: usize) -> Self {
        let chunk_size = chunk_size.next_power_of_two();
        let chunk_shift = chunk_size.trailing_zeros();
        let chunk_mask = (chunk_size - 1) as u32;

        let initial_buf: Box<[u8]> = vec![0u8; chunk_size].into_boxed_slice();
        let initial_ptr = Box::into_raw(initial_buf) as *mut u8;

        let mut chunk_ptrs = Vec::with_capacity(max_chunks);
        chunk_ptrs.push(AtomicPtr::new(initial_ptr));
        for _ in 1..max_chunks {
            chunk_ptrs.push(AtomicPtr::new(std::ptr::null_mut()));
        }

        Self {
            n: AtomicU64::new(1),
            _pad: [0u8; 56],
            chunk_shift,
            chunk_mask,
            chunk_size,
            max_chunks,
            allocated_chunks: AtomicUsize::new(1),
            chunks: chunk_ptrs.into_boxed_slice(),
            chunk_lock: Mutex::new(()),
        }
    }

    /// Creates a new `Arc<Arena>` with the specified byte capacity.
    pub fn with_capacity(capacity: usize) -> Arc<Self> {
        Arc::new(Self::new(capacity))
    }

    /// Returns `true` if this arena grows dynamically via chunk expansion.
    #[inline]
    pub fn is_chunked(&self) -> bool {
        self.chunk_shift < 32
    }

    /// Returns the number of chunks currently allocated by this arena.
    #[inline]
    pub fn allocated_chunks(&self) -> usize {
        self.allocated_chunks.load(Ordering::Acquire)
    }

    /// Returns the number of bytes allocated in the arena so far.
    pub fn size(&self) -> usize {
        let s = self.n.load(Ordering::Relaxed);
        if self.chunk_shift >= 32 {
            if s > self.chunk_size as u64 {
                self.chunk_size
            } else {
                s as usize
            }
        } else {
            s as usize
        }
    }

    /// Returns the total committed capacity in bytes of this arena.
    #[inline]
    pub fn capacity(&self) -> usize {
        if self.chunk_shift >= 32 {
            self.chunk_size
        } else {
            self.allocated_chunks.load(Ordering::Acquire) * self.chunk_size
        }
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
    /// Returns the allocated byte offset, or `None` if the arena is full.
    pub fn alloc(&self, size: u32, alignment: u32, overflow: u32) -> Option<u32> {
        debug_assert!(alignment.is_power_of_two());

        let padded = size as u64 + alignment as u64 - 1;

        if self.chunk_shift >= 32 {
            // Fast path for single-chunk fixed arena
            let new_size = self.n.fetch_add(padded, Ordering::Relaxed) + padded;
            if new_size + overflow as u64 > self.chunk_size as u64 {
                return None;
            }
            let offset = (new_size as u32 - size) & !(alignment - 1);
            return Some(offset);
        }

        self.alloc_chunked(size, alignment, overflow, padded)
    }

    fn alloc_chunked(&self, size: u32, alignment: u32, overflow: u32, padded: u64) -> Option<u32> {
        loop {
            let current = self.n.load(Ordering::Relaxed);
            let chunk_idx = (current >> self.chunk_shift) as usize;
            let offset_in_chunk = current & (self.chunk_mask as u64);

            // Check if this allocation fits in the current chunk
            let new_offset_in_chunk = offset_in_chunk + padded;
            if new_offset_in_chunk + overflow as u64 <= self.chunk_size as u64 {
                // Try CAS to claim space in current chunk
                let new_n = (current & !(self.chunk_mask as u64)) + new_offset_in_chunk;
                if self
                    .n
                    .compare_exchange_weak(current, new_n, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    let aligned = (new_offset_in_chunk as u32 - size) & !(alignment - 1);
                    let offset = ((chunk_idx as u32) << self.chunk_shift) | aligned;
                    return Some(offset);
                }
                continue;
            }

            // Advance to next chunk
            let next_chunk = chunk_idx + 1;
            if next_chunk >= self.max_chunks {
                return None; // Exceeded maximum addressable chunks
            }

            // Ensure next chunk buffer is allocated
            if self.chunks[next_chunk].load(Ordering::Acquire).is_null() {
                let _guard = self.chunk_lock.lock().unwrap();
                if self.chunks[next_chunk].load(Ordering::Relaxed).is_null() {
                    let new_buf: Box<[u8]> = vec![0u8; self.chunk_size].into_boxed_slice();
                    let ptr = Box::into_raw(new_buf) as *mut u8;
                    self.chunks[next_chunk].store(ptr, Ordering::Release);
                    self.allocated_chunks.fetch_add(1, Ordering::Release);
                }
            }

            // Advance cursor to beginning of next chunk
            let next_chunk_start = ((next_chunk as u64) << self.chunk_shift) | 1;
            let _ = self.n.compare_exchange(
                current,
                next_chunk_start,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
    }

    /// Returns a shared slice of bytes from the arena given an offset and length.
    #[inline(always)]
    pub fn get_bytes(&self, offset: u32, size: u32) -> &[u8] {
        if offset == 0 || size == 0 {
            return &[];
        }
        let ptr = self.get_pointer(offset);
        // SAFETY: `ptr` covers exactly `size` initialized bytes within a valid chunk.
        unsafe { std::slice::from_raw_parts(ptr, size as usize) }
    }

    /// Returns an exclusive mutable slice of bytes from the arena.
    ///
    /// # Safety
    /// Caller must ensure no concurrent readers or writers access this byte range.
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get_bytes_mut(&self, offset: u32, size: u32) -> &mut [u8] {
        if offset == 0 || size == 0 {
            return &mut [];
        }
        let ptr = self.get_pointer_mut(offset);
        std::slice::from_raw_parts_mut(ptr, size as usize)
    }

    /// Returns a raw pointer to the data at the specified offset.
    #[inline(always)]
    pub fn get_pointer(&self, offset: u32) -> *const u8 {
        if offset == 0 {
            return std::ptr::null();
        }
        if self.chunk_shift >= 32 {
            // Single-chunk fixed arena: direct base + offset
            let base = self.chunks[0].load(Ordering::Relaxed);
            debug_assert!((offset as usize) < self.chunk_size);
            // SAFETY: `offset` is within chunk bounds.
            return unsafe { base.add(offset as usize) };
        }

        let chunk_idx = (offset >> self.chunk_shift) as usize;
        let chunk_offset = (offset & self.chunk_mask) as usize;
        debug_assert!(chunk_idx < self.chunks.len());
        let base = self.chunks[chunk_idx].load(Ordering::Acquire);
        debug_assert!(!base.is_null());
        // SAFETY: `chunk_offset` is within chunk bounds.
        unsafe { base.add(chunk_offset) }
    }

    /// Returns a raw mutable pointer to the data at the specified offset.
    #[inline(always)]
    pub fn get_pointer_mut(&self, offset: u32) -> *mut u8 {
        if offset == 0 {
            return std::ptr::null_mut();
        }
        if self.chunk_shift >= 32 {
            let base = self.chunks[0].load(Ordering::Relaxed);
            debug_assert!((offset as usize) < self.chunk_size);
            // SAFETY: `offset` is within chunk bounds.
            return unsafe { base.add(offset as usize) };
        }

        let chunk_idx = (offset >> self.chunk_shift) as usize;
        let chunk_offset = (offset & self.chunk_mask) as usize;
        debug_assert!(chunk_idx < self.chunks.len());
        let base = self.chunks[chunk_idx].load(Ordering::Acquire);
        debug_assert!(!base.is_null());
        // SAFETY: `chunk_offset` is within chunk bounds.
        unsafe { base.add(chunk_offset) }
    }

    /// Converts a raw pointer within the arena back to its byte offset.
    #[inline]
    pub fn get_pointer_offset(&self, ptr: *const u8) -> u32 {
        if ptr.is_null() {
            return 0;
        }

        let ptr_usize = ptr as usize;
        let allocated = self.allocated_chunks.load(Ordering::Acquire);

        for idx in 0..allocated {
            let base = self.chunks[idx].load(Ordering::Relaxed) as usize;
            if base == 0 {
                continue;
            }
            let size = self.chunk_size;
            if ptr_usize >= base && ptr_usize < base + size {
                let offset_in_chunk = (ptr_usize - base) as u32;
                if self.chunk_shift >= 32 {
                    return offset_in_chunk;
                } else {
                    return ((idx as u32) << self.chunk_shift) | offset_in_chunk;
                }
            }
        }

        0
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        let max = self.chunks.len();
        for i in 0..max {
            let ptr = self.chunks[i].load(Ordering::Relaxed);
            if !ptr.is_null() {
                // SAFETY: `ptr` was allocated via `Box::into_raw` with length `self.chunk_size`.
                unsafe {
                    let _ = Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, self.chunk_size));
                }
            }
        }
    }
}

/// Type alias for [`Arena`] when configured with dynamic chunk growth.
pub type ChunkedArena = Arena;
