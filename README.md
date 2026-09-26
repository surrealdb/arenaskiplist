<h1 align="center">arenaskiplist</h1>

<p align="center">A fast, lock-free, concurrent arena-backed skip list for Rust.</p>

<br>

<p align="center">
    <a href="https://github.com/surrealdb/arenaskiplist"><img src="https://img.shields.io/badge/status-beta-ff00bb.svg?style=flat-square"></a>
    &nbsp;
    <a href="https://docs.rs/arenaskiplist/"><img src="https://img.shields.io/docsrs/arenaskiplist?style=flat-square"></a>
    &nbsp;
    <a href="https://crates.io/crates/arenaskiplist"><img src="https://img.shields.io/crates/v/arenaskiplist?style=flat-square"></a>
    &nbsp;
    <a href="https://github.com/surrealdb/arenaskiplist"><img src="https://img.shields.io/badge/license-Apache_License_2.0-00bfff.svg?style=flat-square"></a>
</p>

`arenaskiplist` provides a concurrent, lock-free skip list that allocates all nodes, variable-height towers, and key/value bytes directly inside a contiguous, pre-allocated [`Arena`].

It is designed for write-heavy workloads, LSM-tree memtables, WAL buffers, and high-throughput ingestion pipelines where avoiding per-node dynamic heap allocations is paramount.

## Features

- **Zero Per-Node Allocations**: All nodes and entries are packed contiguously into an arena using lock-free atomic bump allocation.
- **Lock-Free Concurrency**: Multiple writers insert concurrently via atomic compare-and-swap operations.
- **Lock-Free Reads**: Lookups and range scans proceed concurrently without acquiring locks or updating reference counts.
- **Splice Optimization**: Reusable [`Inserter`] caches search paths to accelerate sequential and localized batch insertions.
- **MVCC & LSM Versioning**: First-class support for 64-bit versioning (`insert_with_version`, `get_version_le`) for snapshot visibility boundaries.
- **Bidirectional Iteration**: Range scans with forward and reverse traversal ([`DoubleEndedIterator`]).
- **$O(1)$ Teardown**: Deallocating or recycling the entire skip list takes $O(1)$ time by dropping or resetting the arena buffer.

## Quick Start

```rust
use arenaskiplist::{Arena, SkipList};

// Allocate a 16MB arena
let arena = Arena::with_capacity(16 * 1024 * 1024);
let list = SkipList::new(arena);

// Concurrent inserts
list.insert(b"fruit:apple", b"red").unwrap();
list.insert(b"fruit:banana", b"yellow").unwrap();
list.insert(b"fruit:cherry", b"dark red").unwrap();

// Point lookups
if let Some(entry) = list.get(b"fruit:apple") {
    println!("{}: {}", std::str::from_utf8(entry.key()).unwrap(), std::str::from_utf8(entry.value()).unwrap());
}

// Range scanning
for entry in list.range(&b"fruit:banana"[..]..) {
    println!("{:?} => {:?}", entry.key(), entry.value());
}
```

### MVCC Snapshot Reads

```rust
use arenaskiplist::{Arena, SkipList};

let arena = Arena::with_capacity(16 * 1024 * 1024);
let list = SkipList::new(arena);

// Write multiple versions of a key (e.g. sequence numbers)
list.insert_with_version(b"account:1", 100, b"balance: 50").unwrap();
list.insert_with_version(b"account:1", 200, b"balance: 80").unwrap();

// Point lookup always returns the latest version (200)
assert_eq!(list.get(b"account:1").unwrap().version(), 200);

// Snapshot read at sequence 150 returns version 100
let snap = list.get_version_le(b"account:1", 150).unwrap();
assert_eq!(snap.version(), 100);
assert_eq!(snap.value(), b"balance: 50");
```

### Sequential Ingestion with Inserter

```rust
use arenaskiplist::{Arena, Inserter, SkipList};

let arena = Arena::with_capacity(64 * 1024 * 1024);
let list = SkipList::new(arena);
let mut inserter = Inserter::new();

// Ingest sorted keys with cached splice paths
for i in 0..10_000u64 {
    let k = i.to_be_bytes();
    list.insert_with_inserter(&k, &k, &mut inserter).unwrap();
}
```

## Comparison with `crossbeam::SkipMap` and `artmap::ArtMap`

| Feature | `crossbeam_skiplist::SkipMap` | `artmap::ArtMap` | `arenaskiplist::SkipList` |
| :--- | :--- | :--- | :--- |
| **Data Structure** | Lock-Free Skip List | Concurrent Adaptive Radix Tree | **Lock-Free Arena Skip List** |
| **Memory Allocation** | Dynamic per-node heap allocs | Dynamic node allocs (`Node4`..`Node256`) | **Contiguous Arena (Atomic bump allocation)** |
| **Memory Reclamation** | Epoch-based (`crossbeam-epoch`) | Epoch-based (`crossbeam-epoch`) | **Bulk $O(1)$ whole-arena drop/reset** |
| **Allocations / Insert**| ~1.0 allocations | ~1.0 allocations | **0 allocations** |
| **Sequential Optimization**| None | Prefix-shared latching | **[`Inserter`] Splice Caching** |
| **MVCC Versioning** | Manual key encoding | Manual key encoding | **Native 64-bit versioning & `get_version_le`** |
| **Target Workload** | General-purpose concurrent map | High-throughput concurrent map | **LSM MemTables, WAL buffers, Append-heavy caches** |

## License

This project is licensed under the Apache License 2.0.
Portions of the skiplist algorithm are based on Andy Kimball's `arenaskl` and CockroachDB's `pebble`.
