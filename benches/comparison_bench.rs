use arenaskiplist::{Arena, Inserter, SkipList};
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use crossbeam_skiplist::SkipMap;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

const SAMPLE_SIZE: usize = 100_000;
const NUM_THREADS: usize = 8;
const PER_THREAD: usize = 10_000;

fn bench_insert_sequential(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_sequential");
    group.throughput(Throughput::Elements(1));

    // arenaskiplist with Inserter (optimized sequential)
    group.bench_function("arenaskiplist_with_inserter", |b| {
        let arena = Arena::with_capacity(64 * 1024 * 1024);
        let list = SkipList::new(arena);
        let mut ins = Inserter::new();
        let mut key = 0u64;
        b.iter(|| {
            let k = key.to_be_bytes();
            let _ = list.insert_with_inserter(&k, &k, &mut ins);
            key += 1;
        });
    });

    // arenaskiplist standard insert
    group.bench_function("arenaskiplist", |b| {
        let arena = Arena::with_capacity(64 * 1024 * 1024);
        let list = SkipList::new(arena);
        let mut key = 0u64;
        b.iter(|| {
            let k = key.to_be_bytes();
            let _ = list.insert(&k, &k);
            key += 1;
        });
    });

    // crossbeam-skiplist SkipMap
    group.bench_function("crossbeam_skipmap", |b| {
        let map = SkipMap::<[u8; 8], [u8; 8]>::new();
        let mut key = 0u64;
        b.iter(|| {
            let k = key.to_be_bytes();
            let _ = map.insert(k, k);
            key += 1;
        });
    });

    // BTreeMap
    group.bench_function("btreemap", |b| {
        let mut map = BTreeMap::new();
        let mut key = 0u64;
        b.iter(|| {
            let k = key.to_be_bytes();
            map.insert(k, k);
            key += 1;
        });
    });

    // imbl::OrdMap
    group.bench_function("imbl_ordmap", |b| {
        let mut map = imbl::OrdMap::new();
        let mut key = 0u64;
        b.iter(|| {
            let k = key.to_be_bytes();
            map.insert(k, k);
            key += 1;
        });
    });

    // HashMap
    group.bench_function("hashmap", |b| {
        let mut map = HashMap::new();
        let mut key = 0u64;
        b.iter(|| {
            let k = key.to_be_bytes();
            map.insert(k, k);
            key += 1;
        });
    });

    group.finish();
}

fn bench_insert_random(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_random");
    group.throughput(Throughput::Elements(1));

    let mut rng = fastrand::Rng::with_seed(0x12345678);
    let keys: Vec<[u8; 8]> = (0..SAMPLE_SIZE)
        .map(|_| rng.u64(..).to_be_bytes())
        .collect();

    group.bench_function("arenaskiplist", |b| {
        let arena = Arena::with_capacity(64 * 1024 * 1024);
        let list = SkipList::new(arena);
        let mut idx = 0usize;
        b.iter(|| {
            let k = &keys[idx % keys.len()];
            let _ = list.insert(k, k);
            idx += 1;
        });
    });

    // crossbeam-skiplist SkipMap
    group.bench_function("crossbeam_skipmap", |b| {
        let map = SkipMap::<[u8; 8], [u8; 8]>::new();
        let mut idx = 0usize;
        b.iter(|| {
            let k = &keys[idx % keys.len()];
            let _ = map.insert(*k, *k);
            idx += 1;
        });
    });

    group.bench_function("btreemap", |b| {
        let mut map = BTreeMap::new();
        let mut idx = 0usize;
        b.iter(|| {
            let k = &keys[idx % keys.len()];
            map.insert(*k, *k);
            idx += 1;
        });
    });

    group.bench_function("imbl_ordmap", |b| {
        let mut map = imbl::OrdMap::new();
        let mut idx = 0usize;
        b.iter(|| {
            let k = &keys[idx % keys.len()];
            map.insert(*k, *k);
            idx += 1;
        });
    });

    group.bench_function("hashmap", |b| {
        let mut map = HashMap::new();
        let mut idx = 0usize;
        b.iter(|| {
            let k = &keys[idx % keys.len()];
            map.insert(*k, *k);
            idx += 1;
        });
    });

    group.finish();
}

fn bench_get_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_hit");
    group.throughput(Throughput::Elements(1));

    let arena = Arena::with_capacity(64 * 1024 * 1024);
    let skl = SkipList::new(arena);
    let skip_map = SkipMap::<[u8; 8], [u8; 8]>::new();
    let mut btree_map = BTreeMap::new();
    let mut imbl_map = imbl::OrdMap::new();
    let mut hash_map = HashMap::new();

    for i in 0..SAMPLE_SIZE as u64 {
        let k = i.to_be_bytes();
        skl.insert(&k, &k).unwrap();
        skip_map.insert(k, k);
        btree_map.insert(k, k);
        imbl_map.insert(k, k);
        hash_map.insert(k, k);
    }

    group.bench_function("arenaskiplist", |b| {
        let mut rng = fastrand::Rng::with_seed(0x12345678);
        b.iter(|| {
            let k = (rng.u64(0..SAMPLE_SIZE as u64)).to_be_bytes();
            black_box(skl.get_value(&k));
        });
    });

    group.bench_function("crossbeam_skipmap", |b| {
        let mut rng = fastrand::Rng::with_seed(0x12345678);
        b.iter(|| {
            let k = (rng.u64(0..SAMPLE_SIZE as u64)).to_be_bytes();
            black_box(skip_map.get(&k));
        });
    });

    group.bench_function("btreemap", |b| {
        let mut rng = fastrand::Rng::with_seed(0x12345678);
        b.iter(|| {
            let k = (rng.u64(0..SAMPLE_SIZE as u64)).to_be_bytes();
            black_box(btree_map.get(&k));
        });
    });

    group.bench_function("imbl_ordmap", |b| {
        let mut rng = fastrand::Rng::with_seed(0x12345678);
        b.iter(|| {
            let k = (rng.u64(0..SAMPLE_SIZE as u64)).to_be_bytes();
            black_box(imbl_map.get(&k));
        });
    });

    group.bench_function("hashmap", |b| {
        let mut rng = fastrand::Rng::with_seed(0x12345678);
        b.iter(|| {
            let k = (rng.u64(0..SAMPLE_SIZE as u64)).to_be_bytes();
            black_box(hash_map.get(&k));
        });
    });

    group.finish();
}

fn bench_range_scan_100(c: &mut Criterion) {
    let mut group = c.benchmark_group("range_scan_100");
    group.throughput(Throughput::Elements(100));

    let arena = Arena::with_capacity(64 * 1024 * 1024);
    let skl = SkipList::new(arena);
    let skip_map = SkipMap::<[u8; 8], [u8; 8]>::new();
    let mut btree_map = BTreeMap::new();
    let mut imbl_map = imbl::OrdMap::new();

    for i in 0..SAMPLE_SIZE as u64 {
        let k = i.to_be_bytes();
        skl.insert(&k, &k).unwrap();
        skip_map.insert(k, k);
        btree_map.insert(k, k);
        imbl_map.insert(k, k);
    }

    let scan_limit = (SAMPLE_SIZE - 100) as u64;

    group.bench_function("arenaskiplist", |b| {
        let mut rng = fastrand::Rng::with_seed(0x12345678);
        b.iter(|| {
            let start = (rng.u64(0..scan_limit)).to_be_bytes();
            for e in skl.range(&start[..]..).take(100) {
                black_box(e);
            }
        });
    });

    group.bench_function("crossbeam_skipmap", |b| {
        let mut rng = fastrand::Rng::with_seed(0x12345678);
        b.iter(|| {
            let start = (rng.u64(0..scan_limit)).to_be_bytes();
            for entry in skip_map.range(start..).take(100) {
                black_box(entry);
            }
        });
    });

    group.bench_function("btreemap", |b| {
        let mut rng = fastrand::Rng::with_seed(0x12345678);
        b.iter(|| {
            let start = (rng.u64(0..scan_limit)).to_be_bytes();
            for entry in btree_map.range(start..).take(100) {
                black_box(entry);
            }
        });
    });

    group.bench_function("imbl_ordmap", |b| {
        let mut rng = fastrand::Rng::with_seed(0x12345678);
        b.iter(|| {
            let start = (rng.u64(0..scan_limit)).to_be_bytes();
            for entry in imbl_map.range(start..).take(100) {
                black_box(entry);
            }
        });
    });

    group.finish();
}

fn bench_concurrent_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_writes_8_threads");
    group.throughput(Throughput::Elements((NUM_THREADS * PER_THREAD) as u64));

    group.bench_function("arenaskiplist", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let arena = Arena::with_capacity(64 * 1024 * 1024);
                let list = Arc::new(SkipList::new(arena));
                let barrier = Arc::new(Barrier::new(NUM_THREADS + 1));
                let handles: Vec<_> = (0..NUM_THREADS)
                    .map(|t| {
                        let list = Arc::clone(&list);
                        let barrier = Arc::clone(&barrier);
                        std::thread::spawn(move || {
                            barrier.wait();
                            let start = t as u64 * PER_THREAD as u64;
                            for i in 0..PER_THREAD as u64 {
                                let k = (start + i).to_be_bytes();
                                let _ = list.insert(&k, &k);
                            }
                        })
                    })
                    .collect();

                barrier.wait();
                let start = Instant::now();
                for h in handles {
                    h.join().unwrap();
                }
                total += start.elapsed();
            }
            total
        });
    });

    group.bench_function("crossbeam_skipmap", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let map = Arc::new(SkipMap::<[u8; 8], [u8; 8]>::new());
                let barrier = Arc::new(Barrier::new(NUM_THREADS + 1));
                let handles: Vec<_> = (0..NUM_THREADS)
                    .map(|t| {
                        let map = Arc::clone(&map);
                        let barrier = Arc::clone(&barrier);
                        std::thread::spawn(move || {
                            barrier.wait();
                            let start = t as u64 * PER_THREAD as u64;
                            for i in 0..PER_THREAD as u64 {
                                let k = (start + i).to_be_bytes();
                                map.insert(k, k);
                            }
                        })
                    })
                    .collect();

                barrier.wait();
                let start = Instant::now();
                for h in handles {
                    h.join().unwrap();
                }
                total += start.elapsed();
            }
            total
        });
    });

    group.finish();
}

fn bench_concurrent_mixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_mixed_4r_4w");
    group.throughput(Throughput::Elements((NUM_THREADS * PER_THREAD) as u64));

    const PRE_POPULATE: u64 = 100_000;
    const NUM_READERS: usize = 4;
    const NUM_WRITERS: usize = 4;

    group.bench_function("arenaskiplist", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let arena = Arena::with_capacity(64 * 1024 * 1024);
                let list = Arc::new(SkipList::new(arena));
                for i in 0..PRE_POPULATE {
                    let k = i.to_be_bytes();
                    list.insert(&k, &k).unwrap();
                }

                let barrier = Arc::new(Barrier::new(NUM_READERS + NUM_WRITERS + 1));
                let mut handles = Vec::new();

                for t in 0..NUM_WRITERS {
                    let list = Arc::clone(&list);
                    let barrier = Arc::clone(&barrier);
                    handles.push(std::thread::spawn(move || {
                        barrier.wait();
                        let start = PRE_POPULATE + (t as u64 * PER_THREAD as u64);
                        for i in 0..PER_THREAD as u64 {
                            let k = (start + i).to_be_bytes();
                            let _ = list.insert(&k, &k);
                        }
                    }));
                }

                for _ in 0..NUM_READERS {
                    let list = Arc::clone(&list);
                    let barrier = Arc::clone(&barrier);
                    handles.push(std::thread::spawn(move || {
                        let mut rng = fastrand::Rng::with_seed(0x87654321);
                        barrier.wait();
                        for _ in 0..PER_THREAD {
                            let k = (rng.u64(0..PRE_POPULATE)).to_be_bytes();
                            black_box(list.get_value(&k));
                        }
                    }));
                }

                barrier.wait();
                let start = Instant::now();
                for h in handles {
                    h.join().unwrap();
                }
                total += start.elapsed();
            }
            total
        });
    });

    group.bench_function("crossbeam_skipmap", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let map = Arc::new(SkipMap::<[u8; 8], [u8; 8]>::new());
                for i in 0..PRE_POPULATE {
                    let k = i.to_be_bytes();
                    map.insert(k, k);
                }

                let barrier = Arc::new(Barrier::new(NUM_READERS + NUM_WRITERS + 1));
                let mut handles = Vec::new();

                for t in 0..NUM_WRITERS {
                    let map = Arc::clone(&map);
                    let barrier = Arc::clone(&barrier);
                    handles.push(std::thread::spawn(move || {
                        barrier.wait();
                        let start = PRE_POPULATE + (t as u64 * PER_THREAD as u64);
                        for i in 0..PER_THREAD as u64 {
                            let k = (start + i).to_be_bytes();
                            map.insert(k, k);
                        }
                    }));
                }

                for _ in 0..NUM_READERS {
                    let map = Arc::clone(&map);
                    let barrier = Arc::clone(&barrier);
                    handles.push(std::thread::spawn(move || {
                        let mut rng = fastrand::Rng::with_seed(0x87654321);
                        barrier.wait();
                        for _ in 0..PER_THREAD {
                            let k = (rng.u64(0..PRE_POPULATE)).to_be_bytes();
                            black_box(map.get(&k));
                        }
                    }));
                }

                barrier.wait();
                let start = Instant::now();
                for h in handles {
                    h.join().unwrap();
                }
                total += start.elapsed();
            }
            total
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_insert_sequential,
    bench_insert_random,
    bench_get_hit,
    bench_range_scan_100,
    bench_concurrent_writes,
    bench_concurrent_mixed,
);
criterion_main!(benches);
