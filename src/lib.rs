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

//! # arenaskiplist: High-Throughput Concurrent Arena Skip List for Rust
//!
//! `arenaskiplist` provides a lock-free, concurrent skip list that allocates all nodes,
//! keys, and values within a contiguous, bump-allocated [`Arena`].
//!
//! ## Design & Features
//!
//! - **Zero Per-Node Allocations**: All nodes, variable-height towers, and key/value bytes
//!   are packed contiguously in pre-allocated arena memory.
//! - **Lock-Free Concurrency**: Multiple threads insert concurrently via atomic compare-and-swap (CAS).
//! - **Non-Blocking Reads**: Lookups and range scans proceed concurrently without acquiring locks
//!   or taking atomic ref-counts.
//! - **Splice Optimization**: The [`Inserter`] cache optimizes sequential and localized writes
//!   by avoiding top-down searches.
//! - **MVCC Versioning Support**: Built-in 64-bit versioning enables multi-version concurrency
//!   control (e.g. LSM-tree memtables and snapshot reads via [`get_version_le`](SkipList::get_version_le)).
//! - **Bidirectional Range Scans**: Bidirectional [`Iter`] supporting [`DoubleEndedIterator`]
//!   and fast key seeks.
//!
//! ## Quick Start
//!
//! ```rust
//! use arenaskiplist::{Arena, SkipList};
//!
//! let arena = Arena::with_capacity(16 * 1024 * 1024);
//! let list = SkipList::new(arena);
//!
//! list.insert(b"fruit:apple", b"red").unwrap();
//! list.insert(b"fruit:banana", b"yellow").unwrap();
//!
//! assert_eq!(list.get_value(b"fruit:apple"), Some(&b"red"[..]));
//! ```
//!
//! ## MVCC Versioning & Snapshot Reads
//!
//! ```rust
//! use arenaskiplist::{Arena, SkipList};
//!
//! let arena = Arena::with_capacity(16 * 1024 * 1024);
//! let list = SkipList::new(arena);
//!
//! list.insert_with_version(b"account:1", 100, b"balance: 50").unwrap();
//! list.insert_with_version(b"account:1", 200, b"balance: 80").unwrap();
//!
//! // Point lookup returns newest version
//! assert_eq!(list.get(b"account:1").unwrap().version(), 200);
//!
//! // Snapshot read at version <= 150
//! let snap = list.get_version_le(b"account:1", 150).unwrap();
//! assert_eq!(snap.version(), 100);
//! assert_eq!(snap.value(), b"balance: 50");
//! ```

pub mod arena;
pub mod entry;
pub mod error;
pub mod inserter;
pub mod iter;
pub mod list;
pub mod node;
pub(crate) mod util;

pub use arena::{Arena, MAX_ARENA_SIZE};
pub use entry::EntryRef;
pub use error::{Error, Result};
pub use inserter::Inserter;
pub use iter::Iter;
pub use list::{default_comparator, Comparator, SkipList};
pub use node::{max_entry_bytes, MAX_HEIGHT, NODE_ALIGNMENT};

/// Type alias for [`SkipList`].
pub type ArenaSkipList = SkipList;
