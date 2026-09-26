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

use crate::arena::Arena;
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

/// Maximum height of a skiplist tower.
pub const MAX_HEIGHT: usize = 20;

/// Probability factor for tower height generation (1 / 4 = 0.25).
pub const P_VALUE: f64 = 0.25;

/// Skiplist node allocation alignment.
pub const NODE_ALIGNMENT: u32 = 8;

#[repr(C)]
pub(crate) struct Links {
    pub(crate) next_offset: AtomicU32,
    pub(crate) prev_offset: AtomicU32,
}

impl Links {
    #[inline]
    pub(crate) fn init(&self, prev_offset: u32, next_offset: u32) {
        self.next_offset.store(next_offset, AtomicOrdering::Release);
        self.prev_offset.store(prev_offset, AtomicOrdering::Release);
    }
}

pub(crate) const LINKS_SIZE: usize = std::mem::size_of::<Links>();

/// Computes a big-endian 4-byte prefix from the start of a key.
/// Shorter keys are right-padded with zero bytes.
#[inline(always)]
pub(crate) fn compute_prefix(key: &[u8]) -> u32 {
    let mut buf = [0u8; 4];
    let len = key.len().min(4);
    buf[..len].copy_from_slice(&key[..len]);
    u32::from_be_bytes(buf)
}

/// Internal node structure stored inside the arena.
/// Key and value bytes are stored contiguously directly after the tower.
#[repr(C)]
pub(crate) struct Node {
    /// Inlined 4-byte key prefix for fast scalar comparisons.
    pub(crate) key_prefix: u32,
    /// Offset to key bytes in arena.
    pub(crate) key_offset: u32,
    /// Monotonic version (or MVCC trailer) associated with this key.
    pub(crate) key_version: u64,
    /// Size of key in bytes.
    pub(crate) key_size: u32,
    /// Size of value in bytes.
    pub(crate) value_size: u32,
    /// Variable-height tower links.
    pub(crate) tower: [Links; 1],
}

/// Maximum node size with full tower height.
pub const MAX_NODE_SIZE: usize = std::mem::size_of::<Node>() + (MAX_HEIGHT - 1) * LINKS_SIZE;

const _: () = assert!(MAX_NODE_SIZE == 184);

/// Computes the upper bound on the number of bytes consumed in the arena by inserting
/// an entry of the given key and value lengths.
#[inline]
pub const fn max_entry_bytes(key_len: usize, value_len: usize) -> usize {
    MAX_NODE_SIZE + key_len + value_len + (NODE_ALIGNMENT as usize - 1)
}

impl Node {
    #[inline]
    pub(crate) fn get_key<'a>(&self, arena: &'a Arena) -> &'a [u8] {
        arena.get_bytes(self.key_offset, self.key_size)
    }

    #[inline]
    pub(crate) fn get_value<'a>(&self, arena: &'a Arena) -> &'a [u8] {
        arena.get_bytes(self.key_offset + self.key_size, self.value_size)
    }

    #[inline]
    pub(crate) fn next_offset(&self, h: usize) -> u32 {
        // SAFETY: Pointer arithmetic bounded by allocated node height.
        unsafe {
            let links = (self.tower.as_ptr()).add(h);
            (*links).next_offset.load(AtomicOrdering::Acquire)
        }
    }

    #[inline]
    pub(crate) fn prev_offset(&self, h: usize) -> u32 {
        // SAFETY: Pointer arithmetic bounded by allocated node height.
        unsafe {
            let links = (self.tower.as_ptr()).add(h);
            (*links).prev_offset.load(AtomicOrdering::Acquire)
        }
    }

    #[inline]
    pub(crate) fn cas_next_offset(&self, h: usize, old: u32, val: u32) -> bool {
        // SAFETY: Pointer arithmetic bounded by allocated node height.
        unsafe {
            let links = (self.tower.as_ptr()).add(h);
            (*links)
                .next_offset
                .compare_exchange(old, val, AtomicOrdering::AcqRel, AtomicOrdering::Acquire)
                .is_ok()
        }
    }

    #[inline]
    pub(crate) fn cas_prev_offset(&self, h: usize, old: u32, val: u32) -> bool {
        // SAFETY: Pointer arithmetic bounded by allocated node height.
        unsafe {
            let links = (self.tower.as_ptr()).add(h);
            (*links)
                .prev_offset
                .compare_exchange(old, val, AtomicOrdering::AcqRel, AtomicOrdering::Acquire)
                .is_ok()
        }
    }

    #[inline]
    pub(crate) fn tower_init(&self, h: usize, prev_offset: u32, next_offset: u32) {
        // SAFETY: Pointer arithmetic bounded by allocated node height.
        unsafe {
            let links = (self.tower.as_ptr()).add(h);
            (*links).init(prev_offset, next_offset);
        }
    }
}

/// Allocates a raw node without key/value payload.
pub(crate) fn new_raw_node(
    arena: &Arena,
    height: u32,
    key_size: u32,
    value_size: u32,
) -> Option<*mut Node> {
    let unused_size = (MAX_HEIGHT - height as usize) * LINKS_SIZE;
    let node_size = MAX_NODE_SIZE - unused_size;

    let node_offset = arena.alloc(
        (node_size as u32) + key_size + value_size,
        NODE_ALIGNMENT,
        unused_size as u32,
    )?;

    let nd = arena.get_pointer_mut(node_offset) as *mut Node;
    // SAFETY: `nd` was freshly allocated by `arena.alloc` with valid alignment and capacity.
    unsafe {
        (*nd).key_prefix = 0;
        (*nd).key_offset = node_offset + node_size as u32;
        (*nd).key_size = key_size;
        (*nd).value_size = value_size;
    }

    Some(nd)
}

/// Allocates and populates a new node with key, version, and value bytes.
pub(crate) fn new_node(
    arena: &Arena,
    height: u32,
    key: &[u8],
    version: u64,
    value: &[u8],
) -> Option<*mut Node> {
    if height < 1 || height > MAX_HEIGHT as u32 {
        panic!("height cannot be less than 1 or greater than MAX_HEIGHT");
    }

    let nd = new_raw_node(arena, height, key.len() as u32, value.len() as u32)?;

    // SAFETY: Writer exclusively owns the newly allocated arena offset range before linking.
    unsafe {
        (*nd).key_prefix = compute_prefix(key);
        (*nd).key_version = version;
        let key_bytes = arena.get_bytes_mut((*nd).key_offset, (*nd).key_size);
        key_bytes.copy_from_slice(key);
        let val_bytes = arena.get_bytes_mut((*nd).key_offset + (*nd).key_size, (*nd).value_size);
        val_bytes.copy_from_slice(value);
    }

    Some(nd)
}
