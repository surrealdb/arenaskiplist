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

use crate::entry::EntryRef;
use crate::list::SkipList;
use crate::node::Node;
use std::cmp::Ordering;
use std::ops::Bound;

/// A bidirectional, range-bounded iterator over entries in a [`SkipList`].
pub struct Iter<'a> {
    pub(crate) list: &'a SkipList,
    pub(crate) nd: *mut Node,
    pub(crate) lower: Option<Vec<u8>>,
    pub(crate) lower_inclusive: bool,
    pub(crate) upper: Option<Vec<u8>>,
    pub(crate) upper_inclusive: bool,
    pub(crate) lower_node: *mut Node,
    pub(crate) upper_node: *mut Node,
    pub(crate) started: bool,
    pub(crate) exhausted: bool,
}

impl<'a> Iter<'a> {
    pub(crate) fn new(
        list: &'a SkipList,
        start_bound: Bound<&[u8]>,
        end_bound: Bound<&[u8]>,
    ) -> Self {
        let (lower, lower_inclusive) = match start_bound {
            Bound::Included(b) => (Some(b.to_vec()), true),
            Bound::Excluded(b) => (Some(b.to_vec()), false),
            Bound::Unbounded => (None, true),
        };

        let (upper, upper_inclusive) = match end_bound {
            Bound::Included(b) => (Some(b.to_vec()), true),
            Bound::Excluded(b) => (Some(b.to_vec()), false),
            Bound::Unbounded => (None, false),
        };

        Self {
            list,
            nd: list.head,
            lower,
            lower_inclusive,
            upper,
            upper_inclusive,
            lower_node: std::ptr::null_mut(),
            upper_node: std::ptr::null_mut(),
            started: false,
            exhausted: false,
        }
    }

    /// Checks if the cursor is currently positioned at a valid in-bounds entry.
    #[inline]
    pub fn is_valid(&self) -> bool {
        !self.exhausted
            && !self.nd.is_null()
            && self.nd != self.list.head
            && self.nd != self.list.tail
            && self.nd != self.lower_node
            && self.nd != self.upper_node
    }

    /// Returns the current entry reference if valid.
    #[inline]
    pub fn current(&self) -> Option<EntryRef<'a>> {
        if !self.is_valid() {
            return None;
        }
        // SAFETY: `self.is_valid()` guarantees `nd` is non-null, neither head nor tail, and points to a valid node.
        let node = unsafe { &*self.nd };
        Some(EntryRef::new(
            node.get_key(&self.list.arena),
            node.key_version,
            node.get_value(&self.list.arena),
        ))
    }

    /// Returns the current key bytes if valid.
    #[inline]
    pub fn key(&self) -> Option<&'a [u8]> {
        if !self.is_valid() {
            return None;
        }
        // SAFETY: `self.is_valid()` guarantees `nd` is a valid node in the arena.
        unsafe { Some((*self.nd).get_key(&self.list.arena)) }
    }

    /// Returns the current value bytes if valid.
    #[inline]
    pub fn value(&self) -> Option<&'a [u8]> {
        if !self.is_valid() {
            return None;
        }
        // SAFETY: `self.is_valid()` guarantees `nd` is a valid node in the arena.
        unsafe { Some((*self.nd).get_value(&self.list.arena)) }
    }

    /// Returns the current version if valid.
    #[inline]
    pub fn version(&self) -> Option<u64> {
        if !self.is_valid() {
            return None;
        }
        // SAFETY: `self.is_valid()` guarantees `nd` is a valid node in the arena.
        unsafe { Some((*self.nd).key_version) }
    }

    #[inline]
    fn node_key(&self) -> &'a [u8] {
        debug_assert!(!self.nd.is_null() && self.nd != self.list.head && self.nd != self.list.tail);
        // SAFETY: Caller guarantees `nd` is a valid node in the arena (non-null and not sentinel).
        unsafe { (*self.nd).get_key(&self.list.arena) }
    }

    /// Positions the cursor at the first entry within the iterator's range bounds.
    pub fn first(&mut self) {
        self.exhausted = false;
        self.started = true;

        if let Some(lower) = self.lower.clone() {
            self.seek_ge(&lower);
            if !self.lower_inclusive && self.is_valid() {
                while self.is_valid() && (self.list.cmp)(self.node_key(), &lower) == Ordering::Equal
                {
                    self.advance();
                }
            }
        } else {
            self.nd = self.list.get_next(self.list.head, 0);
            self.check_upper_bound();
        }
    }

    /// Positions the cursor at the last entry within the iterator's range bounds.
    pub fn last(&mut self) {
        self.exhausted = false;
        self.started = true;
        self.nd = self.list.get_prev(self.list.tail, 0);

        if self.nd == self.list.head || self.nd == self.lower_node {
            self.exhausted = true;
            return;
        }

        if let Some(upper) = self.upper.clone() {
            while self.nd != self.list.head && self.nd != self.list.tail && !self.nd.is_null() {
                let k = self.node_key();
                let cmp = (self.list.cmp)(k, &upper);
                if cmp == Ordering::Less || (cmp == Ordering::Equal && self.upper_inclusive) {
                    break;
                }
                self.nd = self.list.get_prev(self.nd, 0);
                if self.nd == self.list.head || self.nd == self.lower_node {
                    self.exhausted = true;
                    return;
                }
            }
        }

        if let Some(lower) = self.lower.clone() {
            if self.is_valid() {
                let k = self.node_key();
                let cmp = (self.list.cmp)(k, &lower);
                if cmp == Ordering::Less || (cmp == Ordering::Equal && !self.lower_inclusive) {
                    self.lower_node = self.nd;
                    self.nd = self.list.head;
                    self.exhausted = true;
                }
            }
        }
    }

    /// Advances the cursor to the next entry.
    pub fn advance(&mut self) {
        if !self.is_valid() {
            return;
        }
        self.nd = self.list.get_next(self.nd, 0);
        self.check_upper_bound();
    }

    /// Moves the cursor to the previous entry.
    pub fn prev(&mut self) {
        if !self.is_valid() {
            return;
        }
        self.nd = self.list.get_prev(self.nd, 0);
        if self.nd == self.list.head || self.nd == self.lower_node {
            self.exhausted = true;
            return;
        }

        if let Some(ref lower) = self.lower {
            let k = self.node_key();
            let cmp = (self.list.cmp)(k, lower);
            if cmp == Ordering::Less || (cmp == Ordering::Equal && !self.lower_inclusive) {
                self.lower_node = self.nd;
                self.nd = self.list.head;
                self.exhausted = true;
            }
        }
    }

    /// Seeks to the first entry with a key greater than or equal to `target`.
    pub fn seek_ge(&mut self, target: &[u8]) {
        self.started = true;
        self.exhausted = false;
        let (_, next) = self.seek_for_base_splice(target);
        self.nd = next;
        self.check_upper_bound();
    }

    fn check_upper_bound(&mut self) {
        if self.nd == self.list.tail || self.nd == self.upper_node {
            self.exhausted = true;
            return;
        }

        if let Some(ref upper) = self.upper {
            if !self.nd.is_null() && self.nd != self.list.head {
                let current_key = self.node_key();
                let cmp = (self.list.cmp)(current_key, upper);
                if cmp == Ordering::Greater || (cmp == Ordering::Equal && !self.upper_inclusive) {
                    self.upper_node = self.nd;
                    self.nd = self.list.tail;
                    self.exhausted = true;
                }
            }
        }
    }

    fn seek_for_base_splice(&self, key: &[u8]) -> (*mut Node, *mut Node) {
        let mut prev = self.list.head;
        let mut next: *mut Node = std::ptr::null_mut();

        for level in (0..self.list.height() as usize).rev() {
            let prev_level_next = next;
            loop {
                next = self.list.get_next(prev, level);
                if next == prev_level_next || next == self.list.tail {
                    break;
                }

                // SAFETY: `next` is a valid node in the arena checked against tail sentinel.
                let next_key = unsafe { (*next).get_key(&self.list.arena) };
                let cmp = (self.list.cmp)(key, next_key);
                if cmp <= Ordering::Equal {
                    break;
                }
                prev = next;
            }
        }

        (prev, next)
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = EntryRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.exhausted {
            return None;
        }
        if !self.started {
            self.first();
        } else {
            self.advance();
        }

        self.current()
    }
}

impl<'a> DoubleEndedIterator for Iter<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.exhausted {
            return None;
        }
        if !self.started {
            self.last();
        } else {
            self.prev();
        }

        self.current()
    }
}
