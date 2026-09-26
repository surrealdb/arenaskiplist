use arenaskiplist::{Arena, Inserter, SkipList};
use artmap::ArtMap;
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use crossbeam_skiplist::SkipMap;
use std::sync::{Arc, Barrier};
use std::time::Duration;

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

    // artmap ArtMap
    group.bench_function("artmap", |b| {
        let map = ArtMap::<[u8; 8], [u8; 8]>::new();
        let mut key = 0u64;
        b.iter(|| {
            let k = key.to_be_bytes();
            let _ = map.insert(k, k);
            key += 1;
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
    let art_map = ArtMap::<[u8; 8], [u8; 8]>::new();

    for i in 0..SAMPLE_SIZE as u64 {
        let k = i.to_be_bytes();
        skl.insert(&k, &k).unwrap();
        skip_map.insert(k, k);
        art_map.insert(k, k);
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

    group.bench_function("artmap", |b| {
        let mut rng = fastrand::Rng::with_seed(0x12345678);
        b.iter(|| {
            let k = (rng.u64(0..SAMPLE_SIZE as u64)).to_be_bytes();
            black_box(art_map.get(&k));
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
    let art_map = ArtMap::<[u8; 8], [u8; 8]>::new();

    for i in 0..SAMPLE_SIZE as u64 {
        let k = i.to_be_bytes();
        skl.insert(&k, &k).unwrap();
        skip_map.insert(k, k);
        art_map.insert(k, k);
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

    group.bench_function("artmap", |b| {
        let mut rng = fastrand::Rng::with_seed(0x12345678);
        b.iter(|| {
            let start = (rng.u64(0..scan_limit)).to_be_bytes();
            for entry in art_map.range(start..).take(100) {
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
                let start = std::time::Instant::now();
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
                let start = std::time::Instant::now();
                for h in handles {
                    h.join().unwrap();
                }
                total += start.elapsed();
            }
            total
        });
    });

    group.bench_function("artmap", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let map = Arc::new(ArtMap::<[u8; 8], [u8; 8]>::new());
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
                let start = std::time::Instant::now();
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
    bench_get_hit,
    bench_range_scan_100,
    bench_concurrent_writes,
);
criterion_main!(benches);
