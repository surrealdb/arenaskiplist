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

use std::cmp::Ordering;
use std::ops::{Bound, RangeBounds};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;

use crate::arena::Arena;
use crate::entry::EntryRef;
use crate::error::{Error, Result};
use crate::inserter::Inserter;
use crate::iter::Iter;
use crate::node::{compute_prefix, new_node, new_raw_node, Node, MAX_HEIGHT};

/// A custom comparator function for comparing byte slices.
pub type Comparator = fn(&[u8], &[u8]) -> Ordering;

/// Standard lexicographical byte slice comparator.
#[inline(always)]
pub fn default_comparator(a: &[u8], b: &[u8]) -> Ordering {
    a.cmp(b)
}

/// A concurrent, lock-free skip list allocating all nodes, keys, and values within a shared [`Arena`].
///
/// Multi-threaded writers insert concurrently using atomic compare-and-swap operations.
/// Multi-threaded readers proceed without locks or atomic writes.
pub struct SkipList {
    pub(crate) arena: Arc<Arena>,
    pub(crate) cmp: Comparator,
    pub(crate) is_default_cmp: bool,
    pub(crate) head: *mut Node,
    pub(crate) tail: *mut Node,
    pub(crate) height: AtomicU32,
    pub(crate) len: AtomicUsize,
}

// SAFETY: `SkipList` synchronizes all internal mutations through lock-free atomic CAS operations.
unsafe impl Send for SkipList {}
// SAFETY: Concurrent readers and writers access node pointers and tower offsets via atomic operations.
unsafe impl Sync for SkipList {}

impl SkipList {
    /// Creates a new `SkipList` backed by the given [`Arena`] with standard lexicographic comparison.
    pub fn new(arena: Arc<Arena>) -> Self {
        Self::with_comparator(arena, default_comparator)
    }

    /// Creates a new `SkipList` with a custom byte comparator.
    pub fn with_comparator(arena: Arc<Arena>, cmp: Comparator) -> Self {
        let head = new_raw_node(&arena, MAX_HEIGHT as u32, 0, 0)
            .expect("arena is too small to allocate head sentinel node");
        // SAFETY: Initializing freshly allocated head sentinel node.
        unsafe {
            (*head).key_prefix = 0;
            (*head).key_offset = 0;
        }

        let tail = new_raw_node(&arena, MAX_HEIGHT as u32, 0, 0)
            .expect("arena is too small to allocate tail sentinel node");
        // SAFETY: Initializing freshly allocated tail sentinel node.
        unsafe {
            (*tail).key_prefix = u32::MAX;
            (*tail).key_offset = 0;
        }

        let head_offset = arena.get_pointer_offset(head as *const u8);
        let tail_offset = arena.get_pointer_offset(tail as *const u8);

        for i in 0..MAX_HEIGHT {
            // SAFETY: Linking head and tail sentinels across all tower levels.
            unsafe {
                (*head).tower_init(i, 0, tail_offset);
                (*tail).tower_init(i, head_offset, 0);
            }
        }

        let is_default_cmp = std::ptr::fn_addr_eq(
            cmp,
            default_comparator as for<'a, 'b> fn(&'a [u8], &'b [u8]) -> Ordering,
        );
        Self {
            arena,
            cmp,
            is_default_cmp,
            head,
            tail,
            height: AtomicU32::new(1),
            len: AtomicUsize::new(0),
        }
    }

    /// Fast-path key comparison: inlines direct slice comparison if using [`default_comparator`].
    #[inline(always)]
    pub(crate) fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        if self.is_default_cmp {
            a.cmp(b)
        } else {
            (self.cmp)(a, b)
        }
    }

    /// Compares a key against a candidate node, using the inlined key prefix as a fast path.
    #[inline(always)]
    pub(crate) fn compare_key_and_prefix(
        &self,
        key: &[u8],
        key_prefix: u32,
        next: *mut Node,
    ) -> Ordering {
        if self.is_default_cmp {
            // SAFETY: `next` is a valid node pointer in the arena.
            let next_prefix = unsafe { (*next).key_prefix };
            match key_prefix.cmp(&next_prefix) {
                Ordering::Equal => {
                    // SAFETY: `next` is a valid node pointer in the arena.
                    let next_key = unsafe { (*next).get_key(&self.arena) };
                    key.cmp(next_key)
                }
                ord => ord,
            }
        } else {
            // SAFETY: `next` is a valid node pointer in the arena.
            let next_key = unsafe { (*next).get_key(&self.arena) };
            (self.cmp)(key, next_key)
        }
    }

    /// Returns a reference to the underlying [`Arena`].
    #[inline]
    pub fn arena(&self) -> &Arc<Arena> {
        &self.arena
    }

    /// Returns the total capacity in bytes of the underlying arena.
    #[inline]
    pub fn arena_capacity(&self) -> usize {
        self.arena.capacity()
    }

    /// Returns the number of bytes allocated in the arena so far.
    #[inline]
    pub fn size(&self) -> usize {
        self.arena.size()
    }

    /// Returns the current maximum tower height.
    #[inline]
    pub fn height(&self) -> u32 {
        self.height.load(AtomicOrdering::Acquire)
    }

    /// Returns the approximate number of entries in the skip list.
    #[inline]
    pub fn len(&self) -> usize {
        self.len.load(AtomicOrdering::Relaxed)
    }

    /// Returns `true` if the skip list contains no entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Inserts a key-value pair with default version 0.
    #[inline]
    pub fn insert(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.insert_with_version(key, 0, value)
    }

    /// Inserts a key-value pair with an explicit version.
    ///
    /// When multiple versions of the same key exist, higher versions are ordered first.
    pub fn insert_with_version(&self, key: &[u8], version: u64, value: &[u8]) -> Result<()> {
        let mut ins = Inserter::new();
        self.insert_internal(key, version, value, &mut ins)
    }

    /// Inserts a key-value pair reusing an [`Inserter`] splice cache.
    #[inline]
    pub fn insert_with_inserter(&self, key: &[u8], value: &[u8], ins: &mut Inserter) -> Result<()> {
        self.insert_internal(key, 0, value, ins)
    }

    /// Inserts a key-value pair with an explicit version reusing an [`Inserter`] splice cache.
    #[inline]
    pub fn insert_with_version_and_inserter(
        &self,
        key: &[u8],
        version: u64,
        value: &[u8],
        ins: &mut Inserter,
    ) -> Result<()> {
        self.insert_internal(key, version, value, ins)
    }

    fn insert_internal(
        &self,
        key: &[u8],
        version: u64,
        value: &[u8],
        ins: &mut Inserter,
    ) -> Result<()> {
        let key_prefix = compute_prefix(key);
        if self.find_splice(key, key_prefix, version, ins) {
            return Err(Error::RecordExists);
        }

        let (nd, height) = self.alloc_node(key, version, value)?;
        let nd_offset = self.arena.get_pointer_offset(nd as *const u8);

        let mut invalidate_splice = false;

        for i in 0..height as usize {
            let mut prev = ins.spl[i].prev;
            let mut next = ins.spl[i].next;

            if prev.is_null() {
                debug_assert!(next.is_null());
                prev = self.head;
                next = self.tail;
            }

            loop {
                let prev_offset = self.arena.get_pointer_offset(prev as *const u8);
                let next_offset = self.arena.get_pointer_offset(next as *const u8);

                // SAFETY: Nodes are valid pointers into arena memory; tower links are atomically manipulated.
                unsafe {
                    (*nd).tower_init(i, prev_offset, next_offset);

                    let next_prev_offset = (*next).prev_offset(i);
                    if next_prev_offset != prev_offset {
                        let prev_next_offset = (*prev).next_offset(i);
                        if prev_next_offset == next_offset {
                            (*next).cas_prev_offset(i, next_prev_offset, prev_offset);
                        }
                    }

                    if (*prev).cas_next_offset(i, next_offset, nd_offset) {
                        (*next).cas_prev_offset(i, prev_offset, nd_offset);
                        break;
                    }
                }

                let (new_prev, new_next, found) =
                    self.find_splice_for_level(key, key_prefix, version, i, prev);
                if found {
                    debug_assert_eq!(i, 0, "concurrent duplicate inserted at non-base level");
                    return Err(Error::RecordExists);
                }
                prev = new_prev;
                next = new_next;
                invalidate_splice = true;
            }
        }

        if invalidate_splice {
            ins.height = 0;
        } else {
            for i in 0..height as usize {
                ins.spl[i].prev = nd;
            }
        }

        self.len.fetch_add(1, AtomicOrdering::Relaxed);
        Ok(())
    }

    fn alloc_node(&self, key: &[u8], version: u64, value: &[u8]) -> Result<(*mut Node, u32)> {
        let height = Self::random_height();
        let nd = new_node(&self.arena, height, key, version, value).ok_or(Error::ArenaFull)?;

        let mut list_height = self.height();
        while height > list_height {
            match self.height.compare_exchange_weak(
                list_height,
                height,
                AtomicOrdering::AcqRel,
                AtomicOrdering::Acquire,
            ) {
                Ok(_) => break,
                Err(h) => list_height = h,
            }
        }

        Ok((nd, height))
    }

    /// Branchless geometric height generation (p = 1/4).
    #[inline(always)]
    fn random_height() -> u32 {
        let zeros = fastrand::u64(..).trailing_zeros();
        1 + (zeros / 2).min((MAX_HEIGHT - 1) as u32)
    }

    fn find_splice(&self, key: &[u8], key_prefix: u32, version: u64, ins: &mut Inserter) -> bool {
        let list_height = self.height();
        let mut level: i32;
        let mut prev = self.head;

        if ins.height < list_height {
            ins.height = list_height;
            level = ins.height as i32;
        } else {
            level = 0;
            for l in 0..list_height as usize {
                let spl = &ins.spl[l];
                if self.get_next(spl.prev, l) != spl.next {
                    continue;
                }

                if (spl.prev != self.head
                    && !self.key_is_after_node(spl.prev, key, key_prefix, version))
                    || (spl.next != self.tail
                        && self.key_is_after_node(spl.next, key, key_prefix, version))
                {
                    level = list_height as i32;
                } else {
                    prev = spl.prev;
                }
                break;
            }
        }

        let mut found = false;
        let mut next: *mut Node = std::ptr::null_mut();

        for l in (0..level as usize).rev() {
            let prev_level_next = next;

            loop {
                next = self.get_next(prev, l);
                if next == prev_level_next || next == self.tail {
                    break;
                }

                let cmp = self.compare_key_and_prefix(key, key_prefix, next);
                if cmp == Ordering::Less {
                    break;
                }
                if cmp == Ordering::Equal {
                    // SAFETY: `next` is a valid node in the arena.
                    let next_ver = unsafe { (*next).key_version };
                    if version == next_ver {
                        found = true;
                        break;
                    } else if version > next_ver {
                        break;
                    }
                }

                prev = next;
            }

            ins.spl[l].init(prev, next);
        }

        found
    }

    fn find_splice_for_level(
        &self,
        key: &[u8],
        key_prefix: u32,
        version: u64,
        level: usize,
        start: *mut Node,
    ) -> (*mut Node, *mut Node, bool) {
        let mut prev = start;

        loop {
            let next = self.get_next(prev, level);
            if next == self.tail {
                return (prev, next, false);
            }

            let cmp = self.compare_key_and_prefix(key, key_prefix, next);
            if cmp == Ordering::Less {
                return (prev, next, false);
            }
            if cmp == Ordering::Equal {
                // SAFETY: `next` is a valid node in the arena.
                let next_ver = unsafe { (*next).key_version };
                if version == next_ver {
                    return (prev, next, true);
                } else if version > next_ver {
                    return (prev, next, false);
                }
            }

            prev = next;
        }
    }

    #[inline]
    fn key_is_after_node(&self, nd: *mut Node, key: &[u8], key_prefix: u32, version: u64) -> bool {
        if nd == self.head {
            return true;
        }
        if nd == self.tail {
            return false;
        }

        let cmp = if self.is_default_cmp {
            // SAFETY: `nd` is checked non-null and neither head nor tail sentinel.
            let nd_prefix = unsafe { (*nd).key_prefix };
            match nd_prefix.cmp(&key_prefix) {
                Ordering::Equal => {
                    // SAFETY: `nd` is a valid node in the arena.
                    let nd_key = unsafe { (*nd).get_key(&self.arena) };
                    nd_key.cmp(key)
                }
                ord => ord,
            }
        } else {
            // SAFETY: `nd` is a valid node in the arena.
            let nd_key = unsafe { (*nd).get_key(&self.arena) };
            (self.cmp)(nd_key, key)
        };

        match cmp {
            Ordering::Less => true,
            Ordering::Greater => false,
            Ordering::Equal => {
                // SAFETY: `nd` is a valid node in the arena.
                let nd_ver = unsafe { (*nd).key_version };
                if version == nd_ver {
                    false
                } else {
                    version < nd_ver
                }
            }
        }
    }

    #[inline]
    pub(crate) fn get_next(&self, nd: *mut Node, h: usize) -> *mut Node {
        // SAFETY: `nd` is a valid node in the arena.
        let offset = unsafe { (*nd).next_offset(h) };
        self.arena.get_pointer_mut(offset) as *mut Node
    }

    #[inline]
    pub(crate) fn get_prev(&self, nd: *mut Node, h: usize) -> *mut Node {
        // SAFETY: `nd` is a valid node in the arena.
        let offset = unsafe { (*nd).prev_offset(h) };
        self.arena.get_pointer_mut(offset) as *mut Node
    }

    /// Finds the newest entry matching `key`.
    pub fn get(&self, key: &[u8]) -> Option<EntryRef<'_>> {
        let key_prefix = compute_prefix(key);
        let (_, next) = self.seek_for_base_splice_with_prefix(key, key_prefix);
        if next == self.tail {
            return None;
        }

        // SAFETY: `next` is checked against tail sentinel.
        let next_prefix = unsafe { (*next).key_prefix };
        if key_prefix != next_prefix {
            return None;
        }

        // SAFETY: `next` is a valid node in the arena checked against tail sentinel.
        let next_key = unsafe { (*next).get_key(&self.arena) };
        if self.compare(key, next_key) == Ordering::Equal {
            // SAFETY: `next` is a valid node in the arena.
            let node = unsafe { &*next };
            Some(EntryRef::new(
                next_key,
                node.key_version,
                node.get_value(&self.arena),
            ))
        } else {
            None
        }
    }

    /// Returns the value slice corresponding to the newest entry matching `key`.
    #[inline]
    pub fn get_value(&self, key: &[u8]) -> Option<&[u8]> {
        self.get(key).map(|e| e.value())
    }

    /// Returns `true` if an entry matching `key` exists.
    #[inline]
    pub fn contains_key(&self, key: &[u8]) -> bool {
        self.get(key).is_some()
    }

    /// Finds an entry matching `key` with an exact `version`.
    pub fn get_with_version(&self, key: &[u8], version: u64) -> Option<EntryRef<'_>> {
        let key_prefix = compute_prefix(key);
        let mut prev = self.head;
        let mut next: *mut Node = std::ptr::null_mut();

        for level in (0..self.height() as usize).rev() {
            let prev_level_next = next;
            loop {
                next = self.get_next(prev, level);
                if next == prev_level_next || next == self.tail {
                    break;
                }

                let cmp = self.compare_key_and_prefix(key, key_prefix, next);
                if cmp == Ordering::Less {
                    break;
                }
                if cmp == Ordering::Equal {
                    // SAFETY: `next` is a valid node in the arena.
                    let next_ver = unsafe { (*next).key_version };
                    if version == next_ver {
                        // SAFETY: `next` is a valid node in the arena.
                        let node = unsafe { &*next };
                        // SAFETY: `next` is a valid node pointer in the arena.
                        let next_key = unsafe { (*next).get_key(&self.arena) };
                        return Some(EntryRef::new(
                            next_key,
                            next_ver,
                            node.get_value(&self.arena),
                        ));
                    } else if version > next_ver {
                        break;
                    }
                }
                prev = next;
            }
        }

        None
    }

    /// Finds the newest entry matching `key` whose version is less than or equal to `max_version`.
    ///
    /// Essential for MVCC snapshot reads (e.g. read at sequence number).
    pub fn get_version_le(&self, key: &[u8], max_version: u64) -> Option<EntryRef<'_>> {
        let key_prefix = compute_prefix(key);
        let (_, mut next) = self.seek_for_base_splice_with_prefix(key, key_prefix);
        while next != self.tail {
            // SAFETY: `next` is checked against tail sentinel.
            let next_prefix = unsafe { (*next).key_prefix };
            if key_prefix != next_prefix {
                break;
            }
            // SAFETY: `next` is checked against tail sentinel.
            let next_key = unsafe { (*next).get_key(&self.arena) };
            if self.compare(key, next_key) != Ordering::Equal {
                break;
            }
            // SAFETY: `next` is a valid node in the arena.
            let node = unsafe { &*next };
            if node.key_version <= max_version {
                return Some(EntryRef::new(
                    next_key,
                    node.key_version,
                    node.get_value(&self.arena),
                ));
            }
            next = self.get_next(next, 0);
        }
        None
    }

    #[inline(always)]
    pub(crate) fn seek_for_base_splice(&self, key: &[u8]) -> (*mut Node, *mut Node) {
        let key_prefix = compute_prefix(key);
        self.seek_for_base_splice_with_prefix(key, key_prefix)
    }

    pub(crate) fn seek_for_base_splice_with_prefix(
        &self,
        key: &[u8],
        key_prefix: u32,
    ) -> (*mut Node, *mut Node) {
        let mut prev = self.head;
        let mut next: *mut Node = std::ptr::null_mut();

        for level in (0..self.height() as usize).rev() {
            let prev_level_next = next;
            loop {
                next = self.get_next(prev, level);
                if next == prev_level_next || next == self.tail {
                    break;
                }

                let cmp = self.compare_key_and_prefix(key, key_prefix, next);
                if cmp <= Ordering::Equal {
                    break;
                }
                prev = next;
            }
        }

        (prev, next)
    }

    /// Creates an unbounded bidirectional iterator over all entries.
    pub fn iter(&self) -> Iter<'_> {
        Iter::new(self, Bound::Unbounded, Bound::Unbounded)
    }

    /// Creates a range iterator bounded by the specified range.
    pub fn range<'a, K, R>(&'a self, range: R) -> Iter<'a>
    where
        K: AsRef<[u8]> + ?Sized,
        R: RangeBounds<K>,
    {
        let start = match range.start_bound() {
            Bound::Included(b) => Bound::Included(b.as_ref()),
            Bound::Excluded(b) => Bound::Excluded(b.as_ref()),
            Bound::Unbounded => Bound::Unbounded,
        };
        let end = match range.end_bound() {
            Bound::Included(b) => Bound::Included(b.as_ref()),
            Bound::Excluded(b) => Bound::Excluded(b.as_ref()),
            Bound::Unbounded => Bound::Unbounded,
        };
        Iter::new(self, start, end)
    }
}
