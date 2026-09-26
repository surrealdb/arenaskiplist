// Copyright (c) 2026 SurrealDB Ltd
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

use std::fmt;
use std::ops::Deref;

/// A reference to a key-value entry stored within the arena skip list.
#[derive(Clone, Copy)]
pub struct EntryRef<'a> {
    key: &'a [u8],
    version: u64,
    value: &'a [u8],
}

impl<'a> EntryRef<'a> {
    #[inline]
    pub(crate) const fn new(key: &'a [u8], version: u64, value: &'a [u8]) -> Self {
        Self {
            key,
            version,
            value,
        }
    }

    /// Returns a reference to the entry's key bytes.
    #[inline]
    pub const fn key(&self) -> &'a [u8] {
        self.key
    }

    /// Returns the version (or trailer) associated with this entry.
    #[inline]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns a reference to the entry's value bytes.
    #[inline]
    pub const fn value(&self) -> &'a [u8] {
        self.value
    }
}

impl<'a> Deref for EntryRef<'a> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'a> fmt::Debug for EntryRef<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntryRef")
            .field("key", &crate::util::format_bytes(self.key))
            .field("version", &self.version)
            .field("value", &crate::util::format_bytes(self.value))
            .finish()
    }
}

impl<'a> PartialEq for EntryRef<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.version == other.version && self.value == other.value
    }
}

impl<'a> Eq for EntryRef<'a> {}
