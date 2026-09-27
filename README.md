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

## Performance

Benchmarked on bare metal (**AMD Ryzen Threadripper 9970X 32-Core / 64-Thread Processor @ 5.48 GHz, 128 GB DDR5 RAM**, Linux 6.8):

| Data Structure | Read<br><sup>(with&nbsp;standard&nbsp;key)</sup> | Read<br><sup>(with&nbsp;slice&nbsp;key)</sup> | Insert<br><sup>(sequential&nbsp;entries)</sup> | Insert<br><sup>(random&nbsp;entries)</sup> | Range&nbsp;scans<br><sup>(100&nbsp;items)</sup> |
| :--- | ---: | ---: | ---: | ---: | ---: |
| **`arenaskiplist::SkipList`** | **171.9&nbsp;ns**<br><sup>(5.8M/s)</sup> | <img width="16" align="absmiddle" src="/img/rocket.png" alt="🚀">&nbsp;**171.9&nbsp;ns**<br><sup>(5.8M/s)</sup> | <img width="16" align="absmiddle" src="/img/rocket.png" alt="🚀">&nbsp;**39.4&nbsp;ns**<br><sup>(25.4M/s)</sup> | <img width="16" align="absmiddle" src="/img/rocket.png" alt="🚀">&nbsp;**156.4&nbsp;ns**<br><sup>(6.4M/s)</sup> | **793&nbsp;ns**<br><sup>(126.2M/s)</sup> |
| `crossbeam_skiplist::SkipMap` | <img width="16" align="absmiddle" src="/img/rocket.png" alt="🚀">&nbsp;144.7&nbsp;ns<br><sup>(6.9M/s)</sup> | — | 75.3&nbsp;ns<br><sup>(13.3M/s)</sup> | 176.5&nbsp;ns<br><sup>(5.7M/s)</sup> | 2.19&nbsp;µs<br><sup>(45.7M/s)</sup> |
| `imbl::OrdMap` | 41.5&nbsp;ns<br><sup>(24.1M/s)</sup> | — | 52.8&nbsp;ns<br><sup>(19.0M/s)</sup> | 74.2&nbsp;ns<br><sup>(13.5M/s)</sup> | 341&nbsp;ns<br><sup>(293M/s)</sup> |
| `std::collections::BTreeMap` | 58.6&nbsp;ns<br><sup>(17.1M/s)</sup> | — | 31.5&nbsp;ns<br><sup>(31.7M/s)</sup> | 64.7&nbsp;ns<br><sup>(15.5M/s)</sup> | <img width="16" align="absmiddle" src="/img/rocket.png" alt="🚀">&nbsp;**183&nbsp;ns**<br><sup>(546M/s)</sup> |
| `dashmap::DashMap`* | 18.7&nbsp;ns<br><sup>(53.5M/s)</sup> | — | 20.9&nbsp;ns<br><sup>(47.8M/s)</sup> | 19.3&nbsp;ns<br><sup>(51.8M/s)</sup> | N/A |
| `papaya::HashMap`* | 18.8&nbsp;ns<br><sup>(53.2M/s)</sup> | — | 30.4&nbsp;ns<br><sup>(32.9M/s)</sup> | 32.8&nbsp;ns<br><sup>(30.5M/s)</sup> | N/A |
| `std::collections::HashMap`* | 13.1&nbsp;ns<br><sup>(76.1M/s)</sup> | — | 18.7&nbsp;ns<br><sup>(53.5M/s)</sup> | 21.1&nbsp;ns<br><sup>(47.4M/s)</sup> | N/A |

<sup>* `dashmap::DashMap`, `papaya::HashMap`, and `std::collections::HashMap` are marked with `*` as unordered $O(1)$ reference baselines and do not support range queries, sorted scans, or ordered traversals. The rocket icon denotes the fastest implementation among ordered, concurrent range-scannable maps. Sequential insert times for `arenaskiplist::SkipList` utilize its sequential inserter cache (`Inserter`).</sup>

### Multi-Threaded Concurrent Performance

When running multi-threaded workloads with concurrent writers, non-concurrent data structures (`BTreeMap`, `HashMap`, `imbl::OrdMap`) require synchronization via `parking_lot::RwLock`. Under write contention, exclusive lock acquisition serializes all threads, causing severe lock convoying and throughput collapse.

Benchmarked on bare metal (**AMD Ryzen Threadripper 9970X 32-Core / 64-Thread Processor @ 5.48 GHz, 128 GB DDR5 RAM**, Linux 6.8):

| Data Structure | Concurrent&nbsp;Writes<br><sup>(8&nbsp;Threads,&nbsp;100k&nbsp;Ops)</sup> | Mixed&nbsp;Workload<br><sup>(4R&nbsp;+&nbsp;4W,&nbsp;100k&nbsp;Ops)</sup> | Concurrency&nbsp;Model |
| :--- | ---: | ---: | :--- |
| **`arenaskiplist::SkipList`** | **40.50&nbsp;ms**<br><sup>(2.47M/s)</sup> | **28.28&nbsp;ms**<br><sup>(3.54M/s)</sup> | Lock-Free Atomic CAS (Contiguous Arena) |
| `crossbeam_skiplist::SkipMap` | <img width="16" align="absmiddle" src="/img/rocket.png" alt="🚀">&nbsp;**10.31&nbsp;ms**<br><sup>(9.70M/s)</sup> | <img width="16" align="absmiddle" src="/img/rocket.png" alt="🚀">&nbsp;**9.69&nbsp;ms**<br><sup>(10.3M/s)</sup> | Lock-Free Atomic CAS |
| `dashmap::DashMap`* | 3.67&nbsp;ms<br><sup>(27.2M/s)</sup> | 5.10&nbsp;ms<br><sup>(19.6M/s)</sup> | Fine-Grained Sharded RwLock |
| `papaya::HashMap`* | 4.26&nbsp;ms<br><sup>(23.5M/s)</sup> | 6.12&nbsp;ms<br><sup>(16.3M/s)</sup> | Lock-Free Reads + Fine-Grained Latching (EBR) |
| `parking_lot::RwLock<BTreeMap>` | 68.19&nbsp;ms<br><sup>(1.47M/s)</sup> | 38.05&nbsp;ms<br><sup>(2.63M/s)</sup> | Coarse Exclusive Lock |
| `parking_lot::RwLock<HashMap>`* | 76.64&nbsp;ms<br><sup>(1.30M/s)</sup> | 43.17&nbsp;ms<br><sup>(2.32M/s)</sup> | Coarse Exclusive Lock |
| `parking_lot::RwLock<imbl::OrdMap>` | 76.39&nbsp;ms<br><sup>(1.31M/s)</sup> | 54.98&nbsp;ms<br><sup>(1.82M/s)</sup> | Coarse Exclusive Lock |

### Memory Footprint & Allocation Overhead

Benchmarked with 100,000 keys (64-bit integer keys and 64-bit values), measuring idle data structure size in memory when full, peak heap memory during continuous insertion, allocation frequency, and whole-structure teardown cost:

| Data Structure | Idle&nbsp;Memory<br><sup>(100k&nbsp;items)</sup> | Peak&nbsp;Memory<br><sup>(during&nbsp;ingest)</sup> | Allocations<br><sup>(per&nbsp;insert)</sup> | Teardown&nbsp;/&nbsp;Reset<br><sup>(deallocation&nbsp;cost)</sup> |
| :--- | ---: | ---: | ---: | :--- |
| **`arenaskiplist::SkipList`** | **9.42&nbsp;MB**<br><sup>(94.2 B/item)</sup> | **16.00&nbsp;MB** | <img width="16" align="absmiddle" src="/img/rocket.png" alt="🚀">&nbsp;**0** | <img width="16" align="absmiddle" src="/img/rocket.png" alt="🚀">&nbsp;**$O(1)$ zero-cost reset** |
| `crossbeam_skiplist::SkipMap` | 3.82&nbsp;MB<br><sup>(38.2 B/item)</sup> | 3.82&nbsp;MB | ~1.0 | $O(N)$ epoch-deferred reclamation |
| `imbl::OrdMap` | 2.69&nbsp;MB<br><sup>(26.9 B/item)</sup> | 2.69&nbsp;MB | ~0.14 | $O(N)$ recursive heap drop |
| `std::collections::BTreeMap` | 2.58&nbsp;MB<br><sup>(25.8 B/item)</sup> | 2.58&nbsp;MB | ~0.17 | $O(N)$ recursive heap drop |
| `dashmap::DashMap`* | 2.14&nbsp;MB<br><sup>(21.4 B/item)</sup> | 2.15&nbsp;MB | ~0 | $O(N)$ heap drop |
| `papaya::HashMap`* | 3.80&nbsp;MB<br><sup>(38.0 B/item)</sup> | 3.80&nbsp;MB | ~1.0 | $O(N)$ epoch-deferred reclamation |
| `std::collections::HashMap`* | 2.13&nbsp;MB<br><sup>(21.3 B/item)</sup> | 3.19&nbsp;MB | ~0 | $O(N)$ heap drop |

- **1.9× Faster Sequential Ingestion than SkipMap**: With [`Inserter`], `arenaskiplist` achieves **39.4 ns** (25.4M items/sec), outperforming `crossbeam-skiplist::SkipMap` by **1.9×** (75.3 ns).
- **Outperforming SkipMap on Random Ingestion**: With inlined 4-byte key prefixes and branchless $p = 1/4$ height generation, standard random inserts run in **156.4 ns** (6.4M items/sec), **13% faster than `crossbeam-skiplist::SkipMap`** (176.5 ns).
- **2.76× Faster Range Scans than SkipMap**: Traverses 100 items in **793 ns** (126.2M items/sec) compared to **2.19 µs** for `crossbeam-skiplist::SkipMap`.
- **Zero Dynamic Allocations & $O(1)$ Reset**: `arenaskiplist` guarantees **0 per-insert heap allocations** by utilizing atomic bump allocation inside the pre-allocated arena buffer, enabling instant **$O(1)$ whole-arena teardown and reuse**.

## Features

- **Zero Per-Node Allocations**: All nodes and entries are packed contiguously into an arena using lock-free atomic bump allocation.
- **Lock-Free Concurrency**: Multiple writers insert concurrently via atomic compare-and-swap operations.
- **Lock-Free Reads**: Lookups and range scans proceed concurrently without acquiring locks or updating reference counts.
- **Splice Optimization**: Reusable [`Inserter`] caches search paths to accelerate sequential and localized batch insertions.
- **MVCC & LSM Versioning**: First-class support for 64-bit versioning (`insert_with_version`, `get_version_le`) for snapshot visibility boundaries.
- **Bidirectional Iteration**: Range scans with forward and reverse traversal ([`DoubleEndedIterator`]).
- **$O(1)$ Teardown**: Deallocating or recycling the entire skip list takes $O(1)$ time by dropping or resetting the arena buffer.
- **Deterministic Simulation Tested (DST)**: Continuously validated by a seeded PRNG fuzzer against an in-memory `BTreeMap` reference oracle.

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

## Architectural Comparison

| Feature | `crossbeam_skiplist::SkipMap` | `arenaskiplist::SkipList` |
| :--- | :--- | :--- |
| **Data Structure** | Lock-Free Skip List | **Lock-Free Arena Skip List** |
| **Memory Allocation** | Dynamic per-node heap allocs | **Contiguous Arena (Atomic bump allocation)** |
| **Memory Reclamation** | Epoch-based (`crossbeam-epoch`) | **Bulk $O(1)$ whole-arena drop/reset** |
| **Allocations / Insert**| ~1.0 allocations | **0 allocations** |
| **Sequential Optimization**| None | **[`Inserter`] Splice Caching** |
| **MVCC Versioning** | Manual key encoding | **Native 64-bit versioning & `get_version_le`** |
| **Target Workload** | General-purpose concurrent map | **LSM MemTables, WAL buffers, Append-heavy caches** |

## License

This project is licensed under the Apache License 2.0.
Portions of the skiplist algorithm are based on Andy Kimball's `arenaskl` and CockroachDB's `pebble`.
