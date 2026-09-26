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

use crate::node::{Node, MAX_HEIGHT};

#[derive(Clone, Copy, Default)]
pub(crate) struct Splice {
    pub(crate) prev: *mut Node,
    pub(crate) next: *mut Node,
}

impl Splice {
    #[inline]
    pub(crate) fn init(&mut self, prev: *mut Node, next: *mut Node) {
        self.prev = prev;
        self.next = next;
    }
}

/// A reusable splice cache for optimizing sequential and localized insertions.
///
/// When inserting ordered or clustered keys, passing an [`Inserter`] allows the skip list
/// to reuse previous search paths instead of traversing from the top tower on every insert.
#[derive(Default)]
pub struct Inserter {
    pub(crate) spl: [Splice; MAX_HEIGHT],
    pub(crate) height: u32,
}

impl Inserter {
    /// Creates a new, empty [`Inserter`].
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}
