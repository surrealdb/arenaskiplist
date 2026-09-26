use arenaskiplist::{Arena, SkipList};
use std::sync::{Arc, Barrier};
use std::thread;

#[test]
fn test_high_concurrency_stress() {
    let arena = Arena::with_capacity(128 * 1024 * 1024);
    let list = Arc::new(SkipList::new(arena));

    const NUM_THREADS: usize = 16;
    const OPS_PER_THREAD: usize = 10_000;
    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let mut handles = Vec::new();
    for t in 0..NUM_THREADS {
        let list = Arc::clone(&list);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            let mut rng = fastrand::Rng::with_seed((t as u64 + 1) * 0xdeadbeef);
            for i in 0..OPS_PER_THREAD {
                let op = rng.usize(0..10);
                let key_id = rng.u64(0..5000);
                let k = key_id.to_be_bytes();

                if op < 6 {
                    let val = ((t * OPS_PER_THREAD + i) as u64).to_be_bytes();
                    let _ = list.insert(&k, &val);
                } else {
                    let _ = list.get(&k);
                }
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert!(!list.is_empty());
    assert!(list.size() > 0);
}
